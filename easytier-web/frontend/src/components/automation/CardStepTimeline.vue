<script setup lang="ts">
import { computed, ref } from 'vue';
import AutomationInlineSelect from './AutomationInlineSelect.vue';
import type { AutomationRun, AutomationStepResult, AutomationStepType } from '../../types/automation';
import { analyzeAutomationFlow, canInsertBranchStep, type BranchLane } from '../../modules/automation-flow';
import type { AutomationDraftWorkspace } from '../../modules/automation-draft';
import { projectCardSteps } from '../../modules/card-step-projection';
import type { InlineStepRunProgress } from '../../modules/useInlineStepRun';
import InlineStepRunStatus from './InlineStepRunStatus.vue';

const props=defineProps<{
    draft: AutomationDraftWorkspace;
    undoLabel?: string;
    pickerActive?: boolean;
    trialProgress?: InlineStepRunProgress | null;
    trialActive?: boolean;
    batchRun?: AutomationRun | null;
    batchDeviceId?: string;
}>();
const emit=defineEmits<{
    select: [stepId: string];
    edit: [stepId: string];
    trial: [stepId: string];
    'inspect-trial': [];
    delete: [stepId: string];
    undo: [];
    create: [type:AutomationStepType];
    branch: [conditionId:string,lane:BranchLane,type:AutomationStepType];
    convert: [stepId:string,type:'if_gone'|'if_else'];
    move: [stepId:string,delta:-1|1];
}>();
const projected=computed(()=>projectCardSteps(props.draft));
const expandedErrors=ref<string[]>([]);
function toggleError(id:string){expandedErrors.value=expandedErrors.value.includes(id)
    ?expandedErrors.value.filter(x=>x!==id):[...expandedErrors.value,id];}
function errorMessage(error:string|{code?:string;message?:string}|undefined):string {
    if(typeof error==='string')return error;
    return error ? [error.code,error.message].filter(Boolean).join(' · ') : '';
}
const batchDevice=computed(()=>{
    const run=props.batchRun;
    if(!run||!props.batchDeviceId||run.script_snapshot.steps.length!==props.draft.steps.length
        ||run.script_snapshot.steps.some((s,i)=>s.id!==props.draft.steps[i]?.id))return null;
    return run.devices.find(d=>d.device_id===props.batchDeviceId)??null;
});
const batchResults=computed(()=>new Map((batchDevice.value?.steps??[]).map(s=>[s.step_id,s])));
function failureInfo(id:string):{result:AutomationStepResult|null;message:string;code:string;runId:string}|null{
    const result=batchResults.value.get(id),device=batchDevice.value,run=props.batchRun;
    if(!result || !['failed','interrupted','outcome_unknown'].includes(result.state))return null;
    const err=result.error;
    const code=typeof err==='object' ? err.code??'' : '';
    const message=[errorMessage(err),result.message,
        errorMessage(device?.error)].filter(Boolean).filter((x,i,a)=>a.indexOf(x)===i).join('；');
    return {result,message:message||'执行器未返回详细错误；可查看本次运行 ID 查询服务端记录。',
        code,runId:run?.id??''};
}

/** Durable per-step executor results, scoped to exactly this device and script
 * snapshot. Never color a modified draft from an older run. */
const batchStepStates=computed(()=>{
    const run=props.batchRun,deviceId=props.batchDeviceId;
    const map=new Map<string,'running'|'succeeded'|'failed'|'skipped'>();
    if(!run||!deviceId || run.script_snapshot.steps.length!==props.draft.steps.length ||
        run.script_snapshot.steps.some((s,i)=>s.id!==props.draft.steps[i]?.id))return map;
    const device=run.devices.find(x=>x.device_id===deviceId);
    if(!device)return map;
    for(const step of device.steps){
        if(['failed','interrupted','outcome_unknown'].includes(step.state))
            map.set(step.step_id,'failed');
        else if(['succeeded','completed','success','action_sent'].includes(step.state))
            map.set(step.step_id,'succeeded');
        else if(['skipped','skip'].includes(step.state))
            map.set(step.step_id,'skipped');
        else if(['running','starting','executing'].includes(step.state))
            map.set(step.step_id,'running');
    }
    if(typeof device.current_step==='string' && props.draft.steps.some(s=>s.id===device.current_step)
        && ['running','starting','accepted'].includes(device.state)
        && !map.has(device.current_step))
        map.set(device.current_step,'running');
    return map;
});
const layout=computed(()=>analyzeAutomationFlow(props.draft.steps));
const insertType=ref<AutomationStepType>('key');
const actions:{value:AutomationStepType;name:string}[]=[
    {value:'key',name:'导航按键'},{value:'tap',name:'点击'},
    {value:'long_press',name:'长按'},{value:'swipe',name:'滑动'},
    {value:'drag',name:'拖拽'},{value:'input_text',name:'输入文字'},
    {value:'launch_app',name:'启动应用'},{value:'wait_for',name:'等待控件'},
    {value:'wait_gone',name:'等待消失'},{value:'assert',name:'断言控件'},
    {value:'assert_property',name:'断言属性'},{value:'scroll_find',name:'滚动查找'},
    {value:'screenshot',name:'截图检查点'},
];
const blocked=computed(()=>!!layout.value.problem);
const selectedAction=computed(()=>props.draft.steps.find(s=>s.id===props.draft.selectedStepId &&
    s.type!=='if_gone' && s.type!=='if_else'));
function canAddBranch(id:string,lane:BranchLane):boolean {
    const index=props.draft.steps.findIndex(step=>step.id===id);
    return !blocked.value && canInsertBranchStep(props.draft.steps,layout.value,index,lane);
}
function canMove(id:string,delta:-1|1):boolean{
    if(blocked.value)return false;
    const at=props.draft.steps.findIndex(step=>step.id===id),next=at+delta;
    if(at<0||next<0||next>=props.draft.steps.length)return false;
    const first=props.draft.steps[at],second=props.draft.steps[next];
    if(['if_gone','if_else'].includes(first.type)||['if_gone','if_else'].includes(second.type))return false;
    const left=layout.value.entries[at]?.ancestors??[];
    const right=layout.value.entries[next]?.ancestors??[];
    return JSON.stringify(left)===JSON.stringify(right);
}
</script>

<template>
    <!-- The ONLY visible timeline: parent ScriptSelector owns script name/count/collapse. -->
    <section class="card-steps" aria-label="当前脚本步骤">
        <div v-if="trialActive" class="trial-busy-note" role="status">
            <span>当前手机仍有单步试跑等待结束
                <template v-if="trialProgress?.runId"> · 任务 {{ trialProgress.runId.slice(0,8) }}</template>
                <template v-if="trialProgress?.phase"> · {{ trialProgress.label }}</template>
            </span>
            <button type="button" class="trial-inspect" @click.stop="emit('inspect-trial')">
                查看状态 / 停止清理
            </button>
        </div>
        <div v-if="undoLabel" class="undo-line" role="status">
            <span>{{ undoLabel }}</span>
            <button type="button" @click="emit('undo')">撤销删除</button>
        </div>
        <p v-if="projected.flowErrorAt" class="flow-warning" role="status">
            第 {{ projected.flowErrorAt }} 步条件分支不完整，请先修复结构。
        </p>
        <div v-if="!projected.steps.length" class="no-steps">
            暂无步骤。开启控件拾取后，可在原手机画面右键录入。
        </div>
        <ol v-else class="timeline" aria-label="脚本步骤时间线">
            <li v-for="step in projected.steps" :key="step.id" class="timeline-item"
                :class="{ active: step.id === draft.selectedStepId,
                    'batch-running': batchStepStates.get(step.id)==='running',
                    'batch-success': batchStepStates.get(step.id)==='succeeded',
                    'batch-failed': batchStepStates.get(step.id)==='failed',
                    'batch-skipped': batchStepStates.get(step.id)==='skipped' }">
                <button class="step-select" type="button"
                    :aria-pressed="step.id === draft.selectedStepId"
                    :aria-label="'选中第 '+step.number+' 步'"
                    :title="'选中第 '+step.number+' 步：'+step.label" @click="emit('select',step.id)">
                    <span class="number">{{ step.number }}</span>
                    <span class="step-info" :style="{ paddingLeft: Math.min(step.depth,3)*6+'px' }">
                        <strong>{{ step.label }}</strong>
                        <small v-if="step.branch" class="branch">{{ step.branch }}</small>
                        <small>{{ step.context }}{{ step.incomplete ? ' · 待完善' : '' }}</small>
                        <small v-if="batchStepStates.get(step.id)" class="batch-step-label">
                            {{ batchStepStates.get(step.id)==='running'?'● 当前执行'
                                : batchStepStates.get(step.id)==='succeeded'?'✓ 已完成'
                                : batchStepStates.get(step.id)==='failed'?'✕ 执行失败':'— 已跳过' }}
                        </small>
                        <InlineStepRunStatus v-if="trialProgress?.stepId === step.id"
                            :progress="trialProgress" />
                    </span>
                </button>
                <div v-if="failureInfo(step.id)" class="batch-step-failure">
                    <button type="button" class="batch-error-toggle" :aria-expanded="expandedErrors.includes(step.id)"
                        @click.stop="toggleError(step.id)">
                        {{ expandedErrors.includes(step.id)?'收起失败详情':'查看失败原因' }}
                    </button>
                </div>
                <div v-if="failureInfo(step.id) && expandedErrors.includes(step.id)"
                    class="batch-error-detail" role="alert">
                    <strong>第 {{step.number}} 步 · {{failureInfo(step.id)?.result?.state}}</strong>
                    <span v-if="failureInfo(step.id)?.code">错误码：{{failureInfo(step.id)?.code}}</span>
                    <p>{{failureInfo(step.id)?.message}}</p>
                    <small>运行 ID：{{failureInfo(step.id)?.runId}}</small>
                </div>
                <button type="button" class="step-move" :disabled="!canMove(step.id,-1)"
                    :aria-label="'上移第 '+step.number+' 步'" title="只在同一条件分支内上移"
                    @click.stop="emit('move',step.id,-1)"><i class="pi pi-arrow-up" aria-hidden="true" /></button>
                <button type="button" class="step-move" :disabled="!canMove(step.id,1)"
                    :aria-label="'下移第 '+step.number+' 步'" title="只在同一条件分支内下移"
                    @click.stop="emit('move',step.id,1)"><i class="pi pi-arrow-down" aria-hidden="true" /></button>
                <button type="button" class="step-trial" :aria-label="'试跑第 '+step.number+' 步'"
                    title="当前手机单步骤真实试跑"
                    :disabled="trialActive || step.incomplete || step.type === 'if_gone' || step.type === 'if_else'"
                    @click.stop="emit('trial',step.id)">
                    <i class="pi pi-play" aria-hidden="true" />
                </button>
                <button type="button" class="step-edit" :aria-label="'编辑第 '+step.number+' 步'"
                    title="编辑步骤" @click.stop="emit('edit',step.id)">
                    <i class="pi pi-pencil" aria-hidden="true" />
                </button>
                <button type="button" class="step-delete" :aria-label="'删除第 '+step.number+' 步'"
                    title="删除此步骤或其所属条件块" @click.stop="emit('delete',step.id)">
                    <i class="pi pi-trash" aria-hidden="true" />
                </button>
                <div v-if="step.type==='if_gone'||step.type==='if_else'" class="branch-tools">
                    <button type="button" :disabled="!canAddBranch(step.id,'then')"
                        :aria-label="'向第'+step.number+'步 Then 添加'+insertType"
                        @click.stop="emit('branch',step.id,'then',insertType)">+ Then</button>
                    <button v-if="step.type==='if_else'" type="button" :disabled="!canAddBranch(step.id,'else')"
                        :aria-label="'向第'+step.number+'步 Else 添加'+insertType"
                        @click.stop="emit('branch',step.id,'else',insertType)">+ Else</button>
                </div>
            </li>
        </ol>
        <div class="flow-tools" aria-label="结构编辑工具">
            <div class="flow-action-field"><span>插入动作</span>
                <AutomationInlineSelect :model-value="insertType" @update:model-value="insertType=String($event) as AutomationStepType" input-id="automation-insert-action"
                    :options="actions" option-label="name" option-value="value"
                    aria-label="插入动作类型" class="insert-action-select" />
            </div>
            <button type="button" :disabled="blocked || draft.steps.length>=100"
                @click="emit('create',insertType)">+ 新增</button>
        </div>
        <div v-if="selectedAction" class="condition-tools">
            <small>将选中普通步骤转换为条件（会自动创建待完善的分支占位步骤）</small>
            <button type="button" :disabled="blocked || draft.steps.length+1>100"
                @click="emit('convert',selectedAction!.id,'if_gone')">转为 If 消失</button>
            <button type="button" :disabled="blocked || draft.steps.length+2>100"
                @click="emit('convert',selectedAction!.id,'if_else')">转为 If/Else</button>
        </div>
        <small class="draft-hint">授权手机共用唯一草稿，新增步骤须配置完成。▶ 才是真实试跑；定位演示只读。</small>
    </section>
</template>

<style scoped>
.card-steps{padding:3px 0 0;min-width:0}
.trial-busy-note{display:flex;align-items:center;flex-wrap:wrap;gap:6px;margin:4px 0;padding:7px 9px;border:1px solid var(--console-border);border-radius:7px;font-size:11px;overflow-wrap:anywhere}
.trial-busy-note span{flex:1 1 150px;min-width:0}
.trial-inspect{border:1px solid var(--p-primary-color);border-radius:6px;background:transparent;color:var(--p-primary-color);cursor:pointer;padding:5px 8px;font-size:11px}
.timeline{list-style:none;margin:6px 0 0;padding:0;display:flex;flex-direction:column;gap:3px;max-height:260px;overflow-y:auto;overscroll-behavior:contain}
.timeline-item{display:flex;align-items:stretch;flex-wrap:wrap;min-width:0;border:1px solid var(--console-border);border-radius:7px}
.timeline-item.active{background:var(--console-accent-soft);border-color:var(--p-primary-color)}
.step-select{display:flex;align-items:center;min-width:0;gap:5px;flex:1;background:transparent;color:inherit;border:0;text-align:left;padding:6px 3px;cursor:pointer}
.number{display:grid;place-items:center;width:20px;height:20px;flex:0 0 20px;border-radius:50%;border:1px solid var(--console-border);font-size:10px;font-variant-numeric:tabular-nums}
.timeline-item.active .number{border-color:var(--p-primary-color);color:var(--p-primary-color)}
.timeline-item.batch-running{border-color:var(--p-primary-color);background:var(--console-accent-soft);box-shadow:inset 3px 0 var(--p-primary-color)}
.timeline-item.batch-running .number,.timeline-item.batch-running .batch-step-label{color:var(--p-primary-color);font-weight:700}
.timeline-item.batch-success{border-color:var(--p-green-500);background:color-mix(in srgb,var(--p-green-500) 9%,transparent)}
.timeline-item.batch-success .number,.timeline-item.batch-success .batch-step-label{color:var(--p-green-500);font-weight:700}
.timeline-item.batch-failed{border-color:var(--p-red-500);background:color-mix(in srgb,var(--p-red-500) 12%,transparent)}
.timeline-item.batch-failed .number,.timeline-item.batch-failed .batch-step-label{color:var(--p-red-500);font-weight:700}
.timeline-item.batch-skipped{opacity:.7}
.batch-step-failure{order:18;flex-basis:100%;min-width:0;padding:0 6px 3px 31px;white-space:normal}
.batch-error-toggle{border:0;background:transparent;padding:1px 0;text-align:left;
    color:var(--p-red-500);font-size:10px;text-decoration:underline;cursor:pointer}
.batch-error-detail{order:20;width:100%;display:grid;gap:4px;padding:8px;margin:3px 6px 6px;
    border-radius:6px;border:1px solid var(--p-red-500);
    background:color-mix(in srgb,var(--p-red-500) 6%,var(--console-panel));font-size:11px}
.batch-error-detail p{white-space:pre-wrap;overflow-wrap:anywhere;margin:0;line-height:1.55}
.batch-error-detail small{overflow-wrap:anywhere;color:var(--console-muted)}
.timeline-item.batch-running .step-select:focus-visible,.timeline-item.batch-failed .step-select:focus-visible{outline-offset:-2px}
.step-info{min-width:0;display:flex;flex-direction:column;gap:2px}
.step-info strong{font-size:11px;line-height:1.25;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step-info small{font-size:10px;color:var(--console-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step-info .branch{color:var(--p-primary-color)}
.step-trial,.step-edit,.step-delete,.step-move{flex:0 0 23px;width:23px;padding:0;border:0;border-radius:5px;background:transparent;cursor:pointer;font-size:11px}
.step-move{color:var(--console-muted)}
.step-move:disabled{opacity:.35;cursor:not-allowed}
.step-trial{color:var(--p-green-500)}
.step-trial:disabled{opacity:.35;cursor:not-allowed}
.step-edit{color:var(--console-muted)}
.step-edit:hover{color:var(--p-primary-color)}
.step-delete{color:var(--p-red-500)}
.timeline-item>button:hover:not(:disabled){background:var(--console-accent-soft)}
.timeline-item>button:focus-visible,.undo-line button:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:1px}
.no-steps,.flow-warning{margin:8px 0;font-size:11px;line-height:1.5;color:var(--console-muted)}
.flow-warning{color:var(--p-orange-500)}
.draft-hint{display:block;margin-top:6px;font-size:10px;line-height:1.4;color:var(--console-muted)}
.flow-tools,.condition-tools,.branch-tools{display:flex;gap:5px;align-items:center;flex-wrap:wrap;font-size:10px}
.flow-tools,.condition-tools{padding:6px 0;border-top:1px solid var(--console-border);margin-top:5px}
.flow-tools .flow-action-field{display:flex;gap:6px;align-items:center;flex:1 1 180px;min-width:0}
.flow-tools .flow-action-field>span{flex:0 0 auto}
.flow-tools :deep(.insert-action-select){flex:1 1 auto;min-width:112px;max-width:100%;font-size:12px;width:100%;
    background:var(--console-panel-raised);color:inherit;border-radius:5px}
.flow-tools button,.condition-tools button,.branch-tools button{border:1px solid var(--console-border);
    border-radius:5px;background:var(--console-panel-raised);color:inherit;font-size:10px;padding:4px 6px;cursor:pointer}
.flow-tools button:disabled,.condition-tools button:disabled,.branch-tools button:disabled{opacity:.4;cursor:not-allowed}
.condition-tools small{flex:1 0 100%;color:var(--console-muted)}
.branch-tools{flex:1 0 100%;padding:2px 7px 5px 28px;justify-content:flex-end}
.undo-line{display:flex;align-items:center;justify-content:space-between;gap:6px;font-size:10px;color:var(--console-muted)}
.undo-line span{min-width:0;flex:1}
.undo-line button{white-space:nowrap;border:1px solid var(--console-border);border-radius:5px;padding:3px 5px;background:var(--console-panel-raised);color:inherit;cursor:pointer}
</style>
