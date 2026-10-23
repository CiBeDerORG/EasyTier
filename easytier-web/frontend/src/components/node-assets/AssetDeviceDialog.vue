<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import { Button, Checkbox, Dialog, InputText, Select, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useAssetBindingLabels } from '../../composables/useAssetBindingLabels';
import type ApiClient from '../../modules/api';
import type { AddressSpec, ApplyAddressInput, ApplyBindingInput, AssetDevice, AssetDeviceCreateInput,
    AssetDeviceUpdateInput, AssetRole, AssetScope, AssetWarning, DeviceType, GatewayGroup,
    InstanceCandidate, InstancePreview } from '../../types/node-assets';
import { formatAssetError, getAssetError, isUncertainMutationError, useAssetMutation } from '../../composables/useAssetMutation';
import InstanceBindingSelector from './InstanceBindingSelector.vue';
import GatewayBindingEditor from './GatewayBindingEditor.vue';
import AssetAddressFields from './AssetAddressFields.vue';

type EditMode = 'metadata' | 'binding' | 'address';
const props = defineProps<{
    visible: boolean; api: ApiClient; scope: AssetScope; groups: GatewayGroup[]; types: DeviceType[];
    device?: AssetDevice | null; initialMode?: EditMode;
}>();
const emit = defineEmits<{
    close: []; saved: [event: { resource_kind: 'device'; record: AssetDevice } | { resource_kind: 'gateway'; record: GatewayGroup }];
    'context-error': [cause: unknown];
}>();
const { t } = useI18n();
const { bindingReason } = useAssetBindingLabels();
const role = ref<AssetRole>('ordinary');
const mode = ref<EditMode>('metadata');
const gatewayBusy = ref(false);
const candidate = ref<InstanceCandidate | null>(null);
const addressSpec = ref<AddressSpec | null>(null);
const addressPreview = ref<InstancePreview | null>(null);
const bindingPreview = ref<InstancePreview | null>(null);
const form = reactive({
    name: '', model: '', overlay_ip: '', lan_ip: '', remarks: '',
    gateway_group_id: null as string | null, device_type_id: null as string | null, is_gateway_self: false,
});
const bindingLan = ref('');
const saving = ref(false);
const previewing = ref(false);
const saveError = ref('');
const fieldErrors = ref<Record<string, string>>({});
const legacyWarnings = ref<AssetWarning[]>([]);
const uncertainMetadata = ref<AssetDeviceUpdateInput | null>(null);
const createAttempt = useAssetMutation<AssetDeviceCreateInput, AssetDevice>();
const bindingAttempt = useAssetMutation<Omit<ApplyBindingInput, 'request_id'>, AssetDevice>();
const addressAttempt = useAssetMutation<Omit<ApplyAddressInput, 'request_id'>, AssetDevice>();
const uncertain = computed(() => createAttempt.uncertain.value || bindingAttempt.uncertain.value
    || addressAttempt.uncertain.value || uncertainMetadata.value !== null);
const locked = computed(() => saving.value || uncertain.value);
const editing = computed(() => !!props.device);
const legacy = computed(() => props.device?.address_mode === 'legacy_manual');
const selectedGroup = computed(() => props.groups.find(group => group.id === form.gateway_group_id) ?? null);
const groupOptions = computed(() => props.groups.map(group => ({ ...group,
    label: `${group.name} · ${group.id.slice(0, 8)}${group.binding.snapshot?.display_name ? ` · ${group.binding.snapshot.display_name}` : ''}`,
})));
const typeOptions = computed(() => props.types.map(type => ({ ...type, label: `${type.name} · ${type.id.slice(0, 8)}` })));
const selectedGroupName = computed(() => groupOptions.value.find(group => group.id === form.gateway_group_id)?.label
    ?? form.gateway_group_id ?? t('assets.deviceDirectNode'));
const showMetadata = computed(() => !editing.value || mode.value === 'metadata');
const showAddresses = computed(() => !editing.value || mode.value === 'address');
const bindingAllowed = computed(() => props.device && !props.device.gateway_group_id
    && (legacy.value ? props.device.binding.can_attach : props.device.binding.can_refresh));
const addressAllowed = computed(() => !props.device || props.device.binding.can_edit_address);
const bindingOperation = computed(() => legacy.value ? 'attach' as const : 'refresh' as const);
const bindingChangedGeneration = computed(() => {
    const before = bindingPreview.value?.before?.snapshot?.config_ref;
    const after = bindingPreview.value?.after?.snapshot?.config_ref;
    return !!before && !!after && (before.id !== after.id || before.create_time !== after.create_time);
});
let generation = 0;
let previewGeneration = 0;
let disposed = false;
const contextKey = computed(() => JSON.stringify([props.visible, props.scope, props.device?.id, props.initialMode]));
const nullable = (value: string) => value.trim() || null;
function invalidatePreview() {
    previewGeneration += 1;
    previewing.value = false;
    bindingPreview.value = null;
    addressPreview.value = null;
    legacyWarnings.value = [];
}
function defaultChildSpec(): AddressSpec | null {
    if (!form.gateway_group_id) return null;
    if (form.is_gateway_self) return { mode: 'gateway_self', gateway_group_id: form.gateway_group_id, rule_key: null, host_offset: null };
    return { mode: 'gateway_rule', gateway_group_id: form.gateway_group_id,
        rule_key: selectedGroup.value?.binding.snapshot?.rules[0]?.rule_key ?? '', host_offset: 0 };
}
function resetAddressDraft() {
    invalidatePreview();
    if (props.device?.address_spec) {
        addressSpec.value = JSON.parse(JSON.stringify(props.device.address_spec)) as AddressSpec;
    } else if (form.gateway_group_id) {
        addressSpec.value = defaultChildSpec();
    } else if (candidate.value) {
        addressSpec.value = { mode: 'direct_instance', instance_id: candidate.value.instance_id, lan_ip: null };
    } else addressSpec.value = null;
}
function reset() {
    generation += 1;
    invalidatePreview();
    const source = props.device;
    Object.assign(form, { name: source?.name ?? '', model: source?.model ?? '', overlay_ip: source?.overlay_ip ?? '',
        lan_ip: source?.lan_ip ?? '', remarks: source?.remarks ?? '', gateway_group_id: source?.gateway_group_id ?? null,
        device_type_id: source?.device_type_id ?? null, is_gateway_self: source?.is_gateway_self ?? false });
    role.value = 'ordinary'; gatewayBusy.value = false;
    mode.value = source?.gateway_group_id && props.initialMode === 'binding' ? 'address' : props.initialMode ?? 'metadata';
    // A legacy direct record must first use binding preview, including its existing LAN value.
    if (source?.address_mode === 'legacy_manual' && !source.gateway_group_id && mode.value === 'address') mode.value = 'binding';
    candidate.value = null;
    bindingLan.value = source?.lan_ip ?? '';
    saving.value = false; saveError.value = ''; fieldErrors.value = {};
    uncertainMetadata.value = null;
    createAttempt.reset(); bindingAttempt.reset(); addressAttempt.reset();
    resetAddressDraft();
    legacyWarnings.value = source?.warnings ?? [];
}
watch(contextKey, reset, { immediate: true });
watch(candidate, () => {
    invalidatePreview();
    if (!editing.value && !form.gateway_group_id) {
        addressSpec.value = candidate.value ? { mode: 'direct_instance', instance_id: candidate.value.instance_id, lan_ip: null } : null;
    }
}, { flush: 'sync' });
watch(bindingLan, () => { bindingPreview.value = null; previewGeneration += 1; previewing.value = false; }, { flush: 'sync' });
function changeRole(value: AssetRole) {
    if (locked.value || gatewayBusy.value || editing.value || role.value === value) return;
    role.value = value; candidate.value = null; form.gateway_group_id = null; form.is_gateway_self = false;
    addressSpec.value = null; invalidatePreview(); saveError.value = ''; fieldErrors.value = {};
}
function groupChanged() {
    candidate.value = null; form.is_gateway_self = false;
    addressSpec.value = defaultChildSpec(); invalidatePreview();
}
function selfChanged() {
    if (!editing.value || mode.value === 'address') { addressSpec.value = defaultChildSpec(); invalidatePreview(); }
}
function setAddressSpec(value: AddressSpec | null) { addressSpec.value = value; addressPreview.value = null; }
function switchMode(value: EditMode) {
    if (locked.value) return;
    mode.value = value;
    saveError.value = ''; fieldErrors.value = {};
    candidate.value = null; bindingLan.value = props.device?.lan_ip ?? '';
    // Discard uncommitted metadata self changes before selecting a derived address mode.
    form.is_gateway_self = props.device?.is_gateway_self ?? false;
    resetAddressDraft();
}
function close() {
    generation += 1; invalidatePreview();
    createAttempt.reset(); bindingAttempt.reset(); addressAttempt.reset();
    emit('close');
}
function report(cause: unknown) {
    const detail = getAssetError(cause);
    fieldErrors.value = detail.field_errors;
    saveError.value = formatAssetError(cause, t('assets.binding.applyFailed'), key => t(key));
    if ([401, 403, 404].includes(detail.status ?? 0)) emit('context-error', cause);
}
const bindingInput = () => ({ subject: { resource_kind: 'device' as const, id: props.device?.id ?? null },
    operation: bindingOperation.value, instance_id: candidate.value?.instance_id ?? '', lan_ip: nullable(bindingLan.value) });
async function previewBinding() {
    if (locked.value || !candidate.value || !bindingAllowed.value) return;
    const session = generation;
    const request = ++previewGeneration;
    const input = bindingInput();
    const signature = JSON.stringify(input);
    const current = () => !disposed && props.visible && session === generation && request === previewGeneration && signature === JSON.stringify(bindingInput());
    previewing.value = true; bindingPreview.value = null; saveError.value = ''; fieldErrors.value = {};
    try {
        const result = await props.api.previewAssetBinding({ ...props.scope }, input);
        if (current()) { bindingPreview.value = result; fieldErrors.value = result.field_errors; }
    } catch (cause) { if (current()) report(cause); }
    finally { if (current()) previewing.value = false; }
}
async function previewLegacy() {
    if (!props.visible || locked.value || !legacy.value) return;
    const session = generation;
    const request = ++previewGeneration;
    const input = { device_id: props.device!.id, gateway_group_id: form.gateway_group_id,
        overlay_ip: form.overlay_ip.trim(), lan_ip: nullable(form.lan_ip) };
    const signature = JSON.stringify(input);
    const current = () => !disposed && props.visible && session === generation && request === previewGeneration
        && signature === JSON.stringify({ device_id: props.device!.id, gateway_group_id: form.gateway_group_id,
            overlay_ip: form.overlay_ip.trim(), lan_ip: nullable(form.lan_ip) });
    previewing.value = true; saveError.value = ''; fieldErrors.value = {};
    try {
        const result = await props.api.assetsPreviewDeviceAddress({ ...props.scope }, input);
        if (current()) { fieldErrors.value = result.field_errors; legacyWarnings.value = result.warnings; }
    } catch (cause) { if (current()) report(cause); }
    finally { if (current()) previewing.value = false; }
}
function metadata(): AssetDeviceUpdateInput {
    return { name: form.name.trim(), model: nullable(form.model), remarks: nullable(form.remarks), device_type_id: form.device_type_id,
        ...(legacy.value ? { overlay_ip: form.overlay_ip.trim(), lan_ip: nullable(form.lan_ip),
            gateway_group_id: props.device?.gateway_group_id ?? null, is_gateway_self: !!form.gateway_group_id && form.is_gateway_self } : {}) };
}
function validateMetadata(input: AssetDeviceUpdateInput) {
    const errors: Record<string, string> = {};
    if (!input.name) errors.name = t('assets.deviceNameRequired');
    if (legacy.value && !input.overlay_ip) errors.overlay_ip = t('assets.deviceOverlayRequired');
    for (const [field, limit] of [['name', 128], ['model', 128], ['remarks', 2000]] as const) {
        if (Array.from(input[field] ?? '').length > limit) errors[field] = t('assets.deviceTextTooLong', { limit });
    }
    return errors;
}
async function save() {
    if (saving.value || !props.visible || role.value !== 'ordinary') return;
    saveError.value = ''; fieldErrors.value = {};
    const metadataInput = uncertainMetadata.value ?? metadata();
    if (showMetadata.value) {
        fieldErrors.value = validateMetadata(metadataInput);
        if (Object.keys(fieldErrors.value).length) return;
    }
    const preview = mode.value === 'binding' && editing.value ? bindingPreview.value : addressPreview.value;
    if ((!editing.value || mode.value !== 'metadata') && !uncertain.value && (!preview?.can_save || !preview.preview_version)) {
        saveError.value = t('assets.binding.previewRequired'); return;
    }
    if (editing.value && mode.value === 'binding' && !bindingAllowed.value && !uncertain.value) return;
    if (editing.value && mode.value === 'address' && !addressAllowed.value && !uncertain.value) return;
    const scope = { ...props.scope };
    const deviceId = props.device?.id;
    const activeMode = mode.value;
    const session = generation;
    const key = `${scope.owner_user_id}:${scope.machine_id}:device:${deviceId ?? 'new'}:${activeMode}`;
    const token = preview?.preview_version ?? '';
    const spec = addressSpec.value;
    const instanceId = candidate.value?.instance_id ?? '';
    const current = () => !disposed && props.visible && session === generation;
    saving.value = true;
    try {
        let result: AssetDevice;
        if (!deviceId) {
            if (createAttempt.uncertain.value) result = await createAttempt.retry(request => props.api.assetsCreateDevice(scope, request), key);
            else {
                if (!spec) { saveError.value = t('assets.binding.previewRequired'); return; }
                result = await createAttempt.run({ name: metadataInput.name, model: metadataInput.model,
                    remarks: metadataInput.remarks, device_type_id: metadataInput.device_type_id,
                    address_spec: spec, preview_version: token }, request => props.api.assetsCreateDevice(scope, request), key);
            }
        } else if (activeMode === 'binding') {
            const subject = { resource_kind: 'device' as const, id: deviceId };
            result = bindingAttempt.uncertain.value
                ? await bindingAttempt.retry(request => props.api.applyAssetBinding(scope, subject, request), key)
                : await bindingAttempt.run({ operation: bindingOperation.value, instance_id: instanceId,
                    lan_ip: nullable(bindingLan.value), preview_version: token }, request => props.api.applyAssetBinding(scope, subject, request), key);
        } else if (activeMode === 'address') {
            if (addressAttempt.uncertain.value) result = await addressAttempt.retry(request => props.api.updateAssetAddress(scope, deviceId, request), key);
            else {
                if (!spec) { saveError.value = t('assets.binding.previewRequired'); return; }
                result = await addressAttempt.run({ address_spec: spec, preview_version: token },
                    request => props.api.updateAssetAddress(scope, deviceId, request), key);
            }
        } else result = await props.api.assetsUpdateDevice(scope, deviceId, metadataInput);
        if (current()) { uncertainMetadata.value = null; emit('saved', { resource_kind: 'device', record: result }); }
    } catch (cause) {
        if (!current()) return;
        if (deviceId && activeMode === 'metadata') uncertainMetadata.value = isUncertainMutationError(cause) ? metadataInput : null;
        report(cause);
    } finally { if (current()) saving.value = false; }
}
onBeforeUnmount(() => { disposed = true; generation += 1; previewGeneration += 1;
    createAttempt.reset(); bindingAttempt.reset(); addressAttempt.reset(); });
</script>

<template>
    <Dialog :visible="visible" modal :header="t(editing ? 'assets.deviceEdit' : 'assets.deviceCreate')"
        :style="{ width: '46rem', maxWidth: 'calc(100vw - 2rem)' }" :dismissableMask="false"
        @update:visible="value => { if (!value) close(); }">
        <p class="device-context">{{ t('assets.deviceContext', { owner: scope.owner_user_id, node: scope.machine_id }) }}</p>
        <div v-if="!editing" class="role-picker">
            <strong>{{ t('assets.binding.role') }}</strong>
            <div class="mode-actions">
                <Button :label="t('assets.binding.ordinary')" :outlined="role !== 'ordinary'" :disabled="locked || gatewayBusy" @click="changeRole('ordinary')" />
                <Button :label="t('assets.binding.gateway')" :outlined="role !== 'gateway'" :disabled="locked || gatewayBusy" @click="changeRole('gateway')" />
            </div>
            <small class="device-help">{{ t('assets.binding.roleHint') }}</small>
        </div>
        <GatewayBindingEditor v-if="visible && role === 'gateway'" :api="api" :scope="scope"
            @cancel="close" @saved="emit('saved', $event)" @context-error="emit('context-error', $event)" @busy="gatewayBusy = $event" />
        <template v-else>
            <div v-if="device" class="mode-actions">
                <Button :label="t('assets.binding.metadata')" :outlined="mode !== 'metadata'" size="small" :disabled="locked" @click="switchMode('metadata')" />
                <Button v-if="!device.gateway_group_id" :label="t(legacy ? 'assets.binding.attach' : 'assets.binding.review')"
                    :outlined="mode !== 'binding'" size="small" :disabled="locked" @click="switchMode('binding')" />
                <Button v-if="device.gateway_group_id || !legacy" :label="t('assets.binding.address')"
                    :outlined="mode !== 'address'" size="small" :disabled="locked" @click="switchMode('address')" />
            </div>
            <p v-if="device" class="device-help">{{ t(`assets.binding.state.${device.binding.state}`) }}<span v-if="device.binding.reason"> · {{ bindingReason(device.binding.reason) }}</span></p>
            <p v-if="legacy" class="address-warning">{{ t('assets.binding.legacyHint') }}</p>
            <form id="asset-device-form" class="device-form" novalidate @submit.prevent="save">
                <p v-if="editing" class="device-help full-width">{{ t('assets.binding.fixedHint') }}</p>
                <div v-if="device && mode !== 'metadata'" class="readonly-group full-width">
                    <strong>{{ t('assets.binding.previous') }}</strong>
                    <span>{{ t('assets.deviceOverlayIp') }}: <code>{{ device.overlay_ip }}</code></span>
                    <span>{{ t('assets.deviceLanIp') }}: <code>{{ device.lan_ip || '—' }}</code></span>
                </div>
                <template v-if="showMetadata">
                    <div class="device-field full-width">
                        <label for="asset-device-name">{{ t('assets.deviceName') }} *</label>
                        <InputText id="asset-device-name" v-model="form.name" autocomplete="off" :disabled="locked"
                            aria-required="true" :aria-invalid="!!fieldErrors.name" />
                    </div>
                    <div class="device-field">
                        <label for="asset-device-model">{{ t('assets.deviceModel') }}</label>
                        <InputText id="asset-device-model" v-model="form.model" autocomplete="off" :disabled="locked" />
                    </div>
                    <div class="device-field">
                        <label for="asset-device-type">{{ t('assets.deviceType') }}</label>
                        <Select inputId="asset-device-type" v-model="form.device_type_id" :options="typeOptions" optionLabel="label" optionValue="id"
                            :placeholder="t('assets.deviceUnclassified')" showClear filter :disabled="locked" />
                    </div>
                </template>
                <div class="device-field full-width">
                    <label for="asset-device-group">{{ t('assets.deviceGroup') }}</label>
                    <div v-if="editing" id="asset-device-group" class="readonly-group">{{ selectedGroupName }}</div>
                    <Select v-else inputId="asset-device-group" v-model="form.gateway_group_id" :options="groupOptions" optionLabel="label" optionValue="id"
                        :placeholder="t('assets.deviceDirectNode')" showClear filter :disabled="locked" @change="groupChanged" />
                    <small class="device-help">{{ t('assets.deviceGroupHelp') }}</small>
                </div>
                <div v-if="form.gateway_group_id" class="device-field full-width">
                    <div class="self-choice">
                        <Checkbox inputId="asset-device-self" v-model="form.is_gateway_self" binary
                            :disabled="locked || (editing && (mode !== 'metadata' || !legacy))" @update:modelValue="selfChanged" />
                        <label for="asset-device-self">{{ t('assets.deviceGatewaySelf') }}</label>
                    </div>
                    <small class="device-help">{{ t('assets.deviceGatewaySelfHelp') }}</small>
                </div>
                <template v-if="editing && mode === 'metadata'">
                    <div class="device-field">
                        <label for="asset-device-overlay">{{ t('assets.deviceOverlayIp') }} *</label>
                        <InputText id="asset-device-overlay" v-model="form.overlay_ip" :disabled="locked || !legacy" autocomplete="off" spellcheck="false" @update:modelValue="invalidatePreview" />
                    </div>
                    <div class="device-field">
                        <label for="asset-device-lan">{{ t('assets.deviceLanIp') }}</label>
                        <InputText id="asset-device-lan" v-model="form.lan_ip" :disabled="locked || !legacy" autocomplete="off" spellcheck="false" @update:modelValue="invalidatePreview" />
                    </div>
                    <div class="full-width">
                        <Button v-if="legacy" :label="t('assets.deviceCheckAddresses')" text icon="pi pi-check-circle" size="small"
                            :loading="previewing" :disabled="locked || previewing" @click="previewLegacy" />
                        <p v-else class="device-help">{{ t('assets.binding.readonlyAddresses') }}</p>
                        <p v-for="(warning, index) in legacyWarnings" :key="index" class="address-warning">{{ warning.message }}</p>
                    </div>
                </template>
                <template v-if="visible && ((!editing && !form.gateway_group_id) || (editing && mode === 'binding'))">
                    <InstanceBindingSelector v-model="candidate" class="full-width" :api="api" :scope="scope" role="ordinary"
                        :subject="{ resource_kind: 'device', id: device?.id ?? null }" :fixed-instance-id="device?.binding.instance_id"
                        :disabled="locked" @invalidated="invalidatePreview" @context-error="emit('context-error', $event)" />
                </template>
                <AssetAddressFields v-if="visible && showAddresses" :key="`${form.gateway_group_id || 'direct'}:${mode}`" class="full-width"
                    :api="api" :scope="scope" :subject="{ resource_kind: 'device', id: device?.id ?? null }" :gateway="selectedGroup"
                    :address-spec="addressSpec" :disabled="locked || !addressAllowed" @update:address-spec="setAddressSpec"
                    @preview-change="addressPreview = $event" @context-error="emit('context-error', $event)" />
                <template v-if="editing && mode === 'binding'">
                    <div class="device-field full-width">
                        <label for="asset-binding-lan">{{ t('assets.deviceLanIp') }}</label>
                        <InputText id="asset-binding-lan" v-model="bindingLan" :disabled="locked" autocomplete="off" spellcheck="false" />
                        <small class="device-help">{{ t('assets.binding.newIpv4Only') }}</small>
                    </div>
                    <Button class="full-width" :label="t('assets.binding.preview')" icon="pi pi-search" severity="secondary"
                        :loading="previewing" :disabled="locked || !candidate || !bindingAllowed || previewing" @click="previewBinding" />
                    <p v-if="bindingChangedGeneration" class="address-warning full-width">{{ t('assets.binding.generationChanged') }}</p>
                    <div v-if="bindingPreview" class="diff full-width">
                        <div v-if="bindingPreview.before" class="readonly-group">
                            <strong>{{ t('assets.binding.previous') }}</strong>
                            <small v-if="bindingPreview.before.snapshot" class="device-help">{{ t('assets.binding.configIdentity') }}: #{{ bindingPreview.before.snapshot.config_ref.id }} · {{ bindingPreview.before.snapshot.config_ref.create_time }}</small>
                            <span>{{ t('assets.deviceOverlayIp') }}: {{ bindingPreview.before.overlay_ip || '—' }}</span>
                            <span>{{ t('assets.deviceLanIp') }}: {{ bindingPreview.before.lan_ip || '—' }}</span>
                        </div>
                        <div v-if="bindingPreview.addresses" class="readonly-group">
                            <strong>{{ t('assets.binding.proposed') }}</strong>
                            <small v-if="bindingPreview.snapshot" class="device-help">{{ t('assets.binding.configIdentity') }}: #{{ bindingPreview.snapshot.config_ref.id }} · {{ bindingPreview.snapshot.config_ref.create_time }}</small>
                            <span>{{ t('assets.deviceOverlayIp') }}: {{ bindingPreview.addresses.overlay_ip }}</span>
                            <span>{{ t('assets.deviceLanIp') }}: {{ bindingPreview.addresses.lan_ip || '—' }}</span>
                        </div>
                    </div>
                    <p v-for="(warning, index) in bindingPreview?.warnings ?? []" :key="index" class="address-warning full-width">{{ warning.message }}</p>
                </template>
                <div v-if="showMetadata" class="device-field full-width">
                    <label for="asset-device-remarks">{{ t('assets.deviceRemarks') }}</label>
                    <Textarea id="asset-device-remarks" v-model="form.remarks" :rows="3" :disabled="locked" />
                </div>
                <p v-for="(message, field) in fieldErrors" :key="field" class="field-error full-width" role="alert">{{ message }}</p>
                <p v-if="uncertain" class="address-warning full-width" role="status">{{ t('assets.deviceSaveUncertain') }}</p>
                <p v-if="saveError" class="field-error full-width" role="alert">{{ saveError }}</p>
            </form>
        </template>
        <template v-if="role === 'ordinary'" #footer>
            <Button :label="t('assets.deviceCancel')" severity="secondary" @click="close" />
            <Button :label="t(uncertain ? 'assets.binding.retry' : editing && mode === 'metadata' ? 'assets.deviceSave' : 'assets.binding.apply')"
                icon="pi pi-check" type="submit" form="asset-device-form" :loading="saving" :disabled="saving" />
        </template>
    </Dialog>
</template>

<style scoped>
.device-form { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:.75rem 1rem; }
.full-width { grid-column:1/-1; }
.device-context { margin:0 0 1rem; overflow-wrap:anywhere; color:var(--p-text-muted-color,#64748b); font-size:.8125rem; }
.role-picker { display:grid; gap:.6rem; margin-bottom:1rem; }
.mode-actions { display:flex; gap:.5rem; flex-wrap:wrap; margin-bottom:.5rem; }
.device-field { display:flex; flex-direction:column; gap:.35rem; min-width:0; }
.device-field label { font-weight:500; }
.device-help { margin:0; color:var(--p-text-muted-color,#64748b); font-size:.8125rem; }
.field-error { margin:0; font-size:.8125rem; color:var(--p-red-600,#dc2626); overflow-wrap:anywhere; }
.readonly-group { display:grid; gap:.4rem; padding:.625rem .75rem; background:var(--p-content-background); border:1px solid var(--p-content-border-color,#e2e8f0); border-radius:.5rem; overflow-wrap:anywhere; }
.self-choice { display:flex; gap:.5rem; align-items:center; }
.address-warning { margin:.5rem 0; color:var(--p-orange-700,#9a3412); font-size:.8125rem; overflow-wrap:anywhere; }
.diff { display:grid; grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); gap:.5rem; }
@media(max-width:560px) { .device-form { grid-template-columns:minmax(0,1fr); } }
</style>
