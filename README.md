# <img src="frontend/static/favicon.svg" alt="mqtt-inspector icon" width="32" height="32" align="absmiddle" /> mqtt-inspector

mqtt-inspector helps you watch MQTT traffic in real time.

Connect your broker(s), open the web UI, and quickly:

- browse topics
- inspect live messages
- publish test messages
- save reusable commands and pipelines
- monitor traffic and storage at a glance

## What You Get

- Multi-broker support.
- Topic tree with search.
- Live message viewer.
- Publish panel (including retained messages).
- Saved commands for frequent actions.
- Pipeline view for message flow timing.
- Throughput and history charts.
- Optional broker protection with per-user authentication.

## Quick Start

### Docker (recommended)

```bash
docker run \
  -p 3030:3030 \
  -v ./config:/srv/config \
  ghcr.io/klawr/mqtt-inspector:latest
```

Then open http://localhost:3030.

### Docker Compose

```yaml
services:
  mqtt-inspector:
    image: ghcr.io/klawr/mqtt-inspector:latest
    ports:
      - 3030:3030
    volumes:
      - ./config:/srv/config
```

## Configuration Files

The app uses a config directory mounted at /srv/config.

- brokers.json
- commands/
- pipelines/

Example brokers.json:

```json
[
  { "host": "localhost:1883" },
  {
    "host": "broker.example.com:8883",
    "use_tls": true,
    "username": "user1",
    "password": "secret"
  }
]
```

### Protecting a broker in the UI

Any broker can require viewers to enter a password before its messages are shown:

```json
[
  { "host": "localhost:1883", "access_password": "view-secret" }
]
```

- `access_password` is optional and works for any broker, including anonymous and certificate-only ones.
- If it is not set, the MQTT `password` (if any) is used instead, so brokers with username/password
  are protected without extra configuration.
- If both are set, `access_password` is the one viewers must enter.

### Claims-based access (Keycloak / OpenID Connect via reverse proxy)

Instead of passwords, access can be controlled by the roles in an access token from an
OpenID Connect provider such as Keycloak. mqtt-inspector does not log users in itself: a
reverse proxy (e.g. [oauth2-proxy](https://oauth2-proxy.github.io/oauth2-proxy/)) handles the
login and forwards the access token. The backend verifies the token's signature against the
issuer's keys (JWKS), its issuer, expiry and optional audience, so bypassing the proxy does not
grant access.

Claims mode is enabled by setting `MQTT_INSPECTOR_OIDC_ISSUER`:

| Variable | Description |
|----------|-------------|
| `MQTT_INSPECTOR_OIDC_ISSUER` | Issuer URL, must equal the token's `iss`, e.g. `https://auth.example.com/realms/acme`. |
| `MQTT_INSPECTOR_OIDC_JWKS_URL` | Optional. Where to fetch the signing keys. Default: discovered via `<issuer>/.well-known/openid-configuration`. Set it if the backend reaches Keycloak under an internal URL, e.g. `http://keycloak:8080/realms/acme/protocol/openid-connect/certs`. |
| `MQTT_INSPECTOR_OIDC_AUDIENCE` | Optional. Token must contain it in `aud`, or as `azp` (Keycloak's client id). |
| `MQTT_INSPECTOR_OIDC_ROLES_CLAIM` | Where to read roles from. Default `realm_access.roles`. Dot-separated paths, comma-separated list, e.g. `realm_access.roles,groups` or `resource_access.mqtt-inspector.roles`. |
| `MQTT_INSPECTOR_OIDC_USER_ROLE` | Optional. Role required to use the app at all (otherwise every valid token). |
| `MQTT_INSPECTOR_OIDC_ADMIN_ROLE` | Optional. Role required to add/remove brokers, commands and pipelines (otherwise every user may). |
| `MQTT_INSPECTOR_OIDC_CA_CERT` | Optional. PEM file with the CA of an HTTPS issuer signed by a private CA. |

Per broker, `allowed_roles` restricts who can see and publish to it (any of the listed roles);
brokers without it are visible to every user:

```json
[
  { "host": "plant-a.example.com:1883", "allowed_roles": ["plant-a"] },
  { "host": "localhost:1883" }
]
```

- Users only see the brokers they have access to.
- Broker passwords (`password`, `access_password`) are not used as UI locks in claims mode;
  `username`/`password` are still used to log in to the MQTT broker.
- The token is read from `X-Forwarded-Access-Token`, `X-Auth-Request-Access-Token` or
  `Authorization: Bearer`, in that order.
- When the token expires, the connection is closed and the UI reconnects through the proxy with a
  fresh token.
- If the issuer cannot be reached, connections are refused (HTTP 503); missing or invalid tokens
  get 401, missing `MQTT_INSPECTOR_OIDC_USER_ROLE` gets 403.

Example with oauth2-proxy in front:

```yaml
services:
  oauth2-proxy:
    image: quay.io/oauth2-proxy/oauth2-proxy:v7.7.1
    command:
      - --provider=keycloak-oidc
      - --oidc-issuer-url=https://auth.example.com/realms/acme
      - --client-id=mqtt-inspector
      - --client-secret=<secret>
      - --cookie-secret=<32-byte-secret>
      - --email-domain=*
      - --upstream=http://mqtt-inspector:3030
      - --http-address=0.0.0.0:4180
      - --pass-access-token=true   # forwards X-Forwarded-Access-Token
      - --cookie-refresh=4m        # shorter than the access token lifetime (Keycloak default 5m)
    ports:
      - 4180:4180
  mqtt-inspector:
    image: ghcr.io/klawr/mqtt-inspector:latest
    environment:
      MQTT_INSPECTOR_OIDC_ISSUER: https://auth.example.com/realms/acme
      MQTT_INSPECTOR_OIDC_USER_ROLE: mqtt-viewer
      MQTT_INSPECTOR_OIDC_ADMIN_ROLE: mqtt-admin
    volumes:
      - ./config:/srv/config
```

Use `--pass-access-token` rather than `--pass-authorization-header`: the latter forwards the ID
token, which in Keycloak does not contain roles by default. Without `--cookie-refresh` the proxy
keeps forwarding the expired token and the UI cannot reconnect until the page is reloaded.

### TLS with client certificates (mutual TLS)

To connect to brokers that authenticate clients by certificate, mount your PEM files
(e.g. into the config directory) and reference them in `brokers.json`:

```json
[
  {
    "host": "broker.example.com:8883",
    "ca_cert": "certs/ca.pem",
    "client_cert": "certs/client.crt",
    "client_key": "certs/client.key"
  }
]
```

| Field | Description |
|-------|-------------|
| `ca_cert` | Optional. CA certificate(s) used to verify the broker. Replaces the system trust store for this broker — use it for private/self-signed CAs. |
| `client_cert` | Client certificate (chain) presented to the broker. |
| `client_key` | Private key for `client_cert` (PKCS#8, PKCS#1/RSA or SEC1/EC). Must be set together with `client_cert`. |
| `client_key_password` | Optional. Only needed if `client_key` is encrypted (`BEGIN ENCRYPTED PRIVATE KEY`, e.g. AES/3DES with PBKDF2 or scrypt). Ignored for unencrypted keys. |

- Relative paths are resolved against the config directory (`/srv/config`); absolute paths are used as-is.
- Setting any of these fields enables TLS, so `use_tls` can be omitted.
- `client_key_password` is stored in plain text in `brokers.json`, like broker passwords. Legacy
  OpenSSL-encrypted keys (`Proc-Type: 4,ENCRYPTED`) are not supported; convert them with
  `openssl pkcs8 -topk8 -v2 aes-256-cbc -in old.key -out client.key`.
- The same fields are available in the "Add MQTT Broker" dialog when TLS is enabled.
- If a file is missing or invalid, the error is logged and the backend keeps retrying, so fixing the
  mounted file is picked up without a restart.

```bash
docker run \
  -p 3030:3030 \
  -v ./config:/srv/config \
  -v ./certs:/srv/config/certs:ro \
  ghcr.io/klawr/mqtt-inspector:latest
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| MQTT_INSPECTOR_MAX_BROKER_MB | 128 | Max stored message data per broker (MB) before old messages are removed. |
| MQTT_INSPECTOR_MAX_MESSAGE_MB | 1 | Max single message size (MB) sent to the UI. |

Example with custom limits:

```bash
docker run \
  -p 3030:3030 \
  -v ./config:/srv/config \
  -e MQTT_INSPECTOR_MAX_BROKER_MB=256 \
  -e MQTT_INSPECTOR_MAX_MESSAGE_MB=2 \
  ghcr.io/klawr/mqtt-inspector:latest
```

## Testing

System and stress test details:

[test/system/README.md](test/system/README.md)

## License

MIT License. See [LICENSE](LICENSE).