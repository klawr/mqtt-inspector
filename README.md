# mqtt-inspector

Simple service to connect to mqtt brokers and monitor the messages coming through.
Distributes all topics to all connected clients using websockets.

# Features:
  - Connect to multiple brokers at once
  - Explore all topics coming through on a broker
  - Save pipelines to visualize them effectively
  - Publish commands
  - Save favorite commands for publishing

# Deployment:

Using docker directly
```bash
docker run -p 3030:3030 -v ./config:/srv/config ghcr.io/klawr/mqtt-inspector:latest
```

Using compose:
```yaml
services:
  mqtt-inspector:
    image: ghcr.io/klawr/mqtt-inspector:latest
    container_name: mqtt-inspector
    ports:
      - 3030:3030
    volumes:
      - ./config:/srv/config
```