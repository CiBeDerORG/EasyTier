#!/usr/bin/env python3
"""Disposable local fixture only: real old-binary migration, synthetic accounts and configs.

Never point this helper at an existing deployment database. Fixture credentials and
SQLite files are written outside the repository with private permissions.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.request
import uuid

STAMP = '2026-10-23T00:00:00+00:00'
PASSWORD = 'e2e-password'  # Public fixture password, never used on a real deployment.
PASSWORD_HASH = '$argon2id$v=19$m=4096,t=3,p=1$ZWFzeXRpZXItZTJlLXNhbHQ$FHbDjVElaTXuArgEEQrQrO4AdsnqNOcXGA5PFY8cG/s'


def free_port(preferred=0):
    with socket.socket() as sock:
        try:
            sock.bind(('127.0.0.1', preferred))
        except OSError:
            sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def write_json(path, data):
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2))
    os.chmod(path, 0o600)


def wait_ready(child, url):
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        if child.poll() is not None:
            raise RuntimeError('Isolated server exited; inspect its local log.')
        try:
            with opener.open(url, timeout=1) as result:
                if result.status == 200:
                    return
        except Exception:
            time.sleep(.1)
    raise RuntimeError('Isolated server startup timed out.')


def stop_child(child):
    child.send_signal(signal.SIGINT)
    try:
        child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.terminate()
        child.wait(timeout=5)


def read_fixture(path):
    path = path.resolve()
    fixture = json.loads(path.read_text())
    if fixture.get('isolated') is not True or not fixture['baseURL'].startswith('http://127.0.0.1:'):
        raise RuntimeError('Only a disposable loopback fixture is accepted.')
    database = Path(fixture['database']).resolve()
    if not database.is_relative_to(path.parent) or database.name != 'test.db':
        raise RuntimeError('Fixture database must be its private local test.db.')
    return fixture


def config(instance_id, name, ipv4, rules, **extra):
    body = {key: [] for key in ('peer_urls', 'proxy_cidrs', 'listener_urls', 'relay_network_whitelist',
                               'routes', 'exit_nodes', 'mapped_listeners', 'port_forwards', 'peers', 'managed_credentials')}
    body.update(instance_id=instance_id, network_name=name, hostname='fixture', dhcp=False,
                virtual_ipv4=ipv4, network_length=24, proxy_cidrs=rules,
                network_secret='fixture-secret-must-not-appear-in-asset-responses', **extra)
    return body


def prepare(args):
    base = args.directory.resolve()
    base.mkdir(parents=True, exist_ok=True)
    os.chmod(base, 0o700)
    fixture_path = base / 'fixture.json'
    if fixture_path.exists():
        raise RuntimeError('Fixture already exists; use a new private directory.')
    run = Path(tempfile.mkdtemp(prefix='run-', dir=base))
    api_port, config_port = free_port(53087), free_port()
    db = run / 'test.db'
    server_args = ['--db', str(db), '--api-server-addr', '127.0.0.1', '--api-server-port', str(api_port),
                   '--config-server-protocol', 'tcp', '--config-server-port', str(config_port),
                   '--disable-registration', '--console-log-level', 'warn']
    with (run / 'old-migration.log').open('wb') as log:
        child = subprocess.Popen([str(args.old_binary.resolve()), *server_args], stdout=log, stderr=subprocess.STDOUT)
        try:
            wait_ready(child, f'http://127.0.0.1:{api_port}/')
        finally:
            if child.poll() is None:
                stop_child(child)
    connection = sqlite3.connect(db)
    connection.execute('PRAGMA foreign_keys=ON')
    assert connection.execute("SELECT COUNT(*) FROM seaql_migrations WHERE version='m20261023_000012_node_assets'").fetchone()[0] == 1
    assert connection.execute("SELECT COUNT(*) FROM seaql_migrations WHERE version='m20261023_000013_asset_instance_bindings'").fetchone()[0] == 0
    users = {}
    for role in ('admin', 'owner', 'other'):
        name = 'binding-qa-' + role
        user_id = connection.execute('INSERT INTO users(username,password) VALUES(?,?)', (name, PASSWORD_HASH)).lastrowid
        group_id = connection.execute('SELECT id FROM groups WHERE name=?', ('admins' if role == 'admin' else 'users',)).fetchone()[0]
        connection.execute('INSERT INTO users_groups(user_id,group_id) VALUES(?,?)', (user_id, group_id))
        users[role] = {'id': user_id, 'username': name, 'password': PASSWORD}
    owner = users['owner']['id']
    nodes, instances = {}, {}
    definitions = [
        ('ordinary', 'Direct fixture', '10.70.0.10', []),
        ('ordinary2', 'Direct fixture', '10.70.0.11', []),
        ('proxy', 'Proxy fixture', '10.70.0.1', ['192.168.50.0/24']),
        ('mapped', 'Mapped fixture', '10.70.0.2', ['192.168.60.0/24->10.60.0.0/24']),
        ('mapped2', 'Mapped fixture', '10.70.0.3', ['192.168.70.0/24->10.71.0.0/24']),
        ('legacy', 'Legacy attach fixture', '10.70.0.4', ['192.168.80.0/24->10.80.0.0/24']),
    ]
    for index, (key, label) in enumerate((('node', 'OpenWrt binding demo'), ('offlineNode', 'Offline binding demo'), ('apiNode', 'API binding fixture')), 1):
        machine = f'20000000-0000-4000-8000-{index:012d}'
        connection.execute('INSERT INTO devices(user_id,machine_id,hostname,alias,easytier_version,device_os,client_url,first_seen_time,last_seen_time,site_location,device_model,connection_ip,ip_country,ip_location_status) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)',
                           (owner, machine, label, '', '2.6.4', '{}', 'tcp://198.51.100.20:22020', STAMP, STAMP, 'Fixture location', 'Fixture model', '198.51.100.20', 'Fixture region', 'resolved'))
        nodes[key] = {'machine_id': machine, 'label': label}
        instances[key] = {}
        for name, label, ip, rules in definitions:
            instance_id = str(uuid.uuid5(uuid.NAMESPACE_URL, machine + ':' + name))
            body = config(instance_id, label, ip, rules)
            disabled = name == 'mapped2'
            connection.execute('INSERT INTO user_running_network_configs(user_id,device_id,network_instance_id,network_config,source,disabled,create_time,update_time) VALUES(?,?,?,?,?,?,?,?)',
                               (owner, machine, instance_id, json.dumps(body), 'web', disabled, STAMP, STAMP))
            instances[key][name] = {'instance_id': instance_id, 'name': label, 'ipv4': ip, 'rules': rules, 'disabled': disabled}
    # Cross-tenant same machine/instance identifiers must remain scoped.
    machine = nodes['apiNode']['machine_id']
    connection.execute('INSERT INTO devices(user_id,machine_id,hostname,first_seen_time,last_seen_time) VALUES(?,?,?,?,?)',
                       (users['other']['id'], machine, 'Other owner node', STAMP, STAMP))
    shared = instances['apiNode']['ordinary']['instance_id']
    connection.execute('INSERT INTO user_running_network_configs(user_id,device_id,network_instance_id,network_config,source,disabled,create_time,update_time) VALUES(?,?,?,?,?,?,?,?)',
                       (users['other']['id'], machine, shared, json.dumps(config(shared, 'Other private fixture', '10.99.0.99', [])), 'web', False, STAMP, STAMP))
    # Old-format records are inserted before upgrading, not accepted through a new-create bypass.
    legacy = {}
    for node_key in ('apiNode', 'node'):
        node = nodes[node_key]['machine_id']
        ids = {name: str(uuid.uuid4()) for name in ('group', 'direct', 'child', 'self', 'service')}
        connection.execute('INSERT INTO asset_gateway_groups(id,owner_user_id,machine_id,name,remarks,created_at,updated_at) VALUES(?,?,?,?,?,?,?)',
                           (ids['group'], owner, node, 'Legacy gateway fixture', 'Pre-binding migration fixture', STAMP, STAMP))
        for subnet in ('192.168.80.0/24', 'fd80::/64'):
            connection.execute('INSERT INTO asset_gateway_subnets(group_id,owner_user_id,machine_id,cidr) VALUES(?,?,?,?)', (ids['group'], owner, node, subnet))
        for name, group, overlay, lan, is_self in (
            ('direct', None, 'fd00::20', 'fd80::20', False),
            ('child', ids['group'], '10.80.0.21', '192.168.80.21', False),
            ('self', ids['group'], '10.80.0.1', '192.168.80.1', True),
        ):
            connection.execute('INSERT INTO asset_devices(id,owner_user_id,machine_id,gateway_group_id,name,model,overlay_ip,lan_ip,is_gateway_self,remarks,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)',
                               (ids[name], owner, node, group, 'Legacy ' + name + ' fixture', 'Legacy model', overlay, lan, is_self, 'Migrated record', STAMP, STAMP))
        connection.execute('INSERT INTO asset_services(id,owner_user_id,machine_id,device_id,name,port,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)',
                           (ids['service'], owner, node, ids['child'], 'Legacy service fixture', 8443, STAMP, STAMP))
        for protocol in ('tcp', 'udp'):
            connection.execute('INSERT INTO asset_service_bindings(service_id,owner_user_id,machine_id,device_id,protocol,port) VALUES(?,?,?,?,?,?)',
                               (ids['service'], owner, node, ids['child'], protocol, 8443))
        legacy[node_key] = ids
    connection.commit()
    tables = ('devices', 'asset_gateway_groups', 'asset_gateway_subnets', 'asset_devices', 'asset_services', 'asset_service_bindings')
    before = {'migration_count': connection.execute('SELECT COUNT(*) FROM seaql_migrations').fetchone()[0], 'tables': {}}
    for table in tables:
        cursor = connection.execute(f'SELECT * FROM {table} ORDER BY rowid')
        before['tables'][table] = {'columns': [column[0] for column in cursor.description], 'rows': cursor.fetchall()}
    connection.close()
    os.chmod(db, 0o600)
    write_json(run / 'before-upgrade.json', before)
    fixture = {'isolated': True, 'baseURL': f'http://127.0.0.1:{api_port}', **users, **nodes,
               'instances': instances, 'legacy': legacy, 'screenshotsDir': str(base / 'screenshots'),
               'database': str(db), 'configPort': config_port, 'runDir': str(run), 'serverArgs': server_args}
    write_json(fixture_path, fixture)
    print(json.dumps({'fixture': str(fixture_path), 'baseURL': fixture['baseURL'], 'oldMigrationCount': before['migration_count'], 'legacyDevices': 6}))


def start(args):
    f = read_fixture(args.fixture)
    run = Path(f['runDir'])
    binary = run / 'easytier-web'
    if (run / 'server.pid').exists():
        raise RuntimeError('Stop the recorded isolated server before replacing its binary.')
    shutil.copy2(args.new_binary.resolve(), binary)
    with (run / 'server.log').open('ab') as log:
        child = subprocess.Popen([str(binary), *f['serverArgs']], stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    (run / 'server.pid').write_text(str(child.pid))
    wait_ready(child, f['baseURL'] + '/')
    con = sqlite3.connect(f['database'])
    assert con.execute("SELECT COUNT(*) FROM seaql_migrations WHERE version='m20261023_000013_asset_instance_bindings'").fetchone()[0] == 1
    violations = con.execute('PRAGMA foreign_key_check').fetchall()
    assert not violations
    # A later binary refresh must not overwrite evidence from the first real upgrade.
    if (run / 'migration-result.json').exists():
        con.close()
        print(json.dumps({'restarted': True, 'foreignKeyViolations': violations}))
        return
    before = json.loads((run / 'before-upgrade.json').read_text())
    for table, saved in before['tables'].items():
        columns = ','.join(saved['columns'])
        rows = con.execute(f'SELECT {columns} FROM {table} ORDER BY rowid').fetchall()
        assert [list(row) for row in rows] == saved['rows'], table + ' changed during upgrade'
    assert con.execute("SELECT COUNT(*) FROM asset_devices WHERE address_mode!='legacy_manual' OR network_instance_id IS NOT NULL").fetchone()[0] == 0
    result = {'passed': True, 'preservedTables': list(before['tables']), 'legacyDevices': con.execute('SELECT COUNT(*) FROM asset_devices').fetchone()[0],
              'oldMigrationCount': before['migration_count'], 'newMigrationCount': con.execute('SELECT COUNT(*) FROM seaql_migrations').fetchone()[0], 'foreignKeyViolations': violations}
    con.close()
    write_json(run / 'migration-result.json', result)
    print(json.dumps(result))


def stop(args):
    f = read_fixture(args.fixture)
    run = Path(f['runDir'])
    pid_path = run / 'server.pid'
    if not pid_path.exists():
        return
    pid = int(pid_path.read_text())
    exe = Path(f'/proc/{pid}/exe')
    if exe.exists() and exe.resolve() != (run / 'easytier-web').resolve():
        raise RuntimeError('PID belongs to a different binary; refusing to signal it.')
    if exe.exists():
        os.kill(pid, signal.SIGINT)
        deadline = time.monotonic() + 10
        while Path(f'/proc/{pid}').exists() and time.monotonic() < deadline:
            time.sleep(.1)
        if Path(f'/proc/{pid}').exists():
            os.kill(pid, signal.SIGTERM)
    pid_path.unlink()
    print(json.dumps({'stopped': True}))


def mutate(args):
    f = read_fixture(args.fixture)
    change = json.loads(sys.stdin.read())
    node_key = change.get('node', 'apiNode')
    if node_key not in f['instances']:
        raise ValueError('Unknown fixture node')
    owner = f['owner']['id']
    node = f[node_key]['machine_id']
    instance = f['instances'][node_key][change['instance']]['instance_id']
    con = sqlite3.connect(f['database'])
    con.execute('PRAGMA foreign_keys=ON')
    row = con.execute('SELECT id,network_config FROM user_running_network_configs WHERE user_id=? AND device_id=? AND network_instance_id=?', (owner, node, instance)).fetchone()
    action = change['action']
    if action == 'patch':
        body = json.loads(row[1]); body.update(change['patch'])
        con.execute('UPDATE user_running_network_configs SET network_config=? WHERE id=?', (json.dumps(body), row[0]))
    elif action == 'delete':
        con.execute('DELETE FROM user_running_network_configs WHERE user_id=? AND device_id=? AND network_instance_id=?', (owner, node, instance))
    elif action == 'recreate':
        baseline = f['instances'][node_key][change['instance']]
        con.execute('DELETE FROM user_running_network_configs WHERE user_id=? AND device_id=? AND network_instance_id=?', (owner, node, instance))
        body = config(instance, baseline['name'], baseline['ipv4'], baseline['rules'])
        con.execute('INSERT INTO user_running_network_configs(user_id,device_id,network_instance_id,network_config,source,disabled,create_time,update_time) VALUES(?,?,?,?,?,?,?,?)',
                    (owner, node, instance, json.dumps(body), 'web', baseline['disabled'], '2026-10-23T01:00:00+00:00', STAMP))
    else:
        raise ValueError('Unsupported fixture mutation')
    con.commit();con.close()
    print(json.dumps({'mutated': action, 'node': node_key, 'instance': change['instance']}))


def reset_api(args):
    """Restore only the reserved API node; concurrent browser fixture nodes are untouched."""
    f = read_fixture(args.fixture)
    owner, node = f['owner']['id'], f['apiNode']['machine_id']
    con = sqlite3.connect(f['database'])
    con.execute('PRAGMA foreign_keys=ON')
    con.execute('DELETE FROM asset_devices WHERE owner_user_id=? AND machine_id=?', (owner, node))
    con.execute('DELETE FROM asset_gateway_groups WHERE owner_user_id=? AND machine_id=?', (owner, node))
    before = json.loads((Path(f['runDir']) / 'before-upgrade.json').read_text())
    for table in ('asset_gateway_groups', 'asset_gateway_subnets', 'asset_devices', 'asset_services', 'asset_service_bindings'):
        saved = before['tables'][table]
        columns = saved['columns']
        for values in saved['rows']:
            row = dict(zip(columns, values))
            if row['owner_user_id'] == owner and row['machine_id'] == node:
                con.execute(f'INSERT INTO {table}({",".join(columns)}) VALUES({",".join("?" for _ in columns)})', values)
    for baseline in f['instances']['apiNode'].values():
        body = config(baseline['instance_id'], baseline['name'], baseline['ipv4'], baseline['rules'])
        con.execute('DELETE FROM user_running_network_configs WHERE user_id=? AND device_id=? AND network_instance_id=?', (owner, node, baseline['instance_id']))
        con.execute('INSERT INTO user_running_network_configs(user_id,device_id,network_instance_id,network_config,source,disabled,create_time,update_time) VALUES(?,?,?,?,?,?,?,?)',
                    (owner, node, baseline['instance_id'], json.dumps(body), 'web', baseline['disabled'], STAMP, STAMP))
    con.commit(); con.close()
    print(json.dumps({'restored': 'apiNode'}))


parser = argparse.ArgumentParser()
parser.add_argument('action', choices=['prepare', 'start', 'stop', 'mutate', 'reset-api'])
parser.add_argument('--directory', type=Path)
parser.add_argument('--fixture', type=Path)
parser.add_argument('--old-binary', type=Path)
parser.add_argument('--new-binary', type=Path)
args = parser.parse_args()
if args.action == 'prepare':
    if not args.directory or not args.old_binary: parser.error('prepare needs --directory and --old-binary')
elif not args.fixture:
    parser.error('a private --fixture is required')
if args.action == 'start' and not args.new_binary: parser.error('start needs --new-binary')
{'prepare': prepare, 'start': start, 'stop': stop, 'mutate': mutate, 'reset-api': reset_api}[args.action](args)
