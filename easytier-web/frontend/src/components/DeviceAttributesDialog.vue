<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';
import { Button, Checkbox, Dialog, InputText } from 'primevue';
import { Utils } from 'easytier-frontend-lib';
import { useI18n } from 'vue-i18n';
import ApiClient from '../modules/api';
import type { AssetNode } from '../types/node-assets';
import { nodeLocationErrors, normalizeNodeLocation } from '../modules/node-location';
import NodeLocationEditor from './NodeLocationEditor.vue';

type EditableAttributes = Pick<Utils.DeviceAttributes, 'device_model' | 'geo_location'> & { is_public_server: boolean };
const props = defineProps<{ device: Utils.DeviceInfo; node: AssetNode; api?: ApiClient }>();
const emit = defineEmits<{
    close: [];
    updated: [machineId: string, changes: Partial<EditableAttributes> & { alias?: string }];
    saved: [machineId: string];
}>();
const { t } = useI18n();

// This component is mounted for each edit. Polling never replaces its draft.
const alias = ref(props.device.alias ?? '');
const savedAlias = ref(props.device.alias ?? '');
const savedAttributes = ref<EditableAttributes>({
    device_model: props.node.device_model ?? null,
    geo_location: normalizeNodeLocation(props.node.geo_location),
    is_public_server: props.node.is_public_server,
});
const someChangesSaved = ref(false);
const deviceModel = ref(props.node.device_model ?? '');
const geoLocation = ref<Utils.NodeGeoLocation | null>(normalizeNodeLocation(props.node.geo_location));
const isPublicServer = ref(props.node.is_public_server);
const geoLookupPending = ref(false);
const saving = ref(false);
const saveError = ref('');
let disposed = false;
onBeforeUnmount(() => { disposed = true; });

const fieldError = (value: string, limit: number) => {
    if (/[\u0000-\u001f\u007f-\u009f]/u.test(value)) return t('web.device.attributes_invalid_control');
    // Rust counts Unicode scalar values; HTML maxlength counts UTF-16 units.
    if (Array.from(value.trim()).length > limit) return t('web.device.attributes_too_long', { limit });
    return '';
};
const aliasLength = computed(() => Array.from(alias.value.trim()).length);
const aliasError = computed(() => aliasLength.value > 64 ? t('web.device.attributes_too_long', { limit: 64 }) : '');
const aliasChanged = computed(() => alias.value.trim() !== savedAlias.value);
const attributesChanged = computed(() => deviceModel.value.trim() !== (savedAttributes.value.device_model ?? '')
    || isPublicServer.value !== savedAttributes.value.is_public_server
    || JSON.stringify(normalizeNodeLocation(geoLocation.value)) !== JSON.stringify(normalizeNodeLocation(savedAttributes.value.geo_location)));
const hasChanges = computed(() => aliasChanged.value || attributesChanged.value);
const modelError = computed(() => fieldError(deviceModel.value, 128));
const geoInvalid = computed(() => Object.keys(nodeLocationErrors(geoLocation.value, (key, values) => values ? t(key, values) : t(key))).length > 0);
const ipLocationText = computed(() => {
    const parts = [props.device.ip_country, props.device.ip_region, props.device.ip_city].filter(Boolean);
    if (parts.length) return parts.join(' · ');
    switch (props.device.ip_location_status) {
        case 'non_public': return t('web.device.ip_location_non_public');
        case 'not_found': return t('web.device.ip_location_not_found');
        case 'unavailable': return t('web.device.ip_location_unavailable');
        default: return t('web.device.attributes_not_recorded');
    }
});
const observedAt = computed(() => {
    const raw = props.device.ip_observed_at;
    if (!raw) return t('web.device.attributes_not_recorded');
    const date = new Date(raw);
    return Number.isNaN(date.getTime()) ? raw : date.toLocaleString();
});

const close = () => { if (!saving.value) emit('close'); };
const save = async () => {
    if (saving.value || !props.api || !hasChanges.value || aliasError.value || modelError.value || geoInvalid.value || geoLookupPending.value) return;
    const machineId = props.device.machine_id;
    if (props.node.machine_id !== machineId) return;
    const scope = { owner_user_id: props.node.owner_user_id, machine_id: machineId };
    saving.value = true;
    saveError.value = '';
    try {
        if (aliasChanged.value) {
            const value = alias.value.trim();
            await props.api.update_machine_alias(machineId, value);
            if (disposed || props.device.machine_id !== machineId) return;
            savedAlias.value = value;
            someChangesSaved.value = true;
            emit('updated', machineId, { alias: value });
        }
        if (attributesChanged.value) {
            // Retire the old free-text place. Keep the legacy wire field only
            // because the existing endpoint requires it; geographic data is separate.
            const attributes = await props.api.assetsUpdateNode(scope, {
                site_location: null,
                device_model: deviceModel.value.trim() || null,
                geo_location: normalizeNodeLocation(geoLocation.value),
                is_public_server: isPublicServer.value,
            });
            if (disposed || props.device.machine_id !== machineId) return;
            savedAttributes.value = {
                device_model: attributes.device_model ?? null,
                geo_location: normalizeNodeLocation(attributes.geo_location),
                is_public_server: attributes.is_public_server,
            };
            someChangesSaved.value = true;
            emit('updated', machineId, savedAttributes.value);
        }
        emit('saved', machineId);
    } catch (error: any) {
        if (disposed || props.device.machine_id !== machineId) return;
        saveError.value = t(error?.response?.status === 404
            ? 'web.device.attributes_not_found'
            : 'web.device.attributes_save_failed');
    } finally {
        if (!disposed && props.device.machine_id === machineId) saving.value = false;
    }
};
</script>

<template>
    <Dialog :visible="true" modal :header="t('web.device.edit_attributes')"
        :style="{ width: '44rem', maxWidth: 'calc(100vw - 2rem)' }"
        :closable="!saving" :closeOnEscape="!saving" :dismissableMask="!saving"
        @update:visible="value => { if (!value) close(); }">
        <form id="device-attributes-form" class="attributes-form" @submit.prevent="save">
            <p class="attributes-device-name">{{ device.alias || device.hostname || device.machine_id }}</p>
            <p class="muted text-sm">{{ t('web.device.attributes_hint') }}</p>
            <div class="attribute-field">
                <label for="device-alias">{{ t('web.device.alias') }}</label>
                <InputText id="device-alias" v-model="alias" :disabled="saving" autocomplete="off"
                    :placeholder="device.hostname" :aria-invalid="!!aliasError" aria-describedby="device-alias-help" />
                <small id="device-alias-help" :class="{ 'attribute-error': aliasError }">
                    {{ aliasError || t('web.console.alias_hint') }}
                </small>
                <small class="text-right">{{ aliasLength }}/64</small>
            </div>
            <div class="attribute-field">
                <label for="device-model">{{ t('web.device.device_model') }}</label>
                <InputText id="device-model" v-model="deviceModel" :disabled="saving" autocomplete="off"
                    :aria-invalid="!!modelError" aria-describedby="device-model-help" />
                <small id="device-model-help" :class="{ 'attribute-error': modelError }">
                    {{ modelError || t('web.device.device_model_hint') }}
                </small>
            </div>
            <div class="attribute-field">
                <div class="attribute-public-role">
                    <Checkbox inputId="device-public-node" v-model="isPublicServer" binary :disabled="saving" />
                    <label for="device-public-node">{{ t('assets.page.public_node') }}</label>
                </div>
                <small>{{ t('assets.page.public_hint') }}</small>
            </div>
            <NodeLocationEditor v-model="geoLocation" :api="api" id-prefix="device-node-geo" :disabled="saving" @lookup-pending="geoLookupPending = $event" />
            <dl class="automatic-attributes">
                <div><dt>{{ t('web.device.connection_ip') }}</dt><dd>{{ device.connection_ip || t('web.device.attributes_not_recorded') }}</dd></div>
                <div><dt>{{ t('web.device.ip_location') }}</dt><dd>{{ ipLocationText }}</dd></div>
                <div><dt>{{ t('web.device.ip_observed_at') }}</dt><dd>{{ observedAt }}</dd></div>
            </dl>
            <p v-if="saveError" class="attribute-error text-sm" role="alert">{{ saveError }}</p>
            <p v-if="saveError && someChangesSaved" class="muted text-sm" role="status">{{ t('web.device.attributes_partial_saved') }}</p>
        </form>
        <template #footer>
            <Button :label="t('web.common.cancel')" severity="secondary" :disabled="saving" @click="close" />
            <Button :label="t('web.common.save')" icon="pi pi-check" type="submit" form="device-attributes-form"
                :loading="saving" :disabled="saving || !api || !hasChanges || !!aliasError || !!modelError || geoInvalid || geoLookupPending" />
        </template>
    </Dialog>
</template>

<style scoped>
.attributes-form, .attribute-field { display: flex; flex-direction: column; gap: 0.5rem; }
.attributes-form { gap: 1rem; }
.attributes-device-name { font-weight: 600; overflow-wrap: anywhere; }
.attribute-field label { font-weight: 500; }
.attribute-field small, .automatic-attributes dt { color: var(--console-muted, #64748b); }
.attribute-public-role { display: flex; align-items: center; gap: 0.5rem; }
.automatic-attributes { display: flex; flex-direction: column; gap: 0.75rem; padding-top: 1rem; border-top: 1px solid var(--console-border, #e2e8f0); font-size: 0.875rem; }
.automatic-attributes > div { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 2fr); gap: 0.75rem; }
.automatic-attributes dd { margin: 0; overflow-wrap: anywhere; }
.attribute-error, .attribute-field small.attribute-error { color: var(--p-red-600, #dc2626); }
</style>
