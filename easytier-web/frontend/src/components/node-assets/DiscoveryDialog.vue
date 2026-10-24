<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { Button, Dialog, InputText, Message, Textarea } from 'primevue';
import { v4 as uuid } from 'uuid';
import type { AssetDevice } from '../../types/node-assets';
import { DiscoveryApi, type DiscoveryAccess, type DiscoveryChoice, type DiscoveryOperation, type DiscoveryRow } from '../../modules/discovery-api';
const props = defineProps<{ gateway: AssetDevice; ownerId: number; initialAction?: 'access' | 'devices' | 'adb' }>();
const emit = defineEmits<{ close: []; changed: [] }>();
const api = new DiscoveryApi();
const settings = ref<DiscoveryAccess>();
const accessScheme = computed(() => {
    try { return new URL(settings.value?.endpoint_url ?? '').protocol; } catch { return ''; }
});
const history = ref<{id:string;kind:string;status:string;created_at:string}[]>([]);
const selectedOperation = ref('');
const statusLabel = (state?: string) => ({pending:'待处理',confirmed:'已确认',connected:'已连接',applied:'已应用',failed:'失败',saved:'已保存',skipped:'已跳过',superseded:'已被新目标替代',not_applicable:'无需处理',choose_endpoint:'请选择端口',pending_device:'待处理设备'}[state ?? ''] ?? state ?? '—');
async function loadHistory() {const result = await api.request<{items:typeof history.value}>(props.ownerId,props.gateway.id,'/operations');if(active()) history.value=result.items; }
const secretAction = ref<'preserve' | 'replace' | 'clear'>('preserve');
const secret = ref('');
const showAccess = ref(props.initialAction === 'access');
const operation = ref<DiscoveryOperation>();
const parentOperation = ref<DiscoveryOperation>();
const choices = reactive<Record<string, DiscoveryChoice>>({});
const endpoints = reactive<Record<string, string>>({});
const busy = ref(false), error = ref(''), notice = ref('');
const rawResult = ref<unknown>();
const pending = ref<{ path: string; body: unknown }>();
let disposed = false;
let stopEvents: (() => void) | undefined;
const ctx = `${props.ownerId}:${props.gateway.id}`;
const active = () => !disposed && ctx === `${props.ownerId}:${props.gateway.id}`;
const summary = computed(() => operation.value?.snapshot);
const value = (input: unknown) => input === null || input === undefined || input === '' ? '未知' : typeof input === 'string' ? input : JSON.stringify(input);
function adopt(next: DiscoveryOperation) {
    operation.value = next; selectedOperation.value = next.operation_id;
    for (const row of next.rows ?? []) choices[row.row_id] = { row_id: row.row_id, action: row.applied ? 'skip' : row.suggested_action, target_device_id: row.matched[0]?.id, keep_service_ids: [] };
}
async function guarded(work: () => Promise<void>) {
    if (busy.value) return; busy.value = true; error.value = ''; notice.value = '';
    try { await work(); } catch (cause) { if (active()) error.value = cause instanceof Error ? cause.message : '请求失败'; }
    finally { if (active()) busy.value = false; }
}
async function loadAccess() {
    const result = await api.request<DiscoveryAccess>(props.ownerId, props.gateway.id, '/access');
    if (active()) settings.value = result;
}
async function save() {
    await guarded(async () => {
        if (!settings.value) return;
        const body = { endpoint_url: settings.value.endpoint_url, ca_pem: accessScheme.value === 'http:' ? '' : settings.value.ca_pem.trim(), expected_revision: settings.value.config_revision, secret_action: secretAction.value, ...(secretAction.value === 'replace' ? { secret: secret.value } : {}) };
        const result = await api.request<DiscoveryAccess>(props.ownerId, props.gateway.id, '/access', 'PUT', body);
        if (!active()) return; settings.value = result; secret.value = ''; secretAction.value = 'preserve'; notice.value = '接入设置已保存。';
    });
}
async function post(path: string, body: unknown) {
    pending.value = { path, body };
    const result = await api.request<DiscoveryOperation>(props.ownerId, props.gateway.id, path, 'POST', body);
    if (!active()) return;
    pending.value = undefined;
    if (result.preview_id && result.kind) adopt(result); else rawResult.value = result;
    emit('changed');
}
async function scan(kind: 'devices' | 'adb' | 'check') {
    await guarded(async () => { parentOperation.value = undefined; rawResult.value = undefined;
        await post(kind === 'check' ? '/access/check' : `/${kind}/scan`, { request_id: uuid() });
    });
}
async function reload(id = operation.value?.operation_id) {
    if (!id) return;
    const result = await api.request<DiscoveryOperation>(props.ownerId, props.gateway.id, `/operations/${id}`);
    if (active()) adopt(result);
}
async function apply() {
    await guarded(async () => {
        const op = operation.value; if (!op) return;
        await post('/devices/apply', { request_id: uuid(), preview_id: op.preview_id, preview_revision: op.preview_revision, rows: op.rows.filter(row => !row.applied).map(row => {
            const choice = choices[row.row_id]; return { ...choice, target_device_id: ['update', 'replace'].includes(choice.action) ? choice.target_device_id : undefined, keep_service_ids: choice.action === 'replace' ? choice.keep_service_ids : [] };
        }) });
        if (!active()) return;
        await reload(parentOperation.value?.operation_id ?? op.operation_id); parentOperation.value = undefined;
    });
}
function target(row: DiscoveryRow) { return row.candidates.find(c => c.id === choices[row.row_id]?.target_device_id); }
async function supplement() {
    const id = operation.value?.result.supplement_preview_id; if (!id) return;
    parentOperation.value = operation.value; await guarded(() => reload(id));
}
async function resolve() {
    await guarded(async () => {
        const op = operation.value; if (!op) return;
        await post(`/operations/${op.operation_id}/resolve`, { request_id: uuid(), expected_operation_revision: op.revision, choices: Object.entries(endpoints).filter(([,v]) => !!v).map(([row_id, selected_endpoint_id]) => ({ row_id, selected_endpoint_id })) });
        await reload(op.operation_id);
    });
}
async function retryStage(event_id: string, stage: string) {
    await guarded(async () => {
        const op = operation.value; if (!op) return;
        await post(`/operations/${op.operation_id}/retry`, { request_id: uuid(), expected_operation_revision: op.revision, event_id, stage });
        await reload(op.operation_id);
    });
}
onMounted(async () => {
    await guarded(async () => { await loadAccess(); await loadHistory(); });
    if (!active()) return;
    stopEvents = api.subscribe(props.ownerId, event => { const saved = operation.value?.events.find(e => e.event_id === event.event_id); if (saved) Object.assign(saved, event); }, () => { if (active()) notice.value = '结果通知已断开，可点击“读取已存结果”。'; });
    if (props.initialAction && props.initialAction !== 'access') await scan(props.initialAction);
});
onBeforeUnmount(() => { disposed = true; secret.value = ''; stopEvents?.(); });
</script>
<template>
    <Dialog visible modal :header="`OpenWrt 发现 · ${gateway.name}`" :style="{ width: '1100px', maxWidth: '96vw' }" @update:visible="emit('close')">
        <div class="discovery-panel">
            <p class="context">{{ gateway.overlay_ip }} · 当前网关组 {{ gateway.gateway_group_id?.slice(0, 8) }} · 仅处理点击触发的操作</p>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <Message v-if="notice" severity="info" :closable="false">{{ notice }}</Message>
            <div class="actions">
                <Button label="接入设置" icon="pi pi-cog" outlined :disabled="busy" @click="showAccess = !showAccess" />
                <Button label="立即同步" icon="pi pi-search" :disabled="busy" @click="scan('devices')" />
                <Button label="ADB 发现" icon="pi pi-mobile" :disabled="busy" @click="scan('adb')" />
                <Button v-if="operation" label="读取已存结果" icon="pi pi-refresh" text :disabled="busy" @click="guarded(() => reload())" />
                <Button v-if="pending" label="查询 / 重试原请求" severity="warn" :disabled="busy" @click="guarded(() => post(pending!.path, pending!.body))" />
            </div>
            <fieldset v-if="showAccess && settings" :disabled="busy" class="access-grid"><legend>网关接入</legend>
                <label>HTTP / HTTPS /ubus 地址<InputText v-model="settings.endpoint_url" placeholder="http://10.10.10.3:80/ubus" /></label>
                <label>账号<InputText value="easyadb" readonly /></label>
                <small v-if="accessScheme === 'http:'" class="wide">HTTP 无需证书。请填写路由器实际 HTTP 端口（通常为 80）；443 通常用于 HTTPS。</small>
                <label>授权密码<select v-model="secretAction" aria-label="授权密码"><option value="preserve">保留{{ settings.has_secret ? '已保存密码' : '（尚未保存）' }}</option><option value="replace">替换密码</option><option value="clear">清除密码</option></select></label>
                <label v-if="secretAction === 'replace'">新密码（保留原始字符）<InputText v-model="secret" type="password" autocomplete="new-password" /><small>填写 OpenWrt 发现插件生成的 easyadb 授权密钥。</small></label>
                <label v-if="accessScheme === 'https:'" class="wide">私有 CA 证书（PEM，可选）<Textarea v-model="settings.ca_pem" rows="3" /><small>填写完整的 BEGIN CERTIFICATE 证书内容；留空使用默认证书库。授权密钥填写在密码栏。</small></label>
                <div class="actions wide"><Button label="保存接入设置" @click="save" /><Button label="检查已保存接入" outlined @click="scan('check')" /></div>
            </fieldset>
            <div class="actions"><label>最近操作<select v-model="selectedOperation" aria-label="最近操作" :disabled="busy" @change="guarded(() => reload(selectedOperation))"><option value="">选择已保存的操作</option><option v-for="entry in history" :key="entry.id" :value="entry.id">{{ new Date(entry.created_at).toLocaleString() }} · {{ entry.kind }} · {{ entry.id.slice(0, 8) }}</option></select></label><Button label="读取操作记录" text :disabled="busy" @click="guarded(loadHistory)" /></div>
            <template v-if="operation">
                <div class="operation-info"><code>{{ operation.operation_id }}</code><span>{{ operation.kind }} · {{ operation.status }}</span><span>快照有效至 {{ new Date(operation.expires_at).toLocaleTimeString() }}</span></div>
                <Message v-if="summary" :severity="summary.status === 'ok' ? 'info' : 'warn'" :closable="false">来源状态：{{ summary.status }}{{ summary.truncated ? ' · 结果已截断' : '' }} · 扫描时间：{{ summary.scanned_at ? new Date(summary.scanned_at * 1000).toLocaleString() : '未知' }}<div v-if="summary.errors?.length">{{ value(summary.errors) }}</div></Message>
                <Message v-if="operation.result.message || operation.result.code" severity="warn" :closable="false">{{ operation.result.message || operation.result.code }}</Message>
                <dl v-if="operation.result.capabilities"><template v-for="(allowed, name) in operation.result.capabilities" :key="name"><dt>{{ name }}</dt><dd>{{ allowed ? '已授权' : '未授权' }}</dd></template></dl>
                <div v-if="operation.kind === 'devices'" class="review">
                    <p>更新保留设备和服务；覆盖会创建新设备。历史观察不表示设备当前在线，空字段会保留已有资料。</p>
                    <div class="table-scroll"><table><thead><tr><th>发现资料 / 差异</th><th>处理方式</th><th>当前组内目标</th><th>覆盖时保留服务</th></tr></thead><tbody>
                        <tr v-for="row in operation.rows" :key="row.row_id">
                            <td><strong>{{ row.ipv4 || '无唯一 IPv4' }}</strong><div>名称 {{ value(row.input.hostname || row.input.name) }}</div><div>MAC {{ value(row.input.mac) }}</div><div>观察 {{ value(row.input.observation) }} · IPv6 {{ value(row.input.ipv6) }}</div><div>最后观察 {{ value(row.input.last_observed_at || row.input.last_seen) }}</div><div>组网地址 {{ row.addresses?.overlay_ip || '待确认规则' }}</div><small v-if="row.blocked_reason" class="warning">{{ row.blocked_reason }}</small><small v-if="target(row)">原名称 {{ target(row)?.name }} · 原 MAC {{ value(target(row)?.discovery?.mac) }}</small></td>
                            <td><span v-if="row.applied">{{ statusLabel(row.applied.state) }}</span><select v-else v-model="choices[row.row_id].action" :disabled="busy || !!row.blocked_reason"><option value="create">新增</option><option value="update">更新</option><option value="replace">覆盖</option><option value="skip">跳过</option></select></td>
                            <td><select v-if="['update', 'replace'].includes(choices[row.row_id].action)" v-model="choices[row.row_id].target_device_id" :disabled="busy || !!row.applied" @change="choices[row.row_id].keep_service_ids = []"><option :value="undefined">选择目标</option><option v-for="c in row.candidates" :key="c.id" :value="c.id">{{ c.name }} · {{ c.lan_ip || '无 IP' }} · {{ c.id.slice(0, 8) }}</option></select></td>
                            <td><template v-if="choices[row.row_id].action === 'replace'"><Message severity="warn" :closable="false">旧设备将被删除，未勾选服务及其公开关系将移除。</Message><label v-for="s in target(row)?.services ?? []" :key="s.id" class="keep"><input v-model="choices[row.row_id].keep_service_ids" type="checkbox" :value="s.id" :disabled="busy || !!row.applied">{{ s.name }} :{{ s.port }}</label></template></td>
                        </tr>
                    </tbody></table></div>
                    <p v-if="!operation.rows.length">没有可处理的普通设备记录。</p>
                    <Button label="应用已选处理方式" :loading="busy" :disabled="busy || !operation.rows.some(r => !r.applied)" @click="apply" />
                </div>
                <div v-if="operation.kind === 'adb'" class="adb-results">
                    <Button v-if="operation.result.supplement_preview_id" label="处理本次缺失设备的补采预览" outlined :disabled="busy" @click="supplement" />
                    <article v-for="row in operation.result.rows ?? []" :key="row.row_id"><strong>{{ row.ip }}{{ row.port ? `:${row.port}` : '' }}</strong><span>{{ statusLabel(row.state) }} · {{ row.message || row.code }}</span><select v-if="row.endpoints" v-model="endpoints[row.row_id]" :disabled="busy"><option value="">选择一个连接端口</option><option v-for="e in row.endpoints" :key="e.endpoint_id" :value="e.endpoint_id">{{ e.port }}</option></select></article>
                    <Button v-if="operation.result.rows?.some(r => r.endpoints)" label="确认保存的连接端点" :disabled="busy" @click="resolve" />
                </div>
                <div class="event-list"><article v-for="event in operation.events" :key="event.event_id"><code>{{ event.device_id.slice(0, 8) }} · v{{ event.endpoint_revision }}</code><div class="actions"><span>公开映射：{{ statusLabel(event.publication?.state) }}</span><Button v-if="event.publication?.state !== 'applied'" label="重试公开应用" size="small" text :disabled="busy" @click="retryStage(event.event_id, 'publication')" /><span>旧目标清理：{{ statusLabel(event.cleanup.state) }}</span><Button v-if="event.cleanup.state !== 'confirmed'" label="重试清理" size="small" text :disabled="busy" @click="retryStage(event.event_id, 'cleanup')" /><span>新连接：{{ statusLabel(event.connection.state) }} {{ event.connection.code }}</span><Button v-if="!['connected', 'not_applicable', 'superseded'].includes(event.connection.state)" label="重试连接" size="small" text :disabled="busy" @click="retryStage(event.event_id, 'connection')" /></div></article></div>
            </template>
            <details v-if="rawResult"><summary>本次逐行结果</summary><pre>{{ JSON.stringify(rawResult, null, 2) }}</pre></details>
        </div>
        <template #footer><Button label="关闭" text @click="emit('close')" /></template>
    </Dialog>
</template>
<style scoped>
.discovery-panel,.review,.adb-results,.event-list{display:flex;flex-direction:column;gap:14px}.context,.operation-info,small{color:var(--console-muted);font-size:12px}.actions,.operation-info{display:flex;align-items:center;gap:12px;flex-wrap:wrap}.access-grid{display:grid;grid-template-columns:1fr 1fr;gap:16px;border:1px solid var(--console-border);border-radius:10px;padding:16px}.wide{grid-column:1/-1}label{display:flex;flex-direction:column;gap:7px;font-size:13px}select{padding:9px;border-radius:7px;color:var(--p-text-color);background:var(--console-panel);border:1px solid var(--console-border-strong);max-width:100%}.table-scroll{overflow-x:auto}table{width:100%;border-collapse:collapse;font-size:13px;min-width:760px}th,td{padding:12px;text-align:left;vertical-align:top;border-bottom:1px solid var(--console-border)}th{color:var(--console-muted);font-weight:500}td>div,td>small{display:block;margin-top:6px}td:first-child{min-width:240px}.keep{flex-direction:row;align-items:center;margin-top:8px}.warning{color:var(--p-orange-500)}article{padding:12px;border:1px solid var(--console-border);background:var(--console-panel-raised);border-radius:9px;display:flex;gap:12px;flex-wrap:wrap}pre{max-height:260px;overflow:auto;font-size:12px}dl{display:grid;grid-template-columns:1fr 1fr}@media(max-width:650px){.access-grid{grid-template-columns:1fr}}
</style>
