import { getApiBase } from './api-host';

export interface AgentIdentity { agent_id: string; hostname: string; platform: string; arch: string; software_root: string; observed_at: string }
export interface DiscoveredService { id: string; provider: string; name: string; state: string; project?: string; image?: string; observed_at: string; stale?: boolean }
export interface AppField { key: string; secret: boolean; default: string }
export interface DeployApplication { id: string; name: string; variants: string[]; configuration: AppField[]; supported: boolean; reason?: string }
export interface DeployAgent {
    id: string; alias: string; endpoint: string; agent_id?: string; updated_at: string; last_error?: string;
    identity?: AgentIdentity;
    services?: { observed_at: string; providers: Record<string, { status: string; message?: string }>; services: DiscoveredService[] };
    catalog?: { revision: string; applications: DeployApplication[] };
}
export interface DeployTask { task_id: string; application_id: string; action: string; status: string; message?: string; created_at?: string; queried_at?: string; logs?: string }
export interface InstallRequest { task_id: string; application_id: string; action: string; variant: string; catalog_revision: string; parameters: Record<string, string>; secrets: Record<string, string> }

// No response logging: this API carries device credentials and application secrets.
// Fetch has no application retry/refresh policy and no automatic task polling.
export async function deployRequest<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
    const response = await fetch(`${getApiBase()}/api/v1/deploy/agents${path}`, {
        method, credentials: 'include', cache: 'no-store',
        headers: { 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (response.status === 204) return undefined as T;
    const data = await response.json();
    if (!response.ok) throw new Error(data.message ?? `请求失败：HTTP ${response.status}`);
    return data as T;
}
