<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Checkbox, InputText, Message, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type {
    AgentDockerInventory, AgentDockerResource, AgentDockerSnapshot, AssetScope, ServerAgentEntry,
} from '../../types/node-assets';

const props = defineProps<{ api: ApiClient; scope: AssetScope; entry: ServerAgentEntry; bound: boolean }>();
const translations = {
    cn: {
        title: 'Docker 应用与容器', refresh: '查询 Docker', live: '本次查询成功',
        stale: '历史快照 · 非实时', offline: 'Agent 离线或 Docker 查询失败；不能操作历史资源',
        unloaded: '仅在打开服务器设备或手动点击查询时读取 Docker，没有后台轮询。',
        noSnapshot: '尚无成功的 Docker 清单。', empty: '本次完整扫描未发现容器。',
        observedAt: '采集时间', compose: 'Compose 项目', container: '独立容器',
        running: '运行中', stopped: '已停止', partial: '部分运行', unknown: '状态未知',
        external: '外部应用', pending: '待商店校验', changed: '容器成员已变化，需重新确认',
        managed: '受管应用', ports: '映射端口', image: '镜像', services: '成员容器',
        propose: '申请接管', cancel: '取消接管意向', candidate: 'Deploy 应用 ID',
        acknowledge: '我确认这是当前资源。仅登记接管意向，不会修改 Docker 或直接取得受管权限。',
        submit: '确认登记意向', close: '取消选择', cancelled: '已取消接管意向，不影响容器。',
        proposed: '已保存接管意向，等待可信商店模板核验及 Agent 持久登记。',
        warning: '在正式模板验证和受管登记完成前，外部资源仅供查看；此处不会执行启停或安装。',
        failed: 'Docker 请求失败', notReady: '读取到历史数据时禁止接管。',
    },
    en: {
        title: 'Docker resources', refresh: 'Query Docker', live: 'Live scan succeeded',
        stale: 'Historical snapshot · not live', offline: 'Agent offline or Docker scan failed; historical resources are read-only',
        unloaded: 'Docker is queried on opening this device or manual refresh, never polled.',
        noSnapshot: 'No successful Docker snapshot yet.', empty: 'No containers in the latest complete scan.',
        observedAt: 'Observed at', compose: 'Compose project', container: 'Standalone container',
        running: 'Running', stopped: 'Stopped', partial: 'Partly running', unknown: 'Unknown',
        external: 'External', pending: 'Awaiting catalog validation', changed: 'Members changed: confirm again',
        managed: 'Managed', ports: 'Published ports', image: 'Image', services: 'Containers',
        propose: 'Request adoption', cancel: 'Cancel intent', candidate: 'Deploy application ID',
        acknowledge: 'I confirm the live resource. This only records an intent and does not modify Docker or grant management rights.',
        submit: 'Confirm intent', close: 'Cancel selection', cancelled: 'Intent removed; Docker was not changed.',
        proposed: 'Intent saved; trusted catalog and Agent registration still required.',
        warning: 'External resources are read-only until trusted catalog validation and Agent registration. No start/stop/install actions are exposed.',
        failed: 'Docker request failed', notReady: 'Adoption is disabled for historical data.',
    },
};
const { t } = useI18n({ useScope: 'local', messages: translations });
const inventory = ref<AgentDockerInventory | null>(null);
const busy = ref(false);
const error = ref('');
const notice = ref('');
const selectedId = ref('');
const draftAppId = ref('');
const acknowledged = ref(false);
let alive = true;
let requestNumber = 0;

const fresh = computed(() => inventory.value?.fresh === true && inventory.value?.complete === true
    && inventory.value?.status === 'ok' && inventory.value?.live_status === 'online');
const shown = computed<AgentDockerSnapshot | null>(() => {
    if (!inventory.value) return null;
    if (fresh.value) return inventory.value;
    return inventory.value.snapshot ?? null;
});
const resources = computed(() => shown.value?.resources ?? []);
const canConfirm = computed(() => fresh.value && !busy.value && selectedId.value.length > 0
    && /^[a-z0-9][a-z0-9_.-]{1,99}$/.test(draftAppId.value.trim())
    && acknowledged.value);
const statusText = (state: string) => t(['running', 'stopped', 'partial'].includes(state) ? state : 'unknown');
const statusSeverity = (state: string) => state === 'running' ? 'success'
    : state === 'stopped' ? 'secondary' : 'warn';
function typeText(r: AgentDockerResource): string {
    return r.kind === 'compose_project' ? t('compose') : t('container');
}
function stateText(r: AgentDockerResource): string {
    if (r.managed) return t('managed');
    if (r.management_state === 'adoption_pending_catalog_validation') return t('pending');
    if (r.management_state === 'adoption_needs_revalidation') return t('changed');
    return t('external');
}
function portsText(r: AgentDockerResource): string {
    return r.ports?.map(p => `${p.host_ip || '0.0.0.0'}:${p.host_port} → ${p.container_port}/${p.protocol}`).join(', ') || '—';
}
function describe(e: unknown): string {
    const err = e as { response?: { data?: { message?: string } }; message?: string };
    return err.response?.data?.message || err.message || t('failed');
}
function clearDraft() {
    selectedId.value = '';
    draftAppId.value = '';
    acknowledged.value = false;
}
async function refresh() {
    if (!props.bound || busy.value) return;
    const current = ++requestNumber;
    busy.value = true; error.value = ''; notice.value = ''; clearDraft();
    try {
        const data = await props.api.assetsAgentDockerInventory(props.scope, props.entry.asset_device_id);
        if (!alive || current !== requestNumber) return;
        inventory.value = data;
    } catch (e) {
        if (!alive || current !== requestNumber) return;
        error.value = describe(e);
        const previous = inventory.value;
        inventory.value = {
            agent_id: previous?.agent_id ?? '', observed_at: previous?.observed_at ?? '',
            complete: false, status: 'error', resources: [], live_status: 'offline',
            fresh: false, snapshot: previous?.snapshot ?? (previous?.fresh ? previous : null),
        };
    } finally {
        if (alive && current === requestNumber) busy.value = false;
    }
}
function select(r: AgentDockerResource) {
    if (!fresh.value || busy.value || r.managed || r.management_state !== 'external') return;
    selectedId.value = r.resource_id;
    draftAppId.value = r.kind === 'compose_project' ? r.compose_project ?? '' : '';
    acknowledged.value = false;
    error.value = ''; notice.value = '';
}
async function submit() {
    if (!canConfirm.value) return;
    const r = resources.value.find(item => item.resource_id === selectedId.value);
    if (!r || r.managed || r.management_state !== 'external') return;
    const current = ++requestNumber;
    busy.value = true; error.value = ''; notice.value = '';
    try {
        await props.api.assetsAgentDockerAdoptionIntent(props.scope, props.entry.asset_device_id, {
            resource_id: r.resource_id, application_id: draftAppId.value.trim(),
            expected_container_ids: r.containers.map(c => c.container_id), confirm: true,
        });
        if (!alive || current !== requestNumber) return;
        notice.value = t('proposed');
        clearDraft();
        // Re-query to show the server-authoritative intent and current members.
        const data = await props.api.assetsAgentDockerInventory(props.scope, props.entry.asset_device_id);
        if (alive && current === requestNumber) inventory.value = data;
    } catch (e) {
        if (alive && current === requestNumber) { error.value = describe(e); clearDraft(); }
    } finally {
        if (alive && current === requestNumber) busy.value = false;
    }
}
async function cancelIntent(r: AgentDockerResource) {
    if (!fresh.value || busy.value) return;
    const current = ++requestNumber;
    busy.value = true; error.value = ''; notice.value = ''; clearDraft();
    try {
        await props.api.assetsAgentDockerCancelIntent(props.scope, props.entry.asset_device_id, r.resource_id);
        if (!alive || current !== requestNumber) return;
        notice.value = t('cancelled');
        const data = await props.api.assetsAgentDockerInventory(props.scope, props.entry.asset_device_id);
        if (alive && current === requestNumber) inventory.value = data;
    } catch (e) {
        if (alive && current === requestNumber) error.value = describe(e);
    } finally {
        if (alive && current === requestNumber) busy.value = false;
    }
}
watch(
    () => [props.bound, props.entry.asset_device_id, props.scope.owner_user_id, props.scope.machine_id] as const,
    ([bound]) => {
        ++requestNumber; inventory.value = null; busy.value = false;
        error.value = ''; notice.value = ''; clearDraft();
        if (bound) void refresh();
    },
    { immediate: true },
);
onBeforeUnmount(() => { alive = false; ++requestNumber; });
</script>

<template>
    <section v-if="bound" class="agent-docker">
        <div class="agent-docker-head">
            <strong>{{ t('title') }}</strong>
            <Button size="small" icon="pi pi-refresh" :label="t('refresh')" :loading="busy" :disabled="busy" @click="refresh" />
        </div>
        <Tag v-if="fresh" severity="success" :value="t('live')" />
        <Message v-else-if="inventory" severity="warn" :closable="false">{{ t('stale') }} — {{ t('offline') }}</Message>
        <p v-if="shown" class="muted">{{ t('observedAt') }}：{{ shown.observed_at }}</p>
        <p v-else class="muted">{{ inventory ? t('noSnapshot') : t('unloaded') }}</p>
        <p v-if="shown && resources.length === 0" class="muted">{{ t('empty') }}</p>
        <p class="muted">{{ t('warning') }}</p>
        <div v-for="r in resources" :key="r.resource_id" class="resource">
            <div class="resource-title">
                <div class="resource-name">
                    <strong>{{ r.name }}</strong>
                    <span class="muted">{{ typeText(r) }}</span>
                </div>
                <div class="resource-tags">
                    <Tag :severity="statusSeverity(r.state)" :value="statusText(r.state)" />
                    <Tag severity="info" :value="stateText(r)" />
                </div>
            </div>
            <small class="muted">{{ r.resource_id }}</small>
            <div class="muted">{{ t('ports') }}：{{ portsText(r) }}</div>
            <details>
                <summary>{{ t('services') }} ({{ r.containers.length }})</summary>
                <ul>
                    <li v-for="c in r.containers" :key="c.container_id">
                        <strong>{{ c.name }}</strong> · {{ statusText(c.state) }} · {{ t('image') }}: {{ c.image }}
                    </li>
                </ul>
            </details>
            <div v-if="fresh && !r.managed" class="resource-actions">
                <Button v-if="r.management_state === 'external'" size="small" outlined :label="t('propose')"
                    :disabled="busy" @click="select(r)" />
                <Button v-else-if="r.management_state === 'adoption_pending_catalog_validation'
                    || r.management_state === 'adoption_needs_revalidation'"
                    size="small" severity="secondary" outlined :label="t('cancel')" :disabled="busy"
                    @click="cancelIntent(r)" />
            </div>
            <div v-if="fresh && selectedId === r.resource_id" class="adopt-form">
                <label :for="'adopt-'+r.resource_id">{{ t('candidate') }}</label>
                <InputText :id="'adopt-'+r.resource_id" v-model="draftAppId" :disabled="busy"
                    autocomplete="off" spellcheck="false" />
                <div class="confirm-row">
                    <Checkbox :inputId="'adopt-confirm-'+r.resource_id" v-model="acknowledged" binary :disabled="busy" />
                    <label :for="'adopt-confirm-'+r.resource_id">{{ t('acknowledge') }}</label>
                </div>
                <div class="resource-actions">
                    <Button :label="t('submit')" size="small" :disabled="!canConfirm" :loading="busy" @click="submit" />
                    <Button :label="t('close')" size="small" severity="secondary" text @click="clearDraft" />
                </div>
            </div>
        </div>
        <Message v-if="notice" severity="success" :closable="false">{{ notice }}</Message>
        <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
    </section>
</template>

<style scoped>
.agent-docker { display: grid; gap: .7rem; padding-top: .9rem; border-top: 1px solid var(--console-border); }
.agent-docker-head { display:flex; justify-content:space-between; align-items:center; gap:.65rem; flex-wrap:wrap; }
.resource { display:grid; gap:.5rem; padding:.8rem; border:1px solid var(--console-border); border-radius:.6rem; min-width:0; }
.resource-title { display:flex; justify-content:space-between; flex-wrap:wrap; gap:.5rem; }
.resource-name,.resource-tags,.resource-actions { display:flex; flex-wrap:wrap; align-items:center; gap:.4rem; }
.resource-tags { justify-content:flex-end; }
.resource small { overflow-wrap:anywhere; }
.muted { color:var(--console-muted); overflow-wrap:anywhere; }
.resource details { min-width:0; }
.resource summary { cursor:pointer; }
.resource ul { margin:.5rem 0; padding-left:1.4rem; overflow-wrap:anywhere; }
.adopt-form { display:grid; gap:.55rem; padding:.65rem; border:1px dashed var(--console-border); border-radius:.5rem; }
.adopt-form :deep(.p-inputtext) { width:100%; }
.confirm-row { display:flex; align-items:flex-start; gap:.55rem; }
.confirm-row label { font-size:.85rem; line-height:1.45; }
</style>
