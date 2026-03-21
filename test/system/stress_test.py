#!/usr/bin/env python3
"""
System / stress test for mqtt-inspector.

Starts a Mosquitto broker (Docker), the backend binary, WebSocket clients,
and MQTT publishers, then monitors stability for a configurable duration.
"""

import argparse
import json
import os
import random
import signal
import socket
import subprocess
import sys
import threading
import time
from pathlib import Path

# ---------------------------------------------------------------------------
# Dependency bootstrap – install into a local venv if missing
# ---------------------------------------------------------------------------

VENV_DIR = Path(__file__).resolve().parent / ".venv"
REQUIRED = ["paho-mqtt", "websocket-client", "psutil"]


def _ensure_venv():
    """Create venv & install deps if needed."""
    python = VENV_DIR / "bin" / "python"
    if python.exists():
        return str(python)

    print("[setup] Creating virtual environment …")
    subprocess.check_call([sys.executable, "-m", "venv", str(VENV_DIR)])
    pip = str(VENV_DIR / "bin" / "pip")
    subprocess.check_call([pip, "install", "--quiet"] + REQUIRED)
    return str(python)


def _in_venv() -> bool:
    """Check if we're running inside the project venv."""
    return hasattr(sys, "real_prefix") or (
        hasattr(sys, "base_prefix") and sys.base_prefix != sys.prefix
    )


def _reexec_in_venv():
    """Re-execute this script inside the venv if we aren't already."""
    if _in_venv():
        return
    python = _ensure_venv()
    raise SystemExit(subprocess.call([python] + sys.argv))


try:
    _reexec_in_venv()
except SystemExit:
    raise

# These imports only succeed inside the venv
import paho.mqtt.client as paho_mqtt  # noqa: E402
import psutil  # noqa: E402
import websocket  # noqa: E402

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
BACKEND_DIR = REPO_ROOT / "backend"

_stop_event = threading.Event()


def find_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_for_port(host: str, port: int, timeout: float = 30.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1):
                return
        except OSError:
            time.sleep(0.2)
    raise TimeoutError(f"{host}:{port} not reachable within {timeout}s")


# ---------------------------------------------------------------------------
# Mosquitto broker (Docker)
# ---------------------------------------------------------------------------


def start_mosquitto(mqtt_port: int) -> subprocess.Popen:
    """Start Mosquitto in a Docker container, return the Popen handle."""
    name = f"mqtt-inspector-stress-{mqtt_port}"
    # Kill leftover container with the same name, ignore errors
    subprocess.run(
        ["docker", "rm", "-f", name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    # Mosquitto 2.x requires explicit config to allow anonymous connections
    config_dir = Path(__file__).resolve().parent / "_config"
    config_dir.mkdir(parents=True, exist_ok=True)
    mosquitto_conf = config_dir / "mosquitto.conf"
    mosquitto_conf.write_text(
        "listener 1883 0.0.0.0\nallow_anonymous true\n"
    )

    proc = subprocess.Popen(
        [
            "docker",
            "run",
            "--rm",
            "--name",
            name,
            "-p",
            f"127.0.0.1:{mqtt_port}:1883",
            "-v",
            f"{mosquitto_conf}:/mosquitto/config/mosquitto.conf:ro",
            "eclipse-mosquitto:2",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    wait_for_port("127.0.0.1", mqtt_port)
    print(f"[broker] Mosquitto running on 127.0.0.1:{mqtt_port}  (container {name})")
    return proc


def stop_mosquitto(proc: subprocess.Popen):
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()


# ---------------------------------------------------------------------------
# Backend
# ---------------------------------------------------------------------------


def build_backend():
    print("[backend] Building … (cargo build)")
    subprocess.check_call(
        ["cargo", "build"],
        cwd=str(BACKEND_DIR),
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    print("[backend] Build succeeded")


def _find_backend_binary() -> Path:
    """Return the path to the backend binary, preferring the freshly built debug binary."""
    release = BACKEND_DIR / "target" / "release" / "backend"
    debug = BACKEND_DIR / "target" / "debug" / "backend"
    if debug.exists():
        return debug
    if release.exists():
        return release
    raise FileNotFoundError(
        "No backend binary found. Run 'cargo build' in the backend directory first."
    )


def start_backend(
    mqtt_host: str, mqtt_port: int, max_broker_mb: int, http_port: int
) -> subprocess.Popen:
    config_dir = REPO_ROOT / "test" / "system" / "_config"
    config_dir.mkdir(parents=True, exist_ok=True)

    brokers_file = config_dir / "brokers.json"
    brokers_file.write_text(json.dumps([f"{mqtt_host}:{mqtt_port}"]))

    binary = _find_backend_binary()
    wwwroot = REPO_ROOT / "wwwroot"

    env = os.environ.copy()
    env["MQTT_INSPECTOR_MAX_BROKER_MB"] = str(max_broker_mb)
    env["MQTT_INSPECTOR_MAX_MESSAGE_MB"] = "1"

    proc = subprocess.Popen(
        [str(binary), str(wwwroot), str(config_dir)],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    wait_for_port("127.0.0.1", http_port)
    print(f"[backend] Running on 127.0.0.1:{http_port}  (PID {proc.pid})")
    return proc


# ---------------------------------------------------------------------------
# MQTT publishers
# ---------------------------------------------------------------------------


def publisher_thread(
    thread_id: int,
    host: str,
    port: int,
    rate: float,
    msg_size: int,
    counter: dict,
    errors: list,
):
    """Publish messages at *rate* msg/s until _stop_event is set."""
    try:
        client = paho_mqtt.Client(
            paho_mqtt.CallbackAPIVersion.VERSION2,
            client_id=f"stress-pub-{thread_id}",
        )
        client.connect(host, port, keepalive=60)
        client.loop_start()
    except Exception as exc:
        errors.append(f"pub-{thread_id}: connect failed: {exc}")
        return

    interval = 1.0 / rate if rate > 0 else 1.0
    payload = os.urandom(msg_size)
    topic_base = f"stress/pub{thread_id}"
    seq = 0

    try:
        while not _stop_event.is_set():
            topic = f"{topic_base}/{seq % 50}"  # rotate across 50 subtopics
            client.publish(topic, payload, qos=0)
            seq += 1
            counter["sent"] += 1
            # Randomise delay: mostly short bursts, occasionally long pauses
            delay = random.expovariate(rate) if rate > 0 else 1.0
            delay = max(0.01, min(delay, 60.0))
            _stop_event.wait(delay)
    finally:
        client.loop_stop()
        client.disconnect()


# ---------------------------------------------------------------------------
# WebSocket clients
# ---------------------------------------------------------------------------


def ws_client_thread(client_id: int, url: str, counter: dict, errors: list):
    """Connect to the backend WebSocket and count received messages."""
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-{client_id}: connect failed: {exc}")
        return

    try:
        while not _stop_event.is_set():
            try:
                data = ws.recv()
                if data:
                    if isinstance(data, bytes):
                        counter["ws_recv"] += 1
            except websocket.WebSocketTimeoutException:
                continue
            except websocket.WebSocketConnectionClosedException:
                errors.append(f"ws-{client_id}: connection closed unexpectedly")
                break
            except Exception as exc:
                errors.append(f"ws-{client_id}: {exc}")
                break
    finally:
        try:
            ws.close()
        except Exception:
            pass


# ---------------------------------------------------------------------------
# Monitor
# ---------------------------------------------------------------------------


def monitor_thread(pid: int, samples: list, rss_limit_mb: int, errors: list):
    """Sample backend RSS every second."""
    try:
        proc = psutil.Process(pid)
    except psutil.NoSuchProcess:
        errors.append("Backend process vanished before monitoring started")
        return

    while not _stop_event.is_set():
        try:
            mem = proc.memory_info()
            rss_mb = mem.rss / (1024 * 1024)
            samples.append(rss_mb)
            if rss_mb > rss_limit_mb:
                errors.append(
                    f"RSS {rss_mb:.1f} MB exceeded limit {rss_limit_mb} MB"
                )
        except psutil.NoSuchProcess:
            errors.append("Backend process died during test")
            break
        _stop_event.wait(1.0)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def parse_args():
    p = argparse.ArgumentParser(description="mqtt-inspector system/stress test")
    p.add_argument("--duration", type=int, default=int(os.environ.get("STRESS_DURATION", "60")))
    p.add_argument("--publishers", type=int, default=int(os.environ.get("STRESS_PUBLISHERS", "5")))
    p.add_argument("--rate", type=float, default=float(os.environ.get("STRESS_RATE", "10")))
    p.add_argument("--msg-size", type=int, default=int(os.environ.get("STRESS_MSG_SIZE", "1024")))
    p.add_argument("--ws-clients", type=int, default=int(os.environ.get("STRESS_WS_CLIENTS", "3")))
    p.add_argument("--max-broker-mb", type=int, default=int(os.environ.get("STRESS_MAX_BROKER_MB", "128")))
    p.add_argument("--backend-rss-limit", type=int, default=int(os.environ.get("STRESS_RSS_LIMIT", "512")))
    p.add_argument("--skip-build", action="store_true", help="Skip cargo build")
    p.add_argument("--mqtt-host", default=None,
                   help="Use an existing MQTT broker instead of starting one via Docker")
    p.add_argument("--mqtt-port", type=int, default=1883,
                   help="Port of the external MQTT broker (default: 1883)")
    return p.parse_args()


def main():
    args = parse_args()

    mqtt_port = find_free_port()
    http_port = 3030  # backend currently hardcodes this

    print("=" * 60)
    print("  mqtt-inspector  SYSTEM / STRESS TEST")
    print("=" * 60)
    print(f"  Duration        : {args.duration}s")
    print(f"  Publishers      : {args.publishers}  @ {args.rate} msg/s each")
    print(f"  Message size    : {args.msg_size} bytes")
    print(f"  WS clients      : {args.ws_clients}")
    print(f"  Max broker MB   : {args.max_broker_mb}")
    print(f"  RSS limit       : {args.backend_rss_limit} MB")
    print("=" * 60)

    # ---- Start infrastructure ----
    broker_proc = None
    backend_proc = None
    threads = []
    counter = {
        "sent": 0,
        "ws_recv": 0,
    }
    rss_samples: list[float] = []
    errors: list[str] = []

    try:
        if args.mqtt_host:
            mqtt_host = args.mqtt_host
            mqtt_port = args.mqtt_port
            print(f"[broker] Using external MQTT broker at {mqtt_host}:{mqtt_port}")
        else:
            broker_proc = start_mosquitto(mqtt_port)
            mqtt_host = "127.0.0.1"

        if not args.skip_build:
            build_backend()

        backend_proc = start_backend(mqtt_host, mqtt_port, args.max_broker_mb, http_port)

        # ---- Start WebSocket clients ----
        ws_url = f"ws://127.0.0.1:{http_port}/ws"
        for i in range(args.ws_clients):
            t = threading.Thread(
                target=ws_client_thread,
                args=(i, ws_url, counter, errors),
                daemon=True,
            )
            t.start()
            threads.append(t)

        # ---- Start monitor ----
        mon = threading.Thread(
            target=monitor_thread,
            args=(backend_proc.pid, rss_samples, args.backend_rss_limit, errors),
            daemon=True,
        )
        mon.start()
        threads.append(mon)

        # ---- Start publishers ----
        for i in range(args.publishers):
            t = threading.Thread(
                target=publisher_thread,
                args=(i, mqtt_host, mqtt_port, args.rate, args.msg_size, counter, errors),
                daemon=True,
            )
            t.start()
            threads.append(t)

        # ---- Wait ----
        start = time.monotonic()
        while time.monotonic() - start < args.duration:
            elapsed = time.monotonic() - start
            rss_now = rss_samples[-1] if rss_samples else 0
            print(
                f"\r  [{elapsed:6.0f}s / {args.duration}s]  "
                f"sent={counter['sent']:>8,}  "
                f"ws_recv={counter['ws_recv']:>8,}  "
                f"RSS={rss_now:6.1f} MB  "
                f"errors={len(errors)}",
                end="",
                flush=True,
            )
            time.sleep(1)

        print()  # newline after progress

    except KeyboardInterrupt:
        print("\n[!] Interrupted by user")
    finally:
        # ---- Teardown ----
        _stop_event.set()

        # Check backend is still alive before we tear down
        backend_alive = backend_proc is not None and backend_proc.poll() is None
        if not backend_alive and backend_proc is not None:
            errors.append(
                f"Backend exited prematurely with code {backend_proc.returncode}"
            )

        # Liveness check: can we still open a WebSocket?
        if backend_alive:
            try:
                ws = websocket.WebSocket()
                ws.settimeout(5)
                ws.connect(f"ws://127.0.0.1:{http_port}/ws")
                ws.close()
            except Exception as exc:
                errors.append(f"Post-test liveness check failed: {exc}")

        for t in threads:
            t.join(timeout=5)

        if backend_proc:
            backend_proc.terminate()
            try:
                backend_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                errors.append("Backend did not stop gracefully — killing")
                backend_proc.kill()

        if broker_proc:
            stop_mosquitto(broker_proc)

    # ---- Throughput sanity check ----
    # Each WS client should receive roughly as many messages as were sent
    # (minus the startup window).  If ws_recv is far below expectations the
    # backend likely stalled.  We use a deliberately generous threshold: at
    # least 10% of sent messages should have been forwarded to WS clients.
    sent = counter["sent"]
    ws_recv = counter["ws_recv"]
    if sent > 0 and args.ws_clients > 0:
        recv_per_client = ws_recv / args.ws_clients
        ratio = recv_per_client / sent
        if ratio < 0.10:
            errors.append(
                f"WS throughput too low: each client received ~{recv_per_client:,.0f} "
                f"of {sent:,} sent ({ratio:.1%}).  Backend may have stalled."
            )

    # ---- Report ----
    print()
    print("=" * 60)
    print("  RESULTS")
    print("=" * 60)
    print(f"  Messages sent        : {sent:,}")
    print(f"  WS messages received : {ws_recv:,}")
    if rss_samples:
        print(f"  RSS (min / avg / max): {min(rss_samples):.1f} / "
              f"{sum(rss_samples)/len(rss_samples):.1f} / {max(rss_samples):.1f} MB")
    print(f"  Errors               : {len(errors)}")
    for err in errors:
        print(f"    - {err}")
    print("=" * 60)

    if errors:
        print("  VERDICT: FAIL")
        print("=" * 60)
        sys.exit(1)
    else:
        print("  VERDICT: PASS")
        print("=" * 60)
        sys.exit(0)


if __name__ == "__main__":
    main()
