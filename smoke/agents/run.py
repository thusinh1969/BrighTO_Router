#!/usr/bin/env python3
"""Exercise installed agent CLIs against a router: tool -> real file -> next model turn.

No BrighTO SDK, admin credential, or provider credential is required. Each run
uses disposable harness settings and a fresh random fixture, never user config.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import signal
import ssl
import subprocess
import tempfile
import time
from urllib.parse import urlsplit, urlunsplit

HARNESSES = ('claude', 'codex', 'hermes', 'openclaw')


def router_root(value: str) -> str:
    parsed = urlsplit(value.strip())
    if parsed.scheme not in ('http', 'https') or not parsed.hostname:
        raise ValueError('--url must be an HTTP or HTTPS router URL')
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError('--url must not contain credentials, a query, or a fragment')
    path = parsed.path.rstrip('/')
    while path.endswith('/v1'):
        path = path[:-3].rstrip('/')
    if path.endswith(('/chat/completions', '/completions', '/responses', '/messages')):
        raise ValueError('--url must be the router root or /v1 base URL, not an inference endpoint')
    return urlunsplit((parsed.scheme, parsed.netloc, path, '', ''))


def isolated_env(key: str, ca_file: str | None) -> dict[str, str]:
    env = {k: v for k, v in os.environ.items()
           if not k.startswith(('OPENAI', 'ANTHROPIC', 'CLAUDE', 'CODEX', 'HERMES', 'OPENCLAW', 'GEMINI', 'DEEPSEEK'))
           and not k.endswith(('_API_KEY', '_AUTH_TOKEN'))}
    env['BRIGHTO_CLIENT_KEY'] = key
    if ca_file:
        env.update(CODEX_CA_CERTIFICATE=ca_file, SSL_CERT_FILE=ca_file,
                   REQUESTS_CA_BUNDLE=ca_file, NODE_EXTRA_CA_CERTS=ca_file)
    return env


def write_private(path: Path, content: object) -> None:
    path.write_text(json.dumps(content, indent=2), encoding='utf-8')
    path.chmod(0o600)


def client_trust_bundle(ca_file: str | None, directory: Path) -> str | None:
    if not ca_file:
        return None
    # SSL_CERT_FILE/REQUESTS_CA_BUNDLE replace public trust, unlike Node's
    # EXTRA_CA_CERTS. Keep normal roots so a harness's public HTTPS still works.
    roots = ssl.create_default_context().get_ca_certs(binary_form=True)
    bundle = directory / 'client-ca-bundle.pem'
    bundle.write_text(''.join(ssl.DER_cert_to_PEM_cert(root) for root in roots)
                      + '\n' + Path(ca_file).read_text(encoding='utf-8'), encoding='utf-8')
    return str(bundle)


def json_lines(text: str) -> list[dict]:
    result = []
    for line in text.splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        if isinstance(row, dict):
            result.append(row)
    return result


def assert_proof(kind: str, stdout: str, sentinel: str) -> int:
    rows = json_lines(stdout)
    if kind == 'codex':
        items = [r.get('item', {}) for r in rows if r.get('type') == 'item.completed']
        tools = [i for i in items if i.get('type') == 'command_execution'
                 and i.get('exit_code') == 0 and sentinel in i.get('aggregated_output', '')]
        answers = [i.get('text', '').strip() for i in items if i.get('type') == 'agent_message']
        completed = any(r.get('type') == 'turn.completed' for r in rows)
        valid = completed and tools and answers and answers[-1] == sentinel
    elif kind == 'claude':
        blocks = [b for r in rows if r.get('type') == 'assistant'
                 for b in r.get('message', {}).get('content', [])
                 if b.get('type') == 'tool_use' and b.get('name') == 'Read']
        tools = list({b['id']: b for b in blocks}.values())
        valid = tools and any(r.get('type') == 'result' and not r.get('is_error')
                              and r.get('subtype') == 'success' and r.get('result', '').strip() == sentinel
                              for r in rows)
    elif kind == 'hermes':
        tools = [r for r in rows if r.get('type') == 'tool_use' and r.get('name') == 'read_file']
        returned = any(r.get('type') == 'tool_result' and not r.get('is_error')
                       and sentinel in r.get('output', '') for r in rows)
        valid = tools and returned and any(r.get('type') == 'result' and r.get('exit_code') == 0
                                           and r.get('text', '').strip() == sentinel for r in rows)
    else:
        result = json.loads(stdout)
        meta = result.get('meta', {})
        summary = meta.get('toolSummary', {})
        calls = summary.get('calls', 0)
        valid = (not meta.get('aborted') and calls > 0 and summary.get('failures') == 0
                 and 'read' in summary.get('tools', [])
                 and meta.get('terminalReply', {}).get('text', '').strip() == sentinel
                 and not meta.get('executionTrace', {}).get('fallbackUsed'))
        tools = [None] * calls
    if not valid:
        raise ValueError('No proof of a successful file-reading tool AND an exact final answer')
    return len(tools)


def run_one(kind: str, args: argparse.Namespace, key: str) -> dict:
    model = getattr(args, {'claude': 'messages_model', 'codex': 'responses_model'}.get(kind, 'chat_model'))
    if not model:
        raise ValueError(f'{kind} needs --' + {'claude': 'messages-model', 'codex': 'responses-model'}.get(kind, 'chat-model'))
    binary = getattr(args, kind + '_bin')
    if not shutil.which(binary):
        raise ValueError(f'{kind} executable not found: {binary}; install the harness or pass --{kind}-bin')
    root = router_root(args.url)
    with tempfile.TemporaryDirectory(prefix='brighto-agent-smoke-') as directory:
        task_dir = Path(directory)
        env = isolated_env(key, client_trust_bundle(args.ca_file, task_dir))
        work = task_dir / 'workspace'
        work.mkdir()
        sentinel = 'BRIGHTO_TOOL_PROOF_' + secrets.token_hex(12)
        (work / 'harness-proof.txt').write_text(sentinel + '\n', encoding='utf-8')
        prompt = (f'Read {work / "harness-proof.txt"} using your file-reading or shell tool, then reply '
                  'with exactly its contents. Return one plain line: the file text only, '
                  'without line numbers, quotes, labels, or Markdown. Do not guess the contents.')
        if kind == 'claude':
            env.update(ANTHROPIC_BASE_URL=root, ANTHROPIC_API_KEY=key,
                       ANTHROPIC_DEFAULT_SONNET_MODEL=model, ANTHROPIC_DEFAULT_HAIKU_MODEL=model,
                       ANTHROPIC_DEFAULT_OPUS_MODEL=model, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC='1')
            command = [binary, '--bare', '-p', prompt, '--model', model, '--tools', 'Read', '--allowedTools', 'Read',
                       '--max-turns', '3', '--output-format', 'stream-json', '--verbose', '--no-session-persistence',
                       '--system-prompt', 'Read the requested fixture and report its exact content.']
        elif kind == 'codex':
            command = [binary, 'exec', '--ignore-user-config', '--ignore-rules', '--skip-git-repo-check',
                       '--ephemeral', '--json', '--color', 'never', '--sandbox', 'read-only', '-C', str(work), '-m', model]
            overrides = {'model_provider': 'brighto_smoke', 'model_providers.brighto_smoke.name': 'BrighTO smoke',
                         'model_providers.brighto_smoke.base_url': root + '/v1',
                         'model_providers.brighto_smoke.env_key': 'BRIGHTO_CLIENT_KEY',
                         'model_providers.brighto_smoke.wire_api': 'responses',
                         'model_providers.brighto_smoke.requires_openai_auth': False,
                         'model_providers.brighto_smoke.supports_websockets': False,
                         'model_providers.brighto_smoke.request_max_retries': 0,
                         'model_providers.brighto_smoke.stream_max_retries': 0,
                         'model_reasoning_effort': 'low', 'web_search': 'disabled'}
            for name, value in overrides.items():
                command.extend(['-c', name + '=' + json.dumps(value)])
            command.append(prompt)
        elif kind == 'hermes':
            state = task_dir / 'hermes'
            state.mkdir()
            env.update(HERMES_HOME=str(state), TERMINAL_CWD=str(work))
            write_private(state / 'config.yaml', {
                'model': {'provider': 'custom', 'default': model, 'base_url': root + '/v1',
                          'api_key': key, 'api_mode': 'chat_completions'},
                'agent': {'max_turns': 3}, 'toolsets': ['file'],
                'memory': {'memory_enabled': False, 'user_profile_enabled': False}})
            command = [binary, 'chat', '--oneshot', '--ignore-rules', '--query', prompt, '--model', model,
                       '--provider', 'custom', '--toolsets', 'file', '--max-turns', '3',
                       '--run-budget', str(args.timeout), '--format', 'stream-json', '--in', str(work)]
        else:
            state = task_dir / 'openclaw'
            state.mkdir()
            env.update(OPENCLAW_STATE_DIR=str(state), OPENCLAW_CONFIG_PATH=str(state / 'openclaw.json'))
            provider = {'baseUrl': root + '/v1', 'apiKey': '${BRIGHTO_CLIENT_KEY}', 'api': 'openai-completions',
                        'models': [{'id': model, 'name': model, 'reasoning': False, 'input': ['text'],
                                    'cost': {'input': 0, 'output': 0, 'cacheRead': 0, 'cacheWrite': 0},
                                    'contextWindow': args.context_window, 'maxTokens': args.max_tokens}]}
            if args.allow_private_network:
                provider['request'] = {'allowPrivateNetwork': True}
            write_private(state / 'openclaw.json', {
                'agents': {'defaults': {'workspace': str(work), 'skipBootstrap': True,
                                       'model': {'primary': 'brighto/' + model}}},
                'models': {'mode': 'merge', 'providers': {'brighto': provider}}, 'tools': {'allow': ['read']}})
            command = [binary, 'agent', '--local', '--agent', 'main', '--session-id', secrets.token_hex(16),
                       '--message', prompt, '--json', '--timeout', str(args.timeout), '--thinking', 'off']
        started = time.monotonic()
        process = subprocess.Popen(command, cwd=work, env=env, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                   start_new_session=(os.name == 'posix'))
        try:
            stdout, stderr = process.communicate(timeout=args.timeout + 20)
        except subprocess.TimeoutExpired:
            if os.name == 'posix':
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
            stdout, stderr = process.communicate()
            raise ValueError('Harness timed out: ' + (stdout + '\n' + stderr).replace(key, '[redacted]')[-2500:])
        if process.returncode:
            raise ValueError(f'Harness exited {process.returncode}: ' + (stdout + '\n' + stderr).replace(key, '[redacted]')[-2500:])
        try:
            tools = assert_proof(kind, stdout, sentinel)
        except (ValueError, KeyError, TypeError) as exc:
            raise ValueError(str(exc) + ': ' + (stdout + '\n' + stderr).replace(key, '[redacted]')[-5000:]) from exc
        return {'harness': kind, 'status': 'PASS', 'model': model, 'tool_calls': tools,
                'elapsed_seconds': round(time.monotonic() - started, 2), 'transport': urlsplit(root).scheme}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default=os.getenv('BRIGHTO_ROUTER_URL', 'http://127.0.0.1:18080'))
    parser.add_argument('--harness', choices=HARNESSES + ('all',), default='all')
    parser.add_argument('--chat-model')
    parser.add_argument('--responses-model')
    parser.add_argument('--messages-model')
    parser.add_argument('--ca-file', help='PEM trust bundle for a self-signed/private-CA HTTPS router')
    parser.add_argument('--allow-private-network', action='store_true', help='Allow OpenClaw to access your trusted LAN router')
    parser.add_argument('--timeout', type=int, default=120)
    parser.add_argument('--context-window', type=int, default=32768, help='OpenClaw model input capacity; match your backend')
    parser.add_argument('--max-tokens', type=int, default=1024)
    for kind in HARNESSES:
        parser.add_argument('--' + kind + '-bin', default=kind)
    args = parser.parse_args()
    if args.ca_file:
        args.ca_file = str(Path(args.ca_file).resolve(strict=True))
    key = os.getenv('BRIGHTO_API_KEY', '').strip()
    if not key or not key.isascii() or any(ch.isspace() for ch in key):
        parser.error('Set BRIGHTO_API_KEY to a valid client key from Portal; do not use an admin/provider key')
    if args.timeout < 1 or args.context_window < 1 or args.max_tokens < 1:
        parser.error('Timeout and token limits must be positive')
    failed = False
    for kind in HARNESSES if args.harness == 'all' else [args.harness]:
        try:
            result = run_one(kind, args, key)
        except (ValueError, OSError, KeyError, TypeError) as exc:
            failed = True
            result = {'harness': kind, 'status': 'FAIL', 'error': str(exc).replace(key, '[redacted]')}
        print(json.dumps(result), flush=True)
    return int(failed)


if __name__ == '__main__':
    raise SystemExit(main())
