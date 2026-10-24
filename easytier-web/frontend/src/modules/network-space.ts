/** Persisted display metadata. A map node is an existing network member. */
export interface MapNodeConfig {
    device_id: string;
    latitude: number | null;
    longitude: number | null;
    label: string;
    role: 'node' | 'gateway';
    layout: 'topology' | 'home';
}

/** Device-space renderer input. Asset relationships are distinct from live peer links. */
export interface MapLanDevice {
    id: string;
    gateway_device_id: string;
    name: string;
    address: string;
    kind: 'server' | 'desktop' | 'camera' | 'router' | 'other';
    room: 'living' | 'study' | 'bedroom' | 'entry';
    parent_id?: string;
    is_gateway?: boolean;
    asset_device_id?: string;
    group_name?: string;
    /** Resolved asset gateway group; only used to compose the scene. */
    gateway_group_id?: string;
}

export interface NetworkMapConfig {
    revision: number;
    nodes: MapNodeConfig[];
    devices: MapLanDevice[];
}

export interface ServiceEntry {
    id: string;
    network_id: string;
    device_id: string;
    lan_device_id: string | null;
    name: string;
    description: string;
    url: string;
    icon: 'home' | 'photos' | 'media' | 'chart' | 'server' | 'globe' | 'code';
    group: string;
    show_in_navigation: boolean;
    open_in_new_tab: boolean;
    sort_order: number;
}

export type ServiceInput = Omit<ServiceEntry, 'id'>;

/** Renderer input is intentionally separate from persisted metadata. */
export interface MapSceneNode {
    id: string;
    name: string;
    latitude: number;
    longitude: number;
    online: boolean;
    role: 'node' | 'gateway';
}

export interface MapSceneLink {
    id: string;
    source: string;
    target: string;
    kind: 'direct' | 'relay';
    latency_ms: number | null;
    rx_bps: number;
    tx_bps: number;
    stale: boolean;
}

export interface NodeRuntimeInfo {
    peer_id: number;
    inst_id?: string;
    proxy_cidrs?: string[];
    hostname?: string;
    ipv4_addr?: string;
}
