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

//! Test helper: generates a throwaway PKI (CA, server and client certificates)
//! with the `openssl` CLI so no private keys need to be committed.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct TestPki {
    pub dir: PathBuf,
}

impl TestPki {
    pub fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for TestPki {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

pub const KEY_PASSWORD: &str = "s3cret-key-pw";

pub fn openssl_available() -> bool {
    Command::new("openssl")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn openssl(dir: &Path, args: &[&str]) {
    let status = Command::new("openssl")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("failed to run openssl");
    assert!(status.success(), "openssl {args:?} failed");
}

/// Creates in a fresh temp dir:
/// - `ca.crt` / `ca.key`: self-signed CA
/// - `server.crt` / `server.key`: signed by the CA, SAN 127.0.0.1 + localhost
/// - `client.crt` / `client.key`: signed by the CA, SEC1 EC key
/// - `client_rsa.crt` / `client_rsa.key`: signed by the CA, PKCS#8 RSA key
/// - `other_ca.crt` / `other.key`: an unrelated CA, for mismatch tests
/// - `client_rsa_aes.key`, `client_rsa_3des.key`, `client_ec_scrypt.key`:
///   encrypted PKCS#8 versions of the client keys (password `KEY_PASSWORD`)
/// - `client_rsa_legacy.key`: legacy OpenSSL-encrypted ("Proc-Type") RSA key
pub fn generate() -> TestPki {
    let dir = std::env::temp_dir().join(format!("mqtt_inspector_pki_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let pki = TestPki { dir };
    let d = pki.dir.as_path();

    std::fs::write(
        d.join("server.ext"),
        "subjectAltName=IP:127.0.0.1,DNS:localhost\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    std::fs::write(d.join("client.ext"), "extendedKeyUsage=clientAuth\n").unwrap();

    #[rustfmt::skip]
    let steps: &[&[&str]] = &[
        &["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "ca.key", "-out", "ca.crt", "-days", "2", "-subj", "/CN=mqtt-inspector-test-ca"],
        &["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", "other.key", "-out", "other_ca.crt", "-days", "2", "-subj", "/CN=other-ca"],
        // server
        &["req", "-newkey", "rsa:2048", "-nodes", "-keyout", "server.key", "-out", "server.csr", "-subj", "/CN=localhost"],
        &["x509", "-req", "-in", "server.csr", "-CA", "ca.crt", "-CAkey", "ca.key", "-CAcreateserial", "-out", "server.crt", "-days", "2", "-extfile", "server.ext"],
        // client with SEC1 EC key
        &["ecparam", "-genkey", "-name", "prime256v1", "-noout", "-out", "client.key"],
        &["req", "-new", "-key", "client.key", "-out", "client.csr", "-subj", "/CN=mqtt-inspector-client"],
        &["x509", "-req", "-in", "client.csr", "-CA", "ca.crt", "-CAkey", "ca.key", "-CAcreateserial", "-out", "client.crt", "-days", "2", "-extfile", "client.ext"],
        // client with PKCS#8 RSA key
        &["req", "-newkey", "rsa:2048", "-nodes", "-keyout", "client_rsa.key", "-out", "client_rsa.csr", "-subj", "/CN=mqtt-inspector-client-rsa"],
        &["x509", "-req", "-in", "client_rsa.csr", "-CA", "ca.crt", "-CAkey", "ca.key", "-CAcreateserial", "-out", "client_rsa.crt", "-days", "2", "-extfile", "client.ext"],
    ];
    for step in steps {
        openssl(d, step);
    }

    let pass = format!("pass:{KEY_PASSWORD}");
    #[rustfmt::skip]
    let encrypt_steps: Vec<Vec<&str>> = vec![
        vec!["pkcs8", "-topk8", "-in", "client_rsa.key", "-out", "client_rsa_aes.key", "-v2", "aes-256-cbc", "-v2prf", "hmacWithSHA256", "-passout", &pass],
        vec!["pkcs8", "-topk8", "-in", "client_rsa.key", "-out", "client_rsa_3des.key", "-v2", "des3", "-passout", &pass],
        vec!["pkcs8", "-topk8", "-in", "client.key", "-out", "client_ec_scrypt.key", "-scrypt", "-passout", &pass],
        vec!["rsa", "-in", "client_rsa.key", "-out", "client_rsa_legacy.key", "-aes256", "-traditional", "-passout", &pass],
    ];
    for step in &encrypt_steps {
        openssl(d, step);
    }
    make_world_readable(d);
    pki
}

/// OpenSSL writes private keys as 0600. When the tests run as root (as in the
/// CI container), mosquitto drops privileges to its own user before loading
/// the certificates and could not read them. These are throwaway test keys.
#[cfg(unix)]
fn make_world_readable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
}

#[cfg(not(unix))]
fn make_world_readable(_dir: &Path) {}
