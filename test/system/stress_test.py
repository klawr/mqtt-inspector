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
import shutil
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
# Mosquitto broker (Docker or native)
# ---------------------------------------------------------------------------

_HAS_DOCKER = shutil.which("docker") is not None


def start_mosquitto(mqtt_port: int) -> subprocess.Popen:
    """Start Mosquitto, preferring Docker but falling back to native."""
    if _HAS_DOCKER:
        return _start_mosquitto_docker(mqtt_port)
    return _start_mosquitto_native(mqtt_port)


def _start_mosquitto_docker(mqtt_port: int) -> subprocess.Popen:
    """Start Mosquitto in a Docker container, return the Popen handle."""
    name = f"mqtt-inspector-stress-{mqtt_port}"
    subprocess.run(
        ["docker", "rm", "-f", name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

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


def _start_mosquitto_native(mqtt_port: int) -> subprocess.Popen:
    """Start Mosquitto as a native process (no Docker)."""
    config_dir = Path(__file__).resolve().parent / "_config"
    config_dir.mkdir(parents=True, exist_ok=True)
    conf_file = config_dir / f"mosquitto_native_{mqtt_port}.conf"
    conf_file.write_text(
        f"listener {mqtt_port} 127.0.0.1\nallow_anonymous true\n"
    )
    proc = subprocess.Popen(
        ["mosquitto", "-c", str(conf_file)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    wait_for_port("127.0.0.1", mqtt_port)
    print(f"[broker] Mosquitto (native) running on 127.0.0.1:{mqtt_port}  (PID {proc.pid})")
    return proc


def start_mosquitto_with_auth(
    mqtt_port: int, username: str, password: str,
) -> subprocess.Popen:
    """Start Mosquitto with password authentication."""
    if _HAS_DOCKER:
        return _start_mosquitto_with_auth_docker(mqtt_port, username, password)
    return _start_mosquitto_with_auth_native(mqtt_port, username, password)


def _start_mosquitto_with_auth_docker(
    mqtt_port: int, username: str, password: str,
) -> subprocess.Popen:
    """Start a Mosquitto container that requires username/password auth."""
    name = f"mqtt-inspector-stress-auth-{mqtt_port}"
    subprocess.run(
        ["docker", "rm", "-f", name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    config_dir = Path(__file__).resolve().parent / "_config"
    config_dir.mkdir(parents=True, exist_ok=True)

    # Write password file (plain-text, Mosquitto will hash it on startup)
    passwd_file = config_dir / f"passwd_{mqtt_port}"
    passwd_file.write_text(f"{username}:{password}\n")

    # Hash the password file using mosquitto_passwd inside a temp container
    subprocess.run(
        [
            "docker", "run", "--rm",
            "-v", f"{passwd_file}:/tmp/passwd",
            "eclipse-mosquitto:2",
            "mosquitto_passwd", "-U", "/tmp/passwd",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=True,
    )

    conf_file = config_dir / f"mosquitto_auth_{mqtt_port}.conf"
    conf_file.write_text(
        "listener 1883 0.0.0.0\n"
        "allow_anonymous false\n"
        "password_file /mosquitto/config/passwd\n"
    )

    proc = subprocess.Popen(
        [
            "docker", "run", "--rm",
            "--name", name,
            "-p", f"127.0.0.1:{mqtt_port}:1883",
            "-v", f"{conf_file}:/mosquitto/config/mosquitto.conf:ro",
            "-v", f"{passwd_file}:/mosquitto/config/passwd:ro",
            "eclipse-mosquitto:2",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    wait_for_port("127.0.0.1", mqtt_port)
    print(
        f"[broker] Mosquitto (auth/docker) running on 127.0.0.1:{mqtt_port}  "
        f"(container {name}, user={username})"
    )
    return proc


def _start_mosquitto_with_auth_native(
    mqtt_port: int, username: str, password: str,
) -> subprocess.Popen:
    """Start native Mosquitto with password authentication."""
    config_dir = Path(__file__).resolve().parent / "_config"
    config_dir.mkdir(parents=True, exist_ok=True)

    passwd_file = config_dir / f"passwd_native_{mqtt_port}"
    # Create the password file with mosquitto_passwd
    # -c creates a new file, -b takes password from command line
    subprocess.check_call(
        ["mosquitto_passwd", "-c", "-b", str(passwd_file), username, password],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    conf_file = config_dir / f"mosquitto_auth_native_{mqtt_port}.conf"
    conf_file.write_text(
        f"listener {mqtt_port} 127.0.0.1\n"
        f"allow_anonymous false\n"
        f"password_file {passwd_file}\n"
    )

    proc = subprocess.Popen(
        ["mosquitto", "-c", str(conf_file)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    wait_for_port("127.0.0.1", mqtt_port)
    print(
        f"[broker] Mosquitto (auth/native) running on 127.0.0.1:{mqtt_port}  "
        f"(PID {proc.pid}, user={username})"
    )
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
    mqtt_host: str, mqtt_port: int, max_broker_mb: int, http_port: int,
    extra_brokers: list[dict] | None = None,
) -> subprocess.Popen:
    config_dir = REPO_ROOT / "test" / "system" / "_config"
    config_dir.mkdir(parents=True, exist_ok=True)

    brokers = [{"host": f"{mqtt_host}:{mqtt_port}"}]
    if extra_brokers:
        brokers.extend(extra_brokers)
    brokers_file = config_dir / "brokers.json"
    brokers_file.write_text(json.dumps(brokers))

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
    username: str | None = None,
    password: str | None = None,
    topic_base: str | None = None,
):
    """Publish messages at *rate* msg/s until _stop_event is set."""
    try:
        client = paho_mqtt.Client(
            paho_mqtt.CallbackAPIVersion.VERSION2,
            client_id=f"stress-pub-{thread_id}",
        )
        if username and password:
            client.username_pw_set(username, password)
        client.connect(host, port, keepalive=60)
        client.loop_start()
    except Exception as exc:
        errors.append(f"pub-{thread_id}: connect failed: {exc}")
        return

    if topic_base is None:
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
            for entry in params:
                source = entry.get("source", "")
                if source:
                    counter.setdefault("meta_by_broker", {})
                    counter["meta_by_broker"][source] = (
                        counter["meta_by_broker"].get(source, 0) + 1
                    )
        counter["meta_batches"] += 1
    elif method == "messages_evicted_batch":
        if isinstance(params, list):
            counter["eviction_entries"] += len(params)
        counter["eviction_batches"] += 1
    elif method == "topic_messages_clear":
        counter["topic_clears"] += 1
    elif method == "topic_sync_complete":
        counter["topic_syncs"] += 1
    elif method == "mqtt_brokers":
        counter["broker_lists"] += 1
        if isinstance(params, list):
            for b in params:
                name = b.get("broker", "")
                needs_auth = b.get("requires_auth", False)
                counter.setdefault("broker_auth_flags", {})[name] = needs_auth
    elif method == "broker_auth_result":
        counter["auth_results"] += 1
        if params.get("success"):
            counter["auth_successes"] += 1
        else:
            counter["auth_failures"] += 1
    elif method == "topic_summaries":
        counter["topic_summary_msgs"] += 1


def ws_observer_thread(client_id: int, url: str, counter: dict, errors: list, label: str = "observer"):
    """
    Observer client: connects but never sends select_topic.
    Should receive meta batches + eviction batches (text) but NO binary frames.
    """
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-{label}-{client_id}: connect failed: {exc}")
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
                errors.append(f"ws-{label}-{client_id}: connection closed unexpectedly")
                break
            except Exception as exc:
                errors.append(f"ws-{label}-{client_id}: {exc}")
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

                    # After broker list + first meta, send select_topic
                    if not selected:
                        if (
                            counter["broker_lists"] > 0
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

                    # Periodically switch topics (wait for broker list + meta)
                    now = time.monotonic()
                    if (
                        counter["broker_lists"] > 0
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
# Auth-mode WS client threads
# ---------------------------------------------------------------------------


def ws_auth_authenticated_observer_thread(
    client_id: int,
    url: str,
    protected_broker: str,
    password: str,
    counter: dict,
    errors: list,
):
    """
    Auth-mode observer that authenticates to the protected broker.
    Should receive meta from BOTH brokers after authentication succeeds.
    """
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-auth-obs-{client_id}: connect failed: {exc}")
        return

    authenticated = False
    try:
        # Send authenticate_broker immediately
        auth_msg = json.dumps({
            "jsonrpc": "2.0",
            "method": "authenticate_broker",
            "params": {"hostname": protected_broker, "password": password},
        })
        ws.send(auth_msg)

        while not _stop_event.is_set():
            try:
                opcode, data = ws.recv_data()
                if not data:
                    continue
                if opcode == websocket.ABNF.OPCODE_TEXT:
                    text = data.decode("utf-8", errors="replace")
                    _parse_text_message(text, counter)
                    if not authenticated:
                        try:
                            msg = json.loads(text)
                        except json.JSONDecodeError:
                            continue
                        if msg.get("method") == "broker_auth_result":
                            p = msg.get("params", {})
                            if p.get("broker") == protected_broker and p.get("success"):
                                authenticated = True
                                print(f"  [ws-auth-obs-{client_id}] authenticated to {protected_broker}")
                elif opcode == websocket.ABNF.OPCODE_BINARY:
                    counter["observer_unexpected_binary"] += 1
            except websocket.WebSocketTimeoutException:
                continue
            except websocket.WebSocketConnectionClosedException:
                errors.append(f"ws-auth-obs-{client_id}: connection closed")
                break
            except Exception as exc:
                errors.append(f"ws-auth-obs-{client_id}: {exc}")
                break
    finally:
        try:
            ws.close()
        except Exception:
            pass


def ws_auth_subscriber_thread(
    client_id: int,
    url: str,
    protected_broker: str,
    password: str,
    subscribe_topic: str,
    counter: dict,
    errors: list,
):
    """
    Auth-mode subscriber: authenticates to the protected broker, then sends
    select_topic for a topic on that broker.  Should receive binary frames.
    """
    try:
        ws = websocket.WebSocket()
        ws.settimeout(5)
        ws.connect(url)
    except Exception as exc:
        errors.append(f"ws-auth-sub-{client_id}: connect failed: {exc}")
        return

    authenticated = False
    selected = False

    try:
        auth_msg = json.dumps({
            "jsonrpc": "2.0",
            "method": "authenticate_broker",
            "params": {"hostname": protected_broker, "password": password},
        })
        ws.send(auth_msg)

        while not _stop_event.is_set():
            try:
                opcode, data = ws.recv_data()
                if not data:
                    continue

                if opcode == websocket.ABNF.OPCODE_TEXT:
                    text = data.decode("utf-8", errors="replace")
                    _parse_text_message(text, counter)

                    if not authenticated:
                        try:
                            msg = json.loads(text)
                        except json.JSONDecodeError:
                            continue
                        if msg.get("method") == "broker_auth_result":
                            p = msg.get("params", {})
                            if p.get("broker") == protected_broker and p.get("success"):
                                authenticated = True
                                print(f"  [ws-auth-sub-{client_id}] authenticated to {protected_broker}")

                    # After auth + first meta, send select_topic
                    if authenticated and not selected and counter["meta_entries"] > 0:
                        select_msg = json.dumps({
                            "jsonrpc": "2.0",
                            "method": "select_topic",
                            "params": {
                                "broker": protected_broker,
                                "topic": subscribe_topic,
                            },
                        })
                        ws.send(select_msg)
                        selected = True
                        counter["select_topic_sent"] += 1
                        print(
                            f"  [ws-auth-sub-{client_id}] select_topic → "
                            f"{protected_broker} / {subscribe_topic}"
                        )

                elif opcode == websocket.ABNF.OPCODE_BINARY:
                    counter["binary_frames"] += 1

            except websocket.WebSocketTimeoutException:
                continue
            except websocket.WebSocketConnectionClosedException:
                errors.append(f"ws-auth-sub-{client_id}: connection closed")
                break
            except Exception as exc:
                errors.append(f"ws-auth-sub-{client_id}: {exc}")
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
    p.add_argument("--mode", choices=["stress", "auth"], default="stress",
                   help="Test mode: 'stress' (default) for load testing, 'auth' for multi-broker auth")
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


def _teardown(
    threads: list,
    backend_proc: subprocess.Popen | None,
    broker_procs: list[subprocess.Popen],
    http_port: int,
    errors: list,
):
    """Common teardown: signal stop, check liveness, join threads, stop processes."""
    _stop_event.set()

    backend_alive = backend_proc is not None and backend_proc.poll() is None
    if not backend_alive and backend_proc is not None:
        errors.append(f"Backend exited prematurely with code {backend_proc.returncode}")

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

    for proc in broker_procs:
        stop_mosquitto(proc)


def make_counter() -> dict:
    """Create a fresh counter dict with all expected keys."""
    return {
        # Publishers
        "sent": 0,
        # Meta / eviction batches (all clients see these)
        "meta_batches": 0,
        "meta_entries": 0,
        "meta_by_broker": {},
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
        # Broker metadata
        "broker_lists": 0,
        "broker_auth_flags": {},
        # Auth protocol
        "auth_results": 0,
        "auth_successes": 0,
        "auth_failures": 0,
        "topic_summary_msgs": 0,
    }


def main_stress(args):
    """Original stress-test mode: single broker, many publishers, WS clients."""
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
        _teardown(
            threads, backend_proc,
            [broker_proc] if broker_proc else [],
            http_port, errors,
        )

    # ---- Validation checks ----
    sent = counter["sent"]

    # 1. Meta throughput
    if sent > 0:
        meta_ratio = counter["meta_entries"] / sent
        if meta_ratio < 0.05:
            errors.append(
                f"Meta throughput too low: {counter['meta_entries']:,} meta entries "
                f"for {sent:,} sent ({meta_ratio:.1%}).  Backend may have stalled."
            )

    # 2. Observer clients must NOT receive binary frames
    if counter["observer_unexpected_binary"] > 0:
        errors.append(
            f"Observer clients received {counter['observer_unexpected_binary']} "
            f"unexpected binary frames — meta-only isolation is broken."
        )

    # 3. Subscriber/switcher should receive binary frames
    if sent > 100 and counter["select_topic_sent"] > 0 and counter["binary_frames"] == 0:
        errors.append(
            "Subscriber/switcher sent select_topic but received 0 binary frames. "
            "On-demand loading may be broken."
        )

    # 4. select_topic flow: clear + sync expected
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

    # 6. Broker list should have been received
    if counter["broker_lists"] == 0:
        errors.append("No mqtt_brokers messages received — broker list never sent.")

    # 7. Evictions note
    if sent > 1000 and counter["eviction_entries"] == 0:
        print(
            f"  [note] No evictions observed with {sent:,} messages sent "
            f"and {args.max_broker_mb} MB cap. This may be normal for low volume."
        )

    # ---- Report ----
    _print_report(counter, rss_samples, errors)
    return len(errors)


def main_auth(args):
    """
    Multi-broker authentication test mode.

    Starts TWO Mosquitto brokers:
      - open_broker: anonymous access (no password)
      - protected_broker: requires username/password

    Connects the backend to both, then runs three types of WS clients:
      1. unauthenticated observer — should see meta ONLY from open_broker
      2. authenticated observer — authenticates to protected_broker, sees meta from BOTH
      3. authenticated subscriber — authenticates + select_topic on protected_broker,
         should receive binary frames

    Also publishes MQTT messages to both brokers to generate traffic.
    """
    AUTH_USERNAME = "testuser"
    AUTH_PASSWORD = "test_secret_42"

    open_port = find_free_port()
    protected_port = find_free_port()
    http_port = 3030

    print("=" * 70)
    print("  mqtt-inspector  AUTH TEST  (multi-broker authentication)")
    print("=" * 70)
    print(f"  Duration         : {args.duration}s")
    print(f"  Open broker      : 127.0.0.1:{open_port}")
    print(f"  Protected broker : 127.0.0.1:{protected_port}  (user={AUTH_USERNAME})")
    print(f"  Publishers       : 2 (one per broker)")
    print("=" * 70)

    open_broker_proc = None
    protected_broker_proc = None
    backend_proc = None
    threads = []
    # Separate counters for unauthenticated vs authenticated clients
    unauth_counter = make_counter()
    auth_obs_counter = make_counter()
    auth_sub_counter = make_counter()
    errors: list[str] = []

    open_broker = f"127.0.0.1:{open_port}"
    protected_broker = f"127.0.0.1:{protected_port}"

    try:
        # ---- Start brokers ----
        open_broker_proc = start_mosquitto(open_port)
        protected_broker_proc = start_mosquitto_with_auth(
            protected_port, AUTH_USERNAME, AUTH_PASSWORD,
        )

        if not args.skip_build:
            build_backend()

        # ---- Start backend with both brokers ----
        # The open broker is the "primary" broker; the protected broker is extra.
        backend_proc = start_backend(
            "127.0.0.1", open_port, args.max_broker_mb, http_port,
            extra_brokers=[{
                "host": protected_broker,
                "username": AUTH_USERNAME,
                "password": AUTH_PASSWORD,
            }],
        )

        ws_url = f"ws://127.0.0.1:{http_port}/ws"

        # ---- WS clients ----
        # 1. Unauthenticated observer
        t = threading.Thread(
            target=ws_observer_thread,
            args=(0, ws_url, unauth_counter, errors, "unauth"),
            daemon=True,
        )
        t.start()
        threads.append(t)
        print("[ws] 1 unauthenticated observer connected")

        # 2. Authenticated observer
        t = threading.Thread(
            target=ws_auth_authenticated_observer_thread,
            args=(0, ws_url, protected_broker, AUTH_PASSWORD, auth_obs_counter, errors),
            daemon=True,
        )
        t.start()
        threads.append(t)
        print("[ws] 1 authenticated observer connected")

        # 3. Authenticated subscriber (select_topic on protected broker)
        t = threading.Thread(
            target=ws_auth_subscriber_thread,
            args=(
                0, ws_url, protected_broker, AUTH_PASSWORD,
                "auth/protected/0", auth_sub_counter, errors,
            ),
            daemon=True,
        )
        t.start()
        threads.append(t)
        print("[ws] 1 authenticated subscriber connected")

        # ---- Publishers ----
        # Publisher on open broker
        t = threading.Thread(
            target=publisher_thread,
            args=(
                100, "127.0.0.1", open_port,
                args.rate, args.msg_size, 5,
                unauth_counter, errors,
            ),
            daemon=True,
        )
        t.start()
        threads.append(t)
        print(f"[mqtt] Publisher on open broker ({open_broker})")

        # Publisher on protected broker (needs credentials)
        t = threading.Thread(
            target=publisher_thread,
            args=(
                200, "127.0.0.1", protected_port,
                args.rate, args.msg_size, 5,
                auth_sub_counter, errors,
            ),
            kwargs={
                "username": AUTH_USERNAME,
                "password": AUTH_PASSWORD,
                "topic_base": "auth/protected",
            },
            daemon=True,
        )
        t.start()
        threads.append(t)
        print(f"[mqtt] Publisher on protected broker ({protected_broker})")

        # ---- Wait ----
        start = time.monotonic()
        while time.monotonic() - start < args.duration:
            elapsed = time.monotonic() - start
            # Show per-counter broker breakdowns
            u_meta = unauth_counter["meta_by_broker"]
            a_meta = auth_obs_counter["meta_by_broker"]
            s_binary = auth_sub_counter["binary_frames"]
            print(
                f"\r  [{elapsed:6.0f}s / {args.duration}s]  "
                f"unauth_meta={u_meta}  "
                f"auth_meta={a_meta}  "
                f"sub_binary={s_binary:>4}  "
                f"auth_ok={auth_obs_counter['auth_successes']}  "
                f"err={len(errors)}",
                end="",
                flush=True,
            )
            time.sleep(1)

        print()

    except KeyboardInterrupt:
        print("\n[!] Interrupted by user")
    finally:
        broker_procs = [p for p in [open_broker_proc, protected_broker_proc] if p]
        _teardown(threads, backend_proc, broker_procs, http_port, errors)

    # ---- Validation ----

    # 1. Unauthenticated client must NOT see meta from the protected broker
    unauth_protected_meta = unauth_counter["meta_by_broker"].get(protected_broker, 0)
    if unauth_protected_meta > 0:
        errors.append(
            f"SECURITY: Unauthenticated observer received {unauth_protected_meta} "
            f"meta entries from protected broker {protected_broker}!"
        )

    # 2. Unauthenticated client SHOULD see meta from the open broker
    unauth_open_meta = unauth_counter["meta_by_broker"].get(open_broker, 0)
    if unauth_open_meta == 0:
        errors.append(
            f"Unauthenticated observer received 0 meta from open broker {open_broker}. "
            f"Auto-authentication for non-password brokers may be broken."
        )

    # 3. Authenticated observer should see meta from BOTH brokers
    auth_open_meta = auth_obs_counter["meta_by_broker"].get(open_broker, 0)
    auth_protected_meta = auth_obs_counter["meta_by_broker"].get(protected_broker, 0)
    if auth_open_meta == 0:
        errors.append(
            f"Authenticated observer received 0 meta from open broker {open_broker}."
        )
    if auth_protected_meta == 0:
        errors.append(
            f"Authenticated observer received 0 meta from protected broker {protected_broker}. "
            f"Authentication may not have worked."
        )

    # 4. Authenticated observer should have received broker_auth_result success
    if auth_obs_counter["auth_successes"] == 0:
        errors.append("Authenticated observer never received a successful broker_auth_result.")

    # 5. Authenticated subscriber should have received binary frames
    if auth_sub_counter["binary_frames"] == 0:
        errors.append(
            "Authenticated subscriber received 0 binary frames from protected broker. "
            "Authenticated select_topic may be broken."
        )

    # 6. Authenticated subscriber should also have auth success
    if auth_sub_counter["auth_successes"] == 0:
        errors.append("Authenticated subscriber never received a successful broker_auth_result.")

    # 7. No client should have received unexpected binary (observers)
    for label, ctr in [("unauth", unauth_counter), ("auth_obs", auth_obs_counter)]:
        if ctr["observer_unexpected_binary"] > 0:
            errors.append(
                f"{label} observer received {ctr['observer_unexpected_binary']} "
                f"unexpected binary frames."
            )

    # ---- Report ----
    print()
    print("=" * 70)
    print("  AUTH TEST RESULTS")
    print("=" * 70)
    print(f"  Open broker              : {open_broker}")
    print(f"  Protected broker         : {protected_broker}")
    print()
    print("  Unauthenticated observer:")
    print(f"    Meta by broker         : {unauth_counter['meta_by_broker']}")
    print(f"    Broker lists received  : {unauth_counter['broker_lists']}")
    print(f"    Broker auth flags      : {unauth_counter['broker_auth_flags']}")
    print()
    print("  Authenticated observer:")
    print(f"    Meta by broker         : {auth_obs_counter['meta_by_broker']}")
    print(f"    Auth successes         : {auth_obs_counter['auth_successes']}")
    print(f"    Topic summaries        : {auth_obs_counter['topic_summary_msgs']}")
    print()
    print("  Authenticated subscriber:")
    print(f"    Meta by broker         : {auth_sub_counter['meta_by_broker']}")
    print(f"    Auth successes         : {auth_sub_counter['auth_successes']}")
    print(f"    Binary frames          : {auth_sub_counter['binary_frames']}")
    print(f"    select_topic sent      : {auth_sub_counter['select_topic_sent']}")
    print(f"    Topic summaries        : {auth_sub_counter['topic_summary_msgs']}")
    print()
    print(f"  Errors                   : {len(errors)}")
    for err in errors:
        print(f"    - {err}")
    print("=" * 70)

    if errors:
        print("  VERDICT: FAIL")
        print("=" * 70)
        return 1
    else:
        print("  VERDICT: PASS")
        print("=" * 70)
        return 0


def _print_report(counter: dict, rss_samples: list, errors: list):
    """Print the final report for stress mode."""
    print()
    print("=" * 70)
    print("  RESULTS")
    print("=" * 70)
    print(f"  Messages sent           : {counter['sent']:,}")
    print(f"  Meta batches / entries  : {counter['meta_batches']:,} / {counter['meta_entries']:,}")
    print(f"  Eviction batches/entries: {counter['eviction_batches']:,} / {counter['eviction_entries']:,}")
    print(f"  Binary frames received  : {counter['binary_frames']:,}")
    print(f"  Observer stray binary   : {counter['observer_unexpected_binary']:,}")
    print(f"  select_topic sent       : {counter['select_topic_sent']:,}")
    print(f"  topic_messages_clear    : {counter['topic_clears']:,}")
    print(f"  topic_sync_complete     : {counter['topic_syncs']:,}")
    print(f"  Topic switches          : {counter['topic_switches']:,}")
    print(f"  Broker lists            : {counter['broker_lists']:,}")
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
    else:
        print("  VERDICT: PASS")
        print("=" * 70)


def main():
    args = parse_args()
    if args.mode == "auth":
        sys.exit(main_auth(args))
    else:
        sys.exit(main_stress(args))


if __name__ == "__main__":
    main()
