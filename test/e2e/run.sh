#!/usr/bin/env bash
# Browser end-to-end tests. Builds the frontend and backend, then runs every
# *.test.mjs here against a real broker (Docker) and backend.
# Usage: ./run.sh [--skip-build]
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$SCRIPT_DIR"

if [ ! -d node_modules/playwright ] || [ ! -d node_modules/prettier ]; then
    echo "[setup] Installing test dependencies …"
    npm install --no-audit --no-fund --silent
fi
# On CI, also install the system libraries Chromium needs.
npx playwright install ${CI:+--with-deps} chromium >/dev/null

if [ ! -d "$REPO_ROOT/frontend/node_modules" ]; then
    echo "[error] frontend dependencies missing: run 'yarn install' in frontend/ first" >&2
    exit 1
fi

if [ "${1:-}" != "--skip-build" ]; then
    echo "[build] frontend …"
    (cd "$REPO_ROOT/frontend" && npm run build >/dev/null)
    echo "[build] backend …"
    (cd "$REPO_ROOT/backend" && cargo build --quiet)
fi

exec npm test --silent
