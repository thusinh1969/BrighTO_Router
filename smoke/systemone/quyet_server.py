#!/usr/bin/env python3
"""Optional HTTP server for Quyet by Chinh Nguyen (https://github.com/ncchinh/quyet).

Keeps inference outside BrighTO-Router. Install quyet, fastapi, and uvicorn;
run: python quyet_server.py --model chinhnc/Quyet-1.0-Small --device cuda:0
"""
from __future__ import annotations

import argparse
import hmac
import logging
import os
import threading
from typing import Any

from fastapi import FastAPI, Header, HTTPException
from pydantic import BaseModel


class DecisionRequest(BaseModel):
    model: str
    state: Any
    questions: dict[str, Any]
    strict: bool = False


def create_app(model: Any, alias: str, api_key: str = "") -> FastAPI:
    app = FastAPI(title="Quyet System One endpoint")
    lock = threading.Lock()  # One model execution at a time per GPU replica.

    def authorize(authorization: str | None) -> None:
        if api_key and not hmac.compare_digest((authorization or "").encode(), ("Bearer " + api_key).encode()):
            raise HTTPException(401, "Invalid backend API key")

    @app.get("/health")
    @app.get("/healthz")
    def health():
        return {"status": "ok"}

    @app.get("/v1/models")
    def models(authorization: str | None = Header(default=None)):
        authorize(authorization)
        return {"object": "list", "data": [{"id": alias, "object": "model"}]}

    @app.post("/v1/systemone")
    @app.post("/v1/decisions")
    def decide(body: DecisionRequest, authorization: str | None = Header(default=None)):
        authorize(authorization)
        if body.model != alias:
            raise HTTPException(404, "Model not available on this replica")
        if not isinstance(body.state, (str, dict, list)) or not body.questions:
            raise HTTPException(400, "state must be text, an object, or a list; questions must be nonempty")
        try:
            with lock:
                return model.predict(body.state, body.questions, strict=body.strict)
        except ValueError as exc:
            raise HTTPException(400, str(exc)) from exc
        except Exception as exc:
            logging.getLogger("quyet.server").error("Prediction failed (%s)", type(exc).__name__)
            raise HTTPException(500, "Quyet inference failed; check the backend runtime") from exc

    return app


def main():
    import quyet
    import torch
    import uvicorn

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", default="chinhnc/Quyet-1.0-Small")
    parser.add_argument("--revision", help="Optional pinned Hugging Face model revision")
    parser.add_argument("--alias", default="quyet-small")
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8090)
    args = parser.parse_args()
    torch.set_num_threads(int(os.getenv("QUYET_CPU_THREADS", "4")))
    model = quyet.load(args.model, device=args.device, revision=args.revision)
    # Warm the actual model before the HTTP service becomes healthy.
    model.predict("The service is ready.", {"ready": {"type": "noul", "instructions": "The service is ready."}})
    uvicorn.run(create_app(model, args.alias, os.getenv("QUYET_API_KEY", "")),
                host=args.host, port=args.port, log_level="info", access_log=False)


if __name__ == "__main__":
    main()
