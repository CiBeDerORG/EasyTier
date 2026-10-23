<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import { Button, Checkbox, Dialog, InputText, Select, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AssetDevice, AssetDeviceInput, AssetScope, AssetWarning, DeviceType, GatewayGroup } from '../../types/node-assets';
import { formatAssetError, getAssetError, isUncertainMutationError, useAssetMutation } from '../../composables/useAssetMutation';

const props = defineProps<{
    visible: boolean;
    api: ApiClient;
    scope: AssetScope;
    groups: GatewayGroup[];
    types: DeviceType[];
    device?: AssetDevice | null;
}>();
const emit = defineEmits<{ close: []; saved: [device: AssetDevice] }>();
const { t } = useI18n();
const attempt = useAssetMutation<AssetDeviceInput, AssetDevice>();
const form = reactive({
    name: '', model: '', overlay_ip: '', lan_ip: '', remarks: '',
    gateway_group_id: null as string | null,
    device_type_id: null as string | null,
    is_gateway_self: false,
});
const saving = ref(false);
const previewing = ref(false);
const saveError = ref('');
const previewError = ref('');
const fieldErrors = ref<Record<string, string>>({});
const previewErrors = ref<Record<string, string>>({});
const warnings = ref<AssetWarning[]>([]);
const uncertainEditPayload = ref<AssetDeviceInput | null>(null);
const uncertain = computed(() => attempt.uncertain.value || uncertainEditPayload.value !== null);
const fieldsDisabled = computed(() => saving.value || uncertain.value);
const editing = computed(() => !!props.device);
const selectedGroupName = computed(() => form.gateway_group_id
    ? props.groups.find(group => group.id === form.gateway_group_id)?.name || form.gateway_group_id
    : t('assets.deviceDirectNode'));
let generation = 0;
let previewGeneration = 0;
let disposed = false;

const reset = () => {
    generation++;
    previewGeneration++;
    const source = props.device;
    Object.assign(form, {
        name: source?.name ?? '', model: source?.model ?? '',
        overlay_ip: source?.overlay_ip ?? '', lan_ip: source?.lan_ip ?? '',
        remarks: source?.remarks ?? '', gateway_group_id: source?.gateway_group_id ?? null,
        device_type_id: source?.device_type_id ?? null, is_gateway_self: source?.is_gateway_self ?? false,
    });
    saving.value = false;
    previewing.value = false;
    saveError.value = '';
    previewError.value = '';
    fieldErrors.value = {};
    previewErrors.value = {};
    warnings.value = source?.warnings ?? [];
    uncertainEditPayload.value = null;
    attempt.reset();
};
watch(() => [props.visible, props.scope.owner_user_id, props.scope.machine_id, props.device?.id], reset, { immediate: true });
onBeforeUnmount(() => { disposed = true; generation++; previewGeneration++; });

const close = () => {
    generation++;
    previewGeneration++;
    attempt.reset();
    uncertainEditPayload.value = null;
    emit('close');
};
const clearField = (field: string) => {
    delete fieldErrors.value[field];
    if (field === 'overlay_ip' || field === 'lan_ip' || field === 'gateway_group_id') {
        previewGeneration++;
        previewing.value = false;
        previewErrors.value = {};
        previewError.value = '';
        warnings.value = [];
    }
};
const errorFor = (field: string) => fieldErrors.value[field] || previewErrors.value[field] || '';
const nullable = (value: string) => value.trim() || null;
const payload = (): AssetDeviceInput => ({
    name: form.name.trim(), model: nullable(form.model), overlay_ip: form.overlay_ip.trim(),
    lan_ip: nullable(form.lan_ip), remarks: nullable(form.remarks),
    gateway_group_id: form.gateway_group_id, device_type_id: form.device_type_id,
    is_gateway_self: !!form.gateway_group_id && form.is_gateway_self,
});
const previewPayload = () => ({
    device_id: props.device?.id ?? null,
    gateway_group_id: form.gateway_group_id,
    overlay_ip: form.overlay_ip.trim(), lan_ip: nullable(form.lan_ip),
});
const previewSignature = () => JSON.stringify({ ...props.scope, ...previewPayload() });
const previewAddresses = async () => {
    if (!props.visible || fieldsDisabled.value) return;
    const requestGeneration = ++previewGeneration;
    const session = generation;
    const signature = previewSignature();
    const scope = { ...props.scope };
    const input = previewPayload();
    previewing.value = true;
    previewError.value = '';
    try {
        const result = await props.api.assetsPreviewDeviceAddress(scope, input);
        if (disposed || session !== generation || requestGeneration !== previewGeneration || signature !== previewSignature()) return;
        previewErrors.value = result.field_errors;
        warnings.value = result.warnings;
    } catch (error: unknown) {
        if (disposed || session !== generation || requestGeneration !== previewGeneration) return;
        previewError.value = formatAssetError(error, t('assets.devicePreviewFailed'), t);
    } finally {
        if (!disposed && session === generation && requestGeneration === previewGeneration) previewing.value = false;
    }
};
const groupChanged = () => {
    if (!form.gateway_group_id) form.is_gateway_self = false;
    clearField('gateway_group_id');
    if (form.overlay_ip.trim() || form.lan_ip.trim()) void previewAddresses();
};
const validate = (input: AssetDeviceInput) => {
    const errors: Record<string, string> = {};
    if (!input.name) errors.name = t('assets.deviceNameRequired');
    if (!input.overlay_ip) errors.overlay_ip = t('assets.deviceOverlayRequired');
    for (const [field, limit] of [['name', 128], ['model', 128], ['remarks', 2000]] as const) {
        const value = input[field] ?? '';
        if (Array.from(value).length > limit) errors[field] = t('assets.deviceTextTooLong', { limit });
    }
    return errors;
};

const save = async () => {
    if (saving.value || !props.visible) return;
    const input = uncertainEditPayload.value ?? payload();
    fieldErrors.value = validate(input);
    if (Object.keys(fieldErrors.value).length) return;
    const session = generation;
    const scope = { ...props.scope };
    const deviceId = props.device?.id;
    const api = props.api;
    saving.value = true;
    saveError.value = '';
    try {
        const result = deviceId
            ? await api.assetsUpdateDevice(scope, deviceId, input)
            : await attempt.run(input, request => api.assetsCreateDevice(scope, request), `${scope.owner_user_id}:${scope.machine_id}:device`);
        if (disposed || session !== generation || !props.visible) return;
        uncertainEditPayload.value = null;
        emit('saved', result);
    } catch (error: unknown) {
        if (disposed || session !== generation || !props.visible) return;
        const detail = getAssetError(error);
        fieldErrors.value = detail.field_errors;
        saveError.value = formatAssetError(error, t('assets.deviceSaveFailed'), t);
        if (deviceId) uncertainEditPayload.value = isUncertainMutationError(error) ? input : null;
    } finally {
        if (!disposed && session === generation) saving.value = false;
    }
};
</script>

<template>
    <Dialog :visible="visible" modal :header="t(editing ? 'assets.deviceEdit' : 'assets.deviceCreate')"
        :style="{ width: '42rem', maxWidth: 'calc(100vw - 2rem)' }" :dismissableMask="false"
        @update:visible="value => { if (!value) close(); }">
        <form id="asset-device-form" class="device-form" novalidate @submit.prevent="save">
            <p class="device-context">{{ t('assets.deviceContext', { owner: scope.owner_user_id, node: scope.machine_id }) }}</p>
            <p v-if="editing" class="device-help">{{ t('assets.deviceOwnershipFixed') }}</p>
            <div class="device-field full-width">
                <label for="asset-device-name">{{ t('assets.deviceName') }} <span aria-hidden="true">*</span></label>
                <InputText id="asset-device-name" v-model="form.name" autocomplete="off" :disabled="fieldsDisabled"
                    aria-required="true" :aria-invalid="!!errorFor('name')" aria-describedby="asset-device-name-error"
                    @update:modelValue="clearField('name')" />
                <small id="asset-device-name-error" class="field-error">{{ errorFor('name') }}</small>
            </div>
            <div class="device-field">
                <label for="asset-device-model">{{ t('assets.deviceModel') }}</label>
                <InputText id="asset-device-model" v-model="form.model" autocomplete="off" :disabled="fieldsDisabled"
                    :aria-invalid="!!errorFor('model')" aria-describedby="asset-device-model-error"
                    @update:modelValue="clearField('model')" />
                <small id="asset-device-model-error" class="field-error">{{ errorFor('model') }}</small>
            </div>
            <div class="device-field">
                <label for="asset-device-type">{{ t('assets.deviceType') }}</label>
                <Select inputId="asset-device-type" v-model="form.device_type_id" :options="types" optionLabel="name" optionValue="id"
                    :aria-label="t('assets.deviceType')" :placeholder="t('assets.deviceUnclassified')" showClear filter :disabled="fieldsDisabled"
                    :aria-invalid="!!errorFor('device_type_id')" aria-describedby="asset-device-type-error"
                    @update:modelValue="clearField('device_type_id')" />
                <small id="asset-device-type-error" class="field-error">{{ errorFor('device_type_id') }}</small>
            </div>
            <div class="device-field full-width">
                <label for="asset-device-group">{{ t('assets.deviceGroup') }}</label>
                <div v-if="editing" id="asset-device-group" class="readonly-group">{{ selectedGroupName }}</div>
                <Select v-else inputId="asset-device-group" v-model="form.gateway_group_id" :options="groups" optionLabel="name" optionValue="id"
                    :aria-label="t('assets.deviceGroup')" :placeholder="t('assets.deviceDirectNode')" showClear filter :disabled="fieldsDisabled"
                    :aria-invalid="!!errorFor('gateway_group_id')" aria-describedby="asset-device-group-help asset-device-group-error"
                    @change="groupChanged" />
                <small id="asset-device-group-help" class="device-help">{{ t('assets.deviceGroupHelp') }}</small>
                <small id="asset-device-group-error" class="field-error">{{ errorFor('gateway_group_id') }}</small>
            </div>
            <div v-if="form.gateway_group_id" class="device-field full-width">
                <div class="self-choice">
                    <Checkbox inputId="asset-device-self" v-model="form.is_gateway_self" binary :disabled="fieldsDisabled"
                        :aria-invalid="!!errorFor('is_gateway_self')" aria-describedby="asset-device-self-help asset-device-self-error"
                        @update:modelValue="clearField('is_gateway_self')" />
                    <label for="asset-device-self">{{ t('assets.deviceGatewaySelf') }}</label>
                </div>
                <small id="asset-device-self-help" class="device-help">{{ t('assets.deviceGatewaySelfHelp') }}</small>
                <small id="asset-device-self-error" class="field-error">{{ errorFor('is_gateway_self') }}</small>
            </div>
            <div class="device-field">
                <label for="asset-device-overlay">{{ t('assets.deviceOverlayIp') }} <span aria-hidden="true">*</span></label>
                <InputText id="asset-device-overlay" v-model="form.overlay_ip" autocomplete="off" :disabled="fieldsDisabled"
                    spellcheck="false" aria-required="true" :aria-invalid="!!errorFor('overlay_ip')"
                    aria-describedby="asset-device-ip-help asset-device-overlay-error"
                    @update:modelValue="clearField('overlay_ip')" @blur="previewAddresses" />
                <small id="asset-device-overlay-error" class="field-error">{{ errorFor('overlay_ip') }}</small>
            </div>
            <div class="device-field">
                <label for="asset-device-lan">{{ t('assets.deviceLanIp') }}</label>
                <InputText id="asset-device-lan" v-model="form.lan_ip" autocomplete="off" :disabled="fieldsDisabled" spellcheck="false"
                    :aria-invalid="!!errorFor('lan_ip')" aria-describedby="asset-device-ip-help asset-device-lan-error"
                    @update:modelValue="clearField('lan_ip')" @blur="previewAddresses" />
                <small id="asset-device-lan-error" class="field-error">{{ errorFor('lan_ip') }}</small>
            </div>
            <div class="address-feedback full-width">
                <small id="asset-device-ip-help" class="device-help">{{ t('assets.deviceIpHelp') }}</small>
                <Button :label="t('assets.deviceCheckAddresses')" icon="pi pi-check-circle" text size="small"
                    :loading="previewing" :disabled="fieldsDisabled" @click="previewAddresses" />
                <div v-for="(warning, index) in warnings" :key="`${warning.code}:${warning.field}:${index}`" class="address-warning" role="status">
                    <i class="pi pi-exclamation-triangle" aria-hidden="true"></i>
                    <span>{{ warning.field === 'lan_ip' ? t('assets.deviceLanMismatch') : warning.message }}</span>
                </div>
                <p v-if="previewError" class="field-error" role="alert">{{ previewError }}</p>
            </div>
            <div class="device-field full-width">
                <label for="asset-device-remarks">{{ t('assets.deviceRemarks') }}</label>
                <Textarea id="asset-device-remarks" v-model="form.remarks" :rows="3" :disabled="fieldsDisabled"
                    :aria-invalid="!!errorFor('remarks')" aria-describedby="asset-device-remarks-error"
                    @update:modelValue="clearField('remarks')" />
                <small id="asset-device-remarks-error" class="field-error">{{ errorFor('remarks') }}</small>
            </div>
            <p v-if="uncertain" class="address-warning full-width" role="status">{{ t('assets.deviceSaveUncertain') }}</p>
            <p v-if="saveError" class="field-error full-width" role="alert">{{ saveError }}</p>
        </form>
        <template #footer>
            <Button :label="t('assets.deviceCancel')" severity="secondary" @click="close" />
            <Button :label="t(uncertain ? 'assets.deviceRetrySave' : 'assets.deviceSave')" icon="pi pi-check"
                type="submit" form="asset-device-form" :loading="saving" :disabled="saving" />
        </template>
    </Dialog>
</template>

<style scoped>
.device-form { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0.75rem 1rem; }
.full-width, .device-context, .device-form > .device-help { grid-column: 1 / -1; }
.device-context { margin: 0; overflow-wrap: anywhere; color: var(--console-muted, #64748b); font-size: 0.8125rem; }
.device-field { display: flex; flex-direction: column; gap: 0.35rem; min-width: 0; }
.device-field label { font-weight: 500; }
.device-field small:empty { display: none; }
.device-help { margin: 0; color: var(--console-muted, #64748b); font-size: 0.8125rem; }
.field-error { margin: 0; font-size: 0.8125rem; color: var(--p-red-600, #dc2626); overflow-wrap: anywhere; }
.readonly-group { padding: 0.625rem 0.75rem; background: var(--console-ground, var(--p-content-background)); border: 1px solid var(--console-border, #e2e8f0); border-radius: var(--p-border-radius-md, 6px); overflow-wrap: anywhere; }
.self-choice { display: flex; gap: 0.5rem; align-items: center; }
.address-feedback { display: flex; align-items: flex-start; flex-direction: column; gap: 0.5rem; }
.address-warning { display: flex; align-items: flex-start; gap: 0.5rem; margin: 0; color: var(--p-orange-700, #9a3412); font-size: 0.8125rem; overflow-wrap: anywhere; }
.address-warning .pi { margin-top: 0.125rem; }
@media (max-width: 560px) { .device-form { grid-template-columns: minmax(0, 1fr); } }
</style>
