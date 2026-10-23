import assert from 'node:assert/strict';
import test from 'node:test';
import { execFileSync, spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { chmod, copyFile, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Independent local server: restarting it never interrupts the shared browser fixture.
test('real old-backup restoration and idempotency across a new-server restart', { timeout: 120_000 }, async t => {
    const rootFixture = JSON.parse(await readFile(process.env.NODE_ASSETS_E2E_FIXTURE_FILE, 'utf8'));
    assert.equal(rootFixture.isolated, true);
    const base = dirname(process.env.NODE_ASSETS_E2E_FIXTURE_FILE);
    const directory = await mkdtemp(join(base, 'recovery-'));
    const helper = fileURLToPath(new URL('./node-instance-binding-fixture.py', import.meta.url));
    const oldBinary = join(base, 'pre-binding-easytier-web');
    const newBinary = join(rootFixture.runDir, 'easytier-web');
    const fixturePath = join(directory, 'fixture.json');
    const callHelper = (action, extra = []) => execFileSync('python3', [helper, action, '--fixture', fixturePath, ...extra], { stdio: ['ignore', 'pipe', 'pipe'] });
    execFileSync('python3', [helper, 'prepare', '--directory', directory, '--old-binary', oldBinary], { stdio: ['ignore', 'pipe', 'pipe'] });
    const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));
    assert.notEqual(fixture.baseURL, rootFixture.baseURL);
    const backup = join(directory, 'pre-upgrade-backup.db');
    execFileSync('python3', ['-c', 'import sqlite3,sys; source=sqlite3.connect(sys.argv[1]); target=sqlite3.connect(sys.argv[2]); source.backup(target); target.close();source.close()', fixture.database, backup]);
    await chmod(backup, 0o600);
    const scope = { owner_user_id: fixture.owner.id, machine_id: fixture.apiNode.machine_id };
    let cookie = '', oldChild;
    const instance = name => fixture.instances.apiNode[name].instance_id;
    async function request(method, path, body, expected = 200) {
        const url = new URL('/api/v1' + path, fixture.baseURL);
        for (const [key, value] of Object.entries(scope)) url.searchParams.set(key, value);
        const response = await fetch(url, { method, headers: { cookie, 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
        const text = await response.text();
        assert.equal(response.status, expected, `${method} ${path}: ${text}`);
        return text ? JSON.parse(text) : null;
    }
    async function login() {
        const response = await fetch(fixture.baseURL + '/api/v1/auth/login', { method: 'POST', headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ username: fixture.owner.username, password: createHash('md5').update(fixture.owner.password).digest('hex') }) });
        assert.equal(response.status, 200);
        cookie = response.headers.getSetCookie().map(item => item.split(';')[0]).join('; ');
    }
    const replayCases = [];
    async function remember(method, path, input) {
        const response = await request(method, path, input);
        replayCases.push({ method, path, input, response });
        return response;
    }
    async function binding(kind, id, name, operation, extra = {}) {
        const preview = await request('POST', '/assets/validation/binding', { subject: { resource_kind: kind, id }, operation, instance_id: instance(name), ...extra });
        assert.equal(preview.can_save, true);
        return remember('PUT', `/assets/${kind === 'gateway' ? 'gateway-groups' : 'devices'}/${id}/binding`,
            { request_id: randomUUID(), operation, instance_id: instance(name), preview_version: preview.preview_version, ...extra });
    }
    t.after(async () => {
        if (oldChild && oldChild.exitCode === null) {
            oldChild.kill('SIGINT');
            await new Promise(resolve => oldChild.once('exit', resolve));
        }
        callHelper('stop');
    });
    callHelper('start', ['--new-binary', newBinary]);
    await login();
    const initialUpgrade = JSON.parse(await readFile(join(fixture.runDir, 'migration-result.json'), 'utf8'));
    assert.equal(initialUpgrade.passed, true);

    const gatewayPreview = await request('POST', '/assets/validation/binding',
        { subject: { resource_kind: 'gateway', id: null }, operation: 'attach', instance_id: instance('mapped') });
    const group = await remember('POST', '/assets/gateway-groups', { request_id: randomUUID(), name: 'Restart gateway', remarks: null,
        binding: { instance_id: instance('mapped') }, preview_version: gatewayPreview.preview_version });
    const spec = { mode: 'gateway_rule', gateway_group_id: group.id, rule_key: group.binding.snapshot.rules[0].rule_key, host_offset: 33 };
    const address = await request('POST', '/assets/validation/instance-address', { subject: { resource_kind: 'device', id: null }, address_spec: spec });
    const device = await remember('POST', '/assets/devices', { request_id: randomUUID(), name: 'Restart device', model: null, remarks: null,
        device_type_id: null, address_spec: spec, preview_version: address.preview_version });
    const service = await remember('POST', `/assets/devices/${device.id}/services`, { request_id: randomUUID(), name: 'Restart service', port: 9080,
        protocols: ['tcp', 'udp'], remarks: null, access_url: null, icon: null });
    await binding('device', fixture.legacy.apiNode.direct, 'ordinary', 'attach', { lan_ip: null });
    await binding('gateway', fixture.legacy.apiNode.group, 'legacy', 'attach');
    await binding('gateway', group.id, 'mapped', 'refresh');
    const newSpec = { ...spec, host_offset: 34 };
    const corrected = await request('POST', '/assets/validation/instance-address', { subject: { resource_kind: 'device', id: device.id }, address_spec: newSpec });
    const savedAddress = await remember('PUT', `/assets/devices/${device.id}/address`,
        { request_id: randomUUID(), address_spec: newSpec, preview_version: corrected.preview_version });
    // A changed source after the accepted writes must not invalidate a committed retry.
    execFileSync('python3', [helper, 'mutate', '--fixture', fixturePath], { input: JSON.stringify({ node: 'apiNode', instance: 'mapped', action: 'patch', patch: { virtual_ipv4: '10.70.0.77' } }), stdio: ['pipe', 'pipe', 'pipe'] });
    callHelper('stop');
    callHelper('start', ['--new-binary', newBinary]);
    await login();
    for (const item of replayCases) assert.deepEqual(await request(item.method, item.path, item.input), item.response);
    const current = await request('GET', `/assets/devices/${device.id}`);
    assert.equal(current.overlay_ip, savedAddress.overlay_ip); assert.equal(current.lan_ip, savedAddress.lan_ip);
    const services = await request('GET', `/assets/devices/${device.id}/services`);
    assert.equal(services.items.length, 1); assert.equal(services.items[0].id, service.id);
    const migrationCount = Number(execFileSync('python3', ['-c', 'import sqlite3,sys; print(sqlite3.connect(sys.argv[1]).execute("SELECT COUNT(*) FROM seaql_migrations").fetchone()[0])', fixture.database], { encoding: 'utf8' }).trim());
    assert.equal(migrationCount, initialUpgrade.newMigrationCount);
    callHelper('stop');

    // Restore the complete old database + old binary pair, not a down migration of the new DB.
    await copyFile(backup, fixture.database);
    oldChild = spawn(oldBinary, fixture.serverArgs, { stdio: 'ignore' });
    const deadline = Date.now() + 25_000;
    while (true) {
        assert.equal(oldChild.exitCode, null, 'Restored old server must stay running');
        try { if ((await fetch(fixture.baseURL)).status === 200) break; } catch { /* startup */ }
        assert.ok(Date.now() < deadline, 'Restored old server startup');
        await new Promise(resolve => setTimeout(resolve, 100));
    }
    await login();
    const legacy = await request('GET', `/assets/devices/${fixture.legacy.apiNode.direct}`);
    assert.equal(legacy.overlay_ip, 'fd00::20'); assert.equal(legacy.lan_ip, 'fd80::20');
    const originalServices = await request('GET', `/assets/devices/${fixture.legacy.apiNode.child}/services`);
    assert.equal(originalServices.items[0].id, fixture.legacy.apiNode.service);
    const restoreDetails = JSON.parse(execFileSync('python3', ['-c',
        'import sqlite3,json,sys; con=sqlite3.connect(sys.argv[1]); before=json.load(open(sys.argv[2])); checks={};\nfor table,saved in before["tables"].items():\n rows=con.execute("SELECT "+",".join(saved["columns"])+" FROM "+table+" ORDER BY rowid").fetchall(); checks[table]=[list(row) for row in rows]==saved["rows"]\nassert all(checks.values()); print(json.dumps({"tables":checks,"migrationCount":con.execute("SELECT COUNT(*) FROM seaql_migrations").fetchone()[0],"foreignKeyViolations":con.execute("PRAGMA foreign_key_check").fetchall()}))',
        fixture.database, join(fixture.runDir, 'before-upgrade.json')], { encoding: 'utf8' }));
    assert.equal(restoreDetails.migrationCount, initialUpgrade.oldMigrationCount);
    assert.deepEqual(restoreDetails.foreignKeyViolations, []);
    await writeFile(join(base, 'recovery-result.json'), JSON.stringify({ passed: true, isolated: true, beforeMigrationCount: initialUpgrade.oldMigrationCount,
        afterMigrationCount: migrationCount, sameRequestReplaysAcrossRestart: replayCases.length, retainedSavedAddressAndService: true,
        restoredOldServerHttpLogin: true, restoredLegacyIPv6AndService: true, restored: restoreDetails }, null, 2), { mode: 0o600 });
});
