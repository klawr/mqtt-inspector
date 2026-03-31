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