# Python SDK runtime verification

Run from the repository root:

```bash
python3 -m pip install requests
python3 -m unittest discover -s tests -p test_python_sdk.py -v
python3 smoke/python_sdk/run.py
```

The runtime check uses an existing local `thusinh1969/brighto_airouter:v1.1.0` Docker image. Use `--image <local-tag>` to select another image. It never pulls or publishes an image, changes `.env`, or touches the installed router's database.

Prerequisites: Docker, OpenSSL, Node.js/npm, Chromium, and the release mock binary. The existing browser helper installs Playwright when missing and builds release binaries when missing. Two temporary PostgreSQL databases and router containers provide clean HTTP and HTTPS cases; generated certificates are trusted explicitly by the SDK.

Coverage includes every public inference API, model discovery, chat history, streaming, raw events, multipart audio, round-robin and weighted groups, endpoint-type errors, invalid keys, URL `/v1` normalization, and calls after a backend health interval. Playwright uses the Docker-embedded Portal to create, test, and save routes and groups, checking the submitted API payloads. `test_router.py` is exercised in all eight modes, including streamed chat.

Optionally include tiny real calls to a running no-auth local model:

```bash
python3 smoke/python_sdk/run.py --local-llm http://llm-host.local:8088/v1
```

The local model remains running; temporary routes and containers are removed with their isolated database when the check exits. Model inference is not benchmarked by this smoke test. Detailed edge cases such as Unicode SSE, shuffled embedding indices, error propagation, and early stream close are covered by the SDK contract tests.
