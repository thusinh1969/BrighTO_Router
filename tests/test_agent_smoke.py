"""Prevent native-agent smoke from accepting text-only/fallback false positives."""
import importlib.util
import json
import os
import shutil
import ssl
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('agent_smoke', Path(__file__).resolve().parents[1] / 'smoke/agents/run.py')
agent = importlib.util.module_from_spec(spec)
spec.loader.exec_module(agent)


class AgentProofTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which('openssl'), 'certificate test requires installer prerequisite openssl')
    def test_private_ca_does_not_replace_public_trust_roots(self):
        public_roots = {cert['subject'] for cert in ssl.create_default_context().get_ca_certs()}
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            config = work / 'req.cnf'
            config.write_text('[req]\ndistinguished_name=dn\n[dn]\n')
            ca = work / 'ca.pem'
            subprocess.run(['openssl', 'req', '-config', str(config), '-x509', '-newkey', 'rsa:2048',
                            '-nodes', '-days', '1', '-keyout', str(work / 'key.pem'), '-out', str(ca),
                            '-subj', '/CN=BrighTO-Smoke-CA', '-addext', 'basicConstraints=critical,CA:TRUE'],
                           check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            bundle = agent.client_trust_bundle(str(ca), work)
            trusted = ssl.create_default_context(cafile=bundle).get_ca_certs()
            self.assertTrue(public_roots.issubset({cert['subject'] for cert in trusted}))
            self.assertTrue(any(('commonName', 'BrighTO-Smoke-CA') in rdn
                                for cert in trusted for rdn in cert['subject']))

    def test_codex_requires_real_successful_tool_and_completed_final_answer(self):
        rows = [
            {'type': 'item.completed', 'item': {'type': 'command_execution', 'exit_code': 0, 'aggregated_output': 'proof\n'}},
            {'type': 'item.completed', 'item': {'type': 'agent_message', 'text': 'proof'}},
            {'type': 'turn.completed'},
        ]
        self.assertEqual(agent.assert_proof('codex', '\n'.join(map(json.dumps, rows)), 'proof'), 1)
        for bad in (rows[1:], rows[:-1]):
            with self.assertRaises(ValueError):
                agent.assert_proof('codex', '\n'.join(map(json.dumps, bad)), 'proof')
        rows[0]['item']['exit_code'] = 1
        with self.assertRaises(ValueError):
            agent.assert_proof('codex', '\n'.join(map(json.dumps, rows)), 'proof')

    def test_hermes_answer_without_returned_tool_output_is_rejected(self):
        rows = [
            {'type': 'tool_use', 'name': 'read_file'},
            {'type': 'tool_result', 'output': 'proof', 'is_error': False},
            {'type': 'result', 'exit_code': 0, 'text': 'proof'},
        ]
        self.assertEqual(agent.assert_proof('hermes', '\n'.join(map(json.dumps, rows)), 'proof'), 1)
        rows[1]['is_error'] = True
        with self.assertRaises(ValueError):
            agent.assert_proof('hermes', '\n'.join(map(json.dumps, rows)), 'proof')

    def test_claude_deduplicates_read_events_and_rejects_failed_result(self):
        read = {'type': 'assistant', 'message': {'content': [{'type': 'tool_use', 'name': 'Read', 'id': 'tool_1'}]}}
        result = {'type': 'result', 'subtype': 'success', 'is_error': False, 'result': 'proof'}
        self.assertEqual(agent.assert_proof('claude', '\n'.join(map(json.dumps, [read, read, result])), 'proof'), 1)
        result['is_error'] = True
        with self.assertRaises(ValueError):
            agent.assert_proof('claude', '\n'.join(map(json.dumps, [read, result])), 'proof')

    def test_openclaw_fallback_answer_is_not_a_gateway_pass(self):
        result = {'meta': {'toolSummary': {'calls': 1, 'failures': 0, 'tools': ['read']},
                           'terminalReply': {'text': 'proof'}, 'executionTrace': {'fallbackUsed': False}}}
        self.assertEqual(agent.assert_proof('openclaw', json.dumps(result), 'proof'), 1)
        result['meta']['executionTrace']['fallbackUsed'] = True
        with self.assertRaises(ValueError):
            agent.assert_proof('openclaw', json.dumps(result), 'proof')

    def test_base_url_guardrails(self):
        self.assertEqual(agent.router_root('https://llm-host.local:18443/v1/v1/'), 'https://llm-host.local:18443')
        for bad in ('ftp://host', 'http://user:secret@host', 'http://host?key=secret',
                    'http://host/v1/responses', 'http://host/v1/chat/completions'):
            with self.assertRaises(ValueError):
                agent.router_root(bad)

    def test_provider_secrets_do_not_leak_into_disposable_client_environment(self):
        with patch.dict(os.environ, {'OPENAI_API_KEY': 'vendor-secret', 'ANTHROPIC_AUTH_TOKEN': 'vendor-secret',
                                     'CODEX_HOME': '/unrelated/config'}, clear=True):
            env = agent.isolated_env('client-key', '/test/ca.pem')
        self.assertNotIn('OPENAI_API_KEY', env)
        self.assertNotIn('ANTHROPIC_AUTH_TOKEN', env)
        self.assertNotIn('CODEX_HOME', env)
        self.assertEqual(env['BRIGHTO_CLIENT_KEY'], 'client-key')
        self.assertEqual(env['CODEX_CA_CERTIFICATE'], '/test/ca.pem')


if __name__ == '__main__':
    unittest.main()
