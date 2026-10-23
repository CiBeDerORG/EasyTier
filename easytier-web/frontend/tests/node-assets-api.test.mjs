import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { test } from 'node:test';

// This suite must only target an explicitly created disposable local fixture.
const fixturePath = process.env.NODE_ASSETS_E2E_FIXTURE_FILE;
assert.ok(fixturePath, 'NODE_ASSETS_E2E_FIXTURE_FILE is required');
const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));
assert.equal(fixture.isolated, true, 'Only disposable test fixtures are supported');
assert.equal(new URL(fixture.baseURL).hostname, '127.0.0.1');
const scope = { owner_user_id: fixture.owner.id, machine_id: fixture.apiNode.machine_id };
let admin, owner, outsider, group, device, type, service, createBody;

async function request(cookie, method, path, query = {}, body, expected = 200) {
    const url = new URL(`/api/v1${path}`, fixture.baseURL);
    for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const response = await fetch(url, {
        method, headers: { ...(cookie ? { cookie } : {}), ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    assert.equal(response.status, expected, `${method} ${url.pathname}: ${text}`);
    return text ? JSON.parse(text) : null;
}
async function login(user) {
    const response = await fetch(`${fixture.baseURL}/api/v1/auth/login`, {
        method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: user.username, password: createHash('md5').update(user.password).digest('hex') }),
    });
    assert.equal(response.status, 200, `login ${user.username}`);
    return response.headers.getSetCookie().map(value => value.split(';')[0]).join('; ');
}
const fresh = body => ({ ...body, request_id: randomUUID() });
const deviceInput = overrides => ({ name: 'API 演示设备', model: null, overlay_ip: 'fd00::10', lan_ip: null,
    gateway_group_id: group.id, device_type_id: type.id, is_gateway_self: false, remarks: null, ...overrides });

test('real asset API with isolated session, SQLite, and registered nodes', { timeout: 60000 }, async t => {
    await t.test('session JSON errors and exact administrator authorization', async () => {
        const error = await request(null, 'GET', '/assets/context', {}, undefined, 401);
        assert.equal(error.code, 'authentication_required');
        [admin, owner, outsider] = await Promise.all([login(fixture.admin), login(fixture.owner), login(fixture.other)]);
        assert.equal((await request(admin, 'GET', '/assets/context')).can_manage_others, true);
        assert.equal((await request(owner, 'GET', '/assets/context')).can_manage_others, false);
        await request(owner, 'GET', '/assets/accounts', {}, undefined, 403);
        await request(outsider, 'GET', '/assets/nodes', { owner_user_id: scope.owner_user_id }, undefined, 403);
        const accounts = await request(admin, 'GET', '/assets/accounts');
        assert.ok(accounts.items.some(item => item.id === fixture.owner.id));
        const nodes = await request(owner, 'GET', '/assets/nodes', { owner_user_id: scope.owner_user_id });
        assert.ok(nodes.items.some(node => node.machine_id === scope.machine_id));
    });
    await t.test('node properties keep source observations and persist public role', async () => {
        const before = (await request(owner, 'GET', '/assets/nodes', { owner_user_id: scope.owner_user_id })).items.find(node => node.machine_id === scope.machine_id);
        const node = await request(admin, 'PATCH', `/assets/nodes/${scope.machine_id}/metadata`, { owner_user_id: scope.owner_user_id },
            { site_location: '测试机柜', device_model: 'OpenWrt 测试型号', is_public_server: true });
        assert.equal(node.site_location, '测试机柜');
        assert.equal(node.is_public_server, true);
        assert.equal(node.connection_ip, before.connection_ip, 'Manual edits must preserve the observed source IP');
        await request(owner, 'PATCH', `/assets/nodes/${scope.machine_id}/metadata`, { owner_user_id: scope.owner_user_id },
            { site_location: null, device_model: null, is_public_server: false, connection_ip: '8.8.8.8' }, 422);
    });
    await t.test('type initialization and standard gateway subnets', async () => {
        const a = await request(admin, 'POST', '/assets/device-types/initialize', { owner_user_id: scope.owner_user_id }, {});
        const b = await request(admin, 'POST', '/assets/device-types/initialize', { owner_user_id: scope.owner_user_id }, {});
        assert.deepEqual(a.items.map(x => x.id).sort(), b.items.map(x => x.id).sort());
        type = await request(admin, 'POST', '/assets/device-types', { owner_user_id: scope.owner_user_id }, fresh({ name: 'API 测试类型' }));
        await request(owner, 'POST', '/assets/gateway-groups', scope, fresh({ name: '非标准网段', subnets: ['192.168.1.7/24'], remarks: null }), 422);
        group = await request(owner, 'POST', '/assets/gateway-groups', scope, fresh({ name: 'API 测试网关组', subnets: ['192.168.1.0/24', 'fd10::/64'], remarks: null }));
        const duplicate = await request(owner, 'POST', '/assets/gateway-groups', scope,
            fresh({ name: group.name, subnets: ['192.168.1.0/24'], remarks: null }));
        await request(owner, 'DELETE', `/assets/gateway-groups/${duplicate.id}`, scope, undefined, 204);
        const blocked = await request(admin, 'DELETE', `/assets/nodes/${scope.machine_id}`, { owner_user_id: scope.owner_user_id }, undefined, 409);
        assert.ok(blocked.code);
    });
    await t.test('idempotent create, address warning, ownership and reference guards', async () => {
        createBody = fresh(deviceInput({ lan_ip: '192.168.2.10', is_gateway_self: true }));
        device = await request(owner, 'POST', '/assets/devices', scope, createBody);
        assert.equal(device.warnings[0].code, 'lan_outside_gateway_subnets');
        const replay = await request(owner, 'POST', '/assets/devices', scope, createBody);
        assert.equal(replay.id, device.id);
        await request(owner, 'POST', '/assets/devices', scope, { ...createBody, name: 'changed' }, 409);
        await request(outsider, 'GET', `/assets/devices/${device.id}`, scope, undefined, 403);
        await request(owner, 'GET', `/assets/devices/${device.id}`, { ...scope, machine_id: fixture.offlineNode.machine_id }, undefined, 404);
        await request(owner, 'PUT', `/assets/devices/${device.id}`, scope, deviceInput({ gateway_group_id: null }), 409);
        await request(owner, 'POST', '/assets/devices', scope, fresh(deviceInput({ lan_ip: '192.168.2.10' })), 409);
        await request(owner, 'POST', '/assets/devices', scope, fresh(deviceInput({ is_gateway_self: true })), 409);
        await request(owner, 'DELETE', `/assets/gateway-groups/${group.id}`, scope, undefined, 409);
        await request(admin, 'DELETE', `/assets/device-types/${type.id}`, { owner_user_id: scope.owner_user_id }, undefined, 409);
        const counts = await request(owner, 'GET', '/assets/device-counts', { owner_user_id: scope.owner_user_id });
        assert.equal(counts.items.find(row => row.machine_id === scope.machine_id).device_count, 1);
    });
    await t.test('dual protocol atomic port conflicts and safe service fields', async () => {
        const input = { name: 'API 服务', protocols: ['tcp', 'udp'], port: 8443, remarks: null, access_url: 'https://example.com/app', icon: 'server' };
        service = await request(owner, 'POST', `/assets/devices/${device.id}/services`, scope, fresh(input));
        await request(owner, 'POST', `/assets/devices/${device.id}/services`, scope, fresh({ ...input, protocols: ['udp'] }), 409);
        await request(owner, 'POST', `/assets/devices/${device.id}/services`, scope, fresh({ ...input, port: 8081, access_url: 'javascript:alert(1)' }), 422);
        await request(owner, 'POST', `/assets/devices/${device.id}/services`, scope, fresh({ ...input, port: 8081, icon: 'https://example.com/icon.png' }), 422);
        await request(owner, 'PUT', `/assets/services/${service.id}`, scope, { ...input, protocols: ['tcp'] });
        const udp = await request(owner, 'POST', `/assets/devices/${device.id}/services`, scope, fresh({ ...input, protocols: ['udp'] }));
        assert.notEqual(udp.id, service.id);
        await request(owner, 'PUT', `/assets/services/${service.id}`, scope, input, 409);
        const listed = await request(owner, 'GET', `/assets/devices/${device.id}/services`, scope);
        assert.equal(listed.items.length, 2);
        assert.deepEqual(listed.items.find(x => x.id === service.id).protocols, ['tcp']);
    });
    await t.test('query filtering and locating use the requested scope', async () => {
        const filtered = await request(owner, 'GET', '/assets/devices', { ...scope, group_id: group.id, type_id: type.id, keyword: 'API', page_size: 1, locate_id: device.id });
        assert.equal(filtered.total, 1);
        assert.equal(filtered.items[0].id, device.id);
        const literal = await request(owner, 'GET', '/assets/devices', { ...scope, keyword: '%' });
        assert.equal(literal.total, 0);
    });
    await t.test('device deletion cascades every current service and replay remains gone', async () => {
        await request(owner, 'DELETE', `/assets/devices/${device.id}`, scope, undefined, 204);
        await request(owner, 'GET', `/assets/devices/${device.id}/services`, scope, undefined, 404);
        const gone = await request(owner, 'POST', '/assets/devices', scope, createBody, 410);
        assert.equal(gone.code, 'created_resource_deleted');
        await request(owner, 'DELETE', `/assets/gateway-groups/${group.id}`, scope, undefined, 204);
        await request(admin, 'DELETE', `/assets/device-types/${type.id}`, { owner_user_id: scope.owner_user_id }, undefined, 204);
        const counts = await request(owner, 'GET', '/assets/device-counts', { owner_user_id: scope.owner_user_id });
        assert.equal(counts.items.find(row => row.machine_id === scope.machine_id).device_count, 0);
        const machines = await request(owner, 'GET', '/machines');
        assert.ok(machines.machines.some(machine => machine.info.hostname === fixture.apiNode.label));
    });
});
