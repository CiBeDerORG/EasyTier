<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, InputText, Message, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AgentHostResult, AssetScope, ServerAgentEntry } from '../../types/node-assets';

const props = defineProps<{ api: ApiClient; scope: AssetScope; entry: ServerAgentEntry; bound: boolean }>();
const translations = {
    cn: {
        title: '主机状态与安装目录', refresh: '查询主机状态', current: '本次实时查询成功',
        offline: 'Agent 离线 / 查询失败', old: '仅显示上次成功的快照，不代表当前状态',
        notLoaded: '点击查询获取 CPU、内存、磁盘和 Docker 状态。',
        hostname: '主机', platform: '系统 / 架构', cpu: 'CPU 逻辑核心',
        memory: '内存占用', disks: '磁盘剩余', docker: 'Docker Engine', compose: 'Compose',
        available: '可用', unavailable: '不可用', root: 'SOFTWARE_ROOT', save: '保存目录',
        warning: '有受管应用时禁止直接修改目录。请先迁移数据并完成路径校验；保存不会自动搬移文件。',
        updated: '主机安装目录已更新。', failed: '请求失败，Agent 可能离线或授权失效。',
        checkedAt: '采集时间', noSnapshot: '尚无可用的主机快照。',
    },
    en: {
        title: 'Host status and installation root', refresh: 'Query host now', current: 'Live query succeeded',
        offline: 'Agent offline / query failed', old: 'Last successful snapshot only, not current status',
        notLoaded: 'Query CPU, memory, disks and Docker on demand.',
        hostname: 'Hostname', platform: 'OS / architecture', cpu: 'Logical CPU cores',
        memory: 'Memory used', disks: 'Disk available', docker: 'Docker Engine', compose: 'Compose',
        available: 'Available', unavailable: 'Unavailable', root: 'SOFTWARE_ROOT', save: 'Save directory',
        warning: 'Managed apps block direct changes. Migrate data and verify paths first; saving never moves files.',
        updated: 'Installation root updated.', failed: 'Request failed; Agent may be offline or authorization expired.',
        checkedAt: 'Observed at', noSnapshot: 'No cached host snapshot is available.',
    },
};
const { t } = useI18n({ useScope: 'local', messages: translations });
const result = ref<AgentHostResult | null>(null);
const softwareRoot = ref('');
const busy = ref(false);
const error = ref('');
const notice = ref('');
let alive = true;
let seq = 0;
const snapshot = computed(() => result.value?.snapshot ?? null);
const isFresh = computed(() => result.value?.fresh === true);
const canChange = computed(() => props.bound && !busy.value && isFresh.value
    && softwareRoot.value.trim().length > 1 && softwareRoot.value.trim() !== snapshot.value?.software_root);
function bytes(size: number): string {
    if (!Number.isFinite(size) || size < 0) return '—';
    return (size / 1073741824).toFixed(1) + ' GiB';
}
function describe(err: unknown): string {
    const cause = err as { response?: { data?: { message?: string } }; message?: string };
    return cause.response?.data?.message || cause.message || t('failed');
}
async function query() {
    if (!props.bound || busy.value) return;
    const current = ++seq;
    busy.value = true; error.value = ''; notice.value = '';
    try {
        const response = await props.api.assetsAgentHost(props.scope, props.entry.asset_device_id);
        if (!alive || current !== seq) return;
        result.value = response;
        softwareRoot.value = response.snapshot?.software_root ?? '';
    } catch (err) {
        if (alive && current === seq) {
            error.value = describe(err);
            result.value = { live_status: 'offline', fresh: false, snapshot: result.value?.snapshot ?? null };
        }
    } finally {
        if (alive && current === seq) busy.value = false;
    }
}
async function saveRoot() {
    if (!canChange.value) return;
    const current = ++seq;
    busy.value = true; error.value = ''; notice.value = '';
    try {
        const response = await props.api.assetsAgentSetRoot(props.scope, props.entry.asset_device_id, softwareRoot.value.trim());
        if (!alive || current !== seq) return;
        result.value = response;
        softwareRoot.value = response.snapshot?.software_root ?? '';
        notice.value = t('updated');
    } catch (err) {
        if (alive && current === seq) error.value = describe(err);
    } finally {
        if (alive && current === seq) busy.value = false;
    }
}
watch(() => props.bound, value => {
    if (value) void query();
    else { ++seq; result.value = null; softwareRoot.value = ''; error.value = ''; }
}, { immediate: true });
onBeforeUnmount(() => { alive = false; ++seq; });
</script>

<template>
    <section v-if="bound" class="agent-host">
        <div class="agent-host-heading">
            <strong>{{ t('title') }}</strong>
            <Button :label="t('refresh')" size="small" icon="pi pi-refresh" :loading="busy" :disabled="busy" @click="query" />
        </div>
        <template v-if="result">
            <Tag v-if="isFresh" severity="success" :value="t('current')" />
            <Message v-else severity="warn" :closable="false">{{ t('offline') }} — {{ t('old') }}</Message>
        </template>
        <p v-if="!result" class="muted">{{ t('notLoaded') }}</p>
        <dl v-if="snapshot" class="agent-host-data">
            <div><dt>{{ t('hostname') }}</dt><dd>{{ snapshot.hostname }}</dd></div>
            <div><dt>{{ t('platform') }}</dt><dd>{{ snapshot.platform }} / {{ snapshot.arch }}</dd></div>
            <div><dt>{{ t('cpu') }}</dt><dd>{{ snapshot.cpu.logical_cores }}</dd></div>
            <div><dt>{{ t('memory') }}</dt><dd>{{ bytes(snapshot.memory.used_bytes) }} / {{ bytes(snapshot.memory.total_bytes) }}</dd></div>
            <div><dt>{{ t('docker') }}</dt><dd>{{ snapshot.docker.available ? t('available') : t('unavailable') }}
                {{ snapshot.docker.engine_version || '' }}</dd></div>
            <div><dt>{{ t('compose') }}</dt><dd>{{ snapshot.docker.compose_available ? t('available') : t('unavailable') }}
                {{ snapshot.docker.compose_version || '' }}</dd></div>
            <div v-for="disk in snapshot.disks" :key="disk.mount">
                <dt>{{ t('disks') }} · {{ disk.mount }}</dt><dd>{{ bytes(disk.available_bytes) }} / {{ bytes(disk.total_bytes) }}</dd>
            </div>
            <div><dt>{{ t('checkedAt') }}</dt><dd>{{ snapshot.observed_at }}</dd></div>
        </dl>
        <p v-else-if="result" class="muted">{{ t('noSnapshot') }}</p>
        <template v-if="snapshot">
            <label for="host-software-root">{{ t('root') }}</label>
            <InputText id="host-software-root" v-model="softwareRoot" :disabled="busy || !isFresh"
                autocomplete="off" spellcheck="false" />
            <p class="muted">{{ t('warning') }}</p>
            <Button :label="t('save')" icon="pi pi-save" :disabled="!canChange" :loading="busy" @click="saveRoot" />
        </template>
        <Message v-if="notice" severity="success" :closable="false">{{ notice }}</Message>
        <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
    </section>
</template>
<style scoped>
.agent-host { display: grid; gap: .6rem; padding-top: .75rem; border-top: 1px solid var(--console-border); }
.agent-host-heading { display: flex; align-items: center; flex-wrap: wrap; gap: .75rem; justify-content: space-between; }
.agent-host-data { display: grid; gap: .35rem; margin: 0; }
.agent-host-data > div { display: grid; grid-template-columns: 9rem minmax(0, 1fr); gap: .6rem; }
.agent-host-data dt, .muted { color: var(--console-muted); }
.agent-host-data dd { margin: 0; overflow-wrap: anywhere; }
.agent-host :deep(.p-button) { justify-self: start; }
.agent-host :deep(.p-inputtext) { width: 100%; }
@media (max-width: 550px) { .agent-host-data > div { grid-template-columns: 1fr; gap: .15rem; } }
</style>
