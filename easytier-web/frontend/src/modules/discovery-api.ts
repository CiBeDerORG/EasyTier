import { getApiBase } from './api-host';
import { AutomationApiError } from './automation-api';
export type SemanticKind = 'openwrt' | 'phone' | 'other' | 'server' | null;
export type SemanticTag = Exclude<SemanticKind, null>;
export interface DiscoveryAccess { endpoint_url: string; ca_pem: string; has_secret: boolean; config_revision: number; last_result?: unknown }
export interface DiscoveryCandidate { id: string; name: string; lan_ip: string | null; discovery?: Record<string, unknown>; services: { id: string; name: string; port: number; service_type: string }[] }
export interface DiscoveryChoice { row_id: string; action: 'create' | 'update' | 'replace' | 'skip'; target_device_id?: string; keep_service_ids: string[] }
export interface DiscoveryRow { row_id: string; ipv4: string | null; input: Record<string, unknown>; matched: DiscoveryCandidate[]; candidates: DiscoveryCandidate[]; blocked_reason: string | null; suggested_action: DiscoveryChoice['action']; addresses: { overlay_ip?: string; lan_ip?: string } | null; applied?: { state: string; device_id?: string } }
export interface EndpointEvent { event_id: string; device_id: string; owner_user_id: number; endpoint_revision: number; publication?: { state: string }; cleanup: { state: string; code?: string }; connection: { state: string; code?: string; message?: string } }
export interface AdbRow { row_id: string; ip: string; state: string; code?: string; message?: string; port?: number; endpoints?: { endpoint_id: string; port: number }[] }
export interface DiscoveryOperation { operation_id: string; preview_id: string; preview_revision: number; revision: number; kind: string; status: string; expires_at: string; originating_operation_id?: string; rows: DiscoveryRow[]; events: EndpointEvent[]; snapshot?: { status: string; errors?: unknown[]; truncated?: boolean; scanned_at?: number; has_snapshot?: boolean }; result: { state?: string; code?: string; message?: string; capabilities?: Record<string, boolean>; rows?: AdbRow[]; supplement_preview_id?: string } }
export class DiscoveryApi {
    url(owner: number, gateway?: string, path = '') {
        const url = new URL(`${getApiBase()}/api/v1/assets/discovery/${gateway ? `gateways/${encodeURIComponent(gateway)}${path}` : 'events'}`, window.location.href);
        url.searchParams.set('owner_user_id', String(owner)); return url.toString();
    }
    async request<T>(owner: number, gateway: string, path: string, method = 'GET', body?: unknown): Promise<T> {
        const response = await fetch(this.url(owner, gateway, path), { method, credentials: 'include', cache: 'no-store', headers: { 'Content-Type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
        const data = await response.json().catch(() => undefined);
        if (!response.ok || data === undefined) throw new AutomationApiError(response.status, data?.code ?? 'discovery_response_unknown', [data?.message, ...Object.values(data?.field_errors ?? {})].filter(Boolean).join(' · ') || `HTTP ${response.status}`);
        return data as T;
    }
    subscribe(owner: number, receive: (event: EndpointEvent) => void, disconnected: () => void): () => void {
        const source = new EventSource(this.url(owner), { withCredentials: true });
        source.addEventListener('endpoint', event => { try { receive(JSON.parse((event as MessageEvent).data)); } catch { /* malformed event cannot change a target */ } });
        source.onerror = () => { source.close(); disconnected(); }; // Disable EventSource's automatic reconnect.
        return () => source.close();
    }
}
