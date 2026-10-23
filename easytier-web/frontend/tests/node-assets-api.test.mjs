import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, scope, fresh, instance, request, login, createGroup, createDevice, addressPreview } from './node-assets-api-support.mjs';

let admin, owner, outsider, group, device, type, service, createBody;
const metadata = overrides => ({ name: 'API 演示设备', model: null, remarks: null, device_type_id: type.id, ...overrides });

test('real asset API with isolated sessions, SQLite and bound device registration', { timeout: 60000 }, async t => {
    const createdDevices = [];
    const createdGroups = [];
    const createdTypes = [];
    t.after(async () => {
        for (const id of createdDevices.reverse()) {
            const response = await fetch(`${fixture.baseURL}/api/v1/assets/devices/${id}?${new URLSearchParams(scope)}`, { method: 'DELETE', headers: { cookie: owner } });
            assert.ok([204, 404].includes(response.status), 'Fixture device cleanup');
        }
        for (const id of createdGroups.reverse()) {
            const response = await fetch(`${fixture.baseURL}/api/v1/assets/gateway-groups/${id}?${new URLSearchParams(scope)}`, { method: 'DELETE', headers: { cookie: owner } });
            assert.ok([204, 404].includes(response.status), 'Fixture group cleanup');
        }
        for (const id of createdTypes.reverse()) {
            const response = await fetch(`${fixture.baseURL}/api/v1/assets/device-types/${id}?owner_user_id=${scope.owner_user_id}`, { method: 'DELETE', headers: { cookie: owner } });
            assert.ok([204, 404].includes(response.status), 'Fixture type cleanup');
        }
    });
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
        assert.equal(node.connection_ip, before.connection_ip);
        await request(owner, 'PATCH', `/assets/nodes/${scope.machine_id}/metadata`, { owner_user_id: scope.owner_user_id },
            { site_location: null, device_model: null, is_public_server: false, connection_ip: '8.8.8.8' }, 422);
    });
    await t.test('type initialization and instance-bound gateways preserve name and deletion rules', async () => {
        const a = await request(admin, 'POST', '/assets/device-types/initialize', { owner_user_id: scope.owner_user_id }, {});
        const b = await request(admin, 'POST', '/assets/device-types/initialize', { owner_user_id: scope.owner_user_id }, {});
        assert.deepEqual(a.items.map(x => x.id).sort(), b.items.map(x => x.id).sort());
        type = await request(admin, 'POST', '/assets/device-types', { owner_user_id: scope.owner_user_id }, fresh({ name: 'API 测试类型' }));
        createdTypes.push(type.id);
        await request(owner, 'POST', '/assets/gateway-groups', scope, fresh({ name: '手工绕过', subnets: ['192.168.1.0/24'], remarks: null }), 422);
        group = (await createGroup(owner, instance('proxy'), 'API 测试网关组')).record;
        createdGroups.push(group.id);
        const duplicate = (await createGroup(owner, instance('mapped'), group.name)).record;
        createdGroups.push(duplicate.id);
        assert.notEqual(group.id, duplicate.id);
        await request(owner, 'DELETE', `/assets/gateway-groups/${duplicate.id}`, scope, undefined, 204);
        await request(admin, 'DELETE', `/assets/nodes/${scope.machine_id}`, { owner_user_id: scope.owner_user_id }, undefined, 409);
    });
    await t.test('idempotent create, LAN uniqueness, parent and self-device guards', async () => {
        const baselineCount = (await request(owner, 'GET', '/assets/device-counts', { owner_user_id: scope.owner_user_id })).items.find(row => row.machine_id === scope.machine_id).device_count;
        const spec = { mode: 'gateway_self', gateway_group_id: group.id, rule_key: group.binding.snapshot.rules[0].rule_key, host_offset: 10 };
        const created = await createDevice(owner, spec, metadata());
        device = created.record; createBody = created.input; createdDevices.push(device.id);
        assert.equal(device.overlay_ip, fixture.instances.apiNode.proxy.ipv4);
        assert.equal(device.lan_ip, '192.168.50.10');
        assert.equal((await request(owner, 'POST', '/assets/devices', scope, createBody)).id, device.id);
        await request(owner, 'POST', '/assets/devices', scope, { ...createBody, name: 'changed' }, 409);
        await request(outsider, 'GET', `/assets/devices/${device.id}`, scope, undefined, 403);
        await request(owner, 'GET', `/assets/devices/${device.id}`, { ...scope, machine_id: fixture.offlineNode.machine_id }, undefined, 404);
        await request(owner, 'PUT', `/assets/devices/${device.id}`, scope, metadata({ gateway_group_id: null }), 409);
        const duplicateLan = { ...spec, mode: 'gateway_rule' };
        const conflict = await request(owner, 'POST', '/assets/validation/instance-address', scope,
            { subject: { resource_kind: 'device', id: null }, address_spec: duplicateLan }, 409);
        assert.equal(conflict.code, 'lan_ip_in_use');
        const duplicateSelf = { ...spec, rule_key: null, host_offset: null };
        const selfConflict = await request(owner, 'POST', '/assets/validation/instance-address', scope,
            { subject: { resource_kind: 'device', id: null }, address_spec: duplicateSelf }, 409);
        assert.equal(selfConflict.code, 'gateway_self_in_use');
        await request(owner, 'DELETE', `/assets/gateway-groups/${group.id}`, scope, undefined, 409);
        await request(admin, 'DELETE', `/assets/device-types/${type.id}`, { owner_user_id: scope.owner_user_id }, undefined, 409);
        const counts = await request(owner, 'GET', '/assets/device-counts', { owner_user_id: scope.owner_user_id });
        assert.equal(counts.items.find(row => row.machine_id === scope.machine_id).device_count, baselineCount + 1);
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
        assert.equal((await request(owner, 'GET', '/assets/devices', { ...scope, keyword: '%' })).total, 0);
    });
    await t.test('device deletion cascades every current service and replay remains gone', async () => {
        await request(owner, 'DELETE', `/assets/devices/${device.id}`, scope, undefined, 204);
        await request(owner, 'GET', `/assets/devices/${device.id}/services`, scope, undefined, 404);
        await request(owner, 'POST', '/assets/devices', scope, createBody, 410);
        await request(owner, 'DELETE', `/assets/gateway-groups/${group.id}`, scope, undefined, 204);
    });
});
