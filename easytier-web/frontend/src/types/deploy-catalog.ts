/** Catalog metadata is read-only; it grants no installation or adoption rights. */
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
        platforms?: string | null;
        architectures?: string | null;
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
