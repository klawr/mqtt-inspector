# System / Stress Tests

These tests start a real MQTT broker (via Docker), the mqtt-inspector backend,
and multiple simulated clients. They then generate heavy MQTT traffic and verify
that:

* The backend stays alive and responsive.
* Memory usage remains bounded overall; RSS is treated as a sanity bound, not an exact proxy for stored payload bytes.
* WebSocket connections remain stable under load.
* No deadlocks occur (the application keeps processing).

## Prerequisites

| Tool | Purpose |
|------|---------|
| **Docker** | Runs an ephemeral Mosquitto MQTT broker |
| **Cargo** | Builds and runs the backend |
| **Python 3.10+** | Runs the test harness |

Python dependencies (`paho-mqtt`, `websocket-client`, `psutil`) are installed
automatically in a local virtual environment the first time you run the test.

## Quick Start

```bash
cd test/system
./run.sh                         # default: 60 s, 5 publishers, 10 msg/s each
./run.sh --duration 300          # 5 minutes
./run.sh --publishers 20 --rate 50  # heavier load
./run.sh --topics 1 --rate 1000     # few topics, many msgs per topic
./run.sh --help                  # see all options
```

## What the test does

1. **Starts Mosquitto** in a Docker container on a random free port.
2. **Builds & starts the mqtt-inspector backend** with a small byte cap so
   eviction kicks in quickly.
3. **Opens WebSocket clients** in three roles:
   - **Observer** — receives only meta batches (no `select_topic`), verifies no binary frames leak.
   - **Subscriber** — sends `select_topic` once, verifies on-demand binary payload delivery.
   - **Topic-switcher** — rotates `select_topic` every 5 s, exercises the clear→resync cycle.
4. **Spawns publisher threads** that publish MQTT messages at the configured rate.
5. **Monitors** backend process memory (RSS) and WebSocket liveness every second.
   The RSS check allows a small grace margin and only fails if the process stays above the adjusted limit for several consecutive samples.
6. After the configured duration, **stops everything** and prints a summary with
   pass/fail verdict.

## Configuration (Environment / CLI)

| Flag | Env Var | Default | Description |
|------|---------|---------|-------------|
| `--duration` | `STRESS_DURATION` | 60 | Test duration in seconds |
| `--publishers` | `STRESS_PUBLISHERS` | 5 | Number of MQTT publisher threads |
| `--rate` | `STRESS_RATE` | 10 | Messages per second per publisher |
| `--msg-size` | `STRESS_MSG_SIZE` | 1024 | Payload size in bytes |
| `--ws-clients` | `STRESS_WS_CLIENTS` | 3 | Number of WebSocket clients |
| `--max-broker-mb` | `STRESS_MAX_BROKER_MB` | 128 | Backend payload cap per broker (MB) |
| `--topics` | `STRESS_TOPICS` | 50 | Subtopics per publisher (total topics = publishers × topics) |
| `--backend-rss-limit` | `STRESS_RSS_LIMIT` | 512 | Nominal backend RSS sanity bound (MB) |
| `--backend-rss-grace-percent` | `STRESS_RSS_GRACE_PERCENT` | 25 | Allowed RSS overshoot above the nominal limit |
| `--backend-rss-sustain-seconds` | `STRESS_RSS_SUSTAIN_SECONDS` | 5 | Consecutive seconds above the adjusted limit before failing |

## Interpreting the memory check

The backend enforces payload eviction using `MQTT_INSPECTOR_MAX_BROKER_MB`, but
the stress test samples whole-process RSS. Those two numbers are related, not
identical:

* RSS includes allocator overhead, fragmentation, thread stacks, buffers, and other process memory.
* Rust allocations are not guaranteed to return pages to the OS immediately after eviction.
* A brief RSS overshoot does not necessarily indicate unbounded growth or broken eviction.

For that reason, the harness now treats RSS as a **boundedness** check: it fails
only when RSS stays above the nominal limit plus grace for several consecutive
seconds.
