"""Actual Rust HTTP <-> Python HTTP integration on disposable databases.

Only the adb executable is replaced (no device network traffic). Scripts are
expected to fail honestly because UI automation has not been prepared.
No backend route, executor runtime, journal or callback is mocked.
Usage: PINNED_VENV/python automation-http-integration.py
"""
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
from uuid import uuid4

import httpx

repo = Path(__file__).resolve().parents[3]
root = Path(tempfile.mkdtemp(prefix='integrity-automation-'))
print('Disposable evidence directory:', root, flush=True)
binary = repo / 'target/debug/easytier-web'
subprocess.run([sys.executable, str(Path(__file__).with_name('prepare-current-fixture.py')),
                str(repo), str(root), str(binary)], check=True, stdout=subprocess.DEVNULL)
fixture = json.loads((root / 'fixture.json').read_text())
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    runner_port = sock.getsockname()[1]
runner_url = f'http://127.0.0.1:{runner_port}'
token = 'isolated-audit-token-' + 'x' * 32
adb = root / 'fixture-adb'
adb.write_text('#!/bin/sh\ncase "$*" in\nconnect*) echo "connected to fixture";;\n*get-state*) echo device;;\n*getprop*) echo fixture;;\ndisconnect*) echo disconnected;;\n*) exit 1;;\nesac\n')
adb.chmod(0o700)
environment = dict(os.environ, AUTOMATION_DATA_DIR=str(root / 'executor'),
                   PYTHONDONTWRITEBYTECODE='1',
                   AUTOMATION_TOKEN=token, AUTOMATION_CALLBACK_URL=fixture['baseURL'],
                   AUTOMATION_ADB_BINARY=str(adb), EASYTIER_AUTOMATION_RUNNER_URL=runner_url,
                   EASYTIER_AUTOMATION_RUNNER_TOKEN=token)
children = []
files = []

def launch(command, cwd, name):
    log = (root / name).open('wb'); files.append(log)
    child = subprocess.Popen(command, cwd=cwd, env=environment, stdout=log, stderr=subprocess.STDOUT)
    children.append(child)

def wait_http(client, path, status):
    for _ in range(100):
        try:
            if client.get(path).status_code == status:
                return
        except httpx.HTTPError:
            pass
        time.sleep(.1)
    raise AssertionError('Isolated HTTP service did not become ready')

try:
    launch([sys.executable, '-m', 'uvicorn', 'automation_executor.app:app', '--host', '127.0.0.1',
            '--port', str(runner_port), '--no-access-log'], repo / 'automation-executor', 'executor.log')
    launch([str(binary), *fixture['serverArgs']], repo, 'web.log')
    with httpx.Client(base_url=runner_url, trust_env=False) as runner:
        wait_http(runner, '/internal/v1/health', 401)
    with httpx.Client(base_url=fixture['baseURL'], trust_env=False, timeout=25) as client:
        wait_http(client, '/api/v1/auth/check_login_status', 401)
        response = client.post('/api/v1/auth/login', json=dict(username=fixture['owner']['username'],
            password=hashlib.md5(fixture['owner']['password'].encode()).hexdigest()))
        assert response.status_code == 200
        owner = fixture['owner']['id']; node = fixture['apiNode']['machine_id']
        scope = dict(owner_user_id=owner, machine_id=node)
        def api(method, path, body=None, params=None, statuses=(200, 201)):
            response = client.request(method, '/api/v1' + path, params=params, json=body)
            assert response.status_code in statuses, f'{method} {path}: {response.status_code} {response.text}'
            return response.json() if response.content else None
        def auto(method, path, body=None, statuses=(200, 201)):
            return api(method, '/automation' + path, body, dict(owner_user_id=owner), statuses)
        context = api('GET', '/automation/context')
        assert context['runner_configured'] is True
        kind = api('POST', '/assets/device-types', dict(request_id=str(uuid4()), name='Audit phone', semantic_kind='phone'), dict(owner_user_id=owner))
        spec = dict(mode='direct_instance', instance_id=fixture['instances']['apiNode']['ordinary']['instance_id'])
        preview = api('POST', '/assets/validation/instance-address', dict(subject=dict(resource_kind='device', id=None), address_spec=spec), scope)
        device = api('POST', '/assets/devices', dict(request_id=str(uuid4()), name='Audit phone', device_type_id=kind['id'],
                    address_spec=spec, preview_version=preview['preview_version']), scope)
        service = api('POST', f"/assets/devices/{device['id']}/services", dict(request_id=str(uuid4()), name='ADB', service_type='adb',
                    port=37001, protocols=['tcp'], connection_mode='wireless'), scope)
        phones = auto('GET', '/devices')['items']
        assert any(p['id'] == device['id'] and p['connection']['adb_port'] == 37001 for p in phones)
        print('PASS: real asset type + device + ADB service feed automation device list', flush=True)
        prepared = auto('POST', f"/devices/{device['id']}/prepare", dict(action='connect'))
        assert prepared['state'] == 'connected'
        print('PASS: Rust -> Python -> isolated adb executable -> Rust preparation response', flush=True)
        steps = [dict(id=str(uuid4()), type='key', key='HOME')]
        script = auto('POST', '/scripts', dict(name='Audit script', description='', steps=steps))
        assert any(s['id'] == script['id'] for s in auto('GET', '/scripts')['items'])
        edited = auto('PUT', f"/scripts/{script['id']}", dict(name='Audit revised', description='', steps=steps, revision=script['revision']))
        assert edited['revision'] > script['revision']
        auto('PUT', f"/scripts/{script['id']}", dict(name='Stale', description='', steps=steps, revision=script['revision']), (409,))
        print('PASS: real script create/list/update and stale revision conflict', flush=True)
        body = dict(request_id=str(uuid4()), script_id=script['id'], revision=edited['revision'], device_ids=[device['id']], parameters={}, concurrency=1)
        run = auto('POST', '/runs', body)
        replay = auto('POST', '/runs', body)
        assert replay['id'] == run['id']
        # Polling is confined to this test synchronization, not product behavior.
        for _ in range(60):
            observed = auto('GET', f"/runs/{run['id']}")
            if observed['devices'][0]['state'] not in ('queued', 'accepted', 'running', 'dispatch_pending', 'starting'):
                break
            time.sleep(.1)
        assert observed['devices'][0]['state'] == 'failed', observed
        assert observed['devices'][0]['error']['code'] == 'capability_missing', observed
        print('PASS: Rust dispatch -> actual executor -> real HTTP callback persists honest capability_missing', flush=True)
        refreshed = auto('POST', f"/runs/{run['id']}/refresh", {})
        assert refreshed['devices'][0]['state'] == 'failed'
        auto('DELETE', f"/scripts/{script['id']}")
        assert auto('GET', f"/runs/{run['id']}")['script_snapshot']['steps'] == steps
        print('PASS: explicit refresh and deleting a script preserve execution history', flush=True)
        session = auto('POST', '/sessions', dict(device_id=device['id'], quality='wall'))
        closed = auto('POST', f"/sessions/{session['id']}/close", {})
        assert closed.get('cleanup_pending') is False, closed
        print('PASS: real session create/close checks persistent executor identity and cleanup', flush=True)
finally:
    for child in reversed(children):
        child.terminate()
        try: child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            child.kill(); child.wait()
    for log in files:
        log.close()
