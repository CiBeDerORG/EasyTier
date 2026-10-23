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

/** The role of an instance, not the customizable device type used for filtering. */
export type AssetRole = 'gateway' | 'ordinary';

export interface Subject {
    resource_kind: 'gateway' | 'device';
    /** Null identifies a not-yet-created resource during preview. */
    id: string | null;
}

export interface ConfigRef {
    id: number;
    create_time: string;
}

export interface ProxyRule {
    rule_key: string;
    real_cidr: string;
    mapped_cidr: string | null;
    prefix_len: number;
}

export interface InstanceSnapshot {
    instance_id: string;
    display_name: string;
    source: string;
    role: AssetRole | 'unknown';
    config_ref: ConfigRef;
    config_fingerprint: string;
    ipv4: string | null;
    ipv4_source: 'configured' | 'allocated' | 'runtime' | 'missing';
    rules: ProxyRule[];
}

export interface InstanceCandidate extends InstanceSnapshot {
    online: boolean | null;
    disabled: boolean;
    selectable: boolean;
    unavailable_reason: string | null;
    bound_gateway: { id: string; name: string } | null;
}

export type BindingState = 'legacy_unbound' | 'current' | 'changed' | 'missing' | 'replaced' | 'unavailable';

export interface BindingInfo {
    state: BindingState;
    reason: string | null;
    instance_id: string | null;
    snapshot: InstanceSnapshot | null;
    current: InstanceCandidate | null;
    can_attach: boolean;
    can_refresh: boolean;
    can_edit_address: boolean;
}

/** Derived addresses are returned by preview; clients only submit this selection. */
export type AddressSpec =
    | { mode: 'direct_instance'; instance_id: string; lan_ip: string | null }
    | { mode: 'gateway_rule'; gateway_group_id: string; rule_key: string; host_offset: number }
    | { mode: 'gateway_self'; gateway_group_id: string; rule_key: string | null; host_offset: number | null };

export type AddressMode = 'legacy_manual' | AddressSpec['mode'];

export interface AddressValues {
    overlay_ip: string;
    lan_ip: string | null;
}

export interface BindingDiffSide {
    snapshot: InstanceSnapshot | null;
    overlay_ip: string | null;
    lan_ip: string | null;
}

export interface InstancePreview {
    can_save: boolean;
    preview_version: string | null;
    field_errors: Record<string, string>;
    warnings: AssetWarning[];
    addresses: AddressValues | null;
    snapshot: InstanceSnapshot | null;
    before: BindingDiffSide | null;
    after: BindingDiffSide | null;
    affected_device_ids: string[];
}

export interface CandidateQuery {
    /** Determines selectability; the catalog includes all saved instances on the node. */
    role: AssetRole;
    /** Existing subject references must supply both kind and ID. */
    subject_kind?: Subject['resource_kind'];
    subject_id?: string;
}

export interface BindingPreviewInput {
    subject: Subject;
    operation: 'attach' | 'refresh';
    instance_id: string;
    lan_ip?: string | null;
}

export interface AddressPreviewInput {
    subject: Subject;
    address_spec: AddressSpec;
}

export interface ApplyBindingInput {
    request_id: string;
    operation: 'attach' | 'refresh';
    instance_id: string;
    preview_version: string;
    lan_ip?: string | null;
}

export interface ApplyAddressInput {
    request_id: string;
    address_spec: AddressSpec;
    preview_version: string;
}

export interface GatewayCreateInput {
    request_id: string;
    name: string;
    remarks: string | null;
    binding: { instance_id: string };
    preview_version: string;
}

export interface DeviceCreateInput {
    request_id: string;
    name: string;
    model: string | null;
    remarks: string | null;
    device_type_id: string | null;
    address_spec: AddressSpec;
    preview_version: string;
}

/** Payloads passed to useAssetMutation, which supplies the stable request ID. */
export type GatewayGroupCreateInput = Omit<GatewayCreateInput, 'request_id'>;
export type AssetDeviceCreateInput = Omit<DeviceCreateInput, 'request_id'>;

export interface GatewayGroupUpdateInput {
    name: string;
    remarks: string | null;
    /** Only legacy unbound groups accept manually edited subnets. */
    subnets?: string[];
}

export interface AssetDeviceUpdateInput {
    name: string;
    model: string | null;
    remarks: string | null;
    device_type_id: string | null;
    /** Optional network fields are only accepted for legacy manual records. */
    overlay_ip?: string;
    lan_ip?: string | null;
    /** If supplied, this must match the device's existing parent. */
    gateway_group_id?: string | null;
    is_gateway_self?: boolean;
}

/** Existing response fields and legacy form values; not a new-group request. */
export interface GatewayGroupInput {
    name: string;
    remarks: string | null;
    subnets: string[];
}

export interface GatewayGroup extends AssetScope, GatewayGroupInput {
    id: string;
    overlay_ip: string | null;
    binding: BindingInfo;
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

/** Existing response fields and legacy form values; not a new-device request. */
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
    address_mode: AddressMode;
    rule_key: string | null;
    host_offset: number | null;
    network_instance_id: string | null;
    binding: BindingInfo;
    /** Null for legacy_manual records. */
    address_spec: AddressSpec | null;
    created_at: string;
    updated_at: string;
    warnings: AssetWarning[];
}

/** One device list, retaining each record's existing persistence and actions. */
export type AssetDeviceRow =
    | (AssetDevice & { resource_kind: 'device' })
    | (GatewayGroup & { resource_kind: 'gateway' });

export type AssetDeviceRoleFilter = 'all' | AssetRole;

/** Save results identify the appropriate device-list record and API. */
export type AssetSavedResource =
    | { resource_kind: 'gateway'; record: GatewayGroup }
    | { resource_kind: 'device'; record: AssetDevice };

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

/** Keep this request ID and payload together until the outcome is known, including PUT retries. */
export type AssetMutationRequest<T extends object> = T & { request_id: string };

/** Existing callers can keep their create-specific spelling. */
export type CreateAssetRequest<T extends object> = AssetMutationRequest<T>;

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

export interface AssetDeviceRowQuery extends AssetDeviceQuery {
    role: AssetDeviceRoleFilter;
}

export interface PagedDeviceRows extends AssetListResponse<AssetDeviceRow> {
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
    role: AssetDeviceRoleFilter;
    type_id: string | null;
    keyword: string;
}

export type AssetDevicePageEvent = number;

export interface AssetDeviceExpandEvent {
    id: string;
    expanded: boolean;
}
