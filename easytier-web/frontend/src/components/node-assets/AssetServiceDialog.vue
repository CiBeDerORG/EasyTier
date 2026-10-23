<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import { Button, Checkbox, Dialog, InputNumber, InputText, Select, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AssetDevice, AssetIcon, AssetProtocol, AssetScope, AssetService, AssetServiceInput } from '../../types/node-assets';
import { formatAssetError, getAssetError, useAssetMutation } from '../../composables/useAssetMutation';
import { ASSET_SERVICE_ICON_OPTIONS, assetServiceIconClass } from '../../constants/asset-service-icons';

const props = defineProps<{
    visible: boolean;
    api: ApiClient;
    scope: AssetScope;
    device: AssetDevice;
    service?: AssetService | null;
}>();
const emit = defineEmits<{ close: []; saved: [service: AssetService]; 'context-error': [error: unknown] }>();
const { t } = useI18n();
const attempt = useAssetMutation<AssetServiceInput, AssetService>();
const editing = computed(() => !!props.service);
const contextKey = computed(() => `${props.scope.owner_user_id}:${props.scope.machine_id}:${props.device.id}`);
const form = reactive({
    name: '', protocols: ['tcp'] as AssetProtocol[], port: null as number | null,
    remarks: '', access_url: '', icon: null as AssetIcon | null,
});
const savingEdit = ref(false);
const saveError = ref('');
const fieldErrors = ref<Record<string, string>>({});
const addresses = ref<Pick<AssetDevice, 'overlay_ip' | 'lan_ip'>>({
    overlay_ip: props.device.overlay_ip, lan_ip: props.device.lan_ip,
});
const addressLoading = ref(false);
const addressError = ref('');
let revision = 0;
let addressRevision = 0;

const busy = computed(() => savingEdit.value || attempt.pending.value);
const uncertain = computed(() => !editing.value && attempt.uncertain.value);
const fieldsDisabled = computed(() => busy.value || uncertain.value);
const iconOptions = computed(() => ASSET_SERVICE_ICON_OPTIONS.map(option => ({ ...option, label: t(option.labelKey) })));
const formId = computed(() => `asset-service-form-${props.device.id}-${props.service?.id ?? 'new'}`);
const fieldId = (field: string) => `${formId.value}-${field}`;

function adoptDeviceAddresses() {
    // A newer parent device update takes precedence over an earlier read request.
    addressRevision++;
    addresses.value = { overlay_ip: props.device.overlay_ip, lan_ip: props.device.lan_ip };
    addressLoading.value = false;
    addressError.value = '';
}

async function loadDeviceAddresses() {
    if (!props.visible || addressLoading.value) return;
    const current = ++addressRevision;
    const context = contextKey.value;
    const scope = { ...props.scope };
    const deviceId = props.device.id;
    const active = () => current === addressRevision && props.visible && context === contextKey.value;
    addressLoading.value = true;
    addressError.value = '';
    try {
        const device = await props.api.assetsDevice(scope, deviceId);
        if (!active()) return;
        if (device.id !== deviceId || device.owner_user_id !== scope.owner_user_id || device.machine_id !== scope.machine_id) {
            addressError.value = t('assets.services.contextChanged');
            return;
        }
        addresses.value = { overlay_ip: device.overlay_ip, lan_ip: device.lan_ip };
    } catch (error: unknown) {
        if (!active()) return;
        addressError.value = t('assets.services.addressLoadFailed');
        emit('context-error', error);
    } finally {
        if (active()) addressLoading.value = false;
    }
}

const resetDraft = () => {
    revision++;
    attempt.reset();
    savingEdit.value = false;
    saveError.value = '';
    fieldErrors.value = {};
    const record = props.service;
    form.name = record?.name ?? '';
    form.protocols = record ? [...record.protocols] : ['tcp'];
    form.port = record?.port ?? null;
    form.remarks = record?.remarks ?? '';
    form.access_url = record?.access_url ?? '';
    form.icon = record?.icon ?? null;
};

watch([() => props.device.overlay_ip, () => props.device.lan_ip], adoptDeviceAddresses, { flush: 'sync' });
watch(() => [props.visible, contextKey.value, props.service?.id] as const, () => {
    adoptDeviceAddresses();
    if (props.visible) {
        resetDraft();
        void loadDeviceAddresses();
    } else {
        revision++;
        attempt.reset();
        savingEdit.value = false;
    }
}, { immediate: true });
onBeforeUnmount(() => { revision++; addressRevision++; attempt.reset(); });

function validate(): AssetServiceInput | null {
    const errors: Record<string, string> = {};
    const name = form.name.trim();
    const remarks = form.remarks.trim();
    const accessUrl = form.access_url.trim();
    if (!name) errors.name = t('assets.services.nameRequired');
    else if (Array.from(name).length > 128) errors.name = t('assets.services.nameTooLong');
    if (!form.protocols.length) errors.protocols = t('assets.services.protocolRequired');
    if (form.port === null || !Number.isInteger(form.port) || form.port < 1 || form.port > 65535) {
        errors.port = t('assets.services.portInvalid');
    }
    if (Array.from(remarks).length > 2000) errors.remarks = t('assets.services.remarksTooLong');
    if (accessUrl) {
        try {
            const url = new URL(accessUrl);
            if (!/^https?:\/\//i.test(accessUrl) || !['http:', 'https:'].includes(url.protocol) || !url.hostname) {
                errors.access_url = t('assets.services.urlInvalid');
            }
        } catch {
            errors.access_url = t('assets.services.urlInvalid');
        }
        if (Array.from(accessUrl).length > 2048) errors.access_url = t('assets.services.urlTooLong');
    }
    fieldErrors.value = errors;
    if (Object.keys(errors).length) return null;
    return {
        name,
        protocols: (['tcp', 'udp'] as const).filter(protocol => form.protocols.includes(protocol)),
        port: form.port!,
        remarks: remarks || null,
        access_url: accessUrl || null,
        icon: form.icon ?? null,
    };
}

async function save() {
    if (busy.value || !props.visible) return;
    const payload = validate();
    if (!payload) return;
    const scope = { ...props.scope };
    const deviceId = props.device.id;
    const serviceId = props.service?.id;
    const current = revision;
    saveError.value = '';
    try {
        let result: AssetService;
        if (serviceId) {
            savingEdit.value = true;
            result = await props.api.assetsUpdateService(scope, serviceId, payload);
        } else {
            result = await attempt.run(payload,
                request => props.api.assetsCreateService(scope, deviceId, request),
                `${contextKey.value}:service`);
        }
        if (current !== revision || !props.visible) return;
        emit('saved', result);
    } catch (error: unknown) {
        if (current !== revision || !props.visible) return;
        emit('context-error', error);
        fieldErrors.value = getAssetError(error).field_errors;
        saveError.value = formatAssetError(error, t('assets.services.saveFailed'), key => t(key));
    } finally {
        if (current === revision) savingEdit.value = false;
    }
}

function close() {
    // Discarding a form does not cancel a request already accepted by the server.
    revision++;
    addressRevision++;
    attempt.reset();
    emit('close');
}
</script>

<template>
    <Dialog :visible="visible" modal :header="t(editing ? 'assets.services.editTitle' : 'assets.services.addTitle')"
        :style="{ width: '38rem', maxWidth: 'calc(100vw - 2rem)' }" :dismissableMask="false"
        @update:visible="value => { if (!value) close(); }">
        <form :id="formId" class="service-form" @submit.prevent="save">
            <p class="service-context">{{ t('assets.services.deviceContext', { name: device.name }) }}</p>
            <div class="service-field">
                <label :for="fieldId('name')">{{ t('assets.services.name') }} <span aria-hidden="true">*</span></label>
                <InputText :id="fieldId('name')" v-model="form.name" :disabled="fieldsDisabled" autocomplete="off"
                    required :aria-invalid="!!fieldErrors.name" :aria-describedby="fieldId('name-error')" />
                <small v-if="fieldErrors.name" :id="fieldId('name-error')" class="field-error">{{ fieldErrors.name }}</small>
            </div>
            <div class="service-field" :aria-busy="addressLoading">
                <div class="service-fields-row">
                    <div class="service-field">
                        <label :for="fieldId('overlay-ip')">{{ t('assets.deviceOverlayIp') }}</label>
                        <InputText :id="fieldId('overlay-ip')" :modelValue="addresses.overlay_ip || t('assets.services.addressMissing')"
                            readonly :aria-describedby="fieldId('address-help')" />
                    </div>
                    <div class="service-field">
                        <label :for="fieldId('lan-ip')">{{ t('assets.deviceLanIp') }}</label>
                        <InputText :id="fieldId('lan-ip')" :modelValue="addresses.lan_ip || t('assets.services.addressMissing')"
                            readonly :aria-describedby="fieldId('address-help')" />
                    </div>
                </div>
                <small :id="fieldId('address-help')">{{ t('assets.services.addressHint') }}</small>
                <div v-if="addressLoading || addressError" class="service-address-status" role="status">
                    <small v-if="addressLoading">{{ t('assets.services.addressLoading') }}</small>
                    <template v-else>
                        <small class="field-error">{{ addressError }}</small>
                        <Button :label="t('assets.services.retry')" type="button" text size="small" @click="loadDeviceAddresses" />
                    </template>
                </div>
            </div>
            <div class="service-fields-row">
                <fieldset class="service-field protocol-field" :disabled="fieldsDisabled" :aria-describedby="fieldId('protocol-error')">
                    <legend>{{ t('assets.services.protocols') }} <span aria-hidden="true">*</span></legend>
                    <div class="protocol-options">
                        <label :for="fieldId('tcp')" class="protocol-option">
                            <Checkbox :inputId="fieldId('tcp')" v-model="form.protocols" value="tcp" :disabled="fieldsDisabled" /> TCP
                        </label>
                        <label :for="fieldId('udp')" class="protocol-option">
                            <Checkbox :inputId="fieldId('udp')" v-model="form.protocols" value="udp" :disabled="fieldsDisabled" /> UDP
                        </label>
                    </div>
                    <small>{{ t('assets.services.protocolHint') }}</small>
                    <small v-if="fieldErrors.protocols" :id="fieldId('protocol-error')" class="field-error">{{ fieldErrors.protocols }}</small>
                </fieldset>
                <div class="service-field">
                    <label :for="fieldId('port')">{{ t('assets.services.port') }} <span aria-hidden="true">*</span></label>
                    <InputNumber :inputId="fieldId('port')" v-model="form.port" :min="1" :max="65535"
                        :maxFractionDigits="0" :useGrouping="false" :disabled="fieldsDisabled"
                        :aria-invalid="!!fieldErrors.port" :aria-describedby="fieldId('port-error')" />
                    <small v-if="fieldErrors.port" :id="fieldId('port-error')" class="field-error">{{ fieldErrors.port }}</small>
                </div>
            </div>
            <div class="service-field">
                <label :for="fieldId('url')">{{ t('assets.services.accessUrl') }}</label>
                <InputText :id="fieldId('url')" v-model="form.access_url" :disabled="fieldsDisabled" inputmode="url"
                    placeholder="https://example.com/app" autocomplete="off" :aria-invalid="!!fieldErrors.access_url"
                    :aria-describedby="fieldId('url-help')" />
                <small :id="fieldId('url-help')" :class="{ 'field-error': fieldErrors.access_url }">
                    {{ fieldErrors.access_url || t('assets.services.urlHint') }}
                </small>
            </div>
            <div class="service-field">
                <label :for="fieldId('icon')">{{ t('assets.services.icon') }}</label>
                <Select :inputId="fieldId('icon')" v-model="form.icon" :options="iconOptions" optionLabel="label"
                    optionValue="value" :aria-label="t('assets.services.icon')" :showClear="true" :placeholder="t('assets.services.defaultIcon')" :disabled="fieldsDisabled">
                    <template #value="slotProps">
                        <span class="icon-choice"><i :class="assetServiceIconClass(slotProps.value)" aria-hidden="true"></i>
                            {{ slotProps.value ? t(`assets.serviceIcons.${slotProps.value}`) : t('assets.services.defaultIcon') }}</span>
                    </template>
                    <template #option="slotProps">
                        <span class="icon-choice"><i :class="slotProps.option.icon" aria-hidden="true"></i>{{ slotProps.option.label }}</span>
                    </template>
                </Select>
                <small v-if="fieldErrors.icon" class="field-error">{{ fieldErrors.icon }}</small>
            </div>
            <div class="service-field">
                <label :for="fieldId('remarks')">{{ t('assets.services.remarks') }}</label>
                <Textarea :id="fieldId('remarks')" v-model="form.remarks" rows="3" autoResize :disabled="fieldsDisabled"
                    :aria-invalid="!!fieldErrors.remarks" :aria-describedby="fieldId('remarks-error')" />
                <small v-if="fieldErrors.remarks" :id="fieldId('remarks-error')" class="field-error">{{ fieldErrors.remarks }}</small>
            </div>
            <p v-if="uncertain" class="uncertain-note" role="status">{{ t('assets.services.unknownOutcome') }}</p>
            <p v-if="saveError" class="field-error" role="alert">{{ saveError }}</p>
        </form>
        <template #footer>
            <Button :label="t('assets.services.cancel')" severity="secondary" @click="close" />
            <Button :label="t(uncertain ? 'assets.services.retryOriginal' : 'assets.services.save')" icon="pi pi-check"
                type="submit" :form="formId" :loading="busy" :disabled="busy" />
        </template>
    </Dialog>
</template>

<style scoped>
.service-form, .service-field { display: flex; flex-direction: column; gap: 0.45rem; min-width: 0; }
.service-form { gap: 1rem; }
.service-context { margin: 0; color: var(--console-muted, var(--p-text-muted-color)); overflow-wrap: anywhere; }
.service-field label, .service-field legend { font-weight: 600; }
.service-field small { color: var(--console-muted, var(--p-text-muted-color)); }
.service-fields-row { display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; align-items: start; }
.service-address-status { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
.protocol-field { margin: 0; padding: 0; border: 0; }
.protocol-field legend { margin-bottom: 0.6rem; }
.protocol-options, .protocol-option, .icon-choice { display: flex; align-items: center; gap: 0.6rem; }
.protocol-options { gap: 1.2rem; padding-block: 0.25rem; }
.protocol-option { cursor: pointer; }
.field-error, .service-field small.field-error { color: var(--p-red-600, #dc2626); }
.uncertain-note { padding: 0.75rem; margin: 0; border: 1px solid var(--p-orange-400, #fb923c); border-radius: 0.5rem; font-size: 0.875rem; }
@media (max-width: 480px) { .service-fields-row { grid-template-columns: 1fr; } }
</style>
