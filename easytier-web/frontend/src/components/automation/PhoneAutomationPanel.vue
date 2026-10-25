<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue';
import { Button, Dialog, Message, Tag } from 'primevue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationComponentStatus, AutomationDevice, AutomationPreparation, AutomationPreparationLog } from '../../types/automation';

const props = defineProps<{ device: AutomationDevice; ownerId: number; runnerAvailable: boolean }>();
const api = new AutomationApi();
const visible = ref(false);
const status = ref<AutomationComponentStatus | null>(null);
const jobs = ref<AutomationPreparation[]>([]);
const current = ref<AutomationPreparation | null>(null);
const selectedId = ref('');
const logs = ref<AutomationPreparationLog[]>([]);
const busy = ref(false);
const error = ref('');
const queried = ref(false);
let disposed = false;
let epoch = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
const activeStates = new Set(['queued', 'running', 'cancel_requested']);
const labels: Record<string,string> = {
    ready:'已就绪', not_prepared:'未准备', preparing:'准备中',
    adb_unavailable:'ADB离线', unavailable:'控件不可用',
    queued:'排队中', running:'初始化中', cancel_requested:'停止中',
    succeeded:'准备成功', failed:'准备失败', interrupted:'已中断',
    cancelled:'已取消', cleanup_pending:'清理待确认',
};
const caption = (state?: string) => labels[state || ''] || '未查询';
function stopPolling() { if (timer) clearTimeout(timer); timer = undefined; }
function schedule() {
    stopPolling();
    if (visible.value && current.value && activeStates.has(current.value.state))
        timer = setTimeout(() => void poll(), 2000);
}
async function query() {
    if (busy.value || !props.runnerAvailable) return;
    const generation = epoch, did = props.device.id, owner = props.ownerId;
    busy.value = true; error.value = '';
    try {
        const [state, history] = await Promise.all([
            api.componentStatus(owner, did), api.preparationHistory(owner, did),
        ]);
        if (disposed || generation !== epoch) return;
        status.value = state;
        jobs.value = history.items;
        queried.value = true;
        current.value = history.items.find(job => activeStates.has(job.state)) ?? history.items[0] ?? null;
        if (current.value && !selectedId.value) selectedId.value = current.value.id;
        schedule();
    } catch (cause) { if (generation === epoch) error.value = automationErrorMessage(cause); }
    finally { if (generation === epoch) busy.value = false; }
}
async function poll() {
    stopPolling();
    const job = current.value;
    if (!job || !visible.value || !activeStates.has(job.state)) return;
    const generation = epoch;
    try {
        const updated = await api.preparationDetail(props.ownerId, props.device.id, job.id);
        if (disposed || generation !== epoch) return;
        current.value = updated;
        jobs.value = jobs.value.map(item => item.id === updated.id ? updated : item);
        if (!activeStates.has(updated.state)) {
            status.value = await api.componentStatus(props.ownerId, props.device.id);
            if (selectedId.value === updated.id) void fetchLogs(updated.id);
        }
    } catch (cause) { if (generation === epoch) error.value = automationErrorMessage(cause); }
    finally { if (generation === epoch) schedule(); }
}
async function initialize() {
    if (busy.value || !props.runnerAvailable || activeStates.has(current.value?.state ?? '')) return;
    const generation = epoch;
    busy.value = true; error.value = '';
    try {
        const job = await api.startPreparation(props.ownerId, props.device.id, uuid());
        if (disposed || generation !== epoch) return;
        current.value = job;
        jobs.value = [job, ...jobs.value.filter(value => value.id !== job.id)].slice(0,20);
        selectedId.value = job.id;
        logs.value = [];
        status.value = {device_id:props.device.id, executor_id:status.value?.executor_id ?? '',state:'preparing',preparation_id:job.id};
        schedule();
    } catch (cause) { if (generation === epoch) error.value = automationErrorMessage(cause); }
    finally { if (generation === epoch) busy.value = false; }
}
async function fetchLogs(pid: string) {
    const generation = epoch;
    selectedId.value = pid;
    try {
        const value = await api.preparationLogs(props.ownerId, props.device.id, pid);
        if (generation === epoch && selectedId.value === pid) logs.value = value.items;
    } catch (cause) { if (generation === epoch) error.value = automationErrorMessage(cause); }
}
async function cancel() {
    const job = current.value;
    if (!job || !activeStates.has(job.state)) return;
    busy.value = true; error.value = '';
    try { current.value = await api.cancelPreparation(props.ownerId, props.device.id, job.id); schedule(); }
    catch (cause) { error.value = automationErrorMessage(cause); }
    finally { busy.value = false; }
}
function date(value: string) { return new Date(value).toLocaleString(); }
watch(visible, open => {
    if (open) void query();
    else { stopPolling(); ++epoch; busy.value = false; }
});
watch(() => [props.device.id, props.ownerId, props.device.target_fingerprint], () => {
    stopPolling(); ++epoch; busy.value = false; status.value = null; current.value = null; jobs.value = [];
    queried.value = false; selectedId.value = ''; logs.value = []; error.value = '';
    if (visible.value) void query();
});
onBeforeUnmount(() => { disposed = true; ++epoch; stopPolling(); });
</script>

<template>
    <button type="button" class="component-chip" :class="status?.state ?? 'unchecked'"
        :disabled="!runnerAvailable" :title="'管理 UiAutomator 控件组件 · ' + caption(status?.state)"
        :aria-label="'控件状态：' + caption(status?.state)" @click.stop="visible = true">
        <i class="pi pi-microchip" aria-hidden="true" />
        <span>控件 · {{ caption(status?.state) }}</span>
    </button>
    <Dialog v-model:visible="visible" modal append-to="body" :header="device.name + ' · 控件组件'"
        :style="{width:'640px',maxWidth:'calc(100vw - 24px)'}">
        <div class="component-content">
            <p class="hint">UiAutomator2 · 每台手机独立管理。查询只读取当前状态，不会安装、启动或重启组件。</p>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <div class="status-row">
                <strong>当前状态</strong>
                <Tag :value="queried ? caption(status?.state) : '未查询'"
                    :severity="status?.state === 'ready' ? 'success' : status?.state === 'preparing' ? 'info' : 'secondary'" />
            </div>
            <div class="component-actions">
                <Button label="初始化" icon="pi pi-play" size="small" :loading="busy"
                    :disabled="!runnerAvailable || busy || status?.state === 'ready' || status?.state === 'preparing' || activeStates.has(current?.state ?? '')" @click="initialize" />
                <Button label="主动查询状态" icon="pi pi-refresh" size="small" outlined :loading="busy"
                    :disabled="!runnerAvailable || busy" @click="query" />
                <Button v-if="current && activeStates.has(current.state)" label="停止初始化" icon="pi pi-stop"
                    severity="danger" size="small" outlined :disabled="busy" @click="cancel" />
            </div>
            <div v-if="current" class="job-summary">
                <div class="status-row"><strong>最近任务</strong><Tag :value="caption(current.state)" :severity="current.state === 'succeeded' ? 'success' : current.state === 'failed' ? 'danger' : 'secondary'" /></div>
                <span>阶段：{{ current.stage }} · {{ date(current.updated_at) }}</span>
                <span v-if="current.code || current.message">{{ current.code || '' }} {{ current.message || '' }}</span>
            </div>
            <div class="history-heading"><strong>初始化历史</strong><Button label="刷新记录" text size="small" icon="pi pi-refresh" :disabled="busy" @click="query" /></div>
            <div v-if="jobs.length" class="job-list">
                <button v-for="job in jobs" :key="job.id" type="button" :class="{selected:selectedId === job.id}" @click="fetchLogs(job.id)">
                    <span>{{ date(job.created_at) }}</span><span>{{ caption(job.state) }}</span><i class="pi pi-angle-right" />
                </button>
            </div>
            <p v-else class="hint">暂无初始化历史</p>
            <section v-if="selectedId" class="log-panel">
                <div class="history-heading"><strong>阶段日志</strong><Button label="刷新日志" text size="small" icon="pi pi-refresh" @click="fetchLogs(selectedId)" /></div>
                <div v-if="logs.length" class="log-lines">
                    <div v-for="event in logs" :key="event.seq" :class="event.level">
                        <time>{{ date(event.at) }}</time> [{{ event.level }}] {{ event.stage }} · {{ event.message }}
                    </div>
                </div>
                <p v-else class="hint">暂无日志，点击“刷新日志”查询。</p>
            </section>
        </div>
    </Dialog>
</template>

<style scoped>
.component-chip{display:inline-flex;align-items:center;gap:4px;white-space:nowrap;border:1px solid var(--console-border);background:var(--console-panel-raised);color:var(--console-muted);border-radius:6px;padding:3px 5px;font-size:10px;cursor:pointer}
.component-chip.ready{color:var(--p-green-500)}.component-chip.unavailable,.component-chip.adb_unavailable{color:var(--p-red-500)}
.component-chip.preparing{color:var(--p-primary-color)}.component-chip:disabled{opacity:.6;cursor:default}
.component-chip:hover:not(:disabled){border-color:var(--p-primary-color)}
.component-content{display:flex;flex-direction:column;gap:14px}
.hint{font-size:12px;color:var(--console-muted);line-height:1.6}
.status-row,.history-heading{display:flex;justify-content:space-between;align-items:center;gap:10px;font-size:13px}
.component-actions{display:flex;gap:8px;flex-wrap:wrap}
.job-summary{border:1px solid var(--console-border);background:var(--console-panel-raised);border-radius:8px;padding:12px;display:flex;flex-direction:column;gap:8px;font-size:12px;overflow-wrap:anywhere}
.job-list{max-height:160px;overflow:auto;display:flex;flex-direction:column}
.job-list button{display:flex;align-items:center;justify-content:space-between;gap:10px;padding:9px;border:0;border-bottom:1px solid var(--console-border);background:transparent;color:var(--p-text-color);cursor:pointer;text-align:left;font-size:12px}
.job-list button.selected{background:var(--console-panel-raised)}.job-list button:hover{color:var(--p-primary-color)}
.log-panel{display:flex;flex-direction:column;gap:8px}.log-lines{max-height:220px;overflow:auto;background:var(--console-panel-raised);padding:10px;border-radius:6px;font:11px/1.6 monospace;overflow-wrap:anywhere}
.log-lines .error{color:var(--p-red-500)}.log-lines .success{color:var(--p-green-500)}
</style>