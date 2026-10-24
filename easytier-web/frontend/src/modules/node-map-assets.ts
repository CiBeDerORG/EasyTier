import type { AssetDevice, DeviceType, GatewayGroup } from '../types/node-assets';
import type { MapLanDevice } from './network-space';

const knownKinds = new Map<string, MapLanDevice['kind']>([
    ['nas', 'server'], ['server', 'server'], ['服务器', 'server'],
    ['desktop', 'desktop'], ['computer', 'desktop'], ['laptop', 'desktop'], ['pc', 'desktop'], ['电脑', 'desktop'], ['计算机', 'desktop'],
    ['camera', 'camera'], ['ip camera', 'camera'], ['摄像头', 'camera'], ['摄像机', 'camera'],
    ['openwrt', 'router'], ['router', 'router'], ['路由器', 'router'], ['软路由', 'router'],
]);

/** Convert existing asset records to display objects only. No map-specific
 * devices, gateway roles, addresses or room assignments are persisted here. */
export function buildNodeSpaceDevices(
    nodeId: string,
    devices: AssetDevice[],
    groups: GatewayGroup[],
    types: DeviceType[],
): MapLanDevice[] {
    const scopedDevices = devices.filter(device => device.machine_id === nodeId)
        .sort((a, b) => a.id.localeCompare(b.id));
    const scopedGroups = groups.filter(group => group.machine_id === nodeId)
        .sort((a, b) => a.id.localeCompare(b.id));
    const groupById = new Map(scopedGroups.map(group => [group.id, group]));
    const typeById = new Map(types.map(type => [type.id, type]));
    const anchorByGroup = new Map<string, string>();
    const display: MapLanDevice[] = [];

    for (const group of scopedGroups) {
        const self = scopedDevices.find(device => device.gateway_group_id === group.id
            && device.owner_user_id === group.owner_user_id && device.is_gateway_self);
        const anchorId = self?.id ?? `gateway:${group.id}`;
        anchorByGroup.set(group.id, anchorId);
        if (!self) {
            display.push({
                id: anchorId, gateway_device_id: nodeId, name: group.name,
                address: group.overlay_ip ?? '', kind: 'router', room: 'living',
                is_gateway: true, gateway_group_id: group.id, group_name: group.name,
            });
        }
    }

    for (const device of scopedDevices) {
        const candidate = device.gateway_group_id ? groupById.get(device.gateway_group_id) : undefined;
        const group = candidate?.owner_user_id === device.owner_user_id ? candidate : undefined;
        const anchor = group ? anchorByGroup.get(group.id) : undefined;
        const candidateType = device.device_type_id ? typeById.get(device.device_type_id) : undefined;
        const typeName = candidateType?.owner_user_id === device.owner_user_id
            ? candidateType.name.trim().toLowerCase() : '';
        display.push({
            id: device.id, asset_device_id: device.id, gateway_device_id: nodeId,
            name: device.name, address: device.overlay_ip || device.lan_ip || '',
            kind: device.is_gateway_self ? 'router' : knownKinds.get(typeName) ?? 'other',
            // Assets have no room field. The renderer uses an explicitly
            // illustrative placement; this value is not a room assignment.
            room: 'living', is_gateway: device.is_gateway_self,
            ...(group ? { gateway_group_id: group.id, group_name: group.name } : {}),
            ...(device.gateway_group_id && anchor !== device.id ? {
                // Preserve an unresolved parent instead of inventing a direct
                // relationship to the node when a catalog read is incomplete.
                parent_id: anchor ?? `missing-gateway:${device.owner_user_id}:${device.gateway_group_id}`,
            } : {}),
        });
    }
    return display;
}

/** The sole gateway and its self asset share the site's central display object.
 * A second gateway (including a virtual group without a self asset) keeps the
 * node as a separate logical hub, so unrelated gateway devices are not merged. */
export function nodeSpaceCenterGateway(devices: readonly MapLanDevice[]): MapLanDevice | undefined {
    const gateways = devices.filter(device => device.is_gateway && device.gateway_group_id && !device.parent_id);
    return gateways.length === 1 && gateways[0]?.asset_device_id ? gateways[0] : undefined;
}
