#!/usr/bin/env python3
"""Static guardrails for the inference hot path.

This is intentionally small and conservative. It scans only non-test source for patterns that
are forbidden in the fastest router request path.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def read_non_test(rel: str) -> str:
    text = (ROOT / rel).read_text(encoding="utf-8")
    return text.split("#[cfg(test)]", 1)[0]


def strip_non_hot_handlers(text: str) -> str:
    """src/handlers.rs also serves health and Portal HTML.

    The hot-path guard is for inference request handling. Portal serving may read
    PORTAL_STATIC_FILE from disk in local UI design mode and is not part of the
    model request path.
    """
    return re.sub(r"async fn portal\([^\n]*\) -> impl IntoResponse \{.*?\n\}", "", text, flags=re.S)


def fail(msg: str) -> None:
    print(f"HOTPATH_GUARD_FAIL {msg}")
    sys.exit(1)


def assert_absent(rel: str, pattern: str, reason: str) -> None:
    text = read_non_test(rel)
    if rel == "src/handlers.rs":
        text = strip_non_hot_handlers(text)
    if re.search(pattern, text):
        fail(f"{rel}: found /{pattern}/ — {reason}")


def main() -> None:
    for rel in ("src/handlers.rs", "src/proxy/mod.rs"):
        assert_absent(rel, r"\bsqlx::|PgPool|DATABASE_URL", "no DB access in inference path")
        assert_absent(rel, r"\bredis\b|Redis", "no Redis in fastest inference path")
        assert_absent(rel, r"std::fs|tokio::fs|File::|OpenOptions", "no filesystem access in inference path")
        assert_absent(rel, r"env::var|var_os", "no env reads in inference path")
        assert_absent(rel, r"reqwest::Client::new|Client::builder", "use shared reqwest Client from AppState")

    proxy = read_non_test("src/proxy/mod.rs")
    bytes_await_count = len(re.findall(r"\.bytes\(\)\.await", proxy))
    if bytes_await_count:
        if bytes_await_count != 1 or "NONSTREAM_USAGE_BUFFER_LIMIT" not in proxy or ".content_length()" not in proxy:
            fail(
                "src/proxy/mod.rs: .bytes().await is allowed only for one bounded small-response "
                "fast path guarded by Content-Length <= NONSTREAM_USAGE_BUFFER_LIMIT"
            )
    assert_absent(
        "src/proxy/mod.rs",
        r"std::collections::HashSet|HashSet<",
        "proxy retry tracking must be stack/small allocation-free",
    )
    assert_absent(
        "src/route/mod.rs",
        r"Vec<Candidate>|let\s+mut\s+candidates\s*:\s*Vec|let\s+mut\s+tied\s*:\s*Vec",
        "route picker must not allocate candidate vectors on hot path",
    )

    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    if re.search(r"reqwest\s*=\s*\{[^\n]*\"gzip\"", cargo):
        fail("Cargo.toml: reqwest gzip feature enabled — fastest proxy must force identity encoding")

    print("HOTPATH_GUARD_PASS")


if __name__ == "__main__":
    main()
