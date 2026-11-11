# Browser end-to-end tests

Drive the real UI in headless Chromium against a real broker and backend.

```bash
cd test/e2e
./run.sh                # builds frontend + backend, then runs all *.test.mjs
./run.sh --skip-build   # reuse the existing wwwroot and backend binary
```

Needs Docker (for the Mosquitto broker), Node and Cargo. Playwright and its
Chromium are installed on first run.

The backend always listens on port 3030, so nothing else may use it (the tests
check and fail early). Each run starts its own broker on a random port and a
backend with a 1 MB buffer cap, so eviction kicks in within seconds.

| Test                    | Covers                                                                                                                            |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `message_lock.test.mjs` | Locked messages, stepping and the diff view stay put while messages stream into a full buffer; evicted locked messages stay shown |
