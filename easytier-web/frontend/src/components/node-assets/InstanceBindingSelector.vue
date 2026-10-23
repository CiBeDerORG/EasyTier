<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useAssetBindingLabels } from '../../composables/useAssetBindingLabels';
import type ApiClient from '../../modules/api';
import type { AssetRole, AssetScope, InstanceCandidate, Subject } from '../../types/node-assets';
import { formatAssetError, getAssetError } from '../../composables/useAssetMutation';

const props = defineProps<{
    api: ApiClient; scope: AssetScope; role: AssetRole; subject?: Subject;
    modelValue: InstanceCandidate | null; disabled?: boolean; fixedInstanceId?: string | null;
}>();
const emit = defineEmits<{
    'update:modelValue': [value: InstanceCandidate | null]; invalidated: [];
    'context-error': [cause: unknown];
}>();
const { t } = useI18n();
const { instanceSource, candidateReason } = useAssetBindingLabels();
const items = ref<InstanceCandidate[]>([]);
const loading = ref(false);
const error = ref('');
let generation = 0;
let disposed = false;
const scopeKey = computed(() => JSON.stringify([props.scope, props.role, props.subject, props.fixedInstanceId]));
const visibleItems = computed(() => props.fixedInstanceId
    ? items.value.filter(item => item.instance_id === props.fixedInstanceId) : items.value);
const hasSelectable = computed(() => visibleItems.value.some(item => item.selectable));
async function load() {
    if (props.disabled) return;
    const version = ++generation;
    const key = scopeKey.value;
    const selectedId = props.fixedInstanceId || props.modelValue?.instance_id;
    const current = () => !disposed && version === generation && key === scopeKey.value;
    loading.value = true;
    error.value = '';
    emit('invalidated');
    emit('update:modelValue', null);
    try {
        const response = await props.api.listAssetNetworkInstances({ ...props.scope }, {
            role: props.role,
            ...(props.subject?.id ? { subject_kind: props.subject.resource_kind, subject_id: props.subject.id } : {}),
        });
        if (!current()) return;
        items.value = response.items;
        const selected = response.items.find(item => item.instance_id === selectedId && item.selectable);
        if (selected) emit('update:modelValue', selected);
    } catch (cause) {
        if (!current()) return;
        error.value = formatAssetError(cause, t('assets.binding.requestFailed'), key => t(key));
        if ([401, 403, 404].includes(getAssetError(cause).status ?? 0)) emit('context-error', cause);
    } finally { if (current()) loading.value = false; }
}
function select(item: InstanceCandidate) {
    if (props.disabled || loading.value || !item.selectable) return;
    emit('invalidated');
    emit('update:modelValue', item);
}
watch(scopeKey, () => { items.value = []; void load(); }, { immediate: true });
onBeforeUnmount(() => { disposed = true; generation += 1; });
</script>

<template>
    <section class="instance-selector" :aria-label="t('assets.binding.instance')">
        <div class="heading"><strong>{{ t('assets.binding.instance') }} *</strong>
            <Button :label="t('assets.binding.refresh')" icon="pi pi-refresh" size="small" text
                :loading="loading" :disabled="disabled || loading" @click="load" />
        </div>
        <p v-if="loading" role="status" class="muted">{{ t('assets.binding.loading') }}</p>
        <p v-if="error" role="alert" class="error">{{ error }}</p>
        <p v-else-if="!loading && !visibleItems.length" class="muted">{{ t('assets.binding.empty') }}</p>
        <p v-else-if="!loading && !hasSelectable" class="warning">{{ t('assets.binding.noSelectable') }}</p>
        <div class="choices" role="radiogroup" :aria-label="t('assets.binding.instance')">
            <button v-for="item in visibleItems" :key="item.instance_id" type="button" role="radio"
                class="instance" :class="{ selected: modelValue?.instance_id === item.instance_id }"
                :aria-checked="modelValue?.instance_id === item.instance_id"
                :disabled="disabled || loading || !item.selectable" @click="select(item)">
                <span class="heading"><strong>{{ item.display_name || item.instance_id }}</strong><code>{{ item.instance_id.slice(0, 8) }}</code></span>
                <span class="muted">{{ t(item.online === null ? 'assets.binding.unknownOnline' : item.online ? 'assets.binding.online' : 'assets.binding.offline') }}
                    <span v-if="item.disabled"> · {{ t('assets.binding.disabled') }}</span> · {{ instanceSource(item.source) }}</span>
                <span>{{ t('assets.binding.instanceIp') }}: <code>{{ item.ipv4 || t('assets.binding.noValue') }}</code>
                    <span class="muted"> ({{ t(item.ipv4_source === 'missing' ? 'assets.binding.missingIp' : `assets.binding.${item.ipv4_source}`) }})</span></span>
                <span v-if="!item.rules.length" class="muted">{{ t('assets.binding.noRules') }}</span>
                <span v-for="rule in item.rules" :key="rule.rule_key" class="rule"><code>{{ rule.real_cidr }}<template v-if="rule.mapped_cidr"> → {{ rule.mapped_cidr }}</template></code>
                    <span class="muted">{{ t(rule.mapped_cidr ? 'assets.binding.mapping' : 'assets.binding.proxy') }}</span></span>
                <span v-if="item.bound_gateway" class="warning">{{ t('assets.binding.inUse') }}: {{ item.bound_gateway.name }} · {{ item.bound_gateway.id.slice(0, 8) }}</span>
                <span v-if="!item.selectable" class="warning">{{ candidateReason(item) }}</span>
            </button>
        </div>
    </section>
</template>

<style scoped>
.instance-selector,.choices,.instance { display:grid; gap:.55rem; min-width:0; }
.heading { display:flex; align-items:center; justify-content:space-between; gap:.5rem; flex-wrap:wrap; }
.instance { padding:.75rem; text-align:start; border:1px solid var(--p-content-border-color,#e2e8f0); border-radius:.5rem; background:var(--p-content-background,#fff); color:inherit; cursor:pointer; overflow-wrap:anywhere; }
.instance.selected { border-color:var(--p-primary-color,#3b82f6); box-shadow:0 0 0 1px var(--p-primary-color,#3b82f6); }
.instance:disabled { cursor:default; background:var(--p-content-hover-background,#f8fafc); }
.instance:focus-visible { outline:2px solid var(--p-primary-color,#3b82f6); outline-offset:2px; }
.rule { display:flex; flex-wrap:wrap; gap:.5rem; }
.muted { color:var(--p-text-muted-color,#64748b); font-size:.8125rem; }
.warning { color:var(--p-orange-700,#9a3412); font-size:.8125rem; }
.error { color:var(--p-red-600,#dc2626); } p { margin:0; }
</style>
