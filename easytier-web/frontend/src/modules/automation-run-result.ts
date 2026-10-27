/** Read-only terminal/step status translation shared by UI and T17 tests. */
import type { AutomationRun, AutomationRunDevice, AutomationStepResult } from '../types/automation';

export type CardRunPhase = 'idle' | 'checking' | 'queued' | 'running' |
    'succeeded' | 'action_sent' | 'failed' | 'cancelled' | 'unknown';
export interface CardRunView {
    phase: CardRunPhase;
    label: string;
    deviceState: string;
    stepState: string;
    stepId: string;
    runId: string;
    detail: string;
}
const TERMINAL = new Set(['succeeded', 'failed', 'cancelled', 'interrupted', 'outcome_unknown',
    'cleanup_pending', 'completed', 'partial_failed']);
const OK = new Set(['succeeded', 'completed']);
const SUCCESSFUL_STEPS = new Set(['succeeded','completed','assertion_passed','condition_passed','passed','action_completed']);
const FAIL = new Set(['failed', 'partial_failed']);
const UNKNOWN = new Set(['outcome_unknown', 'cleanup_pending']);

export function summarizeCardRun(
    run: AutomationRun, deviceId: string, stepId: string,
): CardRunView {
    const device: AutomationRunDevice | undefined = run.devices.find(d => d.device_id === deviceId);
    const step: AutomationStepResult | undefined = device?.steps.find(s => s.step_id === stepId);
    const deviceState = device?.state ?? 'not_reported';
    const stepState = step?.state ?? 'pending';
    const detail = step?.message || (typeof step?.error === 'string' ? step.error : step?.error?.message) ||
        (typeof device?.error === 'string' ? device.error : device?.error?.message) || '';
    const state = UNKNOWN.has(stepState) || UNKNOWN.has(deviceState) || UNKNOWN.has(run.state)
        ? 'unknown'
        : stepState === 'skipped' ? 'failed'
        : FAIL.has(stepState) || FAIL.has(deviceState) ? 'failed'
        : ['cancelled', 'interrupted'].includes(stepState) ||
            ['cancelled', 'interrupted'].includes(deviceState) ? 'cancelled'
        : stepState === 'action_sent' && OK.has(deviceState) ? 'action_sent'
        : SUCCESSFUL_STEPS.has(stepState) && OK.has(deviceState) ? 'succeeded'
        : TERMINAL.has(deviceState) && !OK.has(stepState) ? 'failed'
        : ['running', 'starting'].includes(stepState) || ['running', 'starting'].includes(deviceState)
            ? 'running' : 'queued';
    const label: Record<CardRunPhase, string> = {
        idle:'尚未执行',checking:'正在查询',queued:'任务排队中',running:'设备执行中',
        succeeded:'本步骤执行已完成（业务结果需独立验证）',
        action_sent:'动作已发送至设备；尚未验证应用业务结果',
        failed:'本步骤执行失败',cancelled:'已取消或中断',
        unknown:'执行结果或清理未确认，禁止自动重放',
    };
    return {phase:state,label:label[state],deviceState,stepState,stepId,runId:run.id,detail};
}

