import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildNodeSpaceDevices, nodeSpaceCenterGateway } from '../src/modules/node-map-assets.ts';

const device = (id, overrides = {}) => ({
    id, machine_id: 'node-a', owner_user_id: 7, name: id, overlay_ip: '10.0.0.2', lan_ip: null,
    gateway_group_id: null, device_type_id: null, is_gateway_self: false, ...overrides,
});
const group = (id, overrides = {}) => ({
    id, machine_id: 'node-a', owner_user_id: 7, name: `Group ${id}`, overlay_ip: '10.0.0.1', ...overrides,
});

test('node space preserves gateway-self, group children and ordinary node devices', () => {
    const devices = [
        device('gateway-device', { gateway_group_id: 'office', is_gateway_self: true }),
        device('printer', { gateway_group_id: 'office' }),
        device('camera', { gateway_group_id: 'home' }),
        device('ordinary'),
        device('other-node', { machine_id: 'node-b' }),
    ];
    const result = buildNodeSpaceDevices('node-a', devices, [group('office'), group('home')], []);
    const rows = new Map(result.map(row => [row.id, row]));
    assert.equal(rows.get('gateway-device').parent_id, undefined);
    assert.equal(rows.get('gateway-device').is_gateway, true);
    assert.equal(rows.get('gateway-device').asset_device_id, 'gateway-device');
    assert.equal(rows.get('gateway-device').gateway_group_id, 'office');
    assert.equal(rows.get('printer').parent_id, 'gateway-device');
    assert.equal(rows.get('printer').group_name, 'Group office');
    assert.equal(rows.has('gateway:office'), false, 'a real gateway device is not duplicated');
    assert.equal(rows.get('camera').parent_id, 'gateway:home');
    assert.equal(rows.get('gateway:home').asset_device_id, undefined, 'group anchors cannot edit a device');
    assert.equal(rows.get('gateway:home').gateway_group_id, 'home');
    assert.equal(rows.get('ordinary').parent_id, undefined);
    assert.equal(rows.has('other-node'), false);
    assert.equal(rows.get('printer').address, '10.0.0.2');
});

test('missing or foreign parents never become an invented direct connection', () => {
    const result = buildNodeSpaceDevices('node-a', [
        device('orphan', { gateway_group_id: 'missing' }),
        device('foreign', { gateway_group_id: 'foreign' }),
    ], [group('foreign', { owner_user_id: 9 })], []);
    for (const id of ['orphan', 'foreign']) {
        const row = result.find(row => row.id === id);
        assert.ok(row.parent_id, 'an unresolved asset parent remains explicit');
        assert.equal(result.some(candidate => candidate.id === row.parent_id), false);
    }
});

test('device hardware comes from declared types, and unknown types stay generic', () => {
    const types = ['NAS', '电脑', '摄像头', 'OpenWrt', '智能家居'].map((name, i) => ({
        id: `type-${i}`, name, owner_user_id: 7,
    }));
    const devices = types.map((type, i) => device(`device-${i}`, { device_type_id: type.id }));
    devices.push(device('gateway', { is_gateway_self: true, device_type_id: 'type-0' }));
    devices.push(device('named-camera', { name: 'NAS camera router', device_type_id: null }));
    const result = new Map(buildNodeSpaceDevices('node-a', devices, [], types).map(row => [row.id, row]));
    assert.deepEqual(devices.slice(0, 5).map(row => result.get(row.id).kind), ['server', 'desktop', 'camera', 'router', 'other']);
    assert.equal(result.get('gateway').kind, 'router');
    assert.equal(result.get('named-camera').kind, 'other', 'names are not evidence of a device type');
    assert.equal(result.get('named-camera').room, 'living', 'no physical room is inferred');
});

test('a sole gateway self asset remains selectable as the combined site center', () => {
    const rows = buildNodeSpaceDevices('node-a', [
        device('gateway-self', { gateway_group_id: 'home', is_gateway_self: true }),
        device('nas', { gateway_group_id: 'home' }),
        device('direct-device'),
    ], [group('home')], []);
    const center = nodeSpaceCenterGateway(rows);
    assert.equal(center?.id, 'gateway-self');
    assert.equal(center?.asset_device_id, 'gateway-self', 'services and editing retain the real asset ID');
    assert.equal(center?.group_name, 'Group home');
    assert.equal(rows.filter(row => row.is_gateway).length, 1, 'the gateway group is not an extra device');
    assert.equal(rows.find(row => row.id === 'nas').parent_id, center.id);
    assert.equal(rows.find(row => row.id === 'direct-device').parent_id, undefined, 'display merging does not reparent direct assets');
});

test('multiple gateways keep a logical site center even when only one has a self asset', () => {
    const devices = [device('home-self', { gateway_group_id: 'home', is_gateway_self: true })];
    const groups = [group('home'), group('office')];
    const rows = buildNodeSpaceDevices('node-a', devices, groups, []);
    assert.equal(nodeSpaceCenterGateway(rows), undefined);
    assert.deepEqual(rows.filter(row => row.is_gateway).map(row => row.id).sort(), ['gateway:office', 'home-self']);

    const withSecondSelf = buildNodeSpaceDevices('node-a', [
        ...devices, device('office-self', { gateway_group_id: 'office', is_gateway_self: true }),
    ], groups, []);
    assert.equal(nodeSpaceCenterGateway(withSecondSelf), undefined);
    assert.deepEqual(withSecondSelf.filter(row => row.is_gateway).map(row => row.asset_device_id).sort(), ['home-self', 'office-self']);
});

test('virtual, missing and foreign gateway identities are never treated as a real merged self device', () => {
    assert.equal(nodeSpaceCenterGateway(buildNodeSpaceDevices('node-a', [], [group('home')], [])), undefined);
    const rows = buildNodeSpaceDevices('node-a', [
        device('orphan-self', { gateway_group_id: 'missing', is_gateway_self: true }),
        device('foreign-self', { gateway_group_id: 'foreign', is_gateway_self: true }),
    ], [group('foreign', { owner_user_id: 9 })], []);
    assert.equal(nodeSpaceCenterGateway(rows), undefined);
    assert.equal(rows.find(row => row.id === 'orphan-self').gateway_group_id, undefined);
    assert.equal(rows.find(row => row.id === 'foreign-self').gateway_group_id, undefined);
});
