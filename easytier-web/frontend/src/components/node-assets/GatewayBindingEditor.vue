<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, InputText, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useAssetBindingLabels } from '../../composables/useAssetBindingLabels';
import type ApiClient from '../../modules/api';
import type { ApplyBindingInput, AssetScope, AssetWarning, GatewayGroup, GatewayGroupCreateInput,
    GatewayGroupUpdateInput, InstanceCandidate, InstancePreview } from '../../types/node-assets';
import { formatAssetError, getAssetError, isUncertainMutationError, useAssetMutation } from '../../composables/useAssetMutation';
import InstanceBindingSelector from './InstanceBindingSelector.vue';

const props = defineProps<{
    api: ApiClient; scope: AssetScope; group?: GatewayGroup | null; initialMode?: 'metadata' | 'binding';
}>();
const emit = defineEmits<{
    cancel: []; saved: [event: { resource_kind: 'gateway'; record: GatewayGroup }]; 'context-error': [cause: unknown];
    busy: [value: boolean];
}>();
const { t } = useI18n();
const { bindingReason } = useAssetBindingLabels();
const name = ref('');
const remarks = ref('');
const subnets = ref<Array<{ key: number; value: string }>>([]);
const mode = ref<'metadata' | 'binding'>('metadata');
const candidate = ref<InstanceCandidate | null>(null);
const preview = ref<InstancePreview | null>(null);
const previewing = ref(false);
const saving = ref(false);
const error = ref('');
const fieldErrors = ref<Record<string, string>>({});
const warnings = ref<AssetWarning[]>([]);
const uncertainMetadata = ref<GatewayGroupUpdateInput | null>(null);
const createAttempt = useAssetMutation<GatewayGroupCreateInput, GatewayGroup>();
const bindingAttempt = useAssetMutation<Omit<ApplyBindingInput, 'request_id'>, GatewayGroup>();
let version = 0;
let previewVersion = 0;
let rowKey = 0;
let disposed = false;
const context = computed(() => JSON.stringify([props.scope, props.group?.id, props.initialMode]));
const legacy = computed(() => props.group?.binding.state === 'legacy_unbound');
const bindingAllowed = computed(() => !props.group || (legacy.value ? props.group.binding.can_attach : props.group.binding.can_refresh));
const uncertain = computed(() => createAttempt.uncertain.value || bindingAttempt.uncertain.value || !!uncertainMetadata.value);
const locked = computed(() => saving.value || uncertain.value);
watch(locked, value => emit('busy', value), { immediate: true });
const showBinding = computed(() => !props.group || mode.value === 'binding');
const operation = computed(() => props.group && !legacy.value ? 'refresh' as const : 'attach' as const);
const bindingInput = () => ({ subject: { resource_kind: 'gateway' as const, id: props.group?.id ?? null },
    operation: operation.value, instance_id: candidate.value?.instance_id ?? '' });
const diff = computed(() => preview.value ? [
    { label: 'assets.binding.previous', value: preview.value.before },
    { label: 'assets.binding.proposed', value: preview.value.after },
] : []);
const replaced = computed(() => {
    const old = preview.value?.before?.snapshot?.config_ref;
    const next = preview.value?.after?.snapshot?.config_ref;
    return !!old && !!next && (old.id !== next.id || old.create_time !== next.create_time);
});
function report(cause: unknown) {
    const detail = getAssetError(cause);
    fieldErrors.value = detail.field_errors;
    error.value = formatAssetError(cause, t('assets.binding.applyFailed'), key => t(key));
    if ([401, 403, 404].includes(detail.status ?? 0)) emit('context-error', cause);
}
function invalidate() {
    previewVersion += 1;
    preview.value = null;
    previewing.value = false;
    warnings.value = [];
}
watch(candidate, invalidate, { flush: 'sync' });
watch(context, () => {
    version += 1;
    invalidate();
    name.value = props.group?.name ?? '';
    remarks.value = props.group?.remarks ?? '';
    subnets.value = (props.group?.subnets.length ? props.group.subnets : ['']).map(value => ({ key: ++rowKey, value }));
    mode.value = props.initialMode ?? 'metadata';
    candidate.value = null;
    saving.value = false;
    error.value = '';
    fieldErrors.value = {};
    uncertainMetadata.value = null;
    createAttempt.reset();
    bindingAttempt.reset();
}, { immediate: true });
function switchMode(value: 'metadata' | 'binding') {
    if (locked.value) return;
    mode.value = value;
    error.value = '';
    fieldErrors.value = {};
    invalidate();
}
async function previewBinding() {
    if (locked.value || !candidate.value || !bindingAllowed.value) return;
    const currentVersion = version;
    const request = ++previewVersion;
    const input = bindingInput();
    const signature = JSON.stringify(input);
    const current = () => !disposed && currentVersion === version && request === previewVersion && signature === JSON.stringify(bindingInput());
    previewing.value = true;
    preview.value = null;
    error.value = '';
    fieldErrors.value = {};
    try {
        const result = await props.api.previewAssetBinding({ ...props.scope }, input);
        if (current()) { preview.value = result; fieldErrors.value = result.field_errors; warnings.value = result.warnings; }
    } catch (cause) { if (current()) report(cause); }
    finally { if (current()) previewing.value = false; }
}
async function previewLegacy() {
    if (locked.value || !props.group || !legacy.value) return;
    const currentVersion = version;
    const request = ++previewVersion;
    const input = { gateway_group_id: props.group.id, subnets: subnets.value.map(row => row.value.trim()) };
    const signature = JSON.stringify(input.subnets);
    const current = () => !disposed && currentVersion === version && request === previewVersion
        && signature === JSON.stringify(subnets.value.map(row => row.value.trim()));
    previewing.value = true;
    error.value = '';
    try {
        const result = await props.api.assetsPreviewSubnets({ ...props.scope }, input);
        if (current()) { fieldErrors.value = result.field_errors; warnings.value = result.warnings; }
    } catch (cause) { if (current()) report(cause); }
    finally { if (current()) previewing.value = false; }
}
async function save() {
    if (saving.value) return;
    error.value = '';
    fieldErrors.value = {};
    if (!name.value.trim() && (!props.group || mode.value === 'metadata')) {
        fieldErrors.value.name = t('assets.groups.name_required'); return;
    }
    const currentVersion = version;
    const scope = { ...props.scope };
    const group = props.group;
    const key = `${scope.owner_user_id}:${scope.machine_id}:gateway:${group?.id ?? 'new'}`;
    const current = () => !disposed && version === currentVersion;
    if (showBinding.value && !uncertain.value && (!bindingAllowed.value || !preview.value?.can_save || !preview.value.preview_version)) {
        error.value = t('assets.binding.previewRequired'); return;
    }
    // Freeze the preview token before disabling inputs. Retrying never creates a new preview.
    const token = preview.value?.preview_version ?? '';
    const instanceId = candidate.value?.instance_id ?? '';
    const metadata: GatewayGroupUpdateInput = uncertainMetadata.value ?? {
        name: name.value.trim(), remarks: remarks.value.trim() || null,
        ...(legacy.value ? { subnets: subnets.value.map(row => row.value.trim()) } : {}),
    };
    saving.value = true;
    try {
        let record: GatewayGroup;
        if (!group) {
            record = createAttempt.uncertain.value
                ? await createAttempt.retry(request => props.api.assetsCreateGroup(scope, request), key)
                : await createAttempt.run({ name: name.value.trim(), remarks: remarks.value.trim() || null,
                    binding: { instance_id: instanceId }, preview_version: token }, request => props.api.assetsCreateGroup(scope, request), key);
        } else if (mode.value === 'binding') {
            const subject = { resource_kind: 'gateway' as const, id: group.id };
            record = bindingAttempt.uncertain.value
                ? await bindingAttempt.retry(request => props.api.applyAssetBinding(scope, subject, request), key)
                : await bindingAttempt.run({ operation: operation.value, instance_id: instanceId, preview_version: token },
                    request => props.api.applyAssetBinding(scope, subject, request), key);
        } else {
            record = await props.api.assetsUpdateGroup(scope, group.id, metadata);
        }
        if (current()) { uncertainMetadata.value = null; emit('saved', { resource_kind: 'gateway', record }); }
    } catch (cause) {
        if (!current()) return;
        if (group && mode.value === 'metadata' && isUncertainMutationError(cause)) uncertainMetadata.value = metadata;
        else if (group && mode.value === 'metadata') uncertainMetadata.value = null;
        report(cause);
    } finally { if (current()) saving.value = false; }
}
onBeforeUnmount(() => { disposed = true; version += 1; invalidate(); createAttempt.reset(); bindingAttempt.reset(); });
</script>

<template>
    <section class="gateway-editor">
        <div v-if="group" class="mode-actions">
            <Button :label="t('assets.binding.metadata')" :outlined="mode !== 'metadata'" size="small" :disabled="locked" @click="switchMode('metadata')" />
            <Button :label="t(legacy ? 'assets.binding.attach' : 'assets.binding.review')" :outlined="mode !== 'binding'" size="small" :disabled="locked" @click="switchMode('binding')" />
        </div>
        <p v-if="group" class="muted">{{ t(`assets.binding.state.${group.binding.state}`) }}<span v-if="group.binding.reason"> · {{ bindingReason(group.binding.reason) }}</span></p>
        <p v-if="legacy" class="notice">{{ t('assets.binding.legacyHint') }}</p>
        <form class="editor-fields" @submit.prevent="save">
            <template v-if="!group || mode === 'metadata'">
                <label for="gateway-editor-name">{{ t('assets.groups.name') }} *</label>
                <InputText id="gateway-editor-name" v-model="name" :disabled="locked" autocomplete="off" :aria-invalid="!!fieldErrors.name" />
                <label for="gateway-editor-remarks">{{ t('assets.groups.remarks') }}</label>
                <Textarea id="gateway-editor-remarks" v-model="remarks" :disabled="locked" rows="3" autoResize />
                <fieldset v-if="group && legacy" class="subnets">
                    <legend>{{ t('assets.groups.subnets') }} *</legend>
                    <div v-for="(row, index) in subnets" :key="row.key" class="subnet-row">
                        <InputText v-model="row.value" :aria-label="t('assets.groups.subnet_number', { number: index + 1 })"
                            :disabled="locked" autocomplete="off" spellcheck="false" @update:modelValue="invalidate" />
                        <Button icon="pi pi-times" text :disabled="locked || subnets.length <= 1"
                            :aria-label="t('assets.groups.remove_subnet', { number: index + 1 })" @click="subnets.splice(index, 1); invalidate()" />
                    </div>
                    <div class="mode-actions">
                        <Button :label="t('assets.groups.add_subnet')" text size="small" :disabled="locked" @click="subnets.push({ key: ++rowKey, value: '' }); invalidate()" />
                        <Button :label="t('assets.groups.preview')" text size="small" :loading="previewing" :disabled="locked || previewing" @click="previewLegacy" />
                    </div>
                </fieldset>
                <div v-else-if="group" class="snapshot">
                    <p>{{ t('assets.binding.instanceIp') }}: <code>{{ group.overlay_ip || '—' }}</code></p>
                    <code v-for="rule in group.binding.snapshot?.rules ?? []" :key="rule.rule_key">{{ rule.real_cidr }}<template v-if="rule.mapped_cidr"> → {{ rule.mapped_cidr }}</template></code>
                </div>
            </template>
            <template v-if="showBinding">
                <p v-if="group" class="muted">{{ t('assets.binding.fixedHint') }}</p>
                <div v-if="group" class="snapshot">
                    <strong>{{ t('assets.binding.previous') }}</strong>
                    <span>{{ t('assets.binding.instanceIp') }}: <code>{{ group.overlay_ip || '—' }}</code></span>
                    <template v-if="group.binding.snapshot">
                        <code v-for="rule in group.binding.snapshot.rules" :key="rule.rule_key">{{ rule.real_cidr }}<template v-if="rule.mapped_cidr"> → {{ rule.mapped_cidr }}</template></code>
                    </template>
                    <template v-else><code v-for="subnet in group.subnets" :key="subnet">{{ subnet }}</code></template>
                </div>
                <InstanceBindingSelector v-model="candidate" :api="api" :scope="scope" role="gateway"
                    :subject="{ resource_kind: 'gateway', id: group?.id ?? null }" :fixed-instance-id="group?.binding.instance_id"
                    :disabled="locked" @invalidated="invalidate" @context-error="emit('context-error', $event)" />
                <Button :label="t('assets.binding.preview')" icon="pi pi-search" severity="secondary" :loading="previewing"
                    :disabled="locked || !candidate || !bindingAllowed || previewing" @click="previewBinding" />
                <p v-if="replaced" class="notice" role="status">{{ t('assets.binding.generationChanged') }}</p>
                <div v-if="preview" class="diff">
                    <div v-for="side in diff.filter(item => item.value)" :key="side.label" class="snapshot">
                        <strong>{{ t(side.label) }}</strong>
                        <span>{{ side.value?.snapshot?.display_name || (side.label === 'assets.binding.previous' ? group?.name : '') }} <code>{{ side.value?.snapshot?.instance_id }}</code></span>
                        <small v-if="side.value?.snapshot" class="muted">{{ t('assets.binding.configIdentity') }}: #{{ side.value.snapshot.config_ref.id }} · {{ side.value.snapshot.config_ref.create_time }}</small>
                        <span>{{ t('assets.binding.instanceIp') }}: <code>{{ side.value?.snapshot?.ipv4 || '—' }}</code></span>
                        <code v-for="rule in side.value?.snapshot?.rules ?? []" :key="rule.rule_key">{{ rule.real_cidr }}<template v-if="rule.mapped_cidr"> → {{ rule.mapped_cidr }}</template></code>
                        <template v-if="side.label === 'assets.binding.previous' && !side.value?.snapshot && legacy">
                            <code v-for="subnet in group?.subnets ?? []" :key="subnet">{{ subnet }}</code>
                        </template>
                    </div>
                </div>
                <p v-if="group" class="muted">{{ t('assets.binding.childrenHint') }}</p>
                <p v-if="preview?.affected_device_ids.length" class="notice">{{ t('assets.binding.changedChildren', { count: preview.affected_device_ids.length }) }}</p>
            </template>
            <p v-for="(message, field) in fieldErrors" :key="field" class="error" role="alert">{{ message }}</p>
            <p v-for="(warning, index) in warnings" :key="index" class="notice" role="status"><strong v-if="warning.device_name">{{ warning.device_name }} · </strong>{{ warning.message }} {{ warning.suggestion }}</p>
            <p v-if="uncertain" class="notice" role="status">{{ t('assets.groups.uncertain') }}</p>
            <p v-if="error" class="error" role="alert">{{ error }}</p>
            <div class="editor-actions">
                <Button :label="t('assets.groups.cancel')" severity="secondary" @click="emit('cancel')" />
                <Button :label="t(uncertain ? 'assets.binding.retry' : showBinding ? 'assets.binding.apply' : 'assets.groups.save')"
                    icon="pi pi-check" type="submit" :loading="saving" :disabled="saving" />
            </div>
        </form>
    </section>
</template>

<style scoped>
.gateway-editor,.editor-fields,.subnets,.snapshot { display:grid; gap:.65rem; min-width:0; }
.mode-actions,.editor-actions,.subnet-row { display:flex; gap:.5rem; align-items:center; flex-wrap:wrap; }
.editor-actions { justify-content:flex-end; margin-top:.5rem; }
.subnets { border:0; margin:0; padding:0; }
.subnet-row > input { min-width:0; flex:1; }
.diff { display:grid; grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); gap:.5rem; }
.snapshot { padding:.75rem; border:1px solid var(--p-content-border-color,#e2e8f0); border-radius:.5rem; overflow-wrap:anywhere; }
p { margin:0; overflow-wrap:anywhere; }
.muted { color:var(--p-text-muted-color,#64748b); font-size:.8125rem; }
.notice { color:var(--p-orange-700,#9a3412); font-size:.8125rem; }
.error { color:var(--p-red-600,#dc2626); font-size:.8125rem; }
</style>
