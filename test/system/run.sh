#!/usr/bin/env bash
# Convenience wrapper for the stress test.
# Usage: ./run.sh [--duration 60] [--publishers 5] [--rate 10] …
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$SCRIPT_DIR/stress_test.py" "$@"
