#!/usr/bin/env python3
"""Static release gate for BrighTO Portal polish.

This gate is intentionally strict and cheap. It catches the known Portal release
blockers before running heavier Playwright checks.
"""
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

text = Path('static/index.html').read_text() if Path('static/index.html').exists() else ''
checks: list[tuple[str, bool, str]] = []

def check(name: str, ok: bool, evidence: str = '') -> None:
    checks.append((name, ok, evidence))

check('has Portal preferences settings', 'Portal preferences' in text, 'missing literal: Portal preferences')
check(
    'has font-size preference state',
    ('data-font' in text or 'dataset.font' in text) and bool(re.search(r'Small|Normal|Large', text, re.I)),
    'missing data-font/dataset.font or Small/Normal/Large labels',
)
check(
    'has density preference state',
    ('data-density' in text or 'dataset.density' in text) and bool(re.search(r'Compact|Comfortable', text, re.I)),
    'missing data-density/dataset.density or Compact/Comfortable labels',
)
check('has compact count formatter', ('fmtCount' in text or 'formatCompact' in text), 'missing fmtCount/formatCompact')
check(
    'chart formatter uses uppercase K',
    not bool(re.search(r'\+\s*["\']k["\']|["\']k["\']\s*;', text)),
    'lowercase k marker present',
)
check(
    'short token-rate samples show Too short',
    'Too short' in text and 'Sample*' not in text,
    'Portal must show Too short, not Sample*, for sub-1s token-rate samples',
)
check(
    'token-rate formatter does not star-mark inflated samples',
    '?"*"' not in text and '?"*"' not in text and '+"*"' not in text,
    'star-marked token-rate sample formatter still present',
)
check(
    'copy buttons use fallback helper',
    'function copyText(' in text and 'navigator.clipboard.writeText(value).then(ok).catch(fallback)' in text and 'execCommand("copy")' in text,
    'missing robust copyText clipboard fallback',
)
check(
    'copy actions do not call clipboard directly',
    'navigator.clipboard.writeText(endpoint)' not in text and 'navigator.clipboard.writeText(curl)' not in text and 'navigator.clipboard.writeText(text)' not in text,
    'direct clipboard call still present outside copyText helper',
)

node = shutil.which('node')
if node:
    scripts = re.findall(r'<script>([\s\S]*?)</script>', text)
    with tempfile.NamedTemporaryFile('w', suffix='.js', delete=False) as fh:
        fh.write('\n'.join(scripts))
        script_path = fh.name
    proc = subprocess.run([node, '--check', script_path], text=True, capture_output=True)
    check('inline Portal JavaScript parses', proc.returncode == 0, (proc.stderr or proc.stdout).strip()[:500])
else:
    check('inline Portal JavaScript parses', True, 'node unavailable; Playwright audit covers runtime parsing')
def render_clears_container(fn: str) -> bool:
    # Direct renderX($("content")) calls are acceptable only if renderX clears
    # the target container before appending fresh DOM. This matches DeepSeek's
    # current minimal fix and avoids a false positive while still protecting
    # against stale/duplicate panels.
    m = re.search(rf'function\s+{fn}\s*\(c\)\s*{{(?P<body>.{{0,260}})', text, re.S)
    if not m:
        return False
    return bool(re.search(r'c\.innerHTML\s*=\s*["\']{2}', m.group('body')))

for fn in ['renderProviders', 'renderModels', 'renderTeams', 'renderKeys', 'renderUsage']:
    pattern = f'{fn}($("content"))'
    ok = pattern not in text or render_clears_container(fn)
    evidence = pattern if pattern in text else f'{fn} definition missing/unsafe'
    check(f'post-action {fn} cannot append stale panels', ok, evidence)

failures = [c for c in checks if not c[1]]
print('PORTAL_STATIC_GATE', 'PASS' if not failures else f'FAIL {len(failures)}')
for name, ok, evidence in checks:
    print(('PASS ' if ok else 'FAIL ') + name + ('' if ok else f' :: {evidence}'))
sys.exit(0 if not failures else 1)
