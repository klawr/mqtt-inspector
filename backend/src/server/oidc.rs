/*
 * Copyright (c) 2024 Kai Lawrence
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

//! Claims-based access control with tokens from an OpenID Connect provider
//! (e.g. Keycloak), forwarded by a reverse proxy such as oauth2-proxy.
//!
//! The backend never performs a login itself. It only verifies the JWT the
//! proxy forwards on the WebSocket upgrade request (signature via the issuer's
//! JWKS, `iss`, `exp`, optional audience) and maps its role claims to
//! permissions.

use std::collections::HashSet;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{Jwk, PublicKeyUse};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};

/// Headers the access token may arrive in, in order of preference.
/// The explicit access-token headers (oauth2-proxy `--pass-access-token` /
/// `--set-xauthrequest`) win over `Authorization: Bearer`, because oauth2-proxy's
/// `--pass-authorization-header` puts the ID token there, which in Keycloak
/// carries no roles by default.
const TOKEN_HEADERS: [&str; 3] = [
    "x-forwarded-access-token",
    "x-auth-request-access-token",
    "authorization",
];

/// Minimum time between two JWKS refreshes triggered by unknown key ids.
const JWKS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
/// Minimum time between fetch attempts while no keys are loaded (issuer down
/// or misconfigured), so tokens can't make every request wait on a fetch.
const JWKS_RETRY_AFTER_FAILURE: Duration = Duration::from_secs(5);
/// Allowed clock skew when checking `exp`/`nbf`.
const CLOCK_LEEWAY_SECS: u64 = 30;

#[derive(Clone, Debug, PartialEq)]
pub struct OidcConfig {
    pub issuer: String,
    pub jwks_url: Option<String>,
    pub audience: Option<String>,
    pub roles_claims: Vec<String>,
    pub user_role: Option<String>,
    pub admin_role: Option<String>,
    pub ca_cert: Option<String>,
}

impl OidcConfig {
    /// Reads the configuration from `MQTT_INSPECTOR_OIDC_*` environment
    /// variables. Returns `None` (password mode) unless an issuer is set.
    pub fn from_env() -> Option<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let get = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let issuer = get("MQTT_INSPECTOR_OIDC_ISSUER")?;
        let roles_claims = get("MQTT_INSPECTOR_OIDC_ROLES_CLAIM")
            .unwrap_or_else(|| "realm_access.roles".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Some(Self {
            issuer,
            jwks_url: get("MQTT_INSPECTOR_OIDC_JWKS_URL"),
            audience: get("MQTT_INSPECTOR_OIDC_AUDIENCE"),
            roles_claims,
            user_role: get("MQTT_INSPECTOR_OIDC_USER_ROLE"),
            admin_role: get("MQTT_INSPECTOR_OIDC_ADMIN_ROLE"),
            ca_cert: get("MQTT_INSPECTOR_OIDC_CA_CERT"),
        })
    }
}

/// The verified identity of a connected user.
#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    pub subject: String,
    pub username: Option<String>,
    pub roles: HashSet<String>,
    /// Token expiry (seconds since the Unix epoch).
    pub expires_at: i64,
    /// May add/remove brokers, commands and pipelines.
    pub is_admin: bool,
}

impl Identity {
    /// True if `allowed_roles` is empty (open broker) or the user has any of them.
    pub fn has_any_role(&self, allowed_roles: &[String]) -> bool {
        allowed_roles.is_empty() || allowed_roles.iter().any(|r| self.roles.contains(r))
    }

    pub fn display_name(&self) -> &str {
        self.username.as_deref().unwrap_or(&self.subject)
    }
}

#[derive(Debug, PartialEq)]
pub enum AuthError {
    /// No token was forwarded (proxy missing or misconfigured) → 401.
    MissingToken,
    /// The token is malformed, expired, wrongly signed or for another issuer/audience → 401.
    InvalidToken(String),
    /// The token is valid but lacks the required role → 403.
    Forbidden(String),
    /// The signing keys could not be loaded → 503.
    Unavailable(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::MissingToken => write!(f, "no access token forwarded"),
            AuthError::InvalidToken(e) => write!(f, "invalid token: {e}"),
            AuthError::Forbidden(e) => write!(f, "forbidden: {e}"),
            AuthError::Unavailable(e) => write!(f, "token verification unavailable: {e}"),
        }
    }
}

/// Extracts the access token from the forwarded request headers.
pub fn token_from_headers(headers: &warp::http::HeaderMap) -> Option<String> {
    TOKEN_HEADERS.iter().find_map(|name| {
        let value = headers.get(*name)?.to_str().ok()?.trim();
        let token = if *name == "authorization" {
            let (scheme, rest) = value.split_once(' ')?;
            if !scheme.eq_ignore_ascii_case("bearer") {
                return None;
            }
            rest.trim()
        } else {
            value
        };
        (!token.is_empty()).then(|| token.to_string())
    })
}

struct SigningKey {
    kid: Option<String>,
    /// Algorithm pinned by the JWK, if it declares one.
    alg: Option<Algorithm>,
    key: DecodingKey,
}

#[derive(Default)]
struct KeyCache {
    keys: Vec<SigningKey>,
    last_attempt: Option<Instant>,
}

pub struct OidcVerifier {
    config: OidcConfig,
    http: reqwest::Client,
    cache: tokio::sync::Mutex<KeyCache>,
}

fn is_asymmetric(alg: Algorithm) -> bool {
    !matches!(alg, Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512)
}

/// Parses a JWKS document, keeping only asymmetric signature keys. Keys that
/// cannot be parsed (e.g. unknown types) are skipped instead of failing the set.
fn parse_jwks(jwks: &serde_json::Value) -> Vec<SigningKey> {
    let Some(keys) = jwks.get("keys").and_then(|k| k.as_array()) else {
        return Vec::new();
    };
    keys.iter()
        .filter_map(|value| serde_json::from_value::<Jwk>(value.clone()).ok())
        .filter(|jwk| {
            !matches!(
                jwk.common.public_key_use,
                Some(PublicKeyUse::Encryption) | Some(PublicKeyUse::Other(_))
            )
        })
        .filter_map(|jwk| {
            let alg = match jwk.common.key_algorithm {
                Some(ka) => {
                    let alg: Algorithm = ka.to_string().parse().ok()?;
                    if !is_asymmetric(alg) {
                        return None;
                    }
                    Some(alg)
                }
                None => None,
            };
            if matches!(
                jwk.algorithm,
                jsonwebtoken::jwk::AlgorithmParameters::OctetKey(_)
            ) {
                return None;
            }
            let key = DecodingKey::from_jwk(&jwk).ok()?;
            Some(SigningKey {
                kid: jwk.common.key_id.clone(),
                alg,
                key,
            })
        })
        .collect()
}

/// Collects role names from the configured claim paths (dot-separated, e.g.
/// `realm_access.roles` or `resource_access.mqtt-inspector.roles` or `groups`).
fn extract_roles(claims: &serde_json::Value, paths: &[String]) -> HashSet<String> {
    let mut roles = HashSet::new();
    for path in paths {
        let mut value = Some(claims);
        for segment in path.split('.') {
            value = value.and_then(|v| v.get(segment));
        }
        match value {
            Some(serde_json::Value::String(s)) => {
                roles.insert(s.clone());
            }
            Some(serde_json::Value::Array(items)) => {
                roles.extend(items.iter().filter_map(|i| i.as_str().map(str::to_string)));
            }
            _ => {}
        }
    }
    roles
}

fn audience_matches(claims: &serde_json::Value, audience: &str) -> bool {
    let in_aud = match claims.get("aud") {
        Some(serde_json::Value::String(s)) => s == audience,
        Some(serde_json::Value::Array(items)) => items.iter().any(|i| i.as_str() == Some(audience)),
        _ => false,
    };
    // Keycloak access tokens carry the client id in `azp`; `aud` only contains
    // it when an audience mapper is configured.
    in_aud || claims.get("azp").and_then(|v| v.as_str()) == Some(audience)
}

impl OidcVerifier {
    pub fn new(config: OidcConfig) -> Result<Self, String> {
        let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(10));
        if let Some(ca_path) = &config.ca_cert {
            let pem = std::fs::read(ca_path).map_err(|e| format!("cannot read {ca_path}: {e}"))?;
            for cert in reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|e| format!("cannot parse {ca_path}: {e}"))?
            {
                builder = builder.add_root_certificate(cert);
            }
        }
        let http = builder
            .build()
            .map_err(|e| format!("cannot create HTTP client: {e}"))?;
        Ok(Self {
            config,
            http,
            cache: tokio::sync::Mutex::new(KeyCache::default()),
        })
    }

    async fn fetch_json(&self, url: &str) -> Result<serde_json::Value, String> {
        self.http
            .get(url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("GET {url} failed: {e}"))?
            .json()
            .await
            .map_err(|e| format!("GET {url} returned invalid JSON: {e}"))
    }

    async fn fetch_keys(&self) -> Result<Vec<SigningKey>, String> {
        let jwks_url = match &self.config.jwks_url {
            Some(url) => url.clone(),
            None => {
                let discovery = format!(
                    "{}/.well-known/openid-configuration",
                    self.config.issuer.trim_end_matches('/')
                );
                self.fetch_json(&discovery)
                    .await?
                    .get("jwks_uri")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| format!("{discovery} has no jwks_uri"))?
            }
        };
        let keys = parse_jwks(&self.fetch_json(&jwks_url).await?);
        if keys.is_empty() {
            return Err(format!("no usable signing keys at {jwks_url}"));
        }
        Ok(keys)
    }

    /// Finds the key for `kid`/`alg`, refreshing the JWKS (rate-limited) when
    /// it is unknown, e.g. after a key rotation.
    async fn decoding_key(
        &self,
        kid: Option<&str>,
        alg: Algorithm,
    ) -> Result<(DecodingKey, Option<Algorithm>), AuthError> {
        let find = |keys: &[SigningKey]| {
            let candidates: Vec<&SigningKey> = keys
                .iter()
                .filter(|k| match kid {
                    Some(kid) => k.kid.as_deref() == Some(kid),
                    None => true,
                })
                .filter(|k| k.alg.is_none_or(|a| a == alg))
                .collect();
            // Without a kid, only accept an unambiguous key.
            match (kid, candidates.as_slice()) {
                (_, [single]) => Some((single.key.clone(), single.alg)),
                (Some(_), [first, ..]) => Some((first.key.clone(), first.alg)),
                _ => None,
            }
        };

        let mut cache = self.cache.lock().await;
        if let Some(found) = find(&cache.keys) {
            return Ok(found);
        }
        let min_interval = if cache.keys.is_empty() {
            JWKS_RETRY_AFTER_FAILURE
        } else {
            JWKS_REFRESH_INTERVAL
        };
        let may_refresh = cache
            .last_attempt
            .is_none_or(|t| t.elapsed() >= min_interval);
        if may_refresh {
            cache.last_attempt = Some(Instant::now());
            match self.fetch_keys().await {
                Ok(keys) => cache.keys = keys,
                Err(e) if cache.keys.is_empty() => return Err(AuthError::Unavailable(e)),
                Err(e) => eprintln!("JWKS refresh failed, keeping cached keys: {e}"),
            }
        } else if cache.keys.is_empty() {
            return Err(AuthError::Unavailable(
                "signing keys not loaded yet; retrying shortly".to_string(),
            ));
        }
        find(&cache.keys)
            .ok_or_else(|| AuthError::InvalidToken("token signed with an unknown key".to_string()))
    }

    /// Verifies the token and returns the identity with its permissions.
    pub async fn verify(&self, token: &str) -> Result<Identity, AuthError> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|e| AuthError::InvalidToken(format!("malformed token: {e}")))?;
        if !is_asymmetric(header.alg) {
            return Err(AuthError::InvalidToken(format!(
                "algorithm {:?} is not allowed",
                header.alg
            )));
        }
        let (key, _) = self.decoding_key(header.kid.as_deref(), header.alg).await?;

        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "sub"]);
        validation.validate_aud = false; // checked below (aud or azp)
        validation.validate_nbf = true;
        validation.leeway = CLOCK_LEEWAY_SECS;
        let claims = jsonwebtoken::decode::<serde_json::Value>(token, &key, &validation)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?
            .claims;

        if let Some(audience) = &self.config.audience {
            if !audience_matches(&claims, audience) {
                return Err(AuthError::InvalidToken(format!(
                    "token is not intended for audience {audience}"
                )));
            }
        }
        self.identity_from_claims(&claims)
    }

    fn identity_from_claims(&self, claims: &serde_json::Value) -> Result<Identity, AuthError> {
        let roles = extract_roles(claims, &self.config.roles_claims);
        let subject = claims
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let username = claims
            .get("preferred_username")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if let Some(role) = &self.config.user_role {
            if !roles.contains(role) {
                return Err(AuthError::Forbidden(format!(
                    "{} lacks role {role}",
                    username.as_deref().unwrap_or(&subject)
                )));
            }
        }
        let is_admin = self
            .config
            .admin_role
            .as_ref()
            .is_none_or(|role| roles.contains(role));
        Ok(Identity {
            subject,
            username,
            expires_at: claims.get("exp").and_then(|v| v.as_i64()).unwrap_or(0),
            roles,
            is_admin,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    // --- helpers ---

    fn base64url(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            for i in 0..chunk.len() + 1 {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        out
    }

    struct RsaKey {
        private_pem: Vec<u8>,
        jwk: serde_json::Value,
    }

    /// Generates an RSA key with openssl and returns it with its public JWK.
    fn rsa_key(kid: &str) -> RsaKey {
        let dir = std::env::temp_dir().join(format!("mqtt_inspector_jwt_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join("key.pem");
        let status = std::process::Command::new("openssl")
            .args(["genrsa", "-out", &key_path.to_string_lossy(), "2048"])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let modulus = std::process::Command::new("openssl")
            .args([
                "rsa",
                "-in",
                &key_path.to_string_lossy(),
                "-noout",
                "-modulus",
            ])
            .output()
            .unwrap();
        let hex = String::from_utf8(modulus.stdout).unwrap();
        let hex = hex.trim().trim_start_matches("Modulus=");
        let n: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let private_pem = std::fs::read(&key_path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        RsaKey {
            private_pem,
            jwk: serde_json::json!({
                "kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid,
                "n": base64url(&n), "e": "AQAB"
            }),
        }
    }

    fn sign(key: &RsaKey, kid: &str, claims: &serde_json::Value) -> String {
        let mut header = jsonwebtoken::Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        jsonwebtoken::encode(
            &header,
            claims,
            &jsonwebtoken::EncodingKey::from_rsa_pem(&key.private_pem).unwrap(),
        )
        .unwrap()
    }

    /// Serves `/.well-known/openid-configuration` and `/certs` like Keycloak.
    async fn serve_issuer(jwks: serde_json::Value) -> String {
        use warp::Filter;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        drop(listener);
        let base = format!("http://{addr}/realms/test");
        let discovery_doc =
            serde_json::json!({ "issuer": base, "jwks_uri": format!("{base}/certs") });
        let discovery = warp::path!("realms" / "test" / ".well-known" / "openid-configuration")
            .map(move || warp::reply::json(&discovery_doc));
        let certs = warp::path!("realms" / "test" / "certs").map(move || warp::reply::json(&jwks));
        tokio::spawn(warp::serve(discovery.or(certs)).run(addr));
        // wait until the server accepts connections
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(addr).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        base
    }

    fn config(issuer: &str) -> OidcConfig {
        OidcConfig {
            issuer: issuer.to_string(),
            jwks_url: None,
            audience: None,
            roles_claims: vec!["realm_access.roles".to_string()],
            user_role: None,
            admin_role: None,
            ca_cert: None,
        }
    }

    fn claims(issuer: &str, roles: &[&str]) -> serde_json::Value {
        let now = chrono::Utc::now().timestamp();
        serde_json::json!({
            "iss": issuer, "sub": "user-1", "preferred_username": "alice",
            "exp": now + 300, "iat": now, "azp": "mqtt-inspector",
            "aud": "account",
            "realm_access": { "roles": roles }
        })
    }

    macro_rules! require_openssl {
        () => {
            if !crate::server::test_certs::openssl_available() {
                eprintln!("Skipping: openssl not available");
                return;
            }
        };
    }

    // --- configuration ---

    #[test]
    fn test_config_disabled_without_issuer() {
        assert_eq!(OidcConfig::from_lookup(|_| None), None);
        assert_eq!(
            OidcConfig::from_lookup(|n| (n == "MQTT_INSPECTOR_OIDC_ISSUER").then(|| " ".into())),
            None
        );
    }

    #[test]
    fn test_config_defaults_and_lists() {
        let cfg = OidcConfig::from_lookup(|n| match n {
            "MQTT_INSPECTOR_OIDC_ISSUER" => Some("https://kc/realms/a".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(cfg.roles_claims, vec!["realm_access.roles"]);
        assert_eq!(cfg.admin_role, None);

        let cfg = OidcConfig::from_lookup(|n| match n {
            "MQTT_INSPECTOR_OIDC_ISSUER" => Some("https://kc/realms/a".into()),
            "MQTT_INSPECTOR_OIDC_ROLES_CLAIM" => Some("realm_access.roles, groups".into()),
            "MQTT_INSPECTOR_OIDC_ADMIN_ROLE" => Some("admin".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(cfg.roles_claims, vec!["realm_access.roles", "groups"]);
        assert_eq!(cfg.admin_role.as_deref(), Some("admin"));
    }

    // --- token extraction ---

    #[test]
    fn test_token_from_headers() {
        let mut headers = warp::http::HeaderMap::new();
        assert_eq!(token_from_headers(&headers), None);
        headers.insert("authorization", "Basic abc".parse().unwrap());
        assert_eq!(token_from_headers(&headers), None);
        headers.insert("authorization", "bearer tok".parse().unwrap());
        assert_eq!(token_from_headers(&headers).as_deref(), Some("tok"));
        // explicit access-token header wins over Authorization (may hold the ID token)
        headers.insert("x-forwarded-access-token", "fwd".parse().unwrap());
        assert_eq!(token_from_headers(&headers).as_deref(), Some("fwd"));

        let mut headers = warp::http::HeaderMap::new();
        headers.insert("x-auth-request-access-token", "xar".parse().unwrap());
        assert_eq!(token_from_headers(&headers).as_deref(), Some("xar"));
    }

    // --- claims helpers ---

    #[test]
    fn test_extract_roles_from_paths() {
        let claims = serde_json::json!({
            "realm_access": { "roles": ["viewer", "admin"] },
            "resource_access": { "mqtt-inspector": { "roles": ["plant-a"] } },
            "groups": ["/team"],
            "single": "solo"
        });
        let roles = extract_roles(
            &claims,
            &[
                "realm_access.roles".into(),
                "resource_access.mqtt-inspector.roles".into(),
                "groups".into(),
                "single".into(),
                "missing.path".into(),
            ],
        );
        let expected: HashSet<String> = ["viewer", "admin", "plant-a", "/team", "solo"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(roles, expected);
    }

    #[test]
    fn test_audience_matches_aud_or_azp() {
        let c = serde_json::json!({ "aud": ["account", "api"], "azp": "web" });
        assert!(audience_matches(&c, "api"));
        assert!(audience_matches(&c, "web"));
        assert!(!audience_matches(&c, "other"));
        assert!(audience_matches(
            &serde_json::json!({ "aud": "api" }),
            "api"
        ));
    }

    #[test]
    fn test_parse_jwks_keeps_only_signature_keys() {
        require_openssl!();
        let key = rsa_key("sig");
        let mut enc = key.jwk.clone();
        enc["use"] = "enc".into();
        enc["alg"] = "RSA-OAEP".into();
        enc["kid"] = "enc".into();
        let jwks = serde_json::json!({ "keys": [
            key.jwk,
            enc,
            { "kty": "oct", "k": "c2VjcmV0", "alg": "HS256", "kid": "hmac" },
            { "kty": "unknown", "kid": "weird" }
        ]});
        let keys = parse_jwks(&jwks);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].kid.as_deref(), Some("sig"));
    }

    // --- full verification against a served JWKS ---

    #[tokio::test]
    async fn test_verify_valid_token_and_roles() {
        require_openssl!();
        let key = rsa_key("k1");
        let issuer = serve_issuer(serde_json::json!({ "keys": [key.jwk] })).await;
        let mut cfg = config(&issuer);
        cfg.admin_role = Some("admin".into());
        let verifier = OidcVerifier::new(cfg).unwrap();

        let id = verifier
            .verify(&sign(&key, "k1", &claims(&issuer, &["viewer", "plant-a"])))
            .await
            .unwrap();
        assert_eq!(id.display_name(), "alice");
        assert!(id.roles.contains("plant-a"));
        assert!(!id.is_admin);
        assert!(id.has_any_role(&["plant-a".into(), "plant-b".into()]));
        assert!(!id.has_any_role(&["plant-b".into()]));
        assert!(id.has_any_role(&[]));

        let admin = verifier
            .verify(&sign(&key, "k1", &claims(&issuer, &["admin"])))
            .await
            .unwrap();
        assert!(admin.is_admin);
    }

    #[tokio::test]
    async fn test_verify_rejects_bad_tokens() {
        require_openssl!();
        let key = rsa_key("k1");
        let attacker = rsa_key("k1"); // same kid, different key
        let issuer = serve_issuer(serde_json::json!({ "keys": [key.jwk] })).await;
        let verifier = OidcVerifier::new(config(&issuer)).unwrap();
        let invalid = |r: Result<Identity, AuthError>| matches!(r, Err(AuthError::InvalidToken(_)));

        // wrong signature
        let forged = sign(&attacker, "k1", &claims(&issuer, &["admin"]));
        assert!(invalid(verifier.verify(&forged).await));

        // expired (beyond leeway)
        let mut expired = claims(&issuer, &[]);
        expired["exp"] = (chrono::Utc::now().timestamp() - 120).into();
        assert!(invalid(verifier.verify(&sign(&key, "k1", &expired)).await));

        // other issuer
        let other = claims("https://evil.example/realms/test", &[]);
        assert!(invalid(verifier.verify(&sign(&key, "k1", &other)).await));

        // symmetric algorithm
        let hs = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &claims(&issuer, &["admin"]),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert!(invalid(verifier.verify(&hs).await));

        // garbage
        assert!(invalid(verifier.verify("not.a.jwt").await));
    }

    #[tokio::test]
    async fn test_verify_audience_and_user_role() {
        require_openssl!();
        let key = rsa_key("k1");
        let issuer = serve_issuer(serde_json::json!({ "keys": [key.jwk] })).await;
        let mut cfg = config(&issuer);
        cfg.audience = Some("mqtt-inspector".into()); // matches azp
        cfg.user_role = Some("viewer".into());
        let verifier = OidcVerifier::new(cfg.clone()).unwrap();

        assert!(verifier
            .verify(&sign(&key, "k1", &claims(&issuer, &["viewer"])))
            .await
            .is_ok());
        assert!(matches!(
            verifier
                .verify(&sign(&key, "k1", &claims(&issuer, &["other"])))
                .await,
            Err(AuthError::Forbidden(_))
        ));

        cfg.audience = Some("another-client".into());
        let verifier = OidcVerifier::new(cfg).unwrap();
        assert!(matches!(
            verifier
                .verify(&sign(&key, "k1", &claims(&issuer, &["viewer"])))
                .await,
            Err(AuthError::InvalidToken(_))
        ));
    }

    #[tokio::test]
    async fn test_verify_unreachable_issuer_is_unavailable() {
        require_openssl!();
        let key = rsa_key("k1");
        let verifier = OidcVerifier::new(config("http://127.0.0.1:9/realms/none")).unwrap();
        let token = sign(&key, "k1", &claims("http://127.0.0.1:9/realms/none", &[]));
        match verifier.verify(&token).await {
            Err(AuthError::Unavailable(e)) => assert!(e.contains("GET"), "{e}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
        // A second request right after the failure must not fetch again.
        let start = Instant::now();
        match verifier.verify(&token).await {
            Err(AuthError::Unavailable(e)) => assert!(e.contains("retrying shortly"), "{e}"),
            other => panic!("expected Unavailable, got {other:?}"),
        }
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
