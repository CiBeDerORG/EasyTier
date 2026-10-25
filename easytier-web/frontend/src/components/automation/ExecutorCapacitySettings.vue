<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Button, InputNumber, Message, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationExecutorCapacity } from '../../types/automation';

const props = defineProps<{ enabled: boolean; connectionRevision: number }>();
const api = new AutomationApi();
const { locale } = useI18n();
const zh = computed(() => locale.value === 'cn' || locale.value.startsWith('zh'));
const t = (cn: string, en: string) => zh.value ? cn : en;
const record = ref<AutomationExecutorCapacity | null>(null);
const draft = ref<Record<'max_running' | 'max_pending' | 'max_media_sessions' | 'max_media_starting', number | null>>({
    max_running: 3, max_pending: 30, max_media_sessions: 10, max_media_starting: 2,
});
const fields = [
    { key: 'max_running', cn: '同时执行手机数', en: 'Concurrent phone jobs', recommended: 5 },
    { key: 'max_pending', cn: '同时待处理任务数', en: 'Pending runs', recommended: 30 },
    { key: 'max_media_sessions', cn: '同时投屏会话数', en: 'Live screen sessions', recommended: 10 },
    { key: 'max_media_starting', cn: '同时启动投屏数', en: 'Concurrent screen startups', recommended: 2 },
] as const;
const busy = ref<'load' | 'save' | null>(null);
const error = ref('');
const notice = ref('');
let generation = 0;
const changed = computed(() => !!record.value && fields.some(f => draft.value[f.key] !== record.value?.[f.key]));
const valid = computed(() => fields.every(f => Number.isSafeInteger(draft.value[f.key]) && (draft.value[f.key] ?? 0) > 0));
const excessive = computed(() => fields.some(f => (draft.value[f.key] ?? 0) > f.recommended));
function accept(value: AutomationExecutorCapacity) {
    record.value = value;
    for (const field of fields) draft.value[field.key] = value[field.key];
}
async function load() {
    if (!props.enabled || busy.value) return;
    const scope = ++generation;
    busy.value = 'load'; error.value = ''; notice.value = '';
    try {
        const result = await api.capacitySettings();
        if (scope === generation) accept(result);
    } catch (cause) {
        if (scope === generation) error.value = automationErrorMessage(cause);
    } finally {
        if (scope === generation) busy.value = null;
    }
}
async function save() {
    if (!record.value || !valid.value || busy.value || !changed.value) return;
    const scope = ++generation;
    busy.value = 'save'; error.value = ''; notice.value = '';
    try {
        const result = await api.saveCapacitySettings({
            expected_executor_id: record.value.executor_id,
            revision: record.value.revision,
            max_running: draft.value.max_running!,
            max_pending: draft.value.max_pending!,
            max_media_sessions: draft.value.max_media_sessions!,
            max_media_starting: draft.value.max_media_starting!,
        });
        if (scope === generation) {
            accept(result);
            notice.value = t('容量配置已保存，新的调度限额立即生效；正在执行的任务不会被强行中断。',
                'Capacity saved and applied. Existing tasks will not be forcibly interrupted.');
        }
    } catch (cause) {
        if (scope === generation) error.value = automationErrorMessage(cause);
    } finally {
        if (scope === generation) busy.value = null;
    }
}
watch(() => [props.enabled, props.connectionRevision] as const, ([enabled]) => {
    ++generation; busy.value = null; record.value = null; notice.value = ''; error.value = '';
    if (enabled) void load();
}, { immediate: true });
</script>

<template>
    <section class="console-panel capacity-settings">
        <div class="capacity-heading">
            <h2>{{ t('执行器容量', 'Executor capacity') }}</h2>
            <Tag v-if="record" :value="record.source === 'database' ? t('已保存配置', 'Saved settings') : t('环境变量默认值', 'Environment defaults')" severity="secondary" />
        </div>
        <p class="capacity-hint">{{ t('这些设置由 EasyTier 统一管理，保存后直接下发到执行器并持久保存，无需编辑 .env 或重启容器。下面的数字仅为推荐值，不是强制上限。', 'Manage and persist executor limits here; saving applies without editing environment files or restarting. Recommendations are not hard caps.') }}</p>
        <p v-if="!enabled" class="capacity-hint">{{ t('请先保存执行器服务器地址和 Token。', 'Configure the executor URL and token first.') }}</p>
        <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
        <Message v-if="notice" severity="success" :closable="false">{{ notice }}</Message>
        <fieldset :disabled="!enabled || !record || !!busy">
            <label v-for="field in fields" :key="field.key" class="capacity-field">
                <span>{{ t(field.cn, field.en) }}</span>
                <InputNumber v-model="draft[field.key]" :min="1" :use-grouping="false"
                    :min-fraction-digits="0" :max-fraction-digits="0" input-class="capacity-number"
                    :aria-label="t(field.cn, field.en)" />
                <small>{{ t('建议 ≤ ', 'Recommended ≤ ') }}{{ field.recommended }}{{ t('，可填写更大数值', '; higher values allowed') }}</small>
            </label>
        </fieldset>
        <Message v-if="excessive" severity="warn" :closable="false">{{ t('部分参数超过建议值，可能增加 CPU、内存或 ADB 负载；系统不强制限制，建议根据 FnOS 实际资源逐步调整。', 'Some values exceed recommendations and may increase CPU, memory or ADB usage. Increase gradually based on available resources.') }}</Message>
        <p v-if="record" class="capacity-hint">{{ t('当前执行器：', 'Executor: ') }}{{ record.executor_id }}</p>
        <div class="capacity-actions">
            <Button :label="t('保存容量设置', 'Save capacity')" icon="pi pi-save" size="small"
                :disabled="!enabled || !record || !valid || !changed || !!busy" :loading="busy === 'save'" @click="save" />
            <Button :label="t('重新读取', 'Reload')" icon="pi pi-refresh" outlined size="small"
                :disabled="!enabled || !!busy" :loading="busy === 'load'" @click="load" />
        </div>
    </section>
</template>

<style scoped>
.capacity-settings{display:flex;flex-direction:column;gap:14px;min-width:0;padding:20px}
.capacity-heading{display:flex;justify-content:space-between;gap:8px;align-items:center;flex-wrap:wrap}
.capacity-heading h2{font-size:16px;font-weight:600}
.capacity-hint{font-size:12px;line-height:1.7;color:var(--console-muted);overflow-wrap:anywhere}
fieldset{display:grid;gap:13px;border:0;padding:0;min-width:0}
.capacity-field{display:grid;gap:7px;font-size:13px}
.capacity-field span{font-weight:550}
.capacity-field small{font-size:11px;color:var(--console-muted)}
.capacity-field :deep(.p-inputnumber){width:100%}
.capacity-field :deep(.capacity-number){width:100%;font-size:13px}
.capacity-actions{display:flex;flex-wrap:wrap;gap:8px}
</style>