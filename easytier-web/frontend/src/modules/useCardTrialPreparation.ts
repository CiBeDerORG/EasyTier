/**
 * T12 · Prepare a phone for a single-step trial without taking its run lease.
 * T13 owns handoff of this page's preview and the authoritative create_run lock.
 * No implicit wireless pairing, privileged ADB shortcut, or action replay.
 */
import { reactive } from 'vue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { AutomationApiError, automationErrorMessage } from './automation-api';
import { initPreflightRow, requiresAutomationHelper } from './automation-preflight';
import type { AutomationStep, AutomationPreparation } from '../types/automation';

export type CardTrialPhase =
    | 'queued' | 'binding' | 'adb_check' | 'connecting'
    | 'helper_check' | 'helper_start' | 'helper_wait'
    | 'ready' | 'failed' | 'unknown' | 'cancelled';

export interface CardTrialTarget {
    ownerId: number;
    deviceId: string;
    targetFingerprint: string;
    steps: AutomationStep[];
    /** The page may inspect its own preview, but T13 must release it before create_run. */
    ownedSessionId?: string | null;
}

export interface CardTrialProgress {
    ownerId: number;
    deviceId: string;
    targetFingerprint: string;
    phase: CardTrialPhase;
    message: string;
    updatedAt: string;
    stage?: string;
    preparationId?: string;
    requestId?: string;
    adbState?: string;
    componentState?: string;
}

type TrialApi = Pick<AutomationApi,
    'devices' | 'adbStatus' | 'componentStatus' | 'prepare' |
    'preparationHistory' | 'startPreparation' | 'preparationDetail' | 'cancelPreparation'>;

export interface CardTrialPreparationOptions {
    concurrency?: number;
    requestTimeoutMs?: number;
    preparationTimeoutMs?: number;
    pollIntervalMs?: number;
}

class StaleTrial extends Error {}
class UnknownTrial extends Error {}

const ACTIVE_PREPARATIONS = new Set(['queued', 'running', 'cancel_requested']);
const DONE_PREPARATIONS = new Set(['succeeded', 'failed', 'interrupted', 'cancelled', 'cleanup_pending']);
const stamp = () => new Date().toISOString();
const targetKey = (ownerId: number, deviceId: string) => ownerId + ':' + deviceId;

function timeout<T>(work: Promise<T>, ms: number, stage: string): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const deadline = new Promise<T>((_, reject) => {
        timer = setTimeout(() => reject(new Error(stage + '超时')), ms);
    });
    return Promise.race([work, deadline]).finally(() => { if (timer !== undefined) clearTimeout(timer); });
}

/** Caller must call dispose() on unmount/account handoff. */
export function useCardTrialPreparation(
    api: TrialApi = new AutomationApi(),
    options: CardTrialPreparationOptions = {},
) {
    const state = reactive<Record<string, CardTrialProgress>>({});
    const maxParallel = Math.max(1, Math.min(8, Math.floor(options.concurrency ?? 3)));
    const requestTimeoutMs = Math.max(1000, options.requestTimeoutMs ?? 12000);
    const preparationTimeoutMs = Math.max(10000, options.preparationTimeoutMs ?? 120000);
    const pollIntervalMs = Math.max(500, options.pollIntervalMs ?? 2000);
    const versions = new Map<string, number>();
    const active = new Map<string, Promise<CardTrialProgress>>();
    const runningPreparations = new Map<string, string>();
    const waiting: Array<() => void> = [];
    let slots = 0;
    let disposed = false;

    async function acquire(): Promise<void> {
        if (slots < maxParallel) { ++slots; return; }
        await new Promise<void>(resolve => waiting.push(resolve));
    }
    function release(): void {
        const next = waiting.shift();
        if (next) next(); // Transfer the reserved slot rather than exceeding the limit.
        else --slots;
    }
    function live(key: string, version: number): boolean {
        return !disposed && versions.get(key) === version;
    }
    function assertLive(key: string, version: number): void {
        if (!live(key, version)) throw new StaleTrial();
    }
    function report(key: string, version: number, patch: Partial<CardTrialProgress>): void {
        if (live(key, version)) state[key] = { ...state[key], ...patch, updatedAt: stamp() };
    }
    async function read<T>(key: string, version: number, stage: string, operation: Promise<T>): Promise<T> {
        const result = await timeout(operation, requestTimeoutMs, stage);
        assertLive(key, version);
        return result;
    }
    function stopError(error: unknown): string {
        return automationErrorMessage(error);
    }
    async function pause(key: string, version: number): Promise<void> {
        await new Promise<void>(resolve => setTimeout(resolve, pollIntervalMs));
        assertLive(key, version);
    }
    async function binding(target: CardTrialTarget, key: string, version: number): Promise<void> {
        report(key, version, { phase: 'binding', message: '核对当前账号授权、ADB 绑定及占用状态' });
        const latest = await read(key, version, '设备绑定查询', api.devices(target.ownerId));
        const device = latest.items.find(item => item.id === target.deviceId);
        if (!device || device.owner_user_id !== target.ownerId ||
            !target.targetFingerprint || device.target_fingerprint !== target.targetFingerprint)
            throw new Error('设备不存在、未获授权或绑定已经变化，请重新选择手机');
        const row = initPreflightRow(device, target.deviceId, target.ownedSessionId);
        if (row.state !== 'checking') throw new Error(row.message);
    }
    async function confirmReady(target: CardTrialTarget, key: string, version: number): Promise<void> {
        // A preparation result is advisory; reconfirm the target and state.
        await binding(target, key, version);
        const adb = await read(key, version, 'ADB 复核', api.adbStatus(target.ownerId, target.deviceId));
        if (adb.device_id !== target.deviceId || adb.state !== 'connected')
            throw new Error('准备完成后 ADB 已不再连接');
        if (requiresAutomationHelper(target.steps)) {
            const component = await read(key, version, '控件组件复核', api.componentStatus(target.ownerId, target.deviceId));
            if (component.device_id !== target.deviceId || component.state !== 'ready')
                throw new Error('准备完成后 UiAutomator2 尚未就绪');
            report(key, version, { componentState: component.state });
        }
        report(key, version, {
            phase: 'ready', adbState: adb.state,
            message: '设备准备就绪；执行前仍需释放本页预览并通过服务端独占校验',
        });
    }
    async function awaitPreparation(
        target: CardTrialTarget, key: string, version: number, job: AutomationPreparation,
    ): Promise<void> {
        runningPreparations.set(key, job.id);
        const started = Date.now();
        for (;;) {
            assertLive(key, version);
            report(key, version, {
                phase: 'helper_wait', preparationId: job.id, stage: job.stage,
                message: '初始化控件组件：' + (job.message || job.stage || job.state),
            });
            if (DONE_PREPARATIONS.has(job.state)) {
                if (job.state !== 'succeeded') throw new Error(
                    '控件组件准备' + (job.message || job.code || job.state) + '；请检查初始化日志',
                );
                return;
            }
            if (!ACTIVE_PREPARATIONS.has(job.state))
                throw new UnknownTrial('组件准备任务状态无法确认，请查询现有任务记录');
            if (Date.now() - started >= preparationTimeoutMs)
                throw new UnknownTrial('等待控件组件初始化超时，任务可能仍在手机上运行；请先查询记录');
            await pause(key, version);
            try {
                job = await read(key, version, '准备任务查询',
                    api.preparationDetail(target.ownerId, target.deviceId, job.id));
            } catch (error) {
                if (error instanceof StaleTrial) throw error;
                throw new UnknownTrial('无法确认初始化任务结果：' + stopError(error));
            }
            if (job.device_id !== target.deviceId || job.target_fingerprint !== target.targetFingerprint)
                throw new Error('初始化任务绑定与当前手机不一致');
        }
    }
    async function execute(target: CardTrialTarget, key: string, version: number, allowActions: boolean): Promise<CardTrialProgress> {
        await acquire();
        try {
            assertLive(key, version);
            await binding(target, key, version);
            report(key, version, { phase: 'adb_check', message: '读取 ADB 连接状态' });
            let adb = await read(key, version, 'ADB 状态查询', api.adbStatus(target.ownerId, target.deviceId));
            if (adb.device_id !== target.deviceId) throw new Error('ADB 状态来自另一台手机');
            report(key, version, { adbState: adb.state });
            if (adb.state !== 'connected') {
                if (!['offline', 'disconnected'].includes(adb.state))
                    throw new Error(adb.state === 'unauthorized'
                        ? '手机未授权 ADB，请人工确认授权' : 'ADB 不可用，请检查网络与无线调试');
                if (!allowActions) throw new Error('ADB 尚未连接，当前仅查询状态，不重复发起连接');
                report(key, version, { phase: 'connecting', message: '使用现有授权端点连接 ADB（不进行无线配对）' });
                try {
                    await read(key, version, 'ADB 连接', api.prepare(target.ownerId, target.deviceId, { action: 'connect' }));
                } catch (error) {
                    if (error instanceof StaleTrial) throw error;
                    if (error instanceof AutomationApiError && [400, 401, 403, 404, 409, 422].includes(error.status)) throw error;
                    throw new UnknownTrial('连接请求结果未知，请先检查 ADB 状态：' + stopError(error));
                }
                adb = await read(key, version, 'ADB 连接复核', api.adbStatus(target.ownerId, target.deviceId));
                if (adb.device_id !== target.deviceId || adb.state !== 'connected')
                    throw new Error('已尝试连接，但 ADB 未就绪；不会自动重复连接');
                report(key, version, { adbState: adb.state });
            }
            if (!requiresAutomationHelper(target.steps)) {
                await confirmReady(target, key, version);
                return state[key];
            }
            report(key, version, { phase: 'helper_check', message: '查询 UiAutomator2 组件状态' });
            const component = await read(key, version, '控件组件状态', api.componentStatus(target.ownerId, target.deviceId));
            if (component.device_id !== target.deviceId) throw new Error('控件组件状态来自另一台手机');
            report(key, version, { componentState: component.state });
            if (component.state === 'ready') {
                await confirmReady(target, key, version);
                return state[key];
            }
            if (!['preparing', 'not_prepared'].includes(component.state))
                throw new Error('控件组件状态异常：' + component.state);
            // Recover an already-running initialization before starting a new one.
            const history = await read(key, version, '组件初始化历史',
                api.preparationHistory(target.ownerId, target.deviceId));
            let job = history.items.find(item => item.device_id === target.deviceId &&
                item.target_fingerprint === target.targetFingerprint && ACTIVE_PREPARATIONS.has(item.state));
            if (!job) {
                if (component.state === 'preparing')
                    throw new UnknownTrial('UiAutomator2 显示准备中但找不到可确认任务，请查看工作台');
                if (!allowActions) throw new Error('UiAutomator2 未准备；当前仅查询，不重新创建初始化任务');
                // A timed-out POST may have been accepted. Do not submit another UUID automatically.
                const requestId = uuid();
                report(key, version, {
                    phase: 'helper_start', requestId, message: '提交一次 UiAutomator2 初始化任务',
                });
                try {
                    job = await read(key, version, '组件初始化提交',
                        api.startPreparation(target.ownerId, target.deviceId, requestId));
                } catch (error) {
                    if (error instanceof StaleTrial) throw error;
                    if (error instanceof AutomationApiError && [400, 401, 403, 404, 409, 422].includes(error.status)) throw error;
                    throw new UnknownTrial('初始化请求结果未知（请求编号 ' + requestId + '），请先查询准备历史');
                }
            }
            if (job.device_id !== target.deviceId || job.target_fingerprint !== target.targetFingerprint)
                throw new Error('返回的控件组件任务不属于当前绑定');
            await awaitPreparation(target, key, version, job);
            await confirmReady(target, key, version);
        } catch (error) {
            if (error instanceof StaleTrial) {
                // The caller already cancelled or switched devices. Never publish a late response.
            } else {
                const unknown = error instanceof UnknownTrial;
                report(key, version, {
                    phase: unknown ? 'unknown' : 'failed',
                    message: stopError(error) + (unknown ? '；不会自动重复有副作用的操作' : ''),
                });
            }
        } finally {
            release();
            runningPreparations.delete(key);
        }
        return state[key];
    }
    function run(target: CardTrialTarget, allowActions = true): Promise<CardTrialProgress> {
        const key = targetKey(target.ownerId, target.deviceId);
        const existing = active.get(key);
        if (existing) return existing;
        if (disposed) return Promise.reject(new Error('试跑准备管理器已销毁'));
        // Unknown side-effect outcomes can only be reconciled through read-only probes.
        if (allowActions && state[key]?.phase === 'unknown')
            return Promise.resolve(state[key]);
        const version = (versions.get(key) ?? 0) + 1;
        versions.set(key, version);
        state[key] = {
            ownerId: target.ownerId, deviceId: target.deviceId,
            targetFingerprint: target.targetFingerprint,
            phase: 'queued', message: '等待空闲的设备准备并发名额', updatedAt: stamp(),
        };
        const work = execute(target, key, version, allowActions);
        active.set(key, work);
        void work.finally(() => { if (active.get(key) === work) active.delete(key); });
        return work;
    }
    /** Read-only recovery after an uncertain POST; never starts a new request. */
    function reconcile(target: CardTrialTarget): Promise<CardTrialProgress> {
        return run(target, false);
    }
    /** Cancelling stops further client requests; it does not silently destroy remote jobs. */
    async function cancel(ownerId: number, deviceId: string): Promise<void> {
        const key = targetKey(ownerId, deviceId);
        versions.set(key, (versions.get(key) ?? 0) + 1);
        const row = state[key];
        if (row) state[key] = { ...row, phase: 'cancelled',
            message: '已停止本页准备流程；正在执行的服务端准备任务需确认清理', updatedAt: stamp() };
        const jobId = runningPreparations.get(key);
        if (jobId && !disposed) {
            try {
                await timeout(api.cancelPreparation(ownerId, deviceId, jobId), requestTimeoutMs, '取消初始化');
            } catch (error) {
                const latest = state[key];
                if (latest?.phase === 'cancelled') state[key] = { ...latest,
                    message: '本页已取消，但服务端清理结果未确认：' + stopError(error), updatedAt: stamp() };
            }
        }
    }
    function dispose(): void {
        disposed = true;
        for (const key of versions.keys()) versions.set(key, (versions.get(key) ?? 0) + 1);
        // In-flight HTTP requests cannot be rolled back; late replies are ignored.
    }
    return { state, run, reconcile, cancel, dispose };
}
