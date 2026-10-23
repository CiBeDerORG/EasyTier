import assert from 'node:assert/strict';
import test from 'node:test';
import { fixture, scope, fresh, instance, request, login, bindingPreview, addressPreview,
    createGroup, createDevice, groupRecord, deviceRecord, mutateInstance, restoreInstance, resetApiFixture } from './node-assets-api-support.mjs';

// These tests deliberately mutate only the disposable apiNode; browser fixtures stay independent.
test('instance binding HTTP contract, legacy conversion and configuration lifecycle', { timeout: 90_000 }, async t => {
    resetApiFixture();
    const owner = await login(fixture.owner), admin = await login(fixture.admin), other = await login(fixture.other);
    const devices = [], groups = [];
    const rememberDevice = result => { devices.push(result.record.id); return result; };
    const rememberGroup = result => { groups.push(result.record.id); return result; };
    const gatewaySubject = id => ({ resource_kind: 'gateway', id });
    const deviceSubject = id => ({ resource_kind: 'device', id });
    const direct = (name = 'ordinary', lan_ip = null) => ({ mode: 'direct_instance', instance_id: instance(name), lan_ip });
    const child = (group, host_offset, self = false) => ({ mode: self ? 'gateway_self' : 'gateway_rule',
        gateway_group_id: group.id, rule_key: group.binding.snapshot.rules[0].rule_key, host_offset });
    const rawAddress = (spec, id = null, expected = 200) => request(owner, 'POST', '/assets/validation/instance-address', scope,
        { subject: deviceSubject(id), address_spec: spec }, expected);
    async function applyBinding(kind, id, name, operation = 'refresh', extra = {}) {
        const preview = await bindingPreview(owner, { resource_kind: kind, id }, instance(name), operation, extra);
        const input = fresh({ operation, instance_id: instance(name), preview_version: preview.preview_version, ...extra });
        const path = `/assets/${kind === 'gateway' ? 'gateway-groups' : 'devices'}/${id}/binding`;
        return { record: await request(owner, 'PUT', path, scope, input), input, preview, path };
    }
    async function applyAddress(id, spec) {
        const preview = await addressPreview(owner, spec, id);
        const input = fresh({ address_spec: spec, preview_version: preview.preview_version });
        const path = `/assets/devices/${id}/address`;
        return { record: await request(owner, 'PUT', path, scope, input), input, preview, path };
    }
    t.after(async () => {
        for (const id of devices.reverse()) {
            const url = new URL(`/api/v1/assets/devices/${id}`, fixture.baseURL);
            Object.entries(scope).forEach(([key, value]) => url.searchParams.set(key, value));
            assert.ok([204, 404].includes((await fetch(url, { method: 'DELETE', headers: { cookie: owner } })).status));
        }
        for (const id of groups.reverse()) {
            const url = new URL(`/api/v1/assets/gateway-groups/${id}`, fixture.baseURL);
            Object.entries(scope).forEach(([key, value]) => url.searchParams.set(key, value));
            assert.ok([204, 404].includes((await fetch(url, { method: 'DELETE', headers: { cookie: owner } })).status));
        }
        for (const name of ['ordinary', 'ordinary2', 'proxy', 'mapped', 'mapped2', 'legacy']) {
            mutateInstance(name, 'recreate');
        }
    });

    await t.test('V02/V03/V16 catalog roles, disabled offline candidates and tenant secrecy', async () => {
        const gateways = await request(owner, 'GET', '/assets/network-instances', { ...scope, role: 'gateway' });
        const ordinary = await request(owner, 'GET', '/assets/network-instances', { ...scope, role: 'ordinary' });
        assert.equal(gateways.items.length, 4); assert.equal(ordinary.items.length, 2);
        const proxy = gateways.items.find(item => item.instance_id === instance('proxy'));
        assert.equal(proxy.role, 'gateway'); assert.equal(proxy.rules[0].mapped_cidr, null);
        const disabled = gateways.items.find(item => item.instance_id === instance('mapped2'));
        assert.equal(disabled.disabled, true); assert.equal(disabled.selectable, true);
        assert.equal(ordinary.items[0].display_name, ordinary.items[1].display_name);
        assert.notEqual(ordinary.items[0].instance_id, ordinary.items[1].instance_id);
        const all = JSON.stringify([gateways, ordinary]);
        assert.doesNotMatch(all, /network_secret|fixture-secret|Other private fixture/);
        await request(other, 'GET', '/assets/network-instances', { ...scope, role: 'ordinary' }, undefined, 403);
        const theirs = await request(other, 'GET', '/assets/network-instances', { ...scope, owner_user_id: fixture.other.id, role: 'ordinary' });
        assert.equal(theirs.items[0].ipv4, '10.99.0.99');
        const managed = await request(admin, 'GET', '/assets/network-instances', { ...scope, role: 'gateway' });
        assert.equal(managed.items.length, 4);
    });

    let proxy, mapped, wide, plain, samePlain, mappedChild, self;
    await t.test('V04/V07 unique gateway instance, duplicate direct instance and scoped guards', async () => {
        proxy = rememberGroup(await createGroup(owner, instance('proxy'), 'Duplicate names allowed')).record;
        mapped = rememberGroup(await createGroup(owner, instance('mapped'), 'Duplicate names allowed')).record;
        await request(owner, 'POST', '/assets/validation/binding', scope,
            { subject: gatewaySubject(null), operation: 'attach', instance_id: instance('proxy') }, 409);
        plain = rememberDevice(await createDevice(owner, direct(), { name: 'Same device name' })).record;
        samePlain = rememberDevice(await createDevice(owner, direct(), { name: 'Same device name' })).record;
        assert.notEqual(plain.id, samePlain.id); assert.equal(plain.overlay_ip, samePlain.overlay_ip);
        await request(owner, 'POST', '/assets/validation/binding', { ...scope, machine_id: fixture.offlineNode.machine_id },
            { subject: gatewaySubject(proxy.id), operation: 'refresh', instance_id: instance('proxy') }, 404);
        await request(owner, 'POST', '/assets/validation/binding', scope,
            { subject: gatewaySubject(null), operation: 'attach', instance_id: instance('ordinary') }, 409);
    });

    await t.test('V05/V15 proxy and mapped hosts, gateway self and metadata protections', async () => {
        const proxied = rememberDevice(await createDevice(owner, child(proxy, 22))).record;
        assert.equal(proxied.overlay_ip, '192.168.50.22'); assert.equal(proxied.lan_ip, '192.168.50.22');
        mappedChild = rememberDevice(await createDevice(owner, child(mapped, 23))).record;
        assert.equal(mappedChild.overlay_ip, '10.60.0.23'); assert.equal(mappedChild.lan_ip, '192.168.60.23');
        self = rememberDevice(await createDevice(owner, { mode: 'gateway_self', gateway_group_id: mapped.id, rule_key: null, host_offset: null })).record;
        assert.equal(self.overlay_ip, '10.70.0.2'); assert.equal(self.lan_ip, null);
        await rawAddress(child(mapped, 24, true), null, 409);
        await rawAddress(child(proxy, 23), mappedChild.id, 409);
        await rawAddress(child(mapped, 23, true), mappedChild.id, 409);
        await rawAddress(direct('ordinary2'), plain.id, 409);
        await request(owner, 'PUT', `/assets/devices/${plain.id}`, scope,
            { name: plain.name, model: null, remarks: 'No network writes', device_type_id: null, overlay_ip: '1.2.3.4' }, 422);
        await request(owner, 'PUT', `/assets/gateway-groups/${mapped.id}`, scope,
            { name: mapped.name, remarks: null, subnets: ['10.10.0.0/24'] }, 422);
    });

    await t.test('V05 non-/24 offsets, /31 and /32 plus invalid boundaries', async () => {
        mutateInstance('mapped2', 'patch', { proxy_cidrs: ['192.168.0.0/16->10.90.0.0/16', '172.31.0.0/31', '172.31.1.7/32'] });
        wide = rememberGroup(await createGroup(owner, instance('mapped2'), 'Wide and point routes')).record;
        const rules = wide.binding.snapshot.rules;
        const wideSpec = { mode: 'gateway_rule', gateway_group_id: wide.id,
            rule_key: rules.find(rule => rule.prefix_len === 16).rule_key, host_offset: 257 };
        const preview = await addressPreview(owner, wideSpec);
        assert.deepEqual(preview.addresses, { overlay_ip: '10.90.1.1', lan_ip: '192.168.1.1' });
        const saved = rememberDevice(await createDevice(owner, wideSpec)).record;
        assert.equal(saved.overlay_ip, preview.addresses.overlay_ip);
        for (const offset of [-1, 1.5, 65536, 4294967296]) {
            assert.equal((await rawAddress({ ...wideSpec, host_offset: offset })).can_save, false);
        }
        const reserved = await addressPreview(owner, { ...wideSpec, host_offset: 0 });
        assert.ok(reserved.warnings.some(warning => warning.code === 'reserved_subnet_address'));
        for (const prefix of [31, 32]) {
            const rule = rules.find(rule => rule.prefix_len === prefix);
            const result = await addressPreview(owner, { ...wideSpec, rule_key: rule.rule_key, host_offset: prefix === 31 ? 1 : 0 });
            assert.equal(result.warnings.length, 0);
        }
    });

    await t.test('V06 stale preview rejects create and fresh preview creates with current data', async () => {
        const spec = direct('ordinary2');
        const preview = await addressPreview(owner, spec);
        mutateInstance('ordinary2', 'patch', { virtual_ipv4: '10.70.0.31' });
        const input = fresh({ name: 'Stale preview device', model: null, remarks: null, device_type_id: null,
            address_spec: spec, preview_version: preview.preview_version });
        const error = await request(owner, 'POST', '/assets/devices', scope, input, 409);
        assert.equal(error.code, 'instance_config_changed');
        const saved = rememberDevice(await createDevice(owner, spec)).record;
        assert.equal(saved.overlay_ip, '10.70.0.31');
        restoreInstance('ordinary2');
    });

    await t.test('V08/V11 create replay survives source changes and unrelated changes preserve previews', async () => {
        const created = rememberDevice(await createDevice(owner, direct('ordinary2')));
        mutateInstance('ordinary2', 'patch', { virtual_ipv4: '10.70.0.41' });
        assert.deepEqual(await request(owner, 'POST', '/assets/devices', scope, created.input), created.record);
        await request(owner, 'POST', '/assets/devices', scope, { ...created.input, name: 'Changed replay payload' }, 409);
        await request(other, 'POST', '/assets/devices', scope, created.input, 403);
        restoreInstance('ordinary2');
        const preview = await addressPreview(owner, direct('ordinary2'));
        mutateInstance('ordinary2', 'patch', { network_name: 'Renamed only', hostname: 'Different hostname' });
        const after = await addressPreview(owner, direct('ordinary2'));
        assert.equal(after.preview_version, preview.preview_version);
        assert.equal((await deviceRecord(owner, created.record.id)).binding.state, 'current');
        restoreInstance('ordinary2');
    });

    await t.test('V09/V10 legacy IPv6 remains editable and explicit binding never rewrites children', async () => {
        const legacy = fixture.legacy.apiNode;
        const original = await deviceRecord(owner, legacy.direct);
        assert.equal(original.overlay_ip, 'fd00::20'); assert.equal(original.binding.state, 'legacy_unbound');
        const updated = await request(owner, 'PUT', `/assets/devices/${legacy.direct}`, scope,
            { name: original.name, model: original.model, remarks: 'IPv6 metadata retained', device_type_id: null,
                gateway_group_id: null, is_gateway_self: false, overlay_ip: original.overlay_ip, lan_ip: original.lan_ip });
        assert.equal(updated.lan_ip, 'fd80::20');
        const invalid = await request(owner, 'POST', '/assets/validation/binding', scope,
            { subject: deviceSubject(legacy.direct), operation: 'attach', instance_id: instance('ordinary') });
        assert.equal(invalid.can_save, false);
        const attached = await applyBinding('device', legacy.direct, 'ordinary', 'attach', { lan_ip: null });
        assert.equal(attached.record.lan_ip, null); assert.equal(attached.record.overlay_ip, '10.70.0.10');
        await request(owner, 'PUT', attached.path, scope, { ...attached.input, lan_ip: undefined }, 409);
        const beforeChild = await deviceRecord(owner, legacy.child), beforeSelf = await deviceRecord(owner, legacy.self);
        const gateway = await applyBinding('gateway', legacy.group, 'legacy', 'attach');
        assert.ok(gateway.preview.affected_device_ids.includes(legacy.child));
        for (const before of [beforeChild, beforeSelf]) {
            const after = await deviceRecord(owner, before.id);
            assert.equal(after.overlay_ip, before.overlay_ip); assert.equal(after.lan_ip, before.lan_ip);
            assert.equal(after.binding.state, 'legacy_unbound');
        }
        const converted = await applyAddress(legacy.child, child(gateway.record, 21));
        const convertedSelf = await applyAddress(legacy.self, child(gateway.record, 1, true));
        assert.equal(converted.record.id, legacy.child); assert.equal(converted.record.overlay_ip, '10.80.0.21');
        assert.equal(convertedSelf.record.overlay_ip, '10.70.0.4');
        const services = await request(owner, 'GET', `/assets/devices/${legacy.child}/services`, scope);
        assert.ok(services.items.some(service => service.id === legacy.service));
        await request(owner, 'POST', '/assets/validation/binding', scope,
            { subject: deviceSubject(legacy.child), operation: 'attach', instance_id: instance('ordinary') }, 409);
    });

    await t.test('V11/V12 parent refresh leaves child IPs untouched and self requires confirmation', async () => {
        mutateInstance('mapped', 'patch', { virtual_ipv4: '10.70.0.42', proxy_cidrs: [...fixture.instances.apiNode.mapped.rules, '192.168.61.0/24->10.61.0.0/24'] });
        assert.equal((await groupRecord(owner, mapped.id)).binding.state, 'changed');
        assert.equal((await deviceRecord(owner, mappedChild.id)).binding.state, 'changed');
        const refreshed = await applyBinding('gateway', mapped.id, 'mapped');
        assert.equal(refreshed.record.overlay_ip, '10.70.0.42');
        const preserved = await deviceRecord(owner, mappedChild.id), oldSelf = await deviceRecord(owner, self.id);
        assert.equal(preserved.overlay_ip, '10.60.0.23'); assert.equal(preserved.binding.state, 'current');
        assert.equal(oldSelf.overlay_ip, '10.70.0.2'); assert.equal(oldSelf.binding.state, 'changed');
        const corrected = await applyAddress(self.id, { mode: 'gateway_self', gateway_group_id: mapped.id, rule_key: null, host_offset: null });
        assert.equal(corrected.record.overlay_ip, '10.70.0.42');
        mutateInstance('mapped', 'patch', { virtual_ipv4: '10.70.0.43' });
        assert.deepEqual(await request(owner, 'PUT', refreshed.path, scope, refreshed.input), refreshed.record);
        assert.deepEqual(await request(owner, 'PUT', corrected.path, scope, corrected.input), corrected.record);
        await request(owner, 'PUT', corrected.path, scope,
            { ...corrected.input, address_spec: { ...corrected.input.address_spec,
                rule_key: refreshed.record.binding.snapshot.rules[0].rule_key, host_offset: 1 } }, 409);
        await applyBinding('gateway', mapped.id, 'mapped');
    });

    await t.test('V12 missing rules require explicit address correction without deleting services', async () => {
        mutateInstance('mapped', 'patch', { proxy_cidrs: ['192.168.61.0/24->10.61.0.0/24'] });
        const refreshed = await applyBinding('gateway', mapped.id, 'mapped');
        const before = await deviceRecord(owner, mappedChild.id);
        assert.equal(before.binding.state, 'changed'); assert.equal(before.binding.reason, 'binding_rule_missing');
        assert.equal(before.overlay_ip, '10.60.0.23');
        const corrected = await applyAddress(mappedChild.id, child(refreshed.record, 23));
        assert.equal(corrected.record.overlay_ip, '10.61.0.23'); assert.equal(corrected.record.lan_ip, '192.168.61.23');
    });

    await t.test('V13 same-ID recreation keeps children replaced until their own confirmation', async () => {
        mutateInstance('mapped', 'delete');
        assert.equal((await groupRecord(owner, mapped.id)).binding.state, 'missing');
        assert.equal((await deviceRecord(owner, mappedChild.id)).binding.state, 'missing');
        mutateInstance('mapped', 'recreate');
        assert.equal((await groupRecord(owner, mapped.id)).binding.state, 'replaced');
        const refreshed = await applyBinding('gateway', mapped.id, 'mapped');
        assert.equal((await deviceRecord(owner, mappedChild.id)).binding.state, 'replaced');
        assert.equal((await deviceRecord(owner, self.id)).binding.state, 'replaced');
        const corrected = await applyAddress(mappedChild.id, child(refreshed.record, 23));
        assert.equal(corrected.record.binding.state, 'current');
    });

    await t.test('V13/V14 missing, changed role and unavailable source still allow metadata/services/deletion', async () => {
        mutateInstance('ordinary', 'delete');
        assert.equal((await deviceRecord(owner, plain.id)).binding.state, 'missing');
        const updated = await request(owner, 'PUT', `/assets/devices/${plain.id}`, scope,
            { name: 'Metadata while source missing', model: 'Retained', remarks: null, device_type_id: null });
        assert.equal(updated.overlay_ip, plain.overlay_ip);
        const service = await request(owner, 'POST', `/assets/devices/${plain.id}/services`, scope,
            fresh({ name: 'Service while source missing', port: 8080, protocols: ['tcp'], remarks: null, access_url: null, icon: null }));
        await request(owner, 'DELETE', `/assets/services/${service.id}`, scope, undefined, 204);
        mutateInstance('ordinary', 'recreate');
        assert.equal((await deviceRecord(owner, plain.id)).binding.state, 'replaced');
        await applyBinding('device', plain.id, 'ordinary');
        mutateInstance('ordinary', 'patch', { proxy_cidrs: ['192.168.90.0/24'] });
        assert.equal((await deviceRecord(owner, plain.id)).binding.reason, 'binding_role_mismatch');
        await request(owner, 'POST', '/assets/validation/binding', scope,
            { subject: deviceSubject(plain.id), operation: 'refresh', instance_id: instance('ordinary') }, 409);
        mutateInstance('ordinary', 'patch', { proxy_cidrs: ['invalid-cidr'] });
        const catalog = await request(owner, 'GET', '/assets/network-instances', { ...scope, role: 'ordinary' });
        const broken = catalog.items.find(item => item.instance_id === instance('ordinary'));
        assert.equal(broken.role, 'unknown'); assert.equal(broken.selectable, false); assert.ok(broken.unavailable_reason);
        assert.equal((await deviceRecord(owner, plain.id)).binding.state, 'unavailable');
        await request(owner, 'DELETE', `/assets/devices/${plain.id}`, scope, undefined, 204);
        restoreInstance('ordinary');
    });

    await t.test('V08 address replay returns gone after deletion instead of resurrecting a device', async () => {
        const before = rememberDevice(await createDevice(owner, direct('ordinary2'))).record;
        const saved = await applyAddress(before.id, direct('ordinary2', '192.168.99.12'));
        await request(owner, 'DELETE', `/assets/devices/${before.id}`, scope, undefined, 204);
        const replay = await request(owner, 'PUT', saved.path, scope, saved.input, 410);
        assert.equal(replay.code, 'created_resource_deleted');
    });
});
