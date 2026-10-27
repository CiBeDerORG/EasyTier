/**
 * T13 - handoff of explicitly owned, run-dialog previews.
 *
 * A successful local WebRTC teardown is NOT authoritative cleanup. Read the
 * current owner-scoped device state after closing, and let the Rust create_run
 * transaction acquire the actual run lease. Never close other users' sessions.
 */
import type { AutomationDevice } from '../types/automation';

export interface TrialPreviewHandle {
    getSessionId(): string | null;
    getState(): { phase: string };
    close(): Promise<void>;
}
export interface TrialHandoffDeviceApi {
    devices(ownerId: number): Promise<{ items: AutomationDevice[] }>;
}
export interface TrialHandoffOptions {
    ownerId: number;
    requestedDeviceIds: string[];
    ownedPreviewIds: Set<string>;
    findPreview(deviceId: string): TrialPreviewHandle | undefined;
    onProgress(message: string): void;
    stillCurrent(): boolean;
    deviceName?(deviceId: string): string;
}

function requireCurrent(options: TrialHandoffOptions): void {
    if (!options.stillCurrent()) throw new Error('账号或试跑目标已变更，停止预览移交。');
}
function requireDevice(devices: AutomationDevice[], ownerId: number, id: string): AutomationDevice {
    const device = devices.find(item => item.id === id);
    if (!device || device.owner_user_id !== ownerId)
        throw new Error('手机已不在当前授权账号中，不能移交预览：' + id);
    return device;
}
async function latestDevices(api: TrialHandoffDeviceApi, options: TrialHandoffOptions): Promise<AutomationDevice[]> {
    requireCurrent(options);
    const response = await api.devices(options.ownerId);
    requireCurrent(options);
    return response.items;
}

/**
 * Call only from explicit "submit trial" flow. This closes precisely the
 * run-dialog sessions tracked by this page; a manually opened session from
 * another page cannot be adopted simply because the operator is an admin.
 *
 * No run is created here. The caller must re-read binding/fingerprint and use
 * the existing POST /runs after successful handoff.
 */
export async function releaseOwnedTrialPreviews(
    api: TrialHandoffDeviceApi, options: TrialHandoffOptions,
): Promise<void> {
    const selected = [...new Set(options.requestedDeviceIds)];
    const targets = selected.filter(id => options.ownedPreviewIds.has(id));
    if (!targets.length) return;

    // Fail before any teardown if a selected device has another active owner,
    // an unfinished script, or a preview whose identity no longer matches.
    const initial = await latestDevices(api, options);
    for (const id of targets) {
        const device = requireDevice(initial, options.ownerId, id);
        const handle = options.findPreview(id);
        if (!handle) throw new Error('手机预览卡片已丢失，不能确认移交：' + id);
        const sid = handle.getSessionId();
        if (device.run_occupied)
            throw new Error('手机已有未确认结束的脚本任务：' + (options.deviceName?.(id) ?? id));
        if (device.active_session && device.active_session.id !== sid)
            throw new Error('手机预览已被其他会话占用，不会强行关闭：' + (options.deviceName?.(id) ?? id));
        // No local handle but a remote active session must never be treated
        // as "already closed". Fail closed, leaving cleanup to its owner.
        if (device.active_session && !sid)
            throw new Error('预览会话所有权无法核实，请先人工处理：' + id);
    }

    for (const id of targets) {
        requireCurrent(options);
        const handle = options.findPreview(id);
        if (!handle) throw new Error('手机预览卡片已丢失，不能确认移交：' + id);
        const sid = handle.getSessionId();
        if (sid) {
            options.onProgress((options.deviceName?.(id) ?? id) + '：正在停止本页预览并清理控制资源');
            // close() must not swallow any cleanup rejection.
            await handle.close();
            requireCurrent(options);
            if (handle.getSessionId() || handle.getState().phase === 'cleanup_pending')
                throw new Error('预览资源清理尚未确认，已阻止执行脚本：' + id);
        }
        // An executor's HTTP close response may be delayed or unknown. Use
        // the main service's persisted session state, not only browser phase.
        const current = requireDevice(await latestDevices(api, options), options.ownerId, id);
        if (current.active_session || current.run_occupied)
            throw new Error('服务端仍报告会话或任务占用，不能移交脚本执行：' + (options.deviceName?.(id) ?? id));
        options.ownedPreviewIds.delete(id);
        options.onProgress((options.deviceName?.(id) ?? id) + '：已确认预览释放；等待服务端正式登记任务');
    }
}
