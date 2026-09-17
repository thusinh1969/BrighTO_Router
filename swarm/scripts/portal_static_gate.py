#!/usr/bin/env python3
"""Static release gate for BrighTO Portal polish.

This gate is intentionally strict and cheap. It catches the known Portal release
blockers before running heavier Playwright checks.
"""
from pathlib import Path
import re
import sys

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
for fn in ['renderProviders', 'renderModels', 'renderTeams', 'renderKeys', 'renderUsage']:
    pattern = f'{fn}($("content"))'
    check(f'no direct post-action {fn} append', pattern not in text, pattern)

failures = [c for c in checks if not c[1]]
print('PORTAL_STATIC_GATE', 'PASS' if not failures else f'FAIL {len(failures)}')
for name, ok, evidence in checks:
    print(('PASS ' if ok else 'FAIL ') + name + ('' if ok else f' :: {evidence}'))
sys.exit(0 if not failures else 1)
