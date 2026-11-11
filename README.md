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