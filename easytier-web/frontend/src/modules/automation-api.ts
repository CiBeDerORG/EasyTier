import { getApiBase } from './api-host';
import type {
    AutomationConnection, AutomationContext, AutomationDevice, AutomationPrepareResult,
    AutomationRun, AutomationRunInput, AutomationScript, AutomationScriptInput, AutomationSession,
    AutomationRunnerSettings, AutomationRunnerSettingsInput, AutomationRunnerCheck, AutomationMediaSettings,
} from '../types/automation';

export class AutomationApiError extends Error {
    constructor(public status: number, public code: string, message: string, public fieldErrors?: Record<string, string>) {
        super(message); this.name = 'AutomationApiError';
    }
}
export function automationErrorMessage(error: unknown): string {
    return error instanceof Error ? error.message : 'Request failed';
}

/** No retries or polling: creation requests carry an explicit stable request_id. */
export default class AutomationApi {
    private readonly base = `${getApiBase()}/api/v1/automation`;
    private url(path: string, ownerId?: number): string {
        const url = new URL(`${this.base}${path}`, window.location.href);
        if (ownerId !== undefined) url.searchParams.set('owner_user_id', String(ownerId));
        return url.toString();
    }
    private async request<T>(path: string, ownerId?: number, method = 'GET', body?: unknown): Promise<T> {
        const response = await fetch(this.url(path, ownerId), {
            method, credentials: 'include', cache: 'no-store',
            headers: { 'Content-Type': 'application/json' },
            body: body === undefined ? undefined : JSON.stringify(body),
        });
        const data = response.status === 204 ? undefined : await response.json().catch(() => undefined);
        if (!response.ok) throw new AutomationApiError(response.status, data?.code ?? 'request_failed', data?.message ?? `HTTP ${response.status}`, data?.field_errors);
        if (response.status !== 204 && data === undefined) throw new AutomationApiError(502, 'invalid_response', 'Server response was not confirmed. Refresh the saved result before retrying.');
        return data as T;
    }
    context() { return this.request<AutomationContext>('/context'); }
    runnerSettings() { return this.request<AutomationRunnerSettings>('/settings'); }
    saveRunnerSettings(body: AutomationRunnerSettingsInput) { return this.request<AutomationRunnerSettings>('/settings', undefined, 'PUT', body); }
    checkRunner(revision: number) { return this.request<AutomationRunnerCheck>('/settings/check', undefined, 'POST', { revision }); }
    async accounts(): Promise<{ items: { id: number; username: string }[] }> {
        const response = await fetch(`${getApiBase()}/api/v1/assets/accounts`, { credentials: 'include', cache: 'no-store' });
        const data = await response.json();
        if (!response.ok) throw new AutomationApiError(response.status, data?.code ?? 'request_failed', data?.message ?? `HTTP ${response.status}`);
        return data;
    }
    devices(ownerId: number) { return this.request<{ items: AutomationDevice[]; issues?: {device_id:string;machine_id:string;name:string;reason:string}[] }>('/devices', ownerId); }
    saveConnection(ownerId: number, id: string, body: AutomationConnection) { return this.request<AutomationDevice>(`/devices/${encodeURIComponent(id)}/connection`, ownerId, 'PUT', body); }
    prepare(ownerId: number, id: string, body: { action: 'connect' | 'pair' | 'inspect' | 'automation'; pairing_port?: number; code?: string }) { return this.request<AutomationPrepareResult>(`/devices/${encodeURIComponent(id)}/prepare`, ownerId, 'POST', body); }
    unlockStatus(ownerId: number, serviceId: string) { return this.request<{ configured: boolean }>(`/adb-services/${encodeURIComponent(serviceId)}/unlock`, ownerId); }
    saveUnlock(ownerId: number, serviceId: string, body: {action:'replace';password:string}|{action:'clear'}) { return this.request<{ configured: boolean }>(`/adb-services/${encodeURIComponent(serviceId)}/unlock`,ownerId,'PUT',body); }
    retryEndpoint(ownerId: number, device: string, event: string, body: {request_id: string; stage: 'cleanup'|'connection'|'publication'}) { return this.request(`/devices/${encodeURIComponent(device)}/endpoint-events/${encodeURIComponent(event)}/retry`, ownerId, 'POST', body); }
    createSession(ownerId: number, body: { device_id: string; quality: 'wall' | 'focus'; media?: AutomationMediaSettings }) { return this.request<AutomationSession>('/sessions', ownerId, 'POST', body); }
    offer(ownerId: number, id: string, body: RTCSessionDescriptionInit) { return this.request<{ sdp: string; type: 'answer'; width: number; height: number; unlock_state?: string }>(`/sessions/${encodeURIComponent(id)}/offer`, ownerId, 'POST', body); }
    closeSession(ownerId: number, id: string) { return this.request<{ id: string; state: string; cleanup_pending?: boolean }>(`/sessions/${encodeURIComponent(id)}/close`, ownerId, 'POST', {}); }
    controlUrl(ownerId: number, id: string): string {
        const url = new URL(this.url(`/sessions/${encodeURIComponent(id)}/control`, ownerId));
        url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
        return url.toString();
    }
    listScripts(ownerId: number) { return this.request<{ items: AutomationScript[] }>('/scripts', ownerId); }
    saveScript(ownerId: number, body: AutomationScriptInput, id?: string) { return this.request<AutomationScript>(id ? `/scripts/${encodeURIComponent(id)}` : '/scripts', ownerId, id ? 'PUT' : 'POST', body); }
    deleteScript(ownerId: number, id: string) { return this.request<{ deleted: boolean }>(`/scripts/${encodeURIComponent(id)}`, ownerId, 'DELETE'); }
    createRun(ownerId: number, body: AutomationRunInput) { return this.request<AutomationRun>('/runs', ownerId, 'POST', body); }
    listRuns(ownerId: number) { return this.request<{ items: AutomationRun[] }>('/runs', ownerId); }
    getRun(ownerId: number, id: string) { return this.request<AutomationRun>(`/runs/${encodeURIComponent(id)}`, ownerId); }
    refreshRun(ownerId: number, id: string) { return this.request<AutomationRun>(`/runs/${encodeURIComponent(id)}/refresh`, ownerId, 'POST', {}); }
    cancelRun(ownerId: number, id: string) { return this.request<AutomationRun>(`/runs/${encodeURIComponent(id)}/cancel`, ownerId, 'POST', {}); }
}
