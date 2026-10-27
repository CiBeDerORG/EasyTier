import type { AssetDevice, AssetScope, ServerAgentEntry } from '../types/node-assets';

/** Only explicit multi-select server semantics qualify, never legacy names. */
export function isServerAgentAsset(device: Pick<AssetDevice, 'semantic_kinds'>): boolean {
    return Array.isArray(device.semantic_kinds) && device.semantic_kinds.includes('server');
}

/** Reject a late response from a different owner, node, device or API version. */
export function matchesServerAgentEntry(
    scope: AssetScope,
    deviceId: string,
    entry: ServerAgentEntry,
): boolean {
    return entry.owner_user_id === scope.owner_user_id
        && entry.machine_id === scope.machine_id
        && entry.asset_device_id === deviceId
        && entry.source === 'existing_asset_device'
        && entry.agent_status === 'not_checked'
        && entry.pairing_available === false;
}
