#!/usr/bin/env python3
"""
System / stress test for mqtt-inspector.

Starts a Mosquitto broker (Docker), the backend binary, WebSocket clients,
and MQTT publishers, then monitors stability for a configurable duration.

Architecture-aware: tests the meta-only + on-demand loading protocol:
  - All WS peers receive text JSON batches (mqtt_message_meta_batch,
    messages_evicted_batch) for every incoming MQTT message.
  - Only peers that have sent a select_topic request receive binary
    mqtt_message frames for the selected topic.
  - The test verifies both paths: observer clients (meta only) and
    subscriber clients (meta + binary payloads via select_topic).
"""

import argparse
import json
import os
import random
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
    num_topics: int,
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

    topic_base = f"stress/pub{thread_id}"
    seq = 0

    try:
        while not _stop_event.is_set():
            topic = f"{topic_base}/{seq % num_topics}"
            # Randomize size around msg_size (±50%)
            size_variation = int(msg_size * 0.5)
            this_size = msg_size + random.randint(-size_variation, size_variation)
            this_size = max(len(str(seq)) + 1, this_size)  # ensure room for seq
            seq_bytes = str(seq).encode("utf-8")
            rand_len = max(0, this_size - len(seq_bytes) - 1)
            payload = seq_bytes + b":" + os.urandom(rand_len)
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


def _parse_text_message(data: str, counter: dict):
    """Parse a text JSON-RPC notification and update counters."""
    try:
        msg = json.loads(data)
    except json.JSONDecodeError:
        return

    method = msg.get("method", "")
    params = msg.get("params", {})

    if method == "mqtt_message_meta_batch":
        if isinstance(params, list):
            counter["meta_entries"] += len(params)
        counter["meta_batches"] += 1
    elif method == "messages_evicted_batch":
        if isinstance(params, list):
            counter["eviction_entries"] += len(params)
        counter["eviction_batches"] += 1
    elif method == "topic_messages_clear":
        counter["topic_clears"] += 1
    elif method == "topic_sync_complete":
        counter["topic_syncs"] += 1
    elif method == "sync_complete":
        counter["initial_syncs"] += 1


def ws_observer_thread(client_id: int, url: str, counter: dict, errors: list):
    """
    Observer client: connects but never sends select_topic.
    Should receive meta batches + eviction batches (text) but NO binary frames.
    """
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-observer-{client_id}: connect failed: {exc}")
        return

    try:
        while not _stop_event.is_set():
            try:
                opcode, data = ws.recv_data()
                if not data:
                    continue
                if opcode == websocket.ABNF.OPCODE_TEXT:
                    _parse_text_message(data.decode("utf-8", errors="replace"), counter)
                elif opcode == websocket.ABNF.OPCODE_BINARY:
                    # Observers should NOT receive binary frames (no topic selected)
                    counter["observer_unexpected_binary"] += 1
            except websocket.WebSocketTimeoutException:
                continue
            except websocket.WebSocketConnectionClosedException:
                errors.append(f"ws-observer-{client_id}: connection closed unexpectedly")
                break
            except Exception as exc:
                errors.append(f"ws-observer-{client_id}: {exc}")
                break
    finally:
        try:
            ws.close()
        except Exception:
            pass


def ws_subscriber_thread(
    client_id: int,
    url: str,
    mqtt_broker: str,
    counter: dict,
    errors: list,
):
    """
    Subscriber client: waits for initial sync, then sends select_topic to
    subscribe to a topic. Should receive meta batches (text) AND binary
    mqtt_message frames for the selected topic.
    """
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-subscriber-{client_id}: connect failed: {exc}")
        return

    got_initial_sync = threading.Event()
    selected = False
    # Pick a topic to subscribe to — use publisher 0's first subtopic
    subscribe_topic = "stress/pub0/0"

    try:
        while not _stop_event.is_set():
            try:
                opcode, data = ws.recv_data()
                if not data:
                    continue

                if opcode == websocket.ABNF.OPCODE_TEXT:
                    text = data.decode("utf-8", errors="replace")
                    _parse_text_message(text, counter)

                    # After initial sync, send select_topic
                    if not selected:
                        try:
                            msg = json.loads(text)
                        except json.JSONDecodeError:
                            continue
                        if msg.get("method") == "sync_complete":
                            got_initial_sync.set()
                        # Wait until we've seen at least one meta batch
                        # (meaning there's data to subscribe to)
                        if (
                            got_initial_sync.is_set()
                            and counter["meta_entries"] > 0
                        ):
                            select_msg = json.dumps(
                                {
                                    "jsonrpc": "2.0",
                                    "method": "select_topic",
                                    "params": {
                                        "broker": mqtt_broker,
                                        "topic": subscribe_topic,
                                    },
                                }
                            )
                            ws.send(select_msg)
                            selected = True
                            counter["select_topic_sent"] += 1
                            print(
                                f"\n  [ws-subscriber-{client_id}] "
                                f"select_topic → {mqtt_broker} / {subscribe_topic}"
                            )

                elif opcode == websocket.ABNF.OPCODE_BINARY:
                    counter["binary_frames"] += 1

            except websocket.WebSocketTimeoutException:
                continue
            except websocket.WebSocketConnectionClosedException:
                errors.append(
                    f"ws-subscriber-{client_id}: connection closed unexpectedly"
                )
                break
            except Exception as exc:
                errors.append(f"ws-subscriber-{client_id}: {exc}")
                break
    finally:
        try:
            ws.close()
        except Exception:
            pass


def ws_topic_switch_thread(
    client_id: int,
    url: str,
    mqtt_broker: str,
    num_publishers: int,
    num_topics: int,
    counter: dict,
    errors: list,
):
    """
    Topic-switching client: periodically changes the selected topic to exercise
    the select_topic → clear → resync → stream cycle.
    """
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-switcher-{client_id}: connect failed: {exc}")
        return

    got_initial_sync = threading.Event()
    last_switch = 0.0
    switch_interval = 5.0  # switch topic every 5 seconds
    topic_index = 0

    try:
        while not _stop_event.is_set():
            try:
                opcode, data = ws.recv_data()
                if not data:
                    continue

                if opcode == websocket.ABNF.OPCODE_TEXT:
                    text = data.decode("utf-8", errors="replace")
                    _parse_text_message(text, counter)

                    try:
                        msg = json.loads(text)
                    except json.JSONDecodeError:
                        continue

                    if msg.get("method") == "sync_complete":
                        got_initial_sync.set()

                    # Periodically switch topics
                    now = time.monotonic()
                    if (
                        got_initial_sync.is_set()
                        and counter["meta_entries"] > 0
                        and now - last_switch >= switch_interval
                    ):
                        # Rotate across different publishers' subtopics
                        topic = f"stress/pub{topic_index % num_publishers}/{topic_index % num_topics}"
                        select_msg = json.dumps(
                            {
                                "jsonrpc": "2.0",
                                "method": "select_topic",
                                "params": {
                                    "broker": mqtt_broker,
                                    "topic": topic,
                                },
                            }
                        )
                        ws.send(select_msg)
                        counter["select_topic_sent"] += 1
                        counter["topic_switches"] += 1
                        topic_index += 1
                        last_switch = now

                elif opcode == websocket.ABNF.OPCODE_BINARY:
                    counter["binary_frames"] += 1

            except websocket.WebSocketTimeoutException:
                continue
            except websocket.WebSocketConnectionClosedException:
                errors.append(
                    f"ws-switcher-{client_id}: connection closed unexpectedly"
                )
                break
            except Exception as exc:
                errors.append(f"ws-switcher-{client_id}: {exc}")
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
    p.add_argument("--topics", type=int, default=int(os.environ.get("STRESS_TOPICS", "50")),
                   help="Number of subtopics per publisher (default: 50)")
    return p.parse_args()


def make_counter() -> dict:
    """Create a fresh counter dict with all expected keys."""
    return {
        # Publishers
        "sent": 0,
        # Meta / eviction batches (all clients see these)
        "meta_batches": 0,
        "meta_entries": 0,
        "eviction_batches": 0,
        "eviction_entries": 0,
        # Binary frames (only subscriber/switcher clients)
        "binary_frames": 0,
        # Observer got unexpected binary (should stay 0)
        "observer_unexpected_binary": 0,
        # select_topic protocol
        "select_topic_sent": 0,
        "topic_clears": 0,
        "topic_syncs": 0,
        "topic_switches": 0,
        # Initial connection
        "initial_syncs": 0,
    }


def main():
    args = parse_args()

    mqtt_port = find_free_port()
    http_port = 3030  # backend currently hardcodes this

    print("=" * 70)
    print("  mqtt-inspector  SYSTEM / STRESS TEST  (meta-only architecture)")
    print("=" * 70)
    print(f"  Duration        : {args.duration}s")
    print(f"  Publishers      : {args.publishers}  @ {args.rate} msg/s each")
    print(f"  Topics/publisher: {args.topics}  (total: {args.publishers * args.topics})")
    print(f"  Message size    : {args.msg_size} bytes")
    print(f"  WS clients      : {args.ws_clients}  (observer / subscriber / switcher)")
    print(f"  Max broker MB   : {args.max_broker_mb}")
    print(f"  RSS limit       : {args.backend_rss_limit} MB")
    print("=" * 70)

    # ---- Start infrastructure ----
    broker_proc = None
    backend_proc = None
    threads = []
    counter = make_counter()
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

        ws_url = f"ws://127.0.0.1:{http_port}/ws"
        mqtt_broker = f"{mqtt_host}:{mqtt_port}"

        # ---- Allocate WS client roles ----
        # At least 1 observer + 1 subscriber + 1 switcher; rest are observers
        n_ws = max(args.ws_clients, 3)
        n_observers = max(1, n_ws - 2)

        # Observers: receive only meta batches (no select_topic)
        for i in range(n_observers):
            t = threading.Thread(
                target=ws_observer_thread,
                args=(i, ws_url, counter, errors),
                daemon=True,
            )
            t.start()
            threads.append(t)
        print(f"[ws] {n_observers} observer client(s) connected")

        # Subscriber: sends select_topic once for a fixed topic
        t = threading.Thread(
            target=ws_subscriber_thread,
            args=(0, ws_url, mqtt_broker, counter, errors),
            daemon=True,
        )
        t.start()
        threads.append(t)
        print("[ws] 1 subscriber client connected")

        # Switcher: periodically rotates select_topic
        t = threading.Thread(
            target=ws_topic_switch_thread,
            args=(0, ws_url, mqtt_broker, args.publishers, args.topics, counter, errors),
            daemon=True,
        )
        t.start()
        threads.append(t)
        print("[ws] 1 topic-switcher client connected")

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
                args=(i, mqtt_host, mqtt_port, args.rate, args.msg_size, args.topics, counter, errors),
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
                f"metas={counter['meta_entries']:>8,}  "
                f"binary={counter['binary_frames']:>6,}  "
                f"evict={counter['eviction_entries']:>6,}  "
                f"RSS={rss_now:6.1f} MB  "
                f"err={len(errors)}",
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

    # ---- Validation checks ----
    sent = counter["sent"]

    # 1. Meta throughput: every WS client receives meta batches with entries
    #    roughly proportional to messages sent.  With batching, we expect
    #    fewer batches but the total entry count should be significant.
    if sent > 0:
        meta_ratio = counter["meta_entries"] / sent
        if meta_ratio < 0.05:
            errors.append(
                f"Meta throughput too low: {counter['meta_entries']:,} meta entries "
                f"for {sent:,} sent ({meta_ratio:.1%}).  Backend may have stalled."
            )

    # 2. Observer clients must NOT receive binary frames (meta-only architecture)
    if counter["observer_unexpected_binary"] > 0:
        errors.append(
            f"Observer clients received {counter['observer_unexpected_binary']} "
            f"unexpected binary frames — meta-only isolation is broken."
        )

    # 3. Subscriber/switcher clients SHOULD receive some binary frames
    #    (they sent select_topic, so should get payloads for selected topic)
    if sent > 100 and counter["select_topic_sent"] > 0 and counter["binary_frames"] == 0:
        errors.append(
            "Subscriber/switcher sent select_topic but received 0 binary frames. "
            "On-demand loading may be broken."
        )

    # 4. select_topic flow: every select_topic should produce a clear + sync
    if counter["select_topic_sent"] > 0:
        if counter["topic_clears"] == 0:
            errors.append(
                f"Sent {counter['select_topic_sent']} select_topic requests "
                f"but received 0 topic_messages_clear responses."
            )
        if counter["topic_syncs"] == 0:
            errors.append(
                f"Sent {counter['select_topic_sent']} select_topic requests "
                f"but received 0 topic_sync_complete responses."
            )

    # 5. Topic-switcher should have switched multiple times
    expected_switches = max(1, (args.duration // 5) - 1)
    if counter["topic_switches"] < expected_switches // 2:
        errors.append(
            f"Topic-switcher only switched {counter['topic_switches']} times "
            f"(expected ~{expected_switches}).  May indicate stalled client."
        )

    # 6. Initial sync: all clients should have received initial sync
    total_ws_clients = max(args.ws_clients, 3)
    if counter["initial_syncs"] < total_ws_clients:
        errors.append(
            f"Only {counter['initial_syncs']} of {total_ws_clients} clients "
            f"received initial sync_complete."
        )

    # 7. Evictions: with a small max_broker_mb, evictions should occur
    #    under sustained load (unless message rate is very low)
    if sent > 1000 and counter["eviction_entries"] == 0:
        # Only a warning, not an error — depends on timing and broker size
        print(
            f"  [note] No evictions observed with {sent:,} messages sent "
            f"and {args.max_broker_mb} MB cap. This may be normal for low volume."
        )

    # ---- Report ----
    print()
    print("=" * 70)
    print("  RESULTS")
    print("=" * 70)
    print(f"  Messages sent           : {sent:,}")
    print(f"  Meta batches / entries  : {counter['meta_batches']:,} / {counter['meta_entries']:,}")
    print(f"  Eviction batches/entries: {counter['eviction_batches']:,} / {counter['eviction_entries']:,}")
    print(f"  Binary frames received  : {counter['binary_frames']:,}")
    print(f"  Observer stray binary   : {counter['observer_unexpected_binary']:,}")
    print(f"  select_topic sent       : {counter['select_topic_sent']:,}")
    print(f"  topic_messages_clear    : {counter['topic_clears']:,}")
    print(f"  topic_sync_complete     : {counter['topic_syncs']:,}")
    print(f"  Topic switches          : {counter['topic_switches']:,}")
    print(f"  Initial syncs           : {counter['initial_syncs']:,}")
    if rss_samples:
        print(
            f"  RSS (min / avg / max)   : {min(rss_samples):.1f} / "
            f"{sum(rss_samples)/len(rss_samples):.1f} / {max(rss_samples):.1f} MB"
        )
    print(f"  Errors                  : {len(errors)}")
    for err in errors:
        print(f"    - {err}")
    print("=" * 70)

    if errors:
        print("  VERDICT: FAIL")
        print("=" * 70)
        sys.exit(1)
    else:
        print("  VERDICT: PASS")
        print("=" * 70)
        sys.exit(0)


if __name__ == "__main__":
    main()
