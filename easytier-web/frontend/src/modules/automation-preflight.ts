import type {
    AutomationAdbStatus, AutomationComponentStatus, AutomationDevice, AutomationStep,
} from '../types/automation';

/** This is an advisory snapshot; the Rust store and executor still authorize at dispatch and execution. */
export type PreflightState = 'checking' | 'ready' | 'needs_preparation' | 'unavailable' | 'query_failed';
export interface PreflightRow {
    id: string;
    name: string;
    targetFingerprint: string | null;
    state: PreflightState;
    message: string;
    selected: boolean;
    adbState?: AutomationAdbStatus['state'];
    componentState?: AutomationComponentStatus['state'];
}
export interface PreflightProbe {
    adbStatus(owner: number, id: string): Promise<AutomationAdbStatus>;
    componentStatus(owner: number, id: string): Promise<AutomationComponentStatus>;
}

export function requiresAutomationHelper(steps: AutomationStep[]): boolean {
    return steps.some(step => step.type !== 'key' && step.type !== 'launch_app');
}

export function initPreflightRow(device: AutomationDevice | undefined, id: string, ownedSessionId?: string | null): PreflightRow {
    const base: PreflightRow = {
        id, name: device?.name || id, targetFingerprint: device?.target_fingerprint ?? null,
        state: 'checking', message: '正在独立查询设备状态', selected: false,
    };
    if (!device) return { ...base, state: 'unavailable', message: '设备不在当前账号的最新设备列表' };
    if (!device.connection.enabled || !device.target_fingerprint)
        return { ...base, state: 'unavailable', message: 'ADB 服务未启用或设备绑定无效' };
    if (device.availability_reason)
        return { ...base, state: 'unavailable', message: '设备不可用：' + device.availability_reason };
    // Only an exact preview owned by this page may pass advisory checking.
    // Run dispatch must still close it and the database will recheck exclusivity.
    if (device.active_session && (!ownedSessionId || device.active_session.id !== ownedSessionId))
        return { ...base, state: 'unavailable', message: '设备正在被其他投屏或手动控制会话占用' };
    if (device.run_occupied)
        return { ...base, state: 'unavailable', message: '设备已有未结束脚本或待确认的清理任务' };
    return base;
}

export async function probePreflightRow(
    row: PreflightRow, owner: number, needsHelper: boolean, api: PreflightProbe,
): Promise<PreflightRow> {
    if (row.state !== 'checking') return row;
    try {
        const adb = await api.adbStatus(owner, row.id);
        if (adb.device_id !== row.id) throw new Error('ADB 检查返回了错误的设备 ID');
        if (adb.state !== 'connected') {
            const names: Record<string, string> = {
                offline: 'ADB 离线', unauthorized: 'ADB 未授权',
                disconnected: 'ADB 未连接', unavailable: 'ADB 状态查询超时或不可用',
            };
            return { ...row, state: 'unavailable', selected: false,
                adbState: adb.state, message: names[adb.state] || 'ADB 状态不可用' };
        }
        if (!needsHelper) return { ...row, state: 'ready', selected: true, adbState: adb.state, message: 'ADB 已连接，可执行原生操作' };
        const component = await api.componentStatus(owner, row.id);
        if (component.device_id !== row.id) throw new Error('控件组件检查返回了错误的设备 ID');
        if (component.state === 'ready')
            return { ...row, state: 'ready', selected: true, adbState: adb.state, componentState: component.state,
                message: 'ADB 和 UiAutomator2 已就绪' };
        if (component.state === 'not_prepared' || component.state === 'preparing')
            return { ...row, state: 'needs_preparation', selected: false, adbState: adb.state, componentState: component.state,
                message: component.state === 'preparing' ? '控件组件正在准备，请稍后重新检查' : '控件组件尚未准备，请先在工作台初始化' };
        return { ...row, state: 'unavailable', selected: false, adbState: adb.state, componentState: component.state,
            message: component.state === 'adb_unavailable' ? '控件组件检测时 ADB 不可用'
                : component.state === 'unavailable' ? '控件组件不可用'
                : '控件组件状态查询失败' };
    } catch (cause) {
        const message = cause instanceof Error ? cause.message : '状态查询失败';
        return { ...row, state: 'query_failed', selected: false,
            message: '预检失败：' + message };
    }
}

/**
 * Each device is probed independently, with bounded parallelism and immediate
 * individual result updates. Never calls pair, connect, prepare or any action endpoint.
 */
export async function probePreflightDevices(
    rows: PreflightRow[], owner: number, needsHelper: boolean,
    api: PreflightProbe, onUpdate: (row: PreflightRow) => void,
    stillValid: () => boolean, concurrency = 3,
): Promise<void> {
    const queue = rows.filter(row => row.state === 'checking');
    const worker = async () => {
        while (queue.length && stillValid()) {
            const row = queue.shift()!;
            const result = await probePreflightRow(row, owner, needsHelper, api);
            if (!stillValid()) return;
            onUpdate(result);
        }
    };
    await Promise.all(Array.from({ length: Math.min(queue.length, Math.max(1, Math.floor(concurrency))) }, () => worker()));
}

export function selectedReadyIds(rows: PreflightRow[]): string[] {
    return rows.filter(row => row.state === 'ready' && row.selected).map(row => row.id);
}
