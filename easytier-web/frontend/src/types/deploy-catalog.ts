/** Catalog metadata is read-only; it grants no installation or adoption rights. */
export interface DeployCatalogConfigField {
    key: string;
    label: string;
    type: 'string' | 'number' | 'boolean' | 'port' | 'ipv4' | 'secret';
    target: 'env' | 'secrets';
    required: boolean;
    editable: boolean;
    generated_by_script: boolean;
    description?: string;
    /** Never present for secret fields. */
    default?: string | number | boolean;
    min_length?: number;
    max_length?: number;
    minimum?: number;
    maximum?: number;
    pattern?: string;
}
export interface DeployCatalogPort {
    id: string;
    label: string;
    protocol: 'tcp' | 'udp';
    purpose: 'web' | 'other';
    host_port_key: string;
    bind_address_key?: string | null;
    container_port: number;
    service?: string | null;
    path?: string | null;
}
export interface DeployCatalogApp {
    id: string;
    name: string;
    summary: string;
    category: 'infrastructure' | 'network' | 'developer' | 'media' | 'productivity' | 'other';
    compose_project: string;
    manifest_path: string;
    manifest_sha256: string;
    git_revision: string;
    detail: {
        runtime_kind: string;
        compose_file: string;
        platforms: string[];
        architectures: string[];
        config: DeployCatalogConfigField[];
        ports: DeployCatalogPort[];
        actions: { install: string; update?: string | null; uninstall?: string | null };
        install_available: false;
        management_rights: false;
        validation: string;
    };
}
export interface DeployCatalogIndex {
    initialized: boolean;
    read_only: true;
    count: number;
    items: DeployCatalogApp[];
    git_revision?: string;
    fetched_at?: string;
}
export interface DeployCatalogRefresh {
    updated: boolean;
    read_only: true;
    git_revision: string;
    fetched_at: string;
    count: number;
}
/** Prepared package metadata only. This is not an install authorization. */
export interface DeployCatalogPackage {
    app_id: string;
    git_revision: string;
    manifest_sha256: string;
    package_sha256: string;
    package_size: number;
    file_count: number;
    built_at: string;
    reused: boolean;
    ready_for_agent: false;
    install_enabled: false;
}
