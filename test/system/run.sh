#!/usr/bin/env bash
# Convenience wrapper for the stress test.
# Usage: ./run.sh [--duration 60] [--publishers 5] [--rate 10] …
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV="$SCRIPT_DIR/.venv"

if [ ! -f "$VENV/bin/python" ] || ! "$VENV/bin/python" -c "import sys" 2>/dev/null; then
    echo "[setup] Creating virtual environment …"
    rm -rf "$VENV"
    python3 -m venv "$VENV"
fi

"$VENV/bin/python" -m pip install --quiet paho-mqtt websocket-client psutil

# Auto-detect MQTT broker: prefer Docker (which can start its own), but if
# Docker is unavailable fall back to the compose sidecar broker.
MQTT_ARGS=()
if ! command -v docker &>/dev/null; then
    # Find the compose sidecar broker: try the service hostname first, then localhost
    MQTT_HOST=""
    for host in mqtt localhost 127.0.0.1; do
        if "$VENV/bin/python" -c \
            "import socket, sys; s=socket.create_connection(('$host',1883),2); s.close()" \
            2>/dev/null; then
            MQTT_HOST="$host"
            break
        fi
    done
    if [ -n "$MQTT_HOST" ]; then
        echo "[setup] Docker not available — using existing broker at $MQTT_HOST:1883"
        MQTT_ARGS+=(--mqtt-host "$MQTT_HOST" --mqtt-port 1883)
    else
        echo "[error] Docker not available and no MQTT broker found at mqtt/localhost:1883" >&2
        exit 1
    fi
fi

exec "$VENV/bin/python" "$SCRIPT_DIR/stress_test.py" "${MQTT_ARGS[@]}" "$@"
