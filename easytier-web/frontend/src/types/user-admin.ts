/** Administrator-only account management; no password hashes or tokens are returned. */
export interface UserAdminContext {
    actor: { id: number; username: string };
    available: boolean;
    can_manage_users: boolean;
}

export interface ManagedUser {
    id: number;
    username: string;
    is_admin: boolean;
    is_current_user: boolean;
    deleted_at: string | null;
    network_instance_count: number;
    disabled_network_instance_count: number;
    node_count: number;
    can_delete: boolean;
}

export interface ManagedUserList {
    items: ManagedUser[];
}

export interface DeleteManagedUserResult {
    user_id: number;
    deleted_at: string;
    already_deleted: boolean;
    disabled_network_instance_count: number;
    runtime_error: boolean;
    runtime: {
        attempted_nodes: number;
        confirmed_nodes: number;
        pending_nodes: number;
        removed_gateway_instances: number;
    } | null;
}
