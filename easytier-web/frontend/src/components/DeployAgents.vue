<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Button, Dialog, InputText, Message, Tag, Textarea } from 'primevue';
import { v4 as uuid } from 'uuid';
import { useI18n } from 'vue-i18n';
import { deployMessages } from '../modules/deploy-messages';

const { t, locale } = useI18n({ useScope: 'local', messages: deployMessages });
import { deployRequest, type DeployAgent, type DeployApplication, type DeployTask, type InstallRequest } from '../modules/deploy-api';

const agents = ref<DeployAgent[]>([]);
const selectedId = ref('');
const selected = computed(() => agents.value.find(agent => agent.id === selectedId.value));
const tasks = ref<DeployTask[]>([]);
const busy = ref(false);
const error = ref('');
const notice = ref('');
const adding = ref(false);
const form = ref({ alias: '', endpoint: 'https://', ca_pem: '', token: '' });
const app = ref<DeployApplication>();
const variant = ref('');
const values = ref<Record<string, string>>({});
const enabled = ref<Record<string, boolean>>({});
const pending = ref<InstallRequest>();
const viewingLog = ref<DeployTask>();
const services = computed(() => selected.value?.services?.services ?? []);
const groups = computed(() => {
    const result = new Map<string, typeof services.value>();
    for (const item of services.value) {
        const name = item.project ? `Compose · ${item.project}` : item.provider;
        result.set(name, [...(result.get(name) ?? []), item]);
    }
    return [...result.entries()];
});
const date = (value?: string) => value ? new Date(value).toLocaleString(locale.value === 'cn' ? 'zh-CN' : 'en-US') : t('notQueried');
const state = (value: string) => ['accepted', 'running', 'succeeded', 'failed', 'interrupted', 'outcome_unknown'].includes(value) ? t(value) : value;

async function perform(work: () => Promise<void>) {
    if (busy.value) return;
    busy.value = true; error.value = ''; notice.value = '';
    try { await work(); } catch (e) { error.value = e instanceof Error ? e.message : t('failedAction'); }
    finally { busy.value = false; }
}
async function loadSaved() {
    agents.value = await deployRequest<DeployAgent[]>('');
    if (!selected.value) selectedId.value = agents.value[0]?.id ?? '';
    tasks.value = selected.value ? await deployRequest<DeployTask[]>(`/${selectedId.value}/tasks`) : [];
}
onMounted(() => perform(loadSaved)); // Saved controller data only. Never probes an Agent.
async function choose(id: string) {
    if (busy.value || pending.value) return;
    selectedId.value = id; app.value = undefined; viewingLog.value = undefined;
    await perform(async () => { tasks.value = await deployRequest<DeployTask[]>(`/${id}/tasks`); });
}
async function add() {
    await perform(async () => {
        const result = await deployRequest<{ id: string }>('', 'POST', form.value);
        form.value = { alias: '', endpoint: 'https://', ca_pem: '', token: '' }; adding.value = false;
        selectedId.value = result.id; await loadSaved(); notice.value = t('savedNotice');
    });
}
async function refresh(action: 'probe' | 'discover' | 'catalog') {
    const id = selectedId.value;
    await perform(async () => {
        try { await deployRequest(`/${id}/${action}`, 'POST'); }
        finally { await loadSaved(); }
    });
}
function configure(application: DeployApplication) {
    if (pending.value) return;
    app.value = application; values.value = {}; enabled.value = {};
    variant.value = selected.value?.identity?.platform === 'openwrt' ? 'openwrt' : application.variants.find(item => item !== 'openwrt') ?? '';
}
async function install(action: 'install' | 'update') {
    if (!selected.value?.catalog || !app.value || pending.value) return;
    const parameters: Record<string, string> = {}; const secrets: Record<string, string> = {};
    for (const field of app.value.configuration) {
        if (enabled.value[field.key]) (field.secret ? secrets : parameters)[field.key] = values.value[field.key] ?? '';
    }
    pending.value = { task_id: uuid(), application_id: app.value.id, action, variant: variant.value, catalog_revision: selected.value.catalog.revision, parameters, secrets };
    await submitPending();
}
async function submitPending() {
    if (!pending.value) return;
    const request = pending.value;
    await perform(async () => {
        try {
            await deployRequest(`/${selectedId.value}/tasks`, 'POST', request);
            pending.value = undefined; values.value = {}; enabled.value = {};
            notice.value = t('submittedNotice');
        } finally { await loadSaved(); }
    });
}
async function taskAction(task: DeployTask, action: 'refresh' | 'fetch-logs' | 'resubmit') {
    await perform(async () => {
        await deployRequest(`/${selectedId.value}/tasks/${task.task_id}/${action}`, 'POST');
        await loadSaved();
        if (action !== 'fetch-logs' && task.task_id === pending.value?.task_id) {
            pending.value = undefined; values.value = {}; enabled.value = {};
        }
        if (action === 'fetch-logs') viewingLog.value = tasks.value.find(item => item.task_id === task.task_id);
    });
}
function clearPending() { pending.value = undefined; notice.value = t('clearedNotice'); }
function singleLinePaste(event: ClipboardEvent) {
    // Native single-line inputs silently strip newlines on paste. Reject the
    // unrepresentable input before the browser can change a chosen credential.
    if (/[\r\n\0]/.test(event.clipboardData?.getData('text') ?? '')) {
        event.preventDefault();
        error.value = t('pasteError');
    }
}
</script>

<template>
    <section class="console-page deploy-page" aria-labelledby="deploy-title">
        <header class="page-heading deploy-header">
            <div class="heading-identity"><span class="entity-icon page-icon"><i class="pi pi-box" aria-hidden="true"></i></span><div><h1 id="deploy-title">{{ t('title') }}</h1><p class="page-description">{{ t('description') }}</p></div></div>
            <div class="actions"><Button :label="t('readSaved')" icon="pi pi-refresh" severity="secondary" :disabled="busy" @click="perform(loadSaved)" />
                <Button :label="t('addDevice')" icon="pi pi-plus" :disabled="busy" @click="adding = true" /></div>
        </header>
        <Message v-if="error" severity="error" :closable="false" role="alert">{{ error }}</Message>
        <Message v-if="notice" severity="info" :closable="false" role="status">{{ notice }}</Message>
        <p v-if="busy" role="status">{{ t('working') }}</p>
        <p class="page-footnote"><i class="pi pi-bolt" aria-hidden="true"></i>{{ t('manual') }}</p>
        <div class="deploy-layout">
            <nav class="console-panel device-selector" :aria-label="t('devices')">
                <div class="selector-heading"><h2>{{ t('devices') }}</h2><span class="mono-value">{{ agents.length }}</span></div><p v-if="!agents.length" class="muted">{{ t('addHint') }}</p>
                <button v-for="agent in agents" :key="agent.id" type="button" :class="{ active: selectedId === agent.id }" :disabled="busy || !!pending" :aria-pressed="selectedId === agent.id" @click="choose(agent.id)">
                    <strong>{{ agent.alias }}</strong><small>{{ agent.endpoint }}</small><Tag :value="agent.agent_id ? t('bound') : t('unverified')" severity="secondary" />
                </button>
            </nav>
            <section v-if="!selected" class="console-panel console-empty-state">
                <i class="pi pi-server" aria-hidden="true"></i><h2>{{ t('emptyTitle') }}</h2>
                <p>{{ t('emptyHint') }}</p><Button :label="t('addDevice')" icon="pi pi-plus" :disabled="busy" @click="adding = true" />
            </section>
            <div v-else class="device-content">
                <section class="console-panel deploy-panel">
                    <div class="panel-heading"><h2>{{ selected.alias }}</h2><Button :label="t('probe')" icon="pi pi-link" :disabled="busy" @click="refresh('probe')" /></div>
                    <p class="mono-value">{{ selected.endpoint }}</p>
                    <p v-if="selected.identity">{{ selected.identity.hostname }} · {{ selected.identity.platform }} / {{ selected.identity.arch }} · {{ t('softwareRoot') }} {{ selected.identity.software_root }}</p>
                    <p class="muted">{{ t('lastDevice') }}{{ date(selected.identity?.observed_at) }}</p>
                    <Message v-if="selected.last_error" severity="warn" :closable="false">{{ t('lastError', { error: selected.last_error }) }}</Message>
                </section>
                <section class="console-panel deploy-panel">
                    <div class="panel-heading"><h2>{{ t('services') }}</h2><Button :label="t('discover')" icon="pi pi-search" :disabled="busy || !selected.agent_id" @click="refresh('discover')" /></div>
                    <p class="muted">{{ t('lastDiscovery') }}{{ date(selected.services?.observed_at) }}</p>
                    <div v-if="selected.services" class="providers"><span v-for="(provider, name) in selected.services.providers" :key="name">{{ name }}：{{ provider.status === 'ok' ? t('providerOk') : provider.status === 'unavailable' ? t('unavailable') : t('providerError') }} <small>{{ provider.message }}</small></span></div>
                    <p v-if="!selected.services">{{ t('discoverHint') }}</p>
                    <p v-else-if="!services.length">{{ t('noServices') }}</p>
                    <div v-for="[group, entries] in groups" :key="group" class="service-group">
                        <h3>{{ group }}</h3><div class="table-scroll"><table><thead><tr><th>{{ t('service') }}</th><th>{{ t('observedState') }}</th><th>{{ t('image') }}</th><th>{{ t('observedAt') }}</th></tr></thead>
                            <tbody><tr v-for="entry in entries" :key="entry.id"><td>{{ entry.name }}</td><td>{{ entry.state }} <Tag v-if="entry.stale" :value="t('stale')" severity="warn" /></td><td>{{ entry.image || '—' }}</td><td>{{ date(entry.observed_at) }}</td></tr></tbody></table></div>
                    </div>
                </section>
                <section class="console-panel deploy-panel">
                    <div class="panel-heading"><h2>{{ t('installApps') }}</h2><Button :label="t('readCatalog')" icon="pi pi-box" :disabled="busy || !selected.agent_id || !!pending" @click="refresh('catalog')" /></div>
                    <p>{{ t('catalogHint') }}</p>
                    <div class="app-list"><button v-for="application in selected.catalog?.applications" :key="application.id" type="button" :disabled="!application.supported || busy || !!pending" :title="application.reason" :class="{ active: app?.id === application.id }" @click="configure(application)">{{ application.name }}<small v-if="!application.supported">{{ application.reason }}</small></button></div>
                    <form v-if="app" class="app-form" @submit.prevent="install('install')">
                        <h3>{{ app.name }}</h3>
                        <label for="deploy-variant">{{ t('variant') }}</label><select id="deploy-variant" v-model="variant" :disabled="busy || !!pending"><option v-for="option in app.variants.filter(v => selected?.identity?.platform === 'openwrt' ? v === 'openwrt' : v !== 'openwrt')" :key="option" :value="option">{{ option }}</option></select>
                        <p class="muted">{{ t('configHint') }}</p>
                        <div class="fields"><div v-for="field in app.configuration" :key="field.key" class="field">
                            <label :for="`enabled-${field.key}`"><input :id="`enabled-${field.key}`" v-model="enabled[field.key]" type="checkbox" :disabled="busy || !!pending" /> {{ field.key }} {{ field.secret ? t('sensitive') : '' }}</label>
                            <InputText :id="`value-${field.key}`" v-model="values[field.key]" :aria-label="field.key" :type="field.secret ? 'password' : 'text'" :placeholder="field.secret ? t('secretHint') : field.default" :disabled="!enabled[field.key] || busy || !!pending" autocomplete="off" @paste="singleLinePaste" />
                        </div></div>
                        <div class="actions"><Button type="submit" :label="t('install')" :disabled="busy || !!pending" /><Button :label="t('update')" severity="secondary" :disabled="busy || !!pending" @click="install('update')" /></div>
                    </form>
                    <Message v-if="pending" severity="warn" :closable="false">{{ t('pendingHint', { id: pending.task_id }) }}</Message>
                    <div v-if="pending" class="actions"><Button :label="t('resend')" :disabled="busy" @click="submitPending" /><Button :label="t('clearDraft')" severity="secondary" :disabled="busy" @click="clearPending" /></div>
                </section>
                <section class="console-panel deploy-panel">
                    <h2>{{ t('tasks') }}</h2><p v-if="!tasks.length">{{ t('noTasks') }}</p>
                    <article v-for="task in tasks" :key="task.task_id" class="task">
                        <div><strong>{{ task.application_id }} · {{ ['install', 'update'].includes(task.action) ? t(task.action) : task.action }}</strong><Tag :value="state(task.status)" severity="secondary" /><p>{{ task.message }}</p><small>{{ t('taskId') }} {{ task.task_id }} · {{ t('lastQuery') }} {{ date(task.queried_at) }}</small></div>
                        <div class="actions"><Button :label="t('queryStatus')" severity="secondary" :disabled="busy" @click="taskAction(task, 'refresh')" /><Button :label="t('readLogs')" severity="secondary" :disabled="busy" @click="taskAction(task, 'fetch-logs')" /><Button v-if="task.status === 'outcome_unknown'" :label="t('resubmit')" severity="secondary" :disabled="busy" @click="taskAction(task, 'resubmit')" /></div>
                    </article>
                </section>
            </div>
        </div>
        <Dialog v-model:visible="adding" modal :header="t('addTitle')" :style="{ width: '36rem', maxWidth: '95vw' }">
            <form class="add-form" @submit.prevent="add"><label for="agent-alias">{{ t('alias') }}</label><InputText id="agent-alias" v-model="form.alias" required />
                <label for="agent-endpoint">{{ t('endpoint') }}</label><InputText id="agent-endpoint" v-model="form.endpoint" required placeholder="https://192.168.1.20:7014" />
                <label for="agent-token">{{ t('credential') }}</label><Textarea id="agent-token" v-model="form.token" required rows="2" class="secret-input" autocomplete="off" spellcheck="false" />
                <label for="agent-ca">{{ t('trust') }}</label><Textarea id="agent-ca" v-model="form.ca_pem" rows="5" :placeholder="t('trustHint')" />
                <p>{{ t('saveHint') }}</p><Button type="submit" :label="t('save')" :loading="busy" />
            </form>
        </Dialog>
        <Dialog :visible="!!viewingLog" modal :header="t('logsTitle')" :style="{ width: '65rem', maxWidth: '95vw' }" @update:visible="viewingLog = undefined">
            <pre class="task-log">{{ viewingLog?.logs || t('noLogs') }}</pre><Button :label="t('nextLogs')" :disabled="busy" @click="viewingLog && taskAction(viewingLog, 'fetch-logs')" />
        </Dialog>
    </section>
</template>

<style scoped>
.secret-input { -webkit-text-security: disc; }
.deploy-header { flex-wrap: wrap; }
.heading-identity { display: flex; align-items: center; gap: 14px; min-width: 0; }
.page-icon { width: 48px; height: 48px; font-size: 20px; border-radius: 13px; }
.actions { display: flex; gap: 8px; flex-wrap: wrap; }
.deploy-layout { display: grid; grid-template-columns: 240px minmax(0, 1fr); gap: 24px; align-items: start; }
.device-selector, .device-content { display: grid; align-content: start; gap: 12px; min-width: 0; }
.device-content { gap: 24px; }
.device-selector { padding: 14px; }
.selector-heading, .panel-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.selector-heading { padding: 4px 2px 10px; font-size: 13px; color: var(--console-muted); }
.panel-heading { flex-wrap: wrap; padding-bottom: 14px; margin-bottom: 14px; border-bottom: 1px solid var(--console-border); }
.device-selector button, .app-list button { background: var(--console-panel-raised); border: 1px solid var(--console-border); border-radius: 9px; text-align: left; padding: 12px; color: inherit; cursor: pointer; overflow-wrap: anywhere; }
.device-selector button { display: grid; justify-items: start; gap: 8px; min-width: 0; }
.device-selector button small { color: var(--console-muted); }
.device-selector button.active, .app-list button.active { background: var(--console-accent-soft); border-color: var(--p-primary-color); box-shadow: inset 2px 0 var(--p-primary-color); }
button:disabled { opacity: .55; cursor: default; }
.deploy-panel { padding: 20px; overflow-wrap: anywhere; }
.deploy-panel h2 { font-weight: 600; font-size: 14px; }
.deploy-panel h3 { font-weight: 550; font-size: 13px; }
.deploy-panel p { font-size: 13px; margin: 10px 0; }
.providers { display: grid; gap: 8px; font-size: 12px; margin: 12px 0; }
.providers small { color: var(--console-muted); }
.service-group { margin-top: 16px; }
.service-group h3 { margin-bottom: 8px; }
.table-scroll { overflow-x: auto; }
table { border-collapse: collapse; width: 100%; font-size: 13px; text-align: left; }
th, td { padding: 12px; border-bottom: 1px solid var(--console-border); overflow-wrap: anywhere; }
th { background: var(--console-ground); color: var(--console-muted); font-weight: 500; font-size: 12px; }
tr:nth-child(even) td { background: var(--console-stripe); }
.app-list { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 16px; }
.app-list small { display: block; color: var(--console-muted); }
.app-form, .add-form { display: grid; gap: 12px; }
.app-form { margin-top: 20px; padding-top: 20px; border-top: 1px solid var(--console-border); }
.add-form { font-size: 13px; }
.add-form p { color: var(--console-muted); }
.fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }
.field { display: grid; gap: 8px; font-size: 13px; }
.field input[type=checkbox] { accent-color: var(--p-primary-color); }
.field label { overflow-wrap: anywhere; }
.field .p-inputtext { width: 100%; }
.task { padding: 16px 0; border-bottom: 1px solid var(--console-border); display: grid; gap: 12px; font-size: 13px; }
.task:last-child { border-bottom: 0; padding-bottom: 0; }
.task .p-tag { margin-left: 8px; }
.task small { overflow-wrap: anywhere; color: var(--console-muted); }
.task-log { padding: 16px; border: 1px solid var(--console-border); border-radius: 9px; background: var(--console-ground); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 60vh; overflow: auto; font-size: 12px; margin-bottom: 16px; }
select { padding: 10px; color: inherit; background: var(--console-panel); border: 1px solid var(--console-border); border-radius: 8px; }
@media (max-width: 1000px) { .deploy-layout { grid-template-columns: 1fr; } .device-selector { grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); } .selector-heading { grid-column: 1 / -1; } }
@media (max-width: 600px) { .fields { grid-template-columns: 1fr; } .deploy-panel { padding: 16px; } .heading-identity { align-items: flex-start; } }
</style>
