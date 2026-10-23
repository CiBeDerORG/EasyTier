<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Button, Dialog, InputText, Message, Tag, Textarea } from 'primevue';
import { v4 as uuid } from 'uuid';
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
const date = (value?: string) => value ? new Date(value).toLocaleString() : '尚未查询';
const stateLabels: Record<string, string> = { accepted: '已接受', running: '执行中（上次查询）', succeeded: '已完成', failed: '失败', interrupted: '执行中断', outcome_unknown: '结果待核实' };
const state = (value: string) => stateLabels[value] ?? value;

async function perform(work: () => Promise<void>) {
    if (busy.value) return;
    busy.value = true; error.value = ''; notice.value = '';
    try { await work(); } catch (e) { error.value = e instanceof Error ? e.message : '操作失败'; }
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
        selectedId.value = result.id; await loadSaved(); notice.value = '已保存设备。点击“连接并验证”后再查询或安装。';
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
            notice.value = '任务已提交。点击“查询状态”或“读取日志”查看结果。';
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
function clearPending() { pending.value = undefined; notice.value = '已清除本页草稿。原任务仍保留在任务记录中，请先核实其结果。'; }
function singleLinePaste(event: ClipboardEvent) {
    // Native single-line inputs silently strip newlines on paste. Reject the
    // unrepresentable input before the browser can change a chosen credential.
    if (/[\r\n\0]/.test(event.clipboardData?.getData('text') ?? '')) {
        event.preventDefault();
        error.value = '应用配置使用单行文件格式，不能无损保存换行、回车或 NUL；粘贴内容未被修改或写入。';
    }
}
</script>

<template>
    <section class="deploy-page" aria-labelledby="deploy-title">
        <header class="deploy-header">
            <div><h1 id="deploy-title">应用与服务</h1><p>查询和安装由你发起。这里显示上次查询结果。</p></div>
            <div class="actions"><Button label="读取已保存数据" icon="pi pi-refresh" severity="secondary" :disabled="busy" @click="perform(loadSaved)" />
                <Button label="添加设备" icon="pi pi-plus" :disabled="busy" @click="adding = true" /></div>
        </header>
        <Message v-if="error" severity="error" :closable="false" role="alert">{{ error }}</Message>
        <Message v-if="notice" severity="info" :closable="false" role="status">{{ notice }}</Message>
        <p v-if="busy" role="status">正在处理本次操作…</p>
        <div class="deploy-layout">
            <nav class="device-selector" aria-label="部署设备">
                <p v-if="!agents.length">添加 Agent 地址后，点击连接并验证。</p>
                <button v-for="agent in agents" :key="agent.id" type="button" :class="{ active: selectedId === agent.id }" :disabled="busy || !!pending" :aria-pressed="selectedId === agent.id" @click="choose(agent.id)">
                    <strong>{{ agent.alias }}</strong><small>{{ agent.endpoint }}</small><small>{{ agent.agent_id ? '已绑定' : '待验证' }}</small>
                </button>
            </nav>
            <div v-if="selected" class="device-content">
                <section class="deploy-panel">
                    <div class="panel-heading"><h2>{{ selected.alias }}</h2><Button label="连接并验证" icon="pi pi-link" :disabled="busy" @click="refresh('probe')" /></div>
                    <p>{{ selected.endpoint }}</p>
                    <p v-if="selected.identity">{{ selected.identity.hostname }} · {{ selected.identity.platform }} / {{ selected.identity.arch }} · 软件目录 {{ selected.identity.software_root }}</p>
                    <p class="muted">上次设备查询：{{ date(selected.identity?.observed_at) }}</p>
                    <Message v-if="selected.last_error" severity="warn" :closable="false">上次请求失败：{{ selected.last_error }}。历史结果仍保留。</Message>
                </section>
                <section class="deploy-panel">
                    <div class="panel-heading"><h2>发现的服务</h2><Button label="发现服务" icon="pi pi-search" :disabled="busy || !selected.agent_id" @click="refresh('discover')" /></div>
                    <p class="muted">上次发现：{{ date(selected.services?.observed_at) }}</p>
                    <div v-if="selected.services" class="providers"><span v-for="(provider, name) in selected.services.providers" :key="name">{{ name }}：{{ provider.status === 'ok' ? '查询成功' : provider.status === 'unavailable' ? '不可用' : '查询失败' }} <small>{{ provider.message }}</small></span></div>
                    <p v-if="!selected.services">点击发现服务，查询容器与系统服务。</p>
                    <p v-else-if="!services.length">本次成功查询的来源没有返回服务。不可用或失败的来源见上方说明。</p>
                    <div v-for="[group, entries] in groups" :key="group" class="service-group">
                        <h3>{{ group }}</h3><div class="table-scroll"><table><thead><tr><th>服务</th><th>查询到的状态</th><th>镜像</th><th>查询时间</th></tr></thead>
                            <tbody><tr v-for="entry in entries" :key="entry.id"><td>{{ entry.name }}</td><td>{{ entry.state }} <Tag v-if="entry.stale" value="历史结果" severity="warn" /></td><td>{{ entry.image || '—' }}</td><td>{{ date(entry.observed_at) }}</td></tr></tbody></table></div>
                    </div>
                </section>
                <section class="deploy-panel">
                    <div class="panel-heading"><h2>安装应用</h2><Button label="读取应用目录" icon="pi pi-box" :disabled="busy || !selected.agent_id || !!pending" @click="refresh('catalog')" /></div>
                    <p>读取设备上的 deploy 应用目录后选择应用。容器应用需要设备具备 Docker 与 Compose v2。</p>
                    <div class="app-list"><button v-for="application in selected.catalog?.applications" :key="application.id" type="button" :disabled="!application.supported || busy || !!pending" :title="application.reason" :class="{ active: app?.id === application.id }" @click="configure(application)">{{ application.name }}<small v-if="!application.supported">{{ application.reason }}</small></button></div>
                    <form v-if="app" class="app-form" @submit.prevent="install('install')">
                        <h3>{{ app.name }}</h3>
                        <label for="deploy-variant">安装方式</label><select id="deploy-variant" v-model="variant" :disabled="busy || !!pending"><option v-for="option in app.variants.filter(v => selected?.identity?.platform === 'openwrt' ? v === 'openwrt' : v !== 'openwrt')" :key="option" :value="option">{{ option }}</option></select>
                        <p class="muted">只提交勾选的配置项。未勾选项保留设备现有值；首次安装使用 deploy 默认值。勾选后留空表示明确提交空值。</p>
                        <div class="fields"><div v-for="field in app.configuration" :key="field.key" class="field">
                            <label :for="`enabled-${field.key}`"><input :id="`enabled-${field.key}`" v-model="enabled[field.key]" type="checkbox" :disabled="busy || !!pending" /> {{ field.key }} {{ field.secret ? '（敏感配置）' : '' }}</label>
                            <InputText :id="`value-${field.key}`" v-model="values[field.key]" :aria-label="field.key" :type="field.secret ? 'password' : 'text'" :placeholder="field.secret ? '留空按应用原有规则处理' : field.default" :disabled="!enabled[field.key] || busy || !!pending" autocomplete="off" @paste="singleLinePaste" />
                        </div></div>
                        <div class="actions"><Button type="submit" label="安装" :disabled="busy || !!pending" /><Button label="更新" severity="secondary" :disabled="busy || !!pending" @click="install('update')" /></div>
                    </form>
                    <Message v-if="pending" severity="warn" :closable="false">提交结果尚待核实。原任务编号 {{ pending.task_id }}。可以查询原任务，或使用同一编号重新发送。</Message>
                    <div v-if="pending" class="actions"><Button label="重新发送原请求" :disabled="busy" @click="submitPending" /><Button label="清除本页草稿" severity="secondary" :disabled="busy" @click="clearPending" /></div>
                </section>
                <section class="deploy-panel">
                    <h2>任务记录</h2><p v-if="!tasks.length">尚无安装任务。</p>
                    <article v-for="task in tasks" :key="task.task_id" class="task">
                        <div><strong>{{ task.application_id }} · {{ task.action }}</strong><Tag :value="state(task.status)" severity="secondary" /><p>{{ task.message }}</p><small>编号 {{ task.task_id }} · 上次查询 {{ date(task.queried_at) }}</small></div>
                        <div class="actions"><Button label="查询状态" severity="secondary" :disabled="busy" @click="taskAction(task, 'refresh')" /><Button label="读取日志" severity="secondary" :disabled="busy" @click="taskAction(task, 'fetch-logs')" /><Button v-if="task.status === 'outcome_unknown'" label="按原编号重发" severity="secondary" :disabled="busy" @click="taskAction(task, 'resubmit')" /></div>
                    </article>
                </section>
            </div>
        </div>
        <Dialog v-model:visible="adding" modal header="添加部署设备" :style="{ width: '36rem', maxWidth: '95vw' }">
            <form class="add-form" @submit.prevent="add"><label for="agent-alias">设备名称</label><InputText id="agent-alias" v-model="form.alias" required />
                <label for="agent-endpoint">服务端能够访问的 Agent HTTPS 地址</label><InputText id="agent-endpoint" v-model="form.endpoint" required placeholder="https://192.168.1.20:7014" />
                <label for="agent-token">设备访问凭证</label><Textarea id="agent-token" v-model="form.token" required rows="2" class="secret-input" autocomplete="off" spellcheck="false" />
                <label for="agent-ca">信任证书 PEM</label><Textarea id="agent-ca" v-model="form.ca_pem" rows="5" placeholder="粘贴 Agent 的 agent.crt；使用系统信任的证书时可留空" />
                <p>凭证保存在服务端。保存后点击连接并验证，才会访问设备。</p><Button type="submit" label="保存设备" :loading="busy" />
            </form>
        </Dialog>
        <Dialog :visible="!!viewingLog" modal header="任务日志（已读取内容）" :style="{ width: '65rem', maxWidth: '95vw' }" @update:visible="viewingLog = undefined">
            <pre class="task-log">{{ viewingLog?.logs || '尚无日志输出。' }}</pre><Button label="读取后续日志" :disabled="busy" @click="viewingLog && taskAction(viewingLog, 'fetch-logs')" />
        </Dialog>
    </section>
</template>

<style scoped>
.secret-input{-webkit-text-security:disc}
.deploy-page{max-width:1500px;margin:0 auto;padding:1.5rem;display:grid;gap:1rem}.deploy-header,.panel-heading{display:flex;justify-content:space-between;align-items:center;gap:1rem;flex-wrap:wrap}.deploy-header h1{margin:0;font-size:1.7rem}.deploy-header p,.muted{color:var(--p-text-muted-color)}.actions{display:flex;gap:.6rem;flex-wrap:wrap}.deploy-layout{display:grid;grid-template-columns:250px minmax(0,1fr);gap:1.3rem}.device-selector,.device-content{display:grid;align-content:start;gap:1rem}.device-selector button,.app-list button{background:var(--p-content-background);border:1px solid var(--p-content-border-color);border-radius:.65rem;text-align:left;padding:.9rem;color:inherit;cursor:pointer}.device-selector button{display:grid;gap:.5rem;overflow-wrap:anywhere}.device-selector button.active,.app-list button.active{border-color:var(--p-primary-color);box-shadow:inset 3px 0 var(--p-primary-color)}button:disabled{opacity:.55;cursor:default}.deploy-panel{border:1px solid var(--p-content-border-color);background:var(--p-content-background);border-radius:.8rem;padding:1.25rem;min-width:0}.deploy-panel h2{margin:0;font-size:1.2rem}.deploy-panel h3{font-size:1rem}.providers{display:grid;gap:.4rem;font-size:.9rem}.providers small{color:var(--p-text-muted-color)}.table-scroll{overflow:auto}table{border-collapse:collapse;width:100%;font-size:.9rem;text-align:left}th,td{padding:.7rem;border-bottom:1px solid var(--p-content-border-color);overflow-wrap:anywhere}.app-list{display:flex;flex-wrap:wrap;gap:.6rem}.app-list small{display:block}.app-form,.add-form{display:grid;gap:.8rem}.app-form{margin-top:1rem}.fields{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:1rem}.field{display:grid;gap:.4rem;font-size:.85rem}.field label{overflow-wrap:anywhere}.field .p-inputtext{width:100%}.task{padding:1rem 0;border-bottom:1px solid var(--p-content-border-color);display:grid;gap:.8rem}.task .p-tag{margin-left:.7rem}.task small{overflow-wrap:anywhere}.task-log{white-space:pre-wrap;overflow-wrap:anywhere;max-height:60vh;overflow:auto;font-size:.85rem}select{padding:.6rem;color:inherit;background:var(--p-content-background);border:1px solid var(--p-content-border-color);border-radius:.4rem}@media(max-width:800px){.deploy-page{padding:.75rem}.deploy-layout{grid-template-columns:1fr}.device-selector{grid-template-columns:repeat(auto-fit,minmax(170px,1fr))}.fields{grid-template-columns:1fr}}
</style>
