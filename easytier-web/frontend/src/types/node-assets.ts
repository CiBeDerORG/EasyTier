/** Manual node assets. These IDs are not EasyTier network instance IDs. */
export interface AssetScope {
    owner_user_id: number;
    machine_id: string;
}

export interface AssetAccount {
    id: number;
    username: string;
}

export interface AssetContextResponse {
    actor: AssetAccount;
    can_manage_others: boolean;
    available: boolean;
}

export interface AssetNode extends AssetScope {
    hostname: string;
    alias: string;
    site_location: string | null;
    device_model: string | null;
    connection_ip: string | null;
    ip_country: string | null;
    ip_region: string | null;
    ip_city: string | null;
    is_public_server: boolean;
    device_count: number;
}

export interface AssetNodeUpdate {
    is_public_server: boolean;
    site_location: string | null;
    device_model: string | null;
}

export interface AssetDeviceCount {
    machine_id: string;
    device_count: number;
}

export interface AssetWarning {
    code: string;
    field: string;
    message: string;
    device_id?: string;
    device_name?: string;
    suggestion?: string;
}

export interface AssetError {
    code: string;
    message: string;
    field_errors: Record<string, string>;
    /** Transport information, not a field in the server response body. */
    status?: number;
}

export interface AssetListResponse<T> {
    items: T[];
}

export interface GatewayGroupInput {
    name: string;
    remarks: string | null;
    subnets: string[];
}

export interface GatewayGroup extends AssetScope, GatewayGroupInput {
    id: string;
    created_at: string;
    updated_at: string;
    warnings: AssetWarning[];
}

export interface DeviceTypeInput {
    name: string;
}

export interface DeviceType extends DeviceTypeInput {
    id: string;
    owner_user_id: number;
    created_at: string;
    updated_at: string;
}

export interface AssetDeviceInput {
    gateway_group_id: string | null;
    device_type_id: string | null;
    name: string;
    model: string | null;
    overlay_ip: string;
    lan_ip: string | null;
    is_gateway_self: boolean;
    remarks: string | null;
}

export interface AssetDevice extends AssetScope, AssetDeviceInput {
    id: string;
    created_at: string;
    updated_at: string;
    warnings: AssetWarning[];
}

export type AssetProtocol = 'tcp' | 'udp';

/** Stable keys, mapped to local PrimeIcons by the service components. */
export type AssetIcon = 'server' | 'globe' | 'database' | 'mobile' | 'desktop'
    | 'home' | 'cloud' | 'cog' | 'play' | 'link' | 'shield' | 'wifi';

export interface AssetServiceInput {
    name: string;
    protocols: AssetProtocol[];
    port: number;
    remarks: string | null;
    access_url: string | null;
    icon: AssetIcon | null;
}

export interface AssetService extends AssetScope, AssetServiceInput {
    id: string;
    device_id: string;
    created_at: string;
    updated_at: string;
}

/** Keep this request ID and payload together until the outcome is known. */
export type CreateAssetRequest<T extends object> = T & { request_id: string };

export interface AssetDeviceQuery {
    page?: number;
    page_size?: number;
    group_id?: string;
    type_id?: string;
    keyword?: string;
    locate_id?: string;
}

export interface PagedDevices extends AssetListResponse<AssetDevice> {
    total: number;
    page: number;
    page_size: number;
}

export interface AssetPreview {
    field_errors: Record<string, string>;
    warnings: AssetWarning[];
}

export interface GatewaySubnetsPreviewInput {
    gateway_group_id?: string | null;
    subnets: string[];
}

export interface DeviceAddressPreviewInput {
    device_id?: string | null;
    gateway_group_id: string | null;
    overlay_ip: string;
    lan_ip: string | null;
}

/** UI-only filters. Never send unsubmitted draft filters as the applied query. */
export interface AssetDeviceFilters {
    group_id: string | null;
    type_id: string | null;
    keyword: string;
}

export type AssetDevicePageEvent = number;

export interface AssetDeviceExpandEvent {
    id: string;
    expanded: boolean;
}
