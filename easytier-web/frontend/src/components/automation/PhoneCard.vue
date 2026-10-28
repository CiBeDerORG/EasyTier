<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { Button, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import { v4 as uuid } from 'uuid';
import AutomationApi from '../../modules/automation-api';
import type { AutomationControl, AutomationDevice, AutomationMediaSettings, AutomationComponentStatus, AutomationPhoneConnectionState, AutomationStep } from '../../types/automation';
import { automationMessages } from './messages';
import type { EndpointEvent } from '../../modules/discovery-api';
import { usePhoneSession } from './usePhoneSession';
import PhoneAutomationPanel from './PhoneAutomationPanel.vue';
import CardPickingOverlay from './CardPickingOverlay.vue';
import CardContextMenu from './CardContextMenu.vue';
import CardStepTimeline from './CardStepTimeline.vue';
import CardStepEditor from './CardStepEditor.vue';
import CardTrialPanel from './CardTrialPanel.vue';
import TrialLocatorCompare from './TrialLocatorCompare.vue';
import type { TrialLocatorDiagnosis } from '../../modules/automation-trial-diagnostics';
import CardRepairEntry from './CardRepairEntry.vue';
import type { TrialPreviewHandle } from '../../modules/automation-run-handoff';
import type { InlineStepRunProgress } from '../../modules/useInlineStepRun';
import type { CardRepairContext } from '../../modules/automation-run-repair';
import CardLocatorDemo from './CardLocatorDemo.vue';
import CardInspectorOverlay from './CardInspectorOverlay.vue';
import UiInspectorDialog from './UiInspectorDialog.vue';
import type { InspectorAction } from '../../modules/automation-inspector-actions';
import type { StepTarget } from '../../modules/automation-step-target';
import type { InspectorCardHighlight, InspectorCardDragGuide, InspectorCardScreenPoint } from '../../modules/automation-inspector-overlay';
import AutomationFloatingPanel from './AutomationFloatingPanel.vue';
import AutomationScriptSelector from './AutomationScriptSelector.vue';
import FloatingBatchRunPanel from './FloatingBatchRunPanel.vue';
import type { FloatingBatchState } from '../../modules/useFloatingBatchRun';
import AutomationScriptCloneDialog from './AutomationScriptCloneDialog.vue';
import AutomationScriptManageDialog from './AutomationScriptManageDialog.vue';
import AutomationScriptSwitchDialog from './AutomationScriptSwitchDialog.vue';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';
import AutomationConditionDeleteDialog from './AutomationConditionDeleteDialog.vue';
import type { AutomationStepDeletion } from '../../modules/automation-card-step-delete';
import { planAutomationStepDeletion } from '../../modules/automation-card-step-delete';
import { analyzeAutomationFlow } from '../../modules/automation-flow';
import type { AutomationScript } from '../../types/automation';
import type { AutomationScriptAction, ScriptGuard, ScriptSwitchDecision } from '../../modules/automation-script-workspace';
import type { LocatorDemoResult } from '../../modules/automation-locator-demo';
import type { AutomationDraftWorkspace } from '../../modules/automation-draft';
import type { PickerMenuAction, PickerPhase, PickerVideoPoint } from '../../modules/automation-picker-state';

const props = defineProps<{
    device: AutomationDevice; ownerId: number; selected: boolean; runnerAvailable: boolean;
    phoneConnection?: AutomationPhoneConnectionState; phoneConnectionError?: string;
    componentStatus?: AutomationComponentStatus;
    forceCleanupBusy?: boolean;
    syncRole?: 'master' | 'follower'; syncActive: boolean; recording: boolean; unlocking?: boolean;
    pickerPhase?: PickerPhase; pickerError?: string;
    /** Single account-scoped draft. Card reads it, never stores a copy. */
    draft: AutomationDraftWorkspace;
    draftSyncLabel?: string;
    draftSyncPhase?: string; draftSyncMessage?: string;
    draftSaveBusy?: boolean; draftSaveMessage?: string; batchRunMessage?: string;
    scripts?: AutomationScript[];
    scriptsBusy?: boolean;
    scriptsLoading?: boolean;
    scriptsError?: string;
    scriptGuard?: ScriptGuard;
    formalPending?: boolean;
    undoDeleteLabel?: string;
    editingDirty?: boolean;
    /** The ONLY Inspector UI, hosted inside this PhoneCard's fullscreen element. */
    inspectorVisible?: boolean;
    inspectorCommitToken?: number;
    inspectorQueuedCount?: number;
    inspectorReplaceStep?: Pick<AutomationStep, 'id' | 'type'> | null;
    inspectorInitialPoint?: PickerVideoPoint | null;
    inspectorPreferredAction?: InspectorAction | null;
    editingStepId?: string;
    repickingStepId?: string;
    demoResult?: LocatorDemoResult | null;
    demoLoading?: boolean; demoError?: string;
    repairContext?: CardRepairContext | null;
    batchRunState: FloatingBatchState;
    batchDraftReady: boolean;
}>();
const emit = defineEmits<{
    selection: [id: string, selected: boolean]; configure: [device: AutomationDevice];
    input: [id: string, message: AutomationControl]; action: [id: string, message: AutomationControl]; unlock: [id: string];
    text: [id: string]; record: [id: string]; connect: [id: string]; 'toggle-screen': [id: string];
    'toggle-picking': [id: string]; 'pick-action': [id: string, point: PickerVideoPoint, action: PickerMenuAction];
    'automation-panel-open': [id: string]; 'automation-panel-close': [id: string];
    'script-refresh': [deviceId: string];
    'script-action': [action: AutomationScriptAction];
    'script-publish': [deviceId:string,name:string,description:string];
    'draft-save': [deviceId:string]; 'draft-fork': [deviceId:string];
    'script-remove': [deviceId:string,scriptId:string,revision:number];
    'script-reconcile': [];
    'inspector-close': [];
    'inspector-edit': [];
    'inspector-add-step': [step: AutomationStep, token: number, acknowledge: (accepted: boolean) => void];
    'inspector-replace-target': [stepId: string, target: StepTarget];
    'step-edit': [deviceId: string, stepId: string];
    'step-create': [deviceId:string,type:import('../../types/automation').AutomationStepType];
    'step-branch-add': [deviceId:string,conditionId:string,lane:'then'|'else',
        type:import('../../types/automation').AutomationStepType];
    'step-convert-condition': [deviceId:string,stepId:string,type:'if_gone'|'if_else'];
    'step-move': [deviceId:string,stepId:string,delta:-1|1];
    'step-delete': [deviceId: string, requestedId: string, expectedFingerprint: string, confirmedTargetId: string,
        ownerId: number, scriptId: string | null, scriptRevision: number | null];
    'step-undo': [deviceId: string];
    'step-close': [deviceId: string,discardConfirmed: boolean]; 'step-save': [deviceId: string, stepId: string, expected: AutomationStep, updated: AutomationStep];
    'step-repick': [deviceId: string, stepId: string]; 'step-dirty': [deviceId: string, dirty: boolean];
    'step-demo': [deviceId: string, stepId: string]; 'demo-clear': [deviceId: string];
    'trial-history': [deviceId: string, runId: string];
    'trial-repair': [deviceId: string, runId: string, stepId: string];
    'repair-repick': [deviceId: string, stepId: string];
    'repair-dismiss': [deviceId: string];
    'batch-start-local': [deviceId:string, params:Record<string,string>];
    'batch-refresh': [deviceId:string]; 'batch-reconcile': [deviceId:string]; 'batch-new-run': [deviceId:string];
    'batch-stop-device': [deviceId:string]; 'force-cleanup':[deviceId:string];
    state: [id: string, phase: string]; geometry: [id: string]; stopSync: []; refresh: [];
}>();
const { t } = useI18n({ useScope: 'local', messages: automationMessages });
const editingStep = computed(() => props.draft.steps.find(item => item.id === props.editingStepId));
const editingStepNumber = computed(() => props.draft.steps.findIndex(item => item.id === props.editingStepId)+1);
const businessVerificationEligible=computed(()=>{
    const step=editingStep.value;
    if(!step||!['assert','assert_property'].includes(step.type))return false;
    const at=props.draft.steps.findIndex(item=>item.id===step.id);
    const layout=analyzeAutomationFlow(props.draft.steps);
    return !layout.problem && at===props.draft.steps.length-1 &&
        (layout.entries[at]?.ancestors.length??1)===0 &&
        !props.draft.steps.some(item=>item.id!==step.id&&item.business_verification===true);
});
const repairStep = computed(() => props.repairContext?.deviceId === props.device.id
    && props.repairContext.ownerId === props.ownerId
    ? props.draft.steps.find(step => step.id === props.repairContext!.stepId) : undefined);
const repairStepNumber = computed(() => repairStep.value
    ? props.draft.steps.findIndex(step => step.id === repairStep.value!.id)+1 : 0);
const trialStepId = ref<string | null>(null);
const trialProgress = ref<InlineStepRunProgress | null>(null);
const trialToast = ref<{severity:'success'|'error';message:string}|null>(null);
const trialCompare=ref<TrialLocatorDiagnosis|null>(null);
let trialToastTimer:ReturnType<typeof setTimeout>|null=null;
function showTrialToast(severity:'success'|'error',message:string){
    if(trialToastTimer!==null)clearTimeout(trialToastTimer);
    trialToast.value={severity,message};
    trialToastTimer=setTimeout(()=>{trialToast.value=null;trialToastTimer=null;},
        severity==='error'?9000:4000);
}
function finishCardTrial(value:{phase:'succeeded'|'action_sent'|'failed'|'cancelled';
    message:string;diagnosis?:TrialLocatorDiagnosis|null}){
    showTrialToast(value.phase==='succeeded'||value.phase==='action_sent'?'success':'error',
        value.message || (value.phase==='failed'?'本次试跑失败':'本次试跑已结束'));
    trialCompare.value=value.phase==='failed' && (value.diagnosis?.count??0)>1
        ? value.diagnosis! : null;
    trialStepId.value=null;
}
onBeforeUnmount(()=>{if(trialToastTimer!==null)clearTimeout(trialToastTimer);});
const batchVideoProtected=computed(()=>props.batchRunState.selected.includes(props.device.id) &&
    ['preparing','submitting','running','cancel_requested','unknown'].includes(props.batchRunState.phase));
let notifiedUnknownTrial='';
function onTrialProgress(value:InlineStepRunProgress){
    if(value.ownerId===props.ownerId && value.deviceId===props.device.id &&
        value.stepId===trialStepId.value){
        trialProgress.value=value;
        if(value.phase==='unknown' && notifiedUnknownTrial!==value.requestId){
            notifiedUnknownTrial=value.requestId;
            showTrialToast('error',value.message || '本次试跑执行结果未确认；请只读查询旧请求，不能重复提交。');
        }
    }
}
watch(()=>props.ownerId,()=>{trialProgress.value=null;});
const trialStep = computed(() => props.draft.steps.find(item => item.id === trialStepId.value) ?? null);
async function openCardTrial(stepId: string, acknowledgedUnknown = false) {
    // A second ▶ must not silently replace the frozen in-flight trial,
    // even if its previous dialog is already preparing the helper.
    if(trialStepId.value !== null){
        showTrialToast('error','当前手机已有单步骤试跑，等待结果后再试。');return;
    }
    if (props.repairContext?.stepId === stepId && props.repairContext.resultState === 'outcome_unknown' && !acknowledgedUnknown){
        showTrialToast('error','上次试跑结果尚未确认，请从执行记录只读核对，避免重复点击。');return;
    }
    if (!props.runnerAvailable || props.recording || !!props.syncRole || !!props.unlocking ||
        !!props.editingStepId || !props.device.target_fingerprint ||
        !props.draft.steps.some(item => item.id === stepId)){
        showTrialToast('error','当前手机尚不满足单步试跑条件：请检查执行器、录制状态、步骤编辑和设备连接。');return;
    }
    // T40: the trial window is teleported into the original phone-screen.
    // Never exit fullscreen or replace its WebRTC stream to start one step.
    notifiedUnknownTrial='';
    trialCompare.value=null;
    trialStepId.value = stepId;
}
function getTrialPreviewHandle(): TrialPreviewHandle {
    return {
        getSessionId: () => session.getSessionId(),
        getState: () => ({ phase: session.state.phase, viewerOnly: session.state.viewerOnly }),
        close: () => close(),
    };
}
const api = new AutomationApi();
const session = usePhoneSession(api);
const video = ref<HTMLVideoElement | null>(null);
const ambientVideo = ref<HTMLVideoElement | null>(null);
const card = ref<HTMLElement | null>(null);
const screen = ref<HTMLElement | null>(null);
const isFullscreen = ref(false);
const automationSubwindowHost = ref<HTMLElement | null>(null);
const detachedTheme=ref<Record<string,string>>({});
function updateDetachedTheme(){
    if(!screen.value)return;
    const css=getComputedStyle(screen.value);
    const rect=screen.value.getBoundingClientRect();
    detachedTheme.value={
        '--automation-dialog-x':Math.max(8,Math.min(window.innerWidth-430,rect.right+24))+'px',
        '--automation-dialog-y':Math.max(8,Math.min(window.innerHeight-220,rect.top+42))+'px',
        ...Object.fromEntries(['--console-panel','--console-panel-raised',
        '--console-border','--console-muted','--console-accent-soft',
        '--console-border-strong','--p-text-color','--p-primary-color',
        '--p-red-500','--p-green-500'].map(k=>[k,css.getPropertyValue(k)])
            .filter(([,v])=>!!v))};
}
/** T24 subwindows Teleport HERE, inside the current phone-screen fullscreen top layer. */
function getAutomationSubwindowHost() { return automationSubwindowHost.value; }
const screenDimensions = ref({ width: 0, height: 0 });
const compactActions = computed(() =>
    screenDimensions.value.width > 0 && (
        screenDimensions.value.width < 310 || screenDimensions.value.height < 440
    ));
const actionBoundsStyle = computed(() => ({
    '--screen-width': screenDimensions.value.width + 'px',
    '--screen-height': screenDimensions.value.height + 'px',
}));
let screenSizeObserver: ResizeObserver | null = null;
function measureScreen() {
    const element = screen.value;
    if (!element) return;
    const width = element.clientWidth, height = element.clientHeight;
    if (width !== screenDimensions.value.width || height !== screenDimensions.value.height)
        screenDimensions.value = { width, height };
    if (menuOpen.value) constrainOrbAnchor();
}
function syncFullscreen() {
    isFullscreen.value = document.fullscreenElement === screen.value;
    closePickerMenu();
    measureScreen();
}
onMounted(() => {
    document.addEventListener('fullscreenchange', syncFullscreen);
    window.addEventListener('resize', measureScreen);
    window.addEventListener('orientationchange', measureScreen);
    if (typeof ResizeObserver !== 'undefined') {
        screenSizeObserver = new ResizeObserver(measureScreen);
        if (screen.value) screenSizeObserver.observe(screen.value);
    }
    measureScreen();
});
const dock = ref<'left' | 'right'>('right');
const anchorFraction = ref(0.5);
const menuOpen = ref(false);
/** T21/T22: per-card UI only; independent of orb menu, stream and running scripts. */
const automationPanelOpen = ref(false);
watch(isFullscreen,()=>void nextTick(updateDetachedTheme));
watch([automationPanelOpen,()=>props.inspectorVisible,()=>props.editingStepId],
    ()=>void nextTick(updateDetachedTheme));

/** Permission selection is exclusive to the floating manual-mode button.
 * Opening/closing/editing the script console has no permission side effects. */
const manualPermissionPending=ref(false);
const manualPermissionUncertain=ref(false);
const cloneSourceId = ref<string | null>(null);
const pendingScriptAction = ref<AutomationScriptAction | null>(null);
/** Visual-only inspector overlay; never an independent video session. */
const inspectorHighlight=ref<InspectorCardHighlight|null>(null);
const inspectorDragMode=ref(false);
const inspectorDragGuide=ref<InspectorCardDragGuide|null>(null);
const inspectorScreenPoint=ref<InspectorCardScreenPoint|null>(null);
let inspectorPointSerial=0;
function inspectorOriginalScreenPick(event:MouseEvent){
    if(!props.inspectorVisible || !inspectorDragMode.value)return;
    const at=point(event,false);
    if(!at)return; // Letterbox/fullscreen background isn't selectable.
    inspectorScreenPoint.value={...at,serial:++inspectorPointSerial};
}
function clearInspectorOverlay(){
    inspectorHighlight.value=null;
    inspectorDragMode.value=false;
    inspectorDragGuide.value=null;
    inspectorScreenPoint.value=null;
}
watch([() => props.inspectorVisible, () => props.inspectorCommitToken,
    () => props.ownerId, () => props.device.id],clearInspectorOverlay);

const pendingStepDeletion = ref<{ plan: AutomationStepDeletion; ownerId: number;
    scriptId: string | null; scriptRevision: number | null } | null>(null);
const stepDeleteError = ref('');
const stepEditorDialog = ref<InstanceType<typeof AutomationFloatingDialog> | null>(null);
function requestStepDeletion(stepId: string) {
    stepDeleteError.value='';
    const plan = planAutomationStepDeletion(props.draft.steps, stepId);
    if(!plan){
        stepDeleteError.value='当前条件结构不完整，不能安全删除；请先修复条件结构。';
        return;
    }
    const context={ plan, ownerId: props.ownerId,
        scriptId: props.draft.editingId ?? null,
        scriptRevision: props.draft.revision ?? null };
    if(plan.requiresConfirmation)pendingStepDeletion.value=context;
    else emit('step-delete',props.device.id,plan.requestedId,plan.fingerprint,plan.targetId,
        context.ownerId,context.scriptId,context.scriptRevision);
}
function confirmStepDeletion(plan: AutomationStepDeletion) {
    const context=pendingStepDeletion.value;
    pendingStepDeletion.value=null;
    if(!context || context.plan!==plan)return;
    emit('step-delete',props.device.id,plan.requestedId,plan.fingerprint,plan.targetId,
        context.ownerId,context.scriptId,context.scriptRevision);
}
function requestEditorClose() { stepEditorDialog.value?.requestClose(); }
function finishEditorClose(reason: 'normal' | 'discard') {
    emit('step-close', props.device.id, reason === 'discard');
}
const cloneSource = computed(() => props.scripts?.find(item => item.id === cloneSourceId.value) ?? null);
const manageScript=ref<{mode:'publish'|'delete';scriptId:string|null}|null>(null);
const manageWaiting=ref(false);
const manageSource=computed(()=> {
    const id=manageScript.value?.scriptId;
    return id? props.scripts?.find(s=>s.id===id)??null:null;
});
function requestFormalPublish(){
    if(props.scriptsBusy||props.formalPending)return;
    const id=props.draft.editingId??null;
    if(id && !props.scripts?.some(item=>item.id===id && item.revision===props.draft.revision)){
        stepDeleteError.value='当前正式脚本修订与列表尚未一致，请先刷新脚本列表。';return;
    }
    manageScript.value={mode:'publish',scriptId:id};
}
function requestFormalRemove(id:string){
    if(props.scriptsBusy||props.formalPending||!props.scripts?.some(item=>item.id===id))return;
    manageScript.value={mode:'delete',scriptId:id};
}
function finishFormalPublish(name:string,description:string){
    if(props.scriptsBusy||manageWaiting.value)return;
    manageWaiting.value=true;
    emit('script-publish',props.device.id,name,description);
}
function finishFormalRemove(id:string,revision:number){
    if(props.scriptsBusy||manageWaiting.value||manageScript.value?.scriptId!==id)return;
    manageWaiting.value=true;
    emit('script-remove',props.device.id,id,revision);
}
watch(()=>props.scriptsBusy,(busy,old)=>{
    if(old&&!busy&&manageWaiting.value){
        manageWaiting.value=false;
        if(!props.scriptsError)manageScript.value=null;
    }
});
watch(()=>props.scriptsError,error=>{
    // Scope/preflight failures can return before setting busy. Never leave
    // the management dialog stuck in a locally "submitted" state.
    if(error && manageWaiting.value)manageWaiting.value=false;
});

const effectiveScriptGuard = computed<ScriptGuard>(() => props.scriptGuard ?? 'loading');
function openAutomationPanel() {
    automationPanelOpen.value=true;
    emit('script-refresh',props.device.id);
}
function closeAutomationPanel() {
    if (!automationPanelOpen.value) return;
    automationPanelOpen.value=false;
    emit('automation-panel-close',props.device.id);
}
function toggleAutomationPanel() {
    if(automationPanelOpen.value)closeAutomationPanel();
    else openAutomationPanel();
}
// Script viewing, fullscreen, editing and closing never change manual mode.
// Only the floating manual icon calls the server permission API.
function requestScriptAction(action:AutomationScriptAction) {
    if(props.scriptsBusy)return;
    if(effectiveScriptGuard.value !== 'clean') {
        pendingScriptAction.value=action;
        return;
    }
    emit('script-action',action);
}
function chooseScript(id:string) {
    const item=props.scripts?.find(script => script.id === id);
    if(!item || (props.draft.editingId === id && props.draft.revision === item.revision))return;
    requestScriptAction({kind:'select',deviceId:props.device.id,scriptId:id,revision:item.revision});
}
function openScriptClone(id:string) {
    if(props.scriptsBusy)return;
    cloneSourceId.value=id;
}
function confirmScriptClone(id:string,revision:number,name:string) {
    requestScriptAction({kind:'clone',deviceId:props.device.id,scriptId:id,revision,name});
}
function decideScriptSwitch(decision:ScriptSwitchDecision) {
    const pending=pendingScriptAction.value;
    pendingScriptAction.value=null;
    if(pending)emit('script-action',{...pending,decision});
}
watch(() => props.editingStepId, id => {
    if(id && !automationPanelOpen.value) openAutomationPanel();
});
watch(() => props.draft.editingId, (next,previous) => {
    if(next && next!==previous) cloneSourceId.value=null;
});
watch(() => [props.ownerId, props.device.id], () => {
    manualPermissionUncertain.value=false;
    automationPanelOpen.value = false;
    cloneSourceId.value=null;
    manageScript.value=null;manageWaiting.value=false;
    pendingScriptAction.value=null;
    pendingStepDeletion.value=null;
    stepDeleteError.value='';
});
const pickerMenu = ref<{ point: PickerVideoPoint; x: number; y: number } | null>(null);
function closePickerMenu() { pickerMenu.value = null; }
function pickAction(action: PickerMenuAction) {
    const current = pickerMenu.value;
    if (!current) return;
    closePickerMenu();
    if (action === 'exit') emit('toggle-picking', props.device.id);
    else emit('pick-action', props.device.id, current.point, action);
}
function showPickerMenu(clientX: number, clientY: number) {
    if (pickingPhase.value !== 'on' || !pickAvailable.value || !screen.value) return;
    const pos = point({clientX,clientY}, false);
    if (!pos) return;
    cancelPointer(); menuOpen.value = false;
    const bounds = screen.value.getBoundingClientRect();
    pickerMenu.value = { point: pos,
        x: Math.max(8, Math.min(clientX - bounds.left, Math.max(8, bounds.width - 230))),
        y: Math.max(8, Math.min(clientY - bounds.top, Math.max(8, bounds.height - 320))) };
}
const floatingStyle = computed(() => ({
    left: dock.value === 'right'
        ? menuOpen.value ? 'calc(100% - 48px)' : 'calc(100% - 20px)'
        : menuOpen.value ? '4px' : '-24px',
    top: `${anchorFraction.value * 100}%`,
}));
let drag: { id: number; firstX: number; firstY: number; firstFraction: number; moved: boolean } | null = null;
let suppressOrbClick = false;
let frameCallback: number | null = null;
function dragStart(event: PointerEvent) {
    if (event.button !== 0 || !screen.value) return;
    drag = { id: event.pointerId, firstX: event.clientX, firstY: event.clientY,
        firstFraction: anchorFraction.value, moved: false };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}

/** Keep the expanded upper/lower action stacks inside the current card surface. */
function expandedOrbRange(): { min: number; max: number } {
    const height = screen.value?.clientHeight ?? 0;
    if (!height) return { min: 0.35, max: 0.65 };
    const compact = window.matchMedia('(max-width:600px)').matches;
    const size = compact ? 36 : 42;
    if (compactActions.value) {
        // In a short landscape/fullscreen surface the actions become a
        // scrollable tray beside the orb. Keep the whole tray within bounds.
        const halfTray = Math.min(Math.max(0, height - 16), 260) / 2 + 8;
        const min = Math.min(0.5, halfTray / height);
        return { min, max: 1 - min };
    }
    // WAKE is the 4th upper action; navigation has 3 lower actions.
    const top = (size * 4 + 4 * 3 + 6 + 10 + size / 2 + 8) / height;
    const bottom = 1 - (size * 3 + 4 * 2 + 6 + 10 + size / 2 + 8) / height;
    if (top > bottom) return { min: 0.5, max: 0.5 };
    return { min: Math.max(0.07, top), max: Math.min(0.93, bottom) };
}
function constrainOrbAnchor() {
    const { min, max } = menuOpen.value ? expandedOrbRange() : { min: 0.07, max: 0.93 };
    anchorFraction.value = Math.max(min, Math.min(max, anchorFraction.value));
}
function dragMove(event: PointerEvent) {
    if (drag?.id !== event.pointerId || !screen.value) return;
    const bounds = screen.value.getBoundingClientRect();
    const x = event.clientX - drag.firstX, y = event.clientY - drag.firstY;
    if (!drag.moved && Math.hypot(x, y) < 7) return;
    event.preventDefault();
    drag.moved = true;
    const { min, max } = menuOpen.value ? expandedOrbRange() : { min: 0.07, max: 0.93 };
    anchorFraction.value = Math.max(min, Math.min(max, drag.firstFraction + y / bounds.height));
    dock.value = event.clientX - bounds.left < bounds.width / 2 ? 'left' : 'right';
}
function dragEnd(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    if (screen.value && drag.moved) {
        const bounds = screen.value.getBoundingClientRect();
        dock.value = event.clientX - bounds.left < bounds.width / 2 ? 'left' : 'right';
        suppressOrbClick = true;
    }
    drag = null;
}
const menuIdleMs = 6000;
let menuTimer: ReturnType<typeof setTimeout> | undefined;
function clearMenuTimer() {
    if (menuTimer !== undefined) clearTimeout(menuTimer);
    menuTimer = undefined;
}
function markMenuInteraction() {
    if (!menuOpen.value) return;
    clearMenuTimer();
    menuTimer = setTimeout(() => { menuOpen.value = false; }, menuIdleMs);
}
watch(menuOpen, open => {
    if (open) { constrainOrbAnchor(); markMenuInteraction(); }
    else clearMenuTimer();
});
function toggleOrb() {
    if (suppressOrbClick) { suppressOrbClick = false; return; }
    menuOpen.value = !menuOpen.value;
    if (menuOpen.value) constrainOrbAnchor();
}
function stopFrameChase() {
    if (frameCallback !== null && video.value?.cancelVideoFrameCallback) video.value.cancelVideoFrameCallback(frameCallback);
    frameCallback = null;
    if (video.value) {
        try { video.value.playbackRate = 1; } catch { /* live stream may not allow changing playback rate */ }
    }
}
function startFrameChase() {
    stopFrameChase();
    const player = video.value;
    if (!player?.requestVideoFrameCallback || !watchMedia.low_latency) return;
    const inspect: VideoFrameRequestCallback = (now, metadata) => {
        if (disposed || session.state.phase !== 'live' || video.value !== player) { stopFrameChase(); return; }
        // Best-effort catch-up on browsers that support live playback rate
        // adjustments. The bounded RTP track independently discards stale
        // interframes until a decodable keyframe is received.
        try { player.playbackRate = now - metadata.expectedDisplayTime > 120 ? 1.12 : 1; }
        catch { /* Unsupported on this browser's MediaStream playback */ }
        frameCallback = player.requestVideoFrameCallback(inspect);
    };
    frameCallback = player.requestVideoFrameCallback(inspect);
}
function videoReady() {
    session.videoReady();
    startFrameChase();
}
const cleaningSaved = ref(false);
const pointer = ref<{ id: number; width: number; height: number } | null>(null);
const currentGeometry = ref({ width: 0, height: 0 });
// Match EasyADB: the card tracks encoded video width, not an expanding grid
// column. Keep the phone's natural width unless the viewport is narrower.
const cardWidth = computed(() => `${(currentGeometry.value.width > 0 ? currentGeometry.value.width : 360) + 26}px`);
const ready = computed(() => session.state.phase === 'live' && session.state.controlReady);
const trialActionProtected = computed(() => !!trialProgress.value &&
    ['submitting','running','unknown'].includes(trialProgress.value.phase));
const manualAllowed = computed(() => ready.value && !session.state.viewerOnly
    && !props.device.run_occupied && !batchVideoProtected.value &&
    !trialActionProtected.value && !props.syncRole);
const touchAllowed = computed(() => ready.value && !session.state.viewerOnly
    && !props.device.run_occupied && !batchVideoProtected.value &&
    !trialActionProtected.value && props.syncRole !== 'follower');
// A permission downgrade is not a stream close. Drop local pointer state
// without sending a last touch.release from the now read-only browser.
watch(() => session.state.viewerOnly, readOnly => { if (readOnly) resetPointer(); });
const pending = computed(() => ['opening', 'connecting'].includes(session.state.phase));
const adbConnectionHint = computed(() => {
    if (props.phoneConnectionError) return props.phoneConnectionError;
    const hints: Record<string,string> = {
        checking: '正在查询 ADB 连接状态', connected: 'ADB 已连接',
        offline: 'ADB 离线', unauthorized: 'ADB 未授权',
        disconnected: 'ADB 未连接', unavailable: 'ADB 状态查询超时',
        query_failed: 'ADB 查询失败', connecting: '正在连接 ADB',
        failed: 'ADB 连接失败',
    };
    return hints[props.phoneConnection || ''] || t('phoneNotConnected');
});
const canOpen = computed(() => props.runnerAvailable && props.device.connection.enabled && !props.device.availability_reason && !props.device.active_session && !pending.value && session.state.phase !== 'live' && session.state.phase !== 'cleanup_pending');
const pickingPhase = computed<PickerPhase>(() => props.pickerPhase ?? 'off');
const pickAvailable = computed(() => ready.value && props.runnerAvailable && !props.syncRole && !props.recording && !props.unlocking);
// Opening the automation floating console automatically enables right-click
// picking on this SAME phone card. Wait for the live stream if it isn't ready.
// Do not re-arm when unrelated reactive props change or after manual opt-out.
watch([automationPanelOpen,pickAvailable],([opened,available],[wasOpen,wasAvailable])=>{
    if(opened && available && (!wasOpen || !wasAvailable))
        emit('automation-panel-open',props.device.id);
});
// Right-click picking remains available inside the automation floating console.
 // The card header has a dedicated ADB connection action instead.
const connectActionHint = computed(() =>
    (props.phoneConnection === 'connected' ? '重新连接 ADB' : '连接 ADB') +
    ' · ' + adbConnectionHint.value +
    ' · 端口 ' + (props.device.connection.adb_port || '未配置')
);
const screenActionHint = computed(() => {
    const port = props.device.connection.adb_port || '未设置';
    const prefix = `ADB 端口：${port} · ${adbConnectionHint.value}`;
    if (!props.runnerAvailable) return prefix + ' · 执行器不可用';
    if (!props.device.connection.enabled) return prefix + ' · 连接未启用';
    if (pending.value) return prefix + ' · 正在打开画面';
    if (session.state.phase === 'live') return prefix + ' · 正在投屏，点击关闭画面';
    if (session.state.phase === 'cleanup_pending') return prefix + ' · 资源清理待确认';
    if (session.state.phase === 'failed') return prefix + ' · 上次打开失败，点击重试';
    return prefix + ' · 点击打开画面';
});
watch(() => session.state.phase, phase => {
    if (phase !== 'live') { closePickerMenu(); clearTouchHold(); }
    emit('state', props.device.id, phase);
    // The browser must not remain fullscreen once this card loses its stream.
    if (phase !== 'live' && document.fullscreenElement === screen.value) {
        void document.exitFullscreen().catch(() => undefined);
    }
});
watch([pickingPhase, () => props.recording, () => props.syncRole, () => props.unlocking], () => {
    if (pickingPhase.value !== 'on' || !pickAvailable.value) { closePickerMenu(); clearTouchHold(); }
});
watch(session.stream, value => {
    if (video.value) video.value.srcObject = value;
    if (!value) {
        stopFrameChase();
        currentGeometry.value = { width: 0, height: 0 };
    }
}, { flush: 'post' });
// An ambient copy only exists in fullscreen, behind the real un-cropped frame.
// It fills unused monitor space without distorting the interactive picture.
watch([isFullscreen, session.stream], ([active, stream]) => {
    if (ambientVideo.value) ambientVideo.value.srcObject = active ? stream : null;
}, { flush: 'post' });

function geometry() { return { width: video.value?.videoWidth ?? 0, height: video.value?.videoHeight ?? 0 }; }
function onGeometry() {
    closePickerMenu();
    const next = geometry();
    if (currentGeometry.value.width && (next.width !== currentGeometry.value.width || next.height !== currentGeometry.value.height)) {
        cancelPointer(); emit('geometry', props.device.id);
    }
    currentGeometry.value = next;
}
function point(event: Pick<MouseEvent, 'clientX' | 'clientY'>, clamp: boolean): PickerVideoPoint | null {
    if (!video.value) return null;
    const { width, height } = geometry();
    if (!width || !height) return null;
    const rect = video.value.getBoundingClientRect();
    // Match EasyADB: map only the undistorted picture (object-fit:contain).
    // Ignore the surrounding fullscreen ambient backdrop when tapping.
    const scale = Math.min(rect.width / width, rect.height / height);
    const left = rect.left + (rect.width - width * scale) / 2;
    const top = rect.top + (rect.height - height * scale) / 2;
    const x = (event.clientX - left) / scale;
    const y = (event.clientY - top) / scale;
    if (!clamp && (x < 0 || y < 0 || x >= width || y >= height)) return null;
    return { x: Math.max(0, Math.min(width - 1, Math.round(x))), y: Math.max(0, Math.min(height - 1, Math.round(y))), width, height };
}
// Right-click is strictly read-only and is never sent as a scrcpy touch.
let touchHold: { id: number; start: PickerVideoPoint; clientX: number; clientY: number;
    timer: ReturnType<typeof setTimeout> } | null = null;
let syntheticContextUntil = 0;
function clearTouchHold(release = true) {
    const hold = touchHold; if (!hold) return;
    clearTimeout(hold.timer); touchHold = null;
    if (release && video.value?.hasPointerCapture(hold.id)) video.value.releasePointerCapture(hold.id);
}
function contextPick(event: MouseEvent) {
    // The browser's existing right-click behavior is left intact outside
    // picker mode. Only an actively authorized picker may capture the event.
    if(pickingPhase.value!=='on' || !pickAvailable.value)return;
    event.preventDefault();event.stopPropagation();
    if (Date.now() < syntheticContextUntil) return;
    showPickerMenu(event.clientX, event.clientY);
}
function keyboardContextPick(event: KeyboardEvent) {
    if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
    if (pickingPhase.value !== 'on') return;
    event.preventDefault();
    const bounds = video.value?.getBoundingClientRect();
    if (bounds) showPickerMenu(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
}
function down(event: PointerEvent) {
    if (!touchAllowed.value || event.button !== 0 || pointer.value || touchHold ||
        menuOpen.value || pickerMenu.value) return;
    const position = point(event, false); if (!position) return;
    event.preventDefault(); video.value?.setPointerCapture(event.pointerId);
    if (event.pointerType === 'touch' && pickingPhase.value === 'on' && pickAvailable.value) {
        // Do not send an Android touch until the browser gesture is classified.
        touchHold = { id: event.pointerId, start: position, clientX: event.clientX, clientY: event.clientY,
            timer: setTimeout(() => {
                const hold = touchHold;
                if (!hold || hold.id !== event.pointerId) return;
                clearTouchHold(); syntheticContextUntil = Date.now() + 800;
                showPickerMenu(hold.clientX, hold.clientY);
            }, 550) };
        return;
    }
    pointer.value = { id: event.pointerId, width: position.width, height: position.height };
    emit('input', props.device.id, { type: 'touch', action: 'down', ...position });
}
function move(event: PointerEvent) {
    if (touchHold?.id === event.pointerId) {
        const hold = touchHold, at = point(event, false);
        if (!at || at.width !== hold.start.width || at.height !== hold.start.height) {
            clearTouchHold(); emit('geometry', props.device.id); return;
        }
        if (Math.hypot(event.clientX - hold.clientX, event.clientY - hold.clientY) < 10) return;
        clearTouchHold(false);
        pointer.value = { id: event.pointerId, width: at.width, height: at.height };
        event.preventDefault();
        emit('input', props.device.id, { type: 'touch', action: 'down', ...hold.start });
        emit('input', props.device.id, { type: 'touch', action: 'move', ...at });
        return;
    }
    if (pointer.value?.id !== event.pointerId) return;
    const position = point(event, true); if (!position) return;
    if (position.width !== pointer.value.width || position.height !== pointer.value.height) { cancelPointer(); emit('geometry', props.device.id); return; }
    event.preventDefault(); emit('input', props.device.id, { type: 'touch', action: 'move', ...position });
}
function up(event: PointerEvent) {
    if (touchHold?.id === event.pointerId) {
        const hold = touchHold, at = point(event, false);
        clearTouchHold(); event.preventDefault();
        if (at && at.width === hold.start.width && at.height === hold.start.height) {
            emit('input', props.device.id, { type: 'touch', action: 'down', ...hold.start });
            emit('input', props.device.id, { type: 'touch', action: 'up', ...at });
        }
        return;
    }
    if (pointer.value?.id !== event.pointerId) return;
    const position = point(event, true); const previous = pointer.value; pointer.value = null;
    if (video.value?.hasPointerCapture(event.pointerId)) video.value.releasePointerCapture(event.pointerId);
    if (position && position.width === previous.width && position.height === previous.height) emit('input', props.device.id, { type: 'touch', action: 'up', ...position });
    else emit('input', props.device.id, { type: 'touch.release' });
}
function cancelPointer() {
    clearTouchHold();
    if (!pointer.value) return;
    const id = pointer.value.id; pointer.value = null;
    if (video.value?.hasPointerCapture(id)) video.value.releasePointerCapture(id);
    emit('input', props.device.id, { type: 'touch.release' });
}
function resetPointer() {
    clearTouchHold();
    if (!pointer.value) return;
    const id = pointer.value.id; pointer.value = null;
    if (video.value?.hasPointerCapture(id)) video.value.releasePointerCapture(id);
}
async function fullscreen() {
    closePickerMenu(); menuOpen.value = false;
    // Fullscreen only the video surface, not its card/header/footer.
    // The screen owns the floating controls and the input dialog portal.
    if (document.fullscreenElement === screen.value) await document.exitFullscreen();
    else await screen.value?.requestFullscreen();
}
function localAction(message: AutomationControl) { if (manualAllowed.value) emit('action', props.device.id, message); }
/**
 * The floating icon is the ONLY initiator of manual/view-only transitions.
 * Frontend state stays fail-closed while a permission RPC is in flight.
 * A rejected or unknown transition is never optimistic success.
 */
const manualPermissionTitle=computed(()=>{
    if(session.state.phase!=='live')return '手控不可用 · 请先打开原手机实时画面';
    if(manualPermissionPending.value)return '正在确认手控权限切换';
    if(manualPermissionUncertain.value)return '权限回执未确认 · 请核对服务器后再操作';
    if(props.device.run_occupied || batchVideoProtected.value)
        return '脚本执行占用手机 · 请等安全释放后再切换手控';
    return session.state.viewerOnly
        ? '手控已关闭 · 点击恢复手控'
        : '手控已开启 · 点击关闭手控（只读观看）';
});
const manualPermissionAllowed=computed(()=>
    ready.value && !manualPermissionPending.value &&
    !props.device.run_occupied && !batchVideoProtected.value &&
    !props.syncRole && !props.recording && !props.unlocking);
async function toggleManualPermission() {
    if(!manualPermissionAllowed.value)return;
    const originalId=session.getSessionId(),owner=props.ownerId,deviceId=props.device.id;
    if(!originalId)return;
    manualPermissionPending.value=true;
    manualPermissionUncertain.value=false;
    try {
        if(session.state.viewerOnly)await session.resumeManual();
        else await session.enterViewOnly();
        if(owner===props.ownerId && deviceId===props.device.id &&
            originalId===session.getSessionId())emit('refresh');
    }catch(cause){
        if(owner===props.ownerId && deviceId===props.device.id &&
            originalId===session.getSessionId()){
            manualPermissionUncertain.value=true;
            session.state.error=cause instanceof Error?cause.message:'手控权限切换未确认';
        }
    }finally{
        manualPermissionPending.value=false;
    }
}
let watching = false, disposed = false, intentVersion = 0, latestEndpoint = -1, restoredEndpoint = -1;
let watchQuality: 'wall' | 'focus' = 'wall';
let watchMedia: AutomationMediaSettings = { max_size: 640, video_bit_rate: 1000000, max_fps: 15, low_latency: true, screen_off: false };
async function open(quality: 'wall' | 'focus', options: AutomationMediaSettings = watchMedia) {
    watching = true; ++intentVersion; watchQuality = quality; watchMedia = { ...options };
    await session.open(props.ownerId, props.device.id, quality, watchMedia);
}
function cancelRestoreIntent() { watching = false; ++intentVersion; }
async function close() {
    closePickerMenu(); cancelRestoreIntent(); cancelPointer(); menuOpen.value = false; clearMenuTimer(); stopFrameChase();
    // Close the fullscreen container *before* tearing down its video.
    if (document.fullscreenElement === screen.value) {
        try { await document.exitFullscreen(); } catch { /* session cleanup still takes priority */ }
    }
    await session.close();
}
async function endpointChanged(event: EndpointEvent) {
    if (event.device_id !== props.device.id || event.endpoint_revision < latestEndpoint || event.endpoint_revision <= restoredEndpoint) return;
    const changed = event.endpoint_revision > latestEndpoint;
    latestEndpoint = event.endpoint_revision;
    const intent = intentVersion;
    resetPointer(); emit('geometry', props.device.id);
    if (changed) await session.close(); // clears queued input and the previous peer before any replacement
    if (disposed || !watching || intent !== intentVersion || event.endpoint_revision !== latestEndpoint) return;
    if (event.cleanup.state !== 'confirmed' || event.connection.state !== 'connected') return;
    await session.close();
    if (disposed || !watching || intent !== intentVersion || event.endpoint_revision !== latestEndpoint || session.state.phase === 'cleanup_pending') return;
    restoredEndpoint = event.endpoint_revision;
    try { await session.open(props.ownerId, props.device.id, watchQuality, watchMedia); }
    catch { /* bounded attempt: session exposes its failure, user can retry */ }
}
async function cleanupSaved() {
    if (!props.device.active_session || cleaningSaved.value) return;
    cleaningSaved.value = true;
    try {
        const result = await api.closeSession(props.ownerId, props.device.active_session.id);
        if (result.cleanup_pending) { session.state.phase = 'failed'; session.state.error = result.message || t('cleanupFirst'); }
        else { session.state.phase = 'closed'; session.state.error = ''; }
        emit('refresh');
    } catch (cause) { session.state.error = cause instanceof Error ? cause.message : t('actionFailed'); }
    finally { cleaningSaved.value = false; }
}
async function retryEndpoint(stage: 'cleanup'|'connection'|'publication') {
    const event = props.device.endpoint_event; if (!event || cleaningSaved.value) return;
    cleaningSaved.value = true;
    try { await api.retryEndpoint(props.ownerId, props.device.id, event.event_id, {request_id:uuid(),stage}); emit('refresh'); }
    catch(cause) { session.state.error = cause instanceof Error ? cause.message : '重试未完成'; }
    finally { cleaningSaved.value = false; }
}
function getState() { return { phase: session.state.phase, ready: ready.value, viewerOnly: session.state.viewerOnly, ...geometry() }; }
function getStream() { return session.stream.value; }
function getSessionId() { return session.getSessionId(); }
defineExpose({ open, close, cancelRestoreIntent, endpointChanged, send: session.send, unlock: session.unlock,
    getState, getStream, getSessionId, cancelPointer, resetPointer, openAutomationPanel, closeAutomationPanel,
    toggleAutomationPanel, getAutomationSubwindowHost });
onBeforeUnmount(() => {
    document.removeEventListener('fullscreenchange', syncFullscreen);
    window.removeEventListener('resize', measureScreen);
    window.removeEventListener('orientationchange', measureScreen);
    screenSizeObserver?.disconnect();
    screenSizeObserver = null;
    if (document.fullscreenElement === screen.value) void document.exitFullscreen().catch(() => undefined);
    if (ambientVideo.value) ambientVideo.value.srcObject = null;
    disposed = true; trialStepId.value = null; watching = false; ++intentVersion; closePickerMenu(); clearMenuTimer(); stopFrameChase(); cancelPointer(); void session.close();
});
</script>

<template>
    <article ref="card" class="phone-card" :class="{ selected, master: syncRole === 'master' }" :style="{ width: cardWidth }">
        <header>
            <div class="device-heading">
                <label class="device-selection" :title="device.name">
                    <input type="checkbox" :checked="selected" :aria-label="device.name"
                        @change="emit('selection', device.id, ($event.target as HTMLInputElement).checked)">
                    <span class="device-name">{{ device.name }}</span>
                    <span v-if="device.gateway_name" class="gateway-name"
                        :title="'所属网关：' + device.gateway_name">{{ device.gateway_name }}</span>
                </label>
                <span class="adb-status" :class="phoneConnection || 'unknown'"
                    role="status" :aria-label="'ADB：' + adbConnectionHint" :title="adbConnectionHint">
                    <span class="adb-status-dot" aria-hidden="true" />
                </span>
            </div>
            <div class="card-header-actions">
                <PhoneAutomationPanel :device="device" :owner-id="ownerId" :runner-available="runnerAvailable" :observed-status="componentStatus" />
                <span class="card-action-tip" :title="connectActionHint">
                    <button type="button" class="card-icon connect-icon"
                        :class="{ connected: phoneConnection === 'connected', connecting: phoneConnection === 'connecting' }"
                        :aria-label="connectActionHint"
                        :disabled="!runnerAvailable || !device.connection.enabled || phoneConnection === 'connecting'"
                        @click="emit('connect', device.id)">
                        <i :class="phoneConnection === 'connecting' ? 'pi pi-spin pi-spinner' : 'pi pi-link'" aria-hidden="true" />
                    </button>
                </span>
                <span class="card-action-tip" title="强制清理本机全部自动化占用（停止脚本与旧预览）">
                    <button type="button" class="card-icon force-cleanup-icon"
                        :disabled="!runnerAvailable || forceCleanupBusy"
                        :aria-label="forceCleanupBusy ? '正在强制清理本机占用' : '强制清理本机全部自动化占用'"
                        @click="emit('force-cleanup',device.id)">
                        <i :class="forceCleanupBusy ? 'pi pi-spin pi-spinner' : 'pi pi-wrench'" aria-hidden="true"/>
                    </button>
                </span>
                <span class="card-action-tip" :title="screenActionHint">
                    <button type="button" class="card-icon adb-port"
                        :class="[session.state.phase, { unavailable: !runnerAvailable || !device.connection.enabled }]"
                        :aria-label="screenActionHint" :disabled="!runnerAvailable || !device.connection.enabled || pending || batchVideoProtected"
                        @click="emit('toggle-screen', device.id)">
                        <i class="pi pi-desktop" aria-hidden="true" />
                    </button>
                </span>
                <Button class="card-settings" icon="pi pi-cog" text rounded
                    :aria-label="t('configure')" :title="t('configure')"
                    :disabled="pending || session.state.phase === 'live'" @click="emit('configure', device)" />
            </div>
        </header>
        <div ref="screen" class="phone-screen" :style="currentGeometry.width && currentGeometry.height ? { aspectRatio: `${currentGeometry.width} / ${currentGeometry.height}` } : undefined" @keydown.esc="menuOpen = false; closePickerMenu()">
            <video v-if="isFullscreen && session.state.phase === 'live'" ref="ambientVideo" class="ambient-video" autoplay muted playsinline aria-hidden="true" tabindex="-1" />
            <video ref="video" class="primary-video" autoplay muted playsinline tabindex="0" :class="{ interactive: touchAllowed }" :aria-label="device.name" @loadedmetadata="onGeometry" @loadeddata="videoReady" @resize="onGeometry" @pointerdown="down" @pointermove="move" @pointerup="up" @pointercancel="cancelPointer" @lostpointercapture="cancelPointer" @contextmenu="contextPick" @keydown="keyboardContextPick" />
            <div v-if="session.state.phase !== 'live'" class="screen-placeholder">
                <i :class="pending ? 'pi pi-spin pi-spinner' : 'pi pi-mobile'" aria-hidden="true" />
                <span>{{ t(session.state.phase) }}</span>
                <Button v-if="canOpen" :label="t('open')" icon="pi pi-play" size="small" @click="emit('toggle-screen', device.id)" />
                <Button v-if="session.state.phase === 'cleanup_pending'" :label="t('clearResources')" icon="pi pi-refresh" size="small" severity="warn" :disabled="batchVideoProtected" @click="close().then(() => emit('refresh'))" />
                <Button v-if="device.active_session && !pending && session.state.phase !== 'cleanup_pending'" :label="t('cleanupSaved')" icon="pi pi-stop" size="small" severity="warn" :loading="cleaningSaved" :disabled="batchVideoProtected" @click="cleanupSaved" />
                <small v-if="!device.connection.enabled">{{ t('disabled') }}</small>
            </div>
            <CardLocatorDemo :demo="demoResult ?? null" :video-width="currentGeometry.width" :video-height="currentGeometry.height" />
            <CardInspectorOverlay v-if="inspectorVisible"
                :video-width="currentGeometry.width" :video-height="currentGeometry.height"
                :highlight="inspectorHighlight" :drag-mode="inspectorDragMode"
                :drag-guide="inspectorDragGuide" @pick="inspectorOriginalScreenPick" />
            <CardPickingOverlay :phase="pickingPhase" :error="pickerError" @exit="emit('toggle-picking', device.id)" />
            <button v-if="pickerMenu" type="button" class="pick-menu-dismiss" tabindex="-1" aria-label="关闭控件快捷菜单" @pointerdown.stop @click.stop="closePickerMenu()" />
            <CardContextMenu v-if="pickerMenu" :left="pickerMenu.x" :top="pickerMenu.y"
                @close="closePickerMenu()" @select="pickAction" />
            <Tag v-if="syncRole" class="role-badge" :value="t(syncRole)" :severity="syncRole === 'master' ? 'info' : 'secondary'" />
            <span v-if="recording" class="record-badge"><span /> REC</span>
            <Button v-if="syncActive" class="fullscreen-stop" severity="danger" :label="t('stopSync')" size="small" @click="emit('stopSync')" />
            <button v-if="menuOpen" class="control-dismiss" :aria-label="t('collapse')" @pointerdown.stop @click.stop="menuOpen = false" />
            <div class="phone-controls" :class="[`dock-${dock}`, { 'compact-actions': compactActions, 'menu-expanded': menuOpen }]"
                :style="[floatingStyle, actionBoundsStyle]" @pointerdown.capture="markMenuInteraction" @click.capture="markMenuInteraction" @pointerdown.stop @click.stop>
                <div v-if="menuOpen" class="floating-actions" role="group" :aria-label="t('localControl')">
                    <div class="input-actions">
                        <Button :icon="session.state.viewerOnly?'pi pi-lock':'pi pi-lock-open'"
                            rounded :severity="session.state.viewerOnly?'warn':'success'"
                            :aria-label="manualPermissionTitle" :title="manualPermissionTitle"
                            :aria-pressed="session.state.phase==='live' && !session.state.viewerOnly"
                            :loading="manualPermissionPending"
                            :disabled="!manualPermissionAllowed"
                            @click="toggleManualPermission" />
                        <Button icon="pi pi-pencil" rounded :aria-label="t('text')" :title="t('text')" :disabled="!manualAllowed" @click="emit('text', device.id)" />
                        <Button icon="pi pi-unlock" rounded severity="primary" :aria-label="t('unlockPhone')" :title="t('unlockPhone')" :loading="unlocking" :disabled="!manualAllowed || unlocking || recording" @click="emit('unlock', device.id)" />
                        <Button icon="pi pi-power-off" rounded :aria-label="t('wake')" :title="t('wake')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'WAKE' })" />
                    </div>
                    <div class="side-actions">
                        <Button icon="pi pi-stop" rounded severity="danger" :aria-label="t('close')" :title="t('close')" :disabled="batchVideoProtected" @click="close().then(() => emit('refresh'))" />
                        <Button icon="pi pi-expand" rounded :aria-label="t('fullscreen')" :title="t('fullscreen')" :disabled="!ready" @click="fullscreen().catch(() => {})" />
                        <Button icon="pi pi-bolt" rounded :severity="automationPanelOpen ? 'primary' : 'secondary'"
                            :aria-label="t('automationScript')" :title="t('automationScript')"
                            :aria-pressed="automationPanelOpen" @click="toggleAutomationPanel" />
                        <Button icon="pi pi-circle-fill" rounded :severity="recording ? 'danger' : 'secondary'" :aria-label="t(recording ? 'stopRecord' : 'record')" :title="t(recording ? 'stopRecord' : 'record')" :disabled="!manualAllowed" @click="emit('record', device.id)" />
                    </div>
                    <div class="nav-actions">
                        <Button icon="pi pi-arrow-left" rounded :aria-label="t('back')" :title="t('back')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'BACK' })" />
                        <Button icon="pi pi-clone" rounded :aria-label="t('recent')" :title="t('recent')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'RECENT' })" />
                        <Button icon="pi pi-home" rounded :aria-label="t('home')" :title="t('home')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'HOME' })" />
                    </div>
                </div>
                <Button class="main-orb" :icon="menuOpen ? 'pi pi-times' : 'pi pi-sliders-h'" rounded :aria-expanded="menuOpen" :aria-label="t('localControl')" :title="syncRole ? t('controlsLocked') : t('localControl')" @pointerdown.stop="dragStart" @pointermove.stop="dragMove" @pointerup.stop="dragEnd" @pointercancel.stop="dragEnd" @click.stop="toggleOrb" />
            </div>
            <UiInspectorDialog v-if="inspectorVisible" :key="inspectorCommitToken"
                :visible="!!inspectorVisible" :device="device" :owner-id="ownerId"
                :queued-count="inspectorQueuedCount ?? draft.steps.length"
                :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
                :screen-width="currentGeometry.width" :screen-height="currentGeometry.height"
                :screen-point="inspectorScreenPoint" :replace-step="inspectorReplaceStep"
                :initial-point="inspectorInitialPoint"
                :preferred-action="inspectorPreferredAction" :commit-token="inspectorCommitToken ?? 0"
                @close="emit('inspector-close')"
                @edit="emit('inspector-edit')"
                @add-step="(step,token,ack) => emit('inspector-add-step',step,token,ack)"
                @replace-target="(stepId,target) => emit('inspector-replace-target',stepId,target)"
                @highlight="inspectorHighlight=$event"
                @drag-mode="inspectorDragMode=$event"
                @drag-guide="inspectorDragGuide=$event" />
            <AutomationFloatingPanel :visible="automationPanelOpen" :fullscreen="isFullscreen"
                :normal-anchor="screen" :device-name="device.name" @close="closeAutomationPanel">
                <AutomationScriptSelector :scripts="scripts ?? []"
                    :selected-id="draft.editingId ?? null" :selected-name="draft.name"
                    :count="draft.steps.length" :busy="!!scriptsBusy" :loading="!!scriptsLoading"
                    :error="scriptsError" :sync-label="draftSyncLabel"
                    :sync-message="draftSyncMessage" :guard="effectiveScriptGuard"
                    :save-busy="!!draftSaveBusy" :save-message="draftSaveMessage"
                    :formal-pending="!!formalPending"
                    @refresh="emit('script-refresh',device.id)"
                    @create="name => requestScriptAction({kind:'create',deviceId:device.id,name})"
                    @select="chooseScript" @clone="openScriptClone"
                    @publish="requestFormalPublish" @remove="requestFormalRemove"
                    @save-draft="emit('draft-save',device.id)"
                    @fork-draft="emit('draft-fork',device.id)"
                    @reconcile="emit('script-reconcile')">
                    <FloatingBatchRunPanel :owner-id="ownerId" :device-id="device.id"
                        :local-name="device.name" :local-available="!!device.connection.enabled && !device.availability_reason"
                        :steps-count="draft.steps.length"
                        :parameter-names="[...new Set(draft.steps.filter(step=>step.type==='input_text' && !!step.parameter).map(step=>step.parameter!.trim()))]"
                        :runner-available="runnerAvailable"
                        :draft-ready="batchDraftReady && !editingDirty"
                        :local-message="batchRunMessage"
                        :block-reason="editingStepId || editingDirty
                            ? '步骤正在编辑，请先在步骤浮窗保存并关闭。'
                            : effectiveScriptGuard==='conflict' ? '服务器草稿出现版本冲突，请先另存新草稿。'
                            : effectiveScriptGuard==='unsynced' ? '工作草稿未同步，请先点击“保存草稿”。'
                            : effectiveScriptGuard==='loading' ? '工作草稿正在从服务器加载。' : ''"
                        :state="batchRunState" :force-cleanup-busy="forceCleanupBusy"
                        @force-cleanup="emit('force-cleanup',device.id)"
                        @start-local="params=>emit('batch-start-local',device.id,params)"
                        @refresh="emit('batch-refresh',device.id)"
                        @stop-device="id=>{if(id===device.id)emit('batch-stop-device',device.id)}"
                        @reconcile="emit('batch-reconcile',device.id)"
                        @new-run="emit('batch-new-run',device.id)" />
                    <CardStepTimeline :draft="draft" :picker-active="pickingPhase === 'on'"
                        :batch-run="batchRunState.owner===ownerId && batchRunState.selected.includes(device.id)
                            ? batchRunState.run : null" :batch-device-id="device.id"
                        :trial-progress="trialProgress" :trial-active="trialStepId !== null"
                        :undo-label="undoDeleteLabel"
                        @edit="emit('step-edit',device.id,$event)"
                        @trial="openCardTrial" @delete="requestStepDeletion"
                        @create="type=>emit('step-create',device.id,type)"
                        @branch="(id,lane,type)=>emit('step-branch-add',device.id,id,lane,type)"
                        @convert="(id,type)=>emit('step-convert-condition',device.id,id,type)"
                        @move="(id,delta)=>emit('step-move',device.id,id,delta)"
                        @undo="emit('step-undo',device.id)" />
                    <p v-if="stepDeleteError" class="step-delete-error" role="alert">{{ stepDeleteError }}</p>
                </AutomationScriptSelector>
            </AutomationFloatingPanel>
            <AutomationScriptManageDialog v-if="manageScript"
                :visible="!!manageScript" :mode="manageScript.mode"
                :source="manageSource" :draft-name="draft.name"
                :draft-description="draft.description" :step-count="draft.steps.length"
                :guard="effectiveScriptGuard" :busy="!!scriptsBusy"
                :save-busy="!!draftSaveBusy" :save-message="draftSaveMessage"
                :error="scriptsError"
                :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
                @close="manageScript=null;manageWaiting=false"
                @save-draft="emit('draft-save',device.id)"
                @publish="finishFormalPublish" @remove="finishFormalRemove" />
            <AutomationScriptCloneDialog :source="cloneSource"
                :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
                :busy="!!scriptsBusy" @cancel="cloneSourceId=null" @confirm="confirmScriptClone" />
            <AutomationScriptSwitchDialog :visible="!!pendingScriptAction"
                :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
                :guard="effectiveScriptGuard" :busy="!!scriptsBusy"
                @cancel="pendingScriptAction=null" @decide="decideScriptSwitch" />
            <AutomationConditionDeleteDialog :plan="pendingStepDeletion?.plan ?? null"
                :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
                @cancel="pendingStepDeletion=null" @confirm="confirmStepDeletion" />
            <AutomationFloatingDialog v-if="editingStep" ref="stepEditorDialog"
                :visible="!!editingStep" :portal-target="automationSubwindowHost"
                :fullscreen="isFullscreen" :dirty="!!editingDirty"
                :title="'编辑第 '+editingStepNumber+' 步'"
                @close="finishEditorClose">
                <CardStepEditor :key="editingStep.id" floating
                    :step="editingStep" :step-number="editingStepNumber"
                    :business-verification-eligible="businessVerificationEligible"
                    :repicking="repickingStepId === editingStep.id"
                    :demo-loading="demoLoading" :demo-result="demoResult"
                    :demo-error="demoError" :demo-available="ready && runnerAvailable"
                    @close="requestEditorClose"
                    @save="(stepId, expected, updated) => emit('step-save',device.id,stepId,expected,updated)"
                    @repick="emit('step-repick',device.id,$event)"
                    @demo="emit('step-demo',device.id,$event)"
                    @clear-demo="emit('demo-clear',device.id)"
                    @dirty-change="emit('step-dirty',device.id,$event)" />
            </AutomationFloatingDialog>
            <!-- Non-fullscreen editors float over the whole browser viewport.
                 Fullscreen moves the SAME host back into the phone-screen top layer. -->
            <Teleport to="body" :disabled="isFullscreen">
                <div ref="automationSubwindowHost" class="automation-subwindow-host"
                    :class="{ 'detached-subwindows': !isFullscreen }"
                    :style="!isFullscreen ? detachedTheme : undefined" />
            </Teleport>
        </div>
        <footer><span class="phase" :class="{ live: session.state.phase === 'live' }">{{ t(session.state.phase) }}<template v-if="session.state.phase === 'live' && session.state.viewerOnly"> · 只读观看，手控已锁定</template></span>
            <small v-if="batchRunState.selected.includes(device.id) && batchRunState.devices[device.id]"
                class="batch-card-status" role="status">
                本机：{{ batchRunState.devices[device.id].phase }}
                <span v-if="batchRunState.devices[device.id].mediaState==='lost'"> · 视频已断流</span>
            </small><span class="mono-value" :title="'ADB：' + adbConnectionHint">
                {{ device.overlay_ip }}<template v-if="device.connection.adb_port > 0">:{{ device.connection.adb_port }}</template>
            </span></footer>
        <!-- T42: failed-step repair uses the same real PhoneCard video and
             its fullscreen subwindow host, never a second preview or footer UI. -->
        <AutomationFloatingDialog :visible="!!repairContext && !!repairStep"
            :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
            :title="'第 '+repairStepNumber+' 步 · 失败回修'"
            :closable="repairContext?.resultState !== 'outcome_unknown'"
            @close="emit('repair-dismiss',device.id)">
            <CardRepairEntry v-if="repairContext && repairStep" :repair="repairContext"
                :step="repairStep" :step-number="repairStepNumber"
                :editing="!!editingStepId" :runner-available="runnerAvailable"
                :saved="effectiveScriptGuard === 'clean'"
                :blocked="!!syncRole || recording || !!unlocking"
                @edit="emit('step-edit',device.id,$event)"
                @repick="emit('repair-repick',device.id,$event)"
                @trial="stepId => openCardTrial(stepId,true)"
                @dismiss="emit('repair-dismiss',device.id)" />
        </AutomationFloatingDialog>
        <div v-if="trialToast" class="trial-toast" :class="'trial-toast-'+trialToast.severity"
            role="alert" aria-live="assertive">
            <i :class="trialToast.severity==='error'?'pi pi-exclamation-triangle':'pi pi-check-circle'" aria-hidden="true" />
            <span>{{trialToast.message}}</span>
            <button type="button" title="关闭提示" aria-label="关闭提示" @click="trialToast=null"><i class="pi pi-times" /></button>
        </div>
        <AutomationFloatingDialog :visible="trialCompare !== null" wide
            :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
            title="单步骤定位 · 多控件匹配对比" @close="trialCompare=null">
            <TrialLocatorCompare v-if="trialCompare" :diagnosis="trialCompare" />
        </AutomationFloatingDialog>
        <CardTrialPanel :visible="trialStepId !== null" :headless="true" :owner-id="ownerId" :device="device"
            :step="trialStep" :preview-handle="getTrialPreviewHandle"
            :portal-target="automationSubwindowHost" :fullscreen="isFullscreen"
            :runner-available="runnerAvailable"
            :blocked="!!recording || !!syncRole || !!unlocking || !!editingStepId"
            @progress="onTrialProgress" @finished="finishCardTrial"
            @close="trialStepId = null"
            @history="runId => { trialStepId = null; emit('trial-history',device.id,runId); }"
            @repair="(runId,stepId) => { trialStepId = null; emit('trial-repair',device.id,runId,stepId); }" />
        <small v-if="session.state.unlockState === 'wake_sent'" class="unlock-notice">手机已发送亮屏指令；该 ADB 服务没有保存开屏密码，请在设备管理中配置后重试。</small>
        <small v-else-if="session.state.unlockState === 'already_unlocked'" class="unlock-notice">已检测到手机处于解锁状态，因此没有输入密码。</small>
        <small v-else-if="session.state.unlockState === 'lock_state_unknown'" class="unlock-notice">手机已唤醒，但 Android 未返回可靠的锁屏状态；为避免把密码输入其他应用，已跳过自动解锁。</small>
        <small v-else-if="session.state.unlockState === 'lock_state_changed'" class="unlock-notice">等待后锁屏状态不明确，已停止自动输入密码；请检查手机锁屏界面。</small>
        <small v-else-if="session.state.unlockState === 'unlock_size_unknown'" class="unlock-notice">读取手机屏幕尺寸失败，无法执行解锁上滑；请检查 ADB 权限。</small>
        <small v-else-if="session.state.unlockState === 'unlock_swipe_failed'" class="unlock-notice">手机拒绝或未完成解锁上滑，已跳过密码输入；请检查 ADB 权限。</small>
        <small v-else-if="session.state.unlockState === 'unlock_skipped'" class="unlock-notice">本次解锁中途停止，未确认最终锁屏状态。</small>
        <small v-else-if="session.state.unlockState === 'unlock_sent'" class="unlock-notice">已发送开屏密码输入和确认指令，但手机尚未确认解锁成功。</small>
        <small v-else-if="session.state.unlockState === 'unlock_confirmed'" class="unlock-notice">已确认手机解锁。</small>
        <div v-if="device.endpoint_event" class="endpoint-status"><small>目标切换：{{ device.endpoint_event.cleanup.state }} / {{ device.endpoint_event.connection.state }}</small><Button v-if="device.endpoint_event.cleanup.state !== 'confirmed'" label="重试清理" text size="small" :loading="cleaningSaved" @click="retryEndpoint('cleanup')" /><Button v-else-if="device.endpoint_event.connection.state === 'pending'" label="重试连接" text size="small" :loading="cleaningSaved" @click="retryEndpoint('connection')" /><Button v-if="device.endpoint_event.publication && !['applied','not_applicable'].includes(device.endpoint_event.publication.state)" label="重试公开应用" text size="small" :loading="cleaningSaved" @click="retryEndpoint('publication')" /></div>
        <p v-if="session.state.error || device.availability_reason" class="phone-error" role="status">{{ session.state.error || device.availability_reason }}</p>
    </article>
</template>

<style scoped>
.step-delete-error{color:var(--p-red-500);font-size:11px;line-height:1.5;margin:4px 0}
.batch-card-status{font-size:10px;color:var(--p-primary-color);max-width:100%;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.endpoint-status{padding:8px 14px;display:flex;gap:6px;flex-wrap:wrap;color:var(--console-muted)}
.phone-card{min-width:0;max-width:100%;flex:0 1 auto;border:1px solid var(--console-border);border-radius:14px;background:var(--console-panel);overflow:hidden;box-shadow:var(--console-shadow)}
.phone-card.selected{border-color:var(--p-primary-color);box-shadow:0 0 0 1px color-mix(in srgb,var(--p-primary-color) 20%,transparent)}
.phone-card.master{box-shadow:0 0 0 2px var(--p-primary-color)}
header{display:flex;align-items:center;justify-content:space-between;padding:10px 10px 10px 14px;gap:6px}
.device-heading{display:flex;align-items:center;gap:4px;min-width:0;flex:1 1 auto}
.device-selection{display:flex;align-items:center;gap:8px;min-width:0;flex:0 1 auto;font-weight:600}
.device-selection .device-name{display:block;min-width:0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
.device-selection .gateway-name{display:block;min-width:0;flex:0 1 auto;max-width:116px;overflow:hidden;
    white-space:nowrap;text-overflow:ellipsis;font-size:11px;font-weight:400;color:var(--console-muted)}
.device-selection input{accent-color:var(--p-primary-color);width:16px;height:16px;flex-shrink:0}
.card-header-actions{display:flex;align-items:center;justify-content:flex-end;flex:0 0 auto;gap:3px}
.card-action-tip{display:inline-flex;align-items:center;justify-content:center;flex:0 0 28px}
.card-icon{display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;padding:0;border:0;border-radius:8px;background:transparent;color:var(--console-muted);font-size:15px;cursor:pointer}
.card-icon:hover:not(:disabled){background:var(--console-accent-soft)}
.card-icon:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:2px}
.card-icon:disabled{cursor:default;opacity:.7}
.connect-icon.connected,.adb-port.live{color:var(--p-green-500)}
.connect-icon.connecting,.adb-port.opening,.adb-port.connecting{color:var(--p-primary-color)}
.adb-port{color:var(--p-orange-500)}
.adb-port.failed,.adb-port.cleanup_pending{color:var(--p-red-500)}
.adb-port.unavailable{color:var(--console-muted)}
.card-header-actions :deep(.card-settings){width:28px;height:28px;min-width:28px;padding:0;flex-shrink:0}
.adb-status{display:inline-flex;align-items:center;justify-content:center;width:20px;height:23px;
    padding:0;flex-shrink:0;border-radius:50%}
.adb-status-dot{width:9px;height:9px;border-radius:50%;background:var(--console-muted);box-shadow:0 0 0 2px color-mix(in srgb,var(--console-muted) 20%,transparent)}
.adb-status.connected .adb-status-dot{background:var(--p-green-500);box-shadow:0 0 0 2px color-mix(in srgb,var(--p-green-500) 22%,transparent)}
.adb-status.failed .adb-status-dot,.adb-status.query_failed .adb-status-dot,.adb-status.disconnected .adb-status-dot,.adb-status.offline .adb-status-dot{background:var(--p-red-500)}.adb-status.unauthorized .adb-status-dot,.adb-status.unavailable .adb-status-dot{background:var(--p-orange-500)}.adb-status.connecting .adb-status-dot,.adb-status.checking .adb-status-dot{background:var(--p-blue-500);animation:adb-pulse 1s ease-in-out infinite}
@keyframes adb-pulse{50%{opacity:.35}}
.phone-screen{width:100%;height:auto;aspect-ratio:9/16;position:relative;background:#07111d;overflow:hidden}.phone-screen video{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;display:block;touch-action:none;user-select:none}.phone-screen video.interactive{cursor:crosshair}
.primary-video:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:-3px}
.pick-menu-dismiss{position:absolute;inset:0;z-index:8;border:0;background:transparent;width:100%;height:100%;cursor:default}
.screen-placeholder{position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:14px;color:#91a6bd;background:radial-gradient(ellipse at center,#11273a,#07111d 75%);padding:18px;text-align:center}.screen-placeholder>.pi{font-size:34px;color:#54738f}.screen-placeholder small{font-size:11px}
.role-badge{position:absolute;top:10px;left:10px;pointer-events:none}.record-badge{position:absolute;top:12px;right:12px;color:#fda4af;font-size:11px;display:flex;align-items:center;gap:5px}.record-badge>span{width:7px;height:7px;border-radius:50%;background:#fb7185}
.phone-controls{position:absolute;z-index:6;transform:translateY(-50%);display:flex;align-items:center;gap:6px;transition:left .16s ease;touch-action:none}
.phone-controls.menu-expanded{z-index:12}
.automation-subwindow-host{position:absolute;inset:0;z-index:13;pointer-events:none}
.automation-subwindow-host.detached-subwindows{position:fixed;inset:0;z-index:1120;pointer-events:none;contain:layout style}
.phone-controls :deep(.p-button){width:42px;height:42px;min-width:42px;box-shadow:0 3px 14px #0006;border:1px solid #ffffff38;backdrop-filter:blur(10px)}
.phone-controls :deep(.p-button-secondary){background:#142c42e8;color:#e1edf7}
.main-orb{background:var(--p-primary-color);cursor:grab;touch-action:none;position:relative;z-index:2}.main-orb:active{cursor:grabbing}
.floating-actions{position:absolute;inset:0;pointer-events:none;overflow:visible;z-index:1}
.floating-actions>div{position:absolute;pointer-events:auto;display:flex;gap:4px;align-items:center;border-radius:12px;padding:3px;background:color-mix(in srgb,#081b30 82%,transparent);box-shadow:0 4px 16px #0006;backdrop-filter:blur(9px)}
.input-actions{bottom:calc(100% + 10px);left:0;flex-direction:column}
.nav-actions{top:calc(100% + 10px);left:0;flex-direction:column}
.side-actions{top:0;flex-direction:row}.side-actions :deep(.p-button){width:37px;height:37px;min-width:37px}
.dock-right .side-actions{right:calc(100% + 7px)}
.dock-left .side-actions{left:calc(100% + 7px)}
/* When portrait width or landscape height is too small, all action groups
   remain in their original DOM order but use a scrollable tray INSIDE the
   existing fullscreen phone-screen; no group may be clipped off screen. */
.phone-controls.compact-actions .floating-actions{
    inset:auto;top:50%;transform:translateY(-50%);
    display:flex;flex-direction:column;gap:5px;align-items:stretch;
    width:min(215px,calc(var(--screen-width) - 62px));
    max-height:calc(var(--screen-height) - 16px);
    overflow-y:auto;overscroll-behavior:contain;
    padding:5px;pointer-events:auto;border-radius:12px;
    background:color-mix(in srgb,#081b30 92%,transparent);
    box-shadow:0 6px 24px #0009;
}
.phone-controls.compact-actions.dock-right .floating-actions{right:calc(100% + 6px)}
.phone-controls.compact-actions.dock-left .floating-actions{left:calc(100% + 6px)}
.phone-controls.compact-actions .floating-actions>div{
    position:static;display:flex;flex-direction:row;flex-wrap:wrap;
    justify-content:center;align-items:center;flex:0 0 auto;width:100%;
    gap:4px;padding:3px;border-radius:8px;
    background:transparent;box-shadow:none;backdrop-filter:none;
}
.phone-controls.compact-actions .floating-actions :deep(.p-button){
    width:36px;height:36px;min-width:36px
}
.control-dismiss{position:absolute;inset:0;z-index:5;background:transparent;border:0;cursor:default}
footer{display:flex;align-items:center;justify-content:space-between;padding:10px 12px;gap:8px;font-size:11px}.phase{color:var(--console-muted)}.phase.live{color:var(--p-green-500)}.phase.live:before{content:'';display:inline-block;width:6px;height:6px;background:currentColor;border-radius:50%;margin-right:6px}.unlock-notice{display:block;padding:0 12px 10px;color:var(--console-muted);font-size:11px}.phone-error{font-size:11px;padding:0 12px 10px;color:var(--p-red-500);overflow-wrap:anywhere;max-height:80px;overflow:auto}.fullscreen-stop{display:none}
.trial-toast{position:absolute;top:14px;right:14px;z-index:70;display:flex;align-items:flex-start;
gap:9px;max-width:min(420px,calc(100% - 28px));padding:12px;border-radius:9px;
background:var(--console-panel-raised);border:1px solid var(--console-border);
box-shadow:0 6px 25px #0003;color:var(--p-text-color);font-size:12px;line-height:1.5}
.trial-toast-error{border-color:var(--p-red-500);color:var(--p-red-500)}
.trial-toast-success{border-color:var(--p-green-500);color:var(--p-green-500)}
.trial-toast button{border:0;background:transparent;cursor:pointer;color:inherit;margin-left:auto}
/* Match EasyADB: only the phone SCREEN enters the browser top layer.
   The decoded image is never stretched or cropped, on desktop or mobile. */
.phone-screen:fullscreen{width:100vw;max-width:none;height:100dvh;max-height:none;aspect-ratio:auto!important;border-radius:0;background:#142033}
.phone-screen:fullscreen .primary-video{object-fit:contain;z-index:1}
/* The existing main video, the automation panel and T24's portal are all
   children of this SAME fullscreen element, never portals to body. */
.phone-screen:fullscreen .automation-subwindow-host{position:absolute;inset:0;z-index:13}
.phone-screen:fullscreen .ambient-video{object-fit:cover;z-index:0;pointer-events:none;filter:blur(28px) brightness(.4);transform:scale(1.04);touch-action:none}
.phone-screen:fullscreen .phone-controls.dock-right{left:auto!important;right:14px;top:50%}
.phone-screen:fullscreen .phone-controls.dock-left{left:14px!important;right:auto;top:50%}
.phone-screen:fullscreen .fullscreen-stop{display:inline-flex;position:absolute;top:12px;left:50%;transform:translateX(-50%);z-index:7}
@media(max-width:600px){.floating-actions :deep(.p-button){width:36px;height:36px;min-width:36px}.side-actions{gap:2px!important}}
</style>
