import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isServerAgentAsset, matchesServerAgentEntry } from '../src/modules/server-agent-entry.ts';

test('only explicit multi-select server semantic enables Agent entry', () => {
    assert.equal(isServerAgentAsset({ semantic_kinds: ['server'] }), true);
    assert.equal(isServerAgentAsset({ semantic_kinds: ['openwrt', 'server'] }), true);
    assert.equal(isServerAgentAsset({ semantic_kinds: ['phone'] }), false);
    assert.equal(isServerAgentAsset({ semantic_kinds: [] }), false);
    assert.equal(isServerAgentAsset({ semantic_kinds: undefined }), false);
    assert.equal(isServerAgentAsset({ semantic_kinds: 'server' }), false);
});

test('stale owner/node/device or future pairing state cannot be displayed as current device', () => {
    const scope = { owner_user_id: 5, machine_id: 'node-001' };
    const base = {
        owner_user_id: 5, machine_id: 'node-001', asset_device_id: 'asset-001',
        device_name: 'NAS', overlay_ip: '10.12.0.9', lan_ip: null,
        agent_status: 'not_checked', pairing_available: false, source: 'existing_asset_device',
    };
    assert.equal(matchesServerAgentEntry(scope, 'asset-001', base), true);
    assert.equal(matchesServerAgentEntry(scope, 'asset-001', { ...base, owner_user_id: 9 }), false);
    assert.equal(matchesServerAgentEntry(scope, 'asset-001', { ...base, machine_id: 'node-002' }), false);
    assert.equal(matchesServerAgentEntry(scope, 'asset-001', { ...base, asset_device_id: 'asset-002' }), false);
    assert.equal(matchesServerAgentEntry(scope, 'asset-001', { ...base, agent_status: 'connected' }), false);
    assert.equal(matchesServerAgentEntry(scope, 'asset-001', { ...base, pairing_available: true }), false);
});
