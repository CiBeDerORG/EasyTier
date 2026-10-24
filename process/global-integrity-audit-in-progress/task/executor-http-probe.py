"""Real FastAPI/runtime/journal smoke; no phone, media mock or network scan.

Run with the pinned HTTP dependencies. Full media installation is checked
separately and is deliberately not hidden by replacing MediaManager.
"""
import os
from pathlib import Path
import sys
import tempfile
from uuid import uuid4

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'automation-executor'))
from fastapi.testclient import TestClient
from automation_executor.app import app

with tempfile.TemporaryDirectory(prefix='integrity-executor-') as directory:
    os.environ.update(AUTOMATION_DATA_DIR=directory, AUTOMATION_TOKEN='audit-fixture-' + 'x' * 32,
                      AUTOMATION_CALLBACK_URL='http://127.0.0.1:9',
                      AUTOMATION_ADB_BINARY='/nonexistent-audit-adb')
    headers = {'Authorization': 'Bearer ' + os.environ['AUTOMATION_TOKEN']}
    target = dict(device_id=str(uuid4()), machine_id=str(uuid4()), owner_user_id=1,
                  name='Isolated audit fixture', ip='10.60.1.20', adb_port=37001,
                  connection_mode='wireless', target_fingerprint=str(uuid4()),
                  route_context='default', adb_service_id=str(uuid4()), endpoint_revision=1)
    with TestClient(app, raise_server_exceptions=False) as client:
        response = client.get('/internal/v1/health')
        assert response.status_code == 401
        print('PASS: unauthenticated health returns 401')
        health = client.get('/internal/v1/health', headers=headers).json()
        identity = health['executor_id']
        print('Health:', {k: health[k] for k in ('state', 'capabilities')})
        response = client.post('/internal/v1/prepare', headers=headers, json={'target': target, 'action': 'inspect'})
        assert response.status_code == 503 and response.json()['code'] == 'adb_unavailable'
        print('PASS: real prepare detects missing ADB (health nevertheless advertised adb=true)')
        sid = str(uuid4())
        body = dict(id=sid, target=target, quality='wall', lease_seconds=60, expected_executor_id=str(uuid4()))
        response = client.post('/internal/v1/sessions', headers=headers, json=body)
        assert response.status_code == 409
        print('PASS: replacement executor identity rejected')
        body['expected_executor_id'] = identity
        response = client.post('/internal/v1/sessions', headers=headers, json=body)
        assert response.status_code == 200, response.text
        print('PASS: real session registry and journal creation')
        sdp = 'v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=Audit\r\nt=0 0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\na=recvonly\r\na=rtpmap:96 H264/90000\r\n'
        response = client.post(f'/internal/v1/sessions/{sid}/offer', headers=headers, json={'sdp': sdp, 'type': 'offer'})
        assert response.status_code == 503 and response.json()['code'] == 'media_dependencies_missing', response.text
        print('CONFIRMED: health webrtc=true but offer returns media_dependencies_missing')
        response = client.post(f'/internal/v1/sessions/{sid}/close', headers=headers, json={'expected_executor_id': identity})
        assert response.status_code == 200 and response.json()['cleanup_pending'] is False
        print('PASS: explicit close confirms journal/media cleanup')
        rid = str(uuid4())
        response = client.post(f'/internal/v1/runs/{rid}/cancel', headers=headers, json={'expected_executor_id': identity})
        assert response.status_code == 200 and response.json()['never_started'] is True
        print('PASS: absent run cancellation creates a durable refusal fence')
    with TestClient(app, raise_server_exceptions=False) as client:
        health = client.get('/internal/v1/health', headers=headers).json()
        assert health['executor_id'] == identity
        response = client.post(f'/internal/v1/runs/{rid}/cancel', headers=headers, json={'expected_executor_id': identity})
        assert response.status_code == 200 and response.json()['never_started'] is True
        print('PASS: executor identity and refusal fence survive restart')
