/**
 * T07 card timeline is a read-only projection of the ONE shared
 * AutomationDraftWorkspace. It never owns or persists a second script.
 */
import type { AutomationStep, AutomationStepType } from '../types/automation';
import type { AutomationDraftWorkspace } from './automation-draft';
import { analyzeAutomationFlow } from './automation-flow.ts';

export type DraftSaveState = 'new' | 'modified' | 'saved';
export interface CardStepRow {
    id: string;
    number: number;
    type: AutomationStepType;
    label: string;
    context: string;
    branch: string;
    depth: number;
    incomplete: boolean;
}
export interface CardStepProjection {
    title: string;
    editingId: string | null;
    revision: number | null;
    saveState: DraftSaveState;
    saveLabel: string;
    steps: CardStepRow[];
    flowErrorAt: number | null;
}

export const cardStepLabels: Record<AutomationStepType,string> = {
    key:'导航按键', tap:'点击', long_press:'长按', swipe:'滑动', drag:'拖拽',
    input_text:'输入文字', launch_app:'打开应用', wait_for:'等待出现',
    wait_gone:'等待消失', if_gone:'消失则执行', if_else:'If / Else 条件分支',
    assert:'检查控件', assert_property:'属性断言', scroll_find:'滚动查找',
    screenshot:'截图检查点',
};

function stepContext(step: AutomationStep): string {
    // Never reveal input_text.text, parameter values or selector text.
    if (step.type === 'key') return step.key ?? '未配置按键';
    if (step.type === 'if_gone') return 'Then · ' + (step.skip_count ?? '?') + ' 步';
    if (step.type === 'if_else') return 'Then · ' + (step.then_count ?? '?') + ' / Else · ' + (step.else_count ?? '?');
    if (step.selector?.resource_id) return 'Resource ID 定位';
    if (step.selector?.xpath) return 'XPath 定位';
    if (step.selector?.description) return '无障碍描述定位';
    if (step.selector?.text) return '文字定位';
    if (step.selector) return '组合控件定位';
    if (step.type === 'input_text') return step.parameter ? '参数化输入' : '待配置输入';
    if (step.type === 'launch_app') return '应用包名配置';
    if (step.x !== undefined && step.y !== undefined) return '画面坐标定位';
    if (step.type === 'screenshot') return '截图检查点';
    return '待配置目标';
}

export function projectCardSteps(draft: AutomationDraftWorkspace): CardStepProjection {
    const dirty = JSON.stringify({name:draft.name,description:draft.description,steps:draft.steps})
        !== draft.baseline;
    const saveState: DraftSaveState = !draft.editingId ? 'new' : dirty ? 'modified' : 'saved';
    const layout = analyzeAutomationFlow(draft.steps);
    return {
        title: draft.name.trim() || '未命名脚本草稿',
        editingId: draft.editingId ?? null,
        revision: draft.revision ?? null,
        saveState,
        // This indicator is ONLY the formal script publication state.
        // The adjacent syncLabel independently reports the server work-draft
        // CAS status (saving / unsynced / synced).
        saveLabel: saveState === 'new' ? '尚未发布为正式脚本'
            : saveState === 'modified' ? '正式脚本有未发布修改'
                : '正式脚本 · 修订 ' + (draft.revision ?? '—'),
        steps: draft.steps.map((step,index) => {
            const ancestry = layout.entries[index]?.ancestors ?? [];
            return {
                id:step.id, number:index+1, type:step.type,
                label: step.type === 'key' && !step.key ? '待配置步骤' : cardStepLabels[step.type],
                context:stepContext(step),
                branch:ancestry.map(item => '步骤 ' + (item.conditionIndex+1) + ' · ' +
                    (item.lane === 'then' ? 'Then' : 'Else')).join(' / '),
                depth:ancestry.length,
                incomplete: step.type === 'key' && !step.key,
            };
        }),
        flowErrorAt:layout.problem ? layout.problem.index+1 : null,
    };
}
