<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import { Button, Dialog, InputNumber, InputText, Select } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AssetNode, AssetProtocol, AssetScope, ServicePublicationInput, ServicePublicationNetwork } from '../../types/node-assets';
import { formatAssetError, getAssetError } from '../../composables/useAssetMutation';

const props = defineProps<{
    visible: boolean;
    api: ApiClient;
    scope: AssetScope;
    formId: string;
    publication: ServicePublicationInput | null;
    serverName: string;
    networkName: string;
    protocols: AssetProtocol[];
    servicePort: number | null;
    targetIp: string;
    fieldErrors: Record<string, string>;
    disabled: boolean;
}>();
const emit = defineEmits<{
    cancel: [];
    confirmed: [publication: ServicePublicationInput, labels: { server_name: string; network_name: string }];
    'context-error': [error: unknown];
}>();
const { t } = useI18n();
const form = reactive({
    machine_id: null as string | null, instance_id: null as string | null,
    bind_ip: '0.0.0.0', bind_port: null as number | null, dst_port: null as number | null,
});
const servers = ref<AssetNode[]>([]);
const networks = ref<ServicePublicationNetwork[]>([]);
const serversLoading = ref(false), networksLoading = ref(false);
const serversLoaded = ref(false), networksLoaded = ref(false);
const serverError = ref(''), networkError = ref('');
const errors = ref<Record<string, string>>({});
const generalError = ref('');
const scopeKey = computed(() => `${props.scope.owner_user_id}:${props.scope.machine_id}`);
const fieldId = (field: string) => `${props.formId}-${field}`;
const selectedServer = computed(() => servers.value.find(server => server.machine_id === form.machine_id));
const selectedNetwork = computed(() => networks.value.find(network => network.instance_id === form.instance_id));
const protocolLabel = computed(() => props.protocols.map(protocol => protocol.toUpperCase()).join(' / '));
let generation = 0, serverRequest = 0, networkRequest = 0;
let disposed = false;
const unavailableReasonKeys: Record<string, string> = {
    centrally_managed: 'centrallyManaged',
    config_not_saved: 'configNotSaved',
    saved_config_not_web: 'savedConfigNotWeb',
    saved_config_invalid: 'savedConfigInvalid',
    instance_id_mismatch: 'instanceIdMismatch',
    saved_config_stopped: 'savedConfigStopped',
    runtime_metadata_missing: 'runtimeMetadataMissing',
    runtime_config_not_web: 'runtimeConfigNotWeb',
    runtime_config_read_only: 'runtimeConfigReadOnly',
};

function unavailableReason(network: ServicePublicationNetwork) {
    const key = network.unavailable_code ? unavailableReasonKeys[network.unavailable_code] : undefined;
    return t(`assets.publication.reasons.${key ?? 'unavailable'}`);
}

const serverOptions = computed(() => {
    const options = servers.value.map(server => ({
        value: server.machine_id,
        label: `${server.alias || server.hostname || server.machine_id} · ${server.machine_id.slice(0, 8)}`,
        disabled: false,
    }));
    if (form.machine_id && !options.some(option => option.value === form.machine_id)) {
        const name = form.machine_id === props.publication?.machine_id ? props.serverName : '';
        options.unshift({ value: form.machine_id,
            label: `${name || form.machine_id}${serversLoaded.value ? ` · ${t('assets.publication.selectionUnavailable')}` : ''}`,
            disabled: true });
    }
    return options;
});
const networkOptions = computed(() => {
    const options = networks.value.map(network => ({
        value: network.instance_id,
        label: `${network.display_name || network.network_name || network.instance_id} · ${network.instance_id.slice(0, 8)}`,
        disabled: !network.selectable,
        reason: network.selectable ? '' : unavailableReason(network),
    }));
    if (form.instance_id && !options.some(option => option.value === form.instance_id)) {
        const name = form.machine_id === props.publication?.machine_id && form.instance_id === props.publication?.instance_id
            ? props.networkName : '';
        options.unshift({ value: form.instance_id,
            label: `${name || form.instance_id}${networksLoaded.value ? ` · ${t('assets.publication.selectionUnavailable')}` : ''}`,
            disabled: true, reason: networksLoaded.value ? t('assets.publication.networkUnavailable') : '' });
    }
    return options;
});

async function loadServers() {
    if (!props.visible || props.disabled) return;
    const currentGeneration = generation, request = ++serverRequest, context = scopeKey.value;
    const active = () => !disposed && props.visible && currentGeneration === generation
        && request === serverRequest && context === scopeKey.value;
    serversLoading.value = true;
    serversLoaded.value = false;
    serverError.value = '';
    try {
        const response = await props.api.assetsNodes(props.scope.owner_user_id);
        if (!active()) return;
        servers.value = response.items.filter(server => server.is_public_server && server.owner_user_id === props.scope.owner_user_id);
        serversLoaded.value = true;
    } catch (error: unknown) {
        if (!active()) return;
        serverError.value = formatAssetError(error, t('assets.publication.serversLoadFailed'), key => t(key));
        if ([401, 403].includes(getAssetError(error).status ?? 0)) emit('context-error', error);
    } finally { if (active()) serversLoading.value = false; }
}

async function loadNetworks() {
    if (!props.visible || props.disabled || !form.machine_id) return;
    const currentGeneration = generation, request = ++networkRequest, context = scopeKey.value;
    const owner = props.scope.owner_user_id, machine = form.machine_id;
    const active = () => !disposed && props.visible && currentGeneration === generation
        && request === networkRequest && context === scopeKey.value && machine === form.machine_id;
    networksLoading.value = true;
    networksLoaded.value = false;
    networkError.value = '';
    try {
        const response = await props.api.assetsServicePublicationNetworks(owner, machine);
        if (!active()) return;
        networks.value = response.items;
        networksLoaded.value = true;
    } catch (error: unknown) {
        if (!active()) return;
        networkError.value = formatAssetError(error, t('assets.publication.networksLoadFailed'), key => t(key));
        // A missing or unavailable publication target does not invalidate the source device.
        if (getAssetError(error).status === 401) emit('context-error', error);
    } finally { if (active()) networksLoading.value = false; }
}

function selectServer(value: string | null) {
    if (props.disabled || value === form.machine_id) return;
    form.machine_id = value;
    form.instance_id = null;
    networkRequest++;
    networks.value = [];
    networksLoaded.value = false;
    networksLoading.value = false;
    networkError.value = '';
    delete errors.value.machine_id;
    delete errors.value.instance_id;
    void loadNetworks();
}

function selectNetwork(value: string | null) {
    if (props.disabled) return;
    form.instance_id = value;
    delete errors.value.instance_id;
}

function resetDraft() {
    generation++;
    serverRequest++;
    networkRequest++;
    const selected = props.publication;
    form.machine_id = selected?.machine_id ?? null;
    form.instance_id = selected?.instance_id ?? null;
    form.bind_ip = selected?.bind_ip ?? '0.0.0.0';
    form.bind_port = selected?.bind_port ?? props.servicePort;
    form.dst_port = selected?.dst_port ?? props.servicePort;
    servers.value = [];
    networks.value = [];
    serversLoaded.value = false;
    networksLoaded.value = false;
    serversLoading.value = false;
    networksLoading.value = false;
    serverError.value = '';
    networkError.value = '';
    generalError.value = props.fieldErrors.publication ?? '';
    errors.value = Object.fromEntries(Object.entries(props.fieldErrors)
        .filter(([key]) => key.startsWith('publication.')).map(([key, value]) => [key.slice('publication.'.length), value]));
    if (props.visible) {
        void loadServers();
        if (form.machine_id) void loadNetworks();
    }
}

watch(() => [props.visible, scopeKey.value] as const, resetDraft, { immediate: true });
onBeforeUnmount(() => { disposed = true; generation++; serverRequest++; networkRequest++; });

function confirm() {
    if (props.disabled || serversLoading.value || networksLoading.value) return;
    generalError.value = '';
    const next: Record<string, string> = {};
    if (!form.machine_id || !serversLoaded.value || !selectedServer.value) next.machine_id = t('assets.publication.serverRequired');
    if (!form.instance_id || !networksLoaded.value || !selectedNetwork.value?.selectable) next.instance_id = t('assets.publication.networkRequired');
    if (!props.protocols.length) next.protocols = t('assets.services.protocolRequired');
    const bindIp = form.bind_ip.trim();
    if (!bindIp || bindIp.length > 128 || /[\s/]/u.test(bindIp)) next.bind_ip = t('assets.publication.bindIpInvalid');
    const validPort = (value: number | null) => value !== null && Number.isInteger(value) && value >= 1 && value <= 65535;
    if (!validPort(form.bind_port)) next.bind_port = t('assets.services.portInvalid');
    if (!validPort(form.dst_port)) next.dst_port = t('assets.services.portInvalid');
    if (!props.targetIp) next.dst_ip = t('assets.publication.targetMissing');
    errors.value = next;
    if (Object.keys(next).length) return;
    emit('confirmed', {
        machine_id: form.machine_id!, instance_id: form.instance_id!, bind_ip: bindIp,
        bind_port: form.bind_port!, dst_port: form.dst_port!,
    }, {
        server_name: selectedServer.value!.alias || selectedServer.value!.hostname || form.machine_id!,
        network_name: selectedNetwork.value!.network_name || selectedNetwork.value!.display_name || form.instance_id!,
    });
}

function cancel() {
    if (props.disabled) return;
    generation++;
    emit('cancel');
}
</script>

<template>
    <Dialog :visible="visible" modal :header="t('assets.publication.title')" :dismissableMask="false"
        :closable="!disabled" :closeOnEscape="!disabled" :style="{ width: '40rem', maxWidth: 'calc(100vw - 2rem)' }"
        @update:visible="value => { if (!value) cancel(); }">
        <form :id="formId" class="publication-form" @submit.prevent="confirm">
            <p class="publication-hint">{{ t('assets.publication.draftHint') }}</p>
            <p v-if="generalError" class="publication-hint field-error" role="alert">{{ generalError }}</p>
            <div class="publication-field">
                <label :for="fieldId('server')">{{ t('assets.publication.server') }} *</label>
                <div class="publication-select-row">
                    <Select :inputId="fieldId('server')" :modelValue="form.machine_id" :options="serverOptions"
                        optionLabel="label" optionValue="value" optionDisabled="disabled" filter
                        :placeholder="t('assets.publication.chooseServer')" :loading="serversLoading"
                        :disabled="disabled || serversLoading" :aria-invalid="!!errors.machine_id"
                        :aria-describedby="fieldId('server-status')" @update:modelValue="selectServer" />
                    <Button :label="t('assets.services.refresh')" type="button" icon="pi pi-refresh" text size="small"
                        :disabled="disabled || serversLoading" :loading="serversLoading" @click="loadServers" />
                </div>
                <div :id="fieldId('server-status')" class="publication-status" aria-live="polite">
                    <small v-if="serversLoading">{{ t('assets.publication.serversLoading') }}</small>
                    <small v-if="serverError" class="field-error">{{ serverError }}</small>
                    <small v-else-if="serversLoaded && !servers.length">{{ t('assets.publication.noServers') }}</small>
                    <small v-if="errors.machine_id" class="field-error">{{ errors.machine_id }}</small>
                    <small v-else-if="serversLoaded && form.machine_id && !selectedServer" class="field-error">{{ t('assets.publication.serverUnavailable') }}</small>
                </div>
            </div>
            <div class="publication-field">
                <label :for="fieldId('network')">{{ t('assets.publication.network') }} *</label>
                <div class="publication-select-row">
                    <Select :inputId="fieldId('network')" :modelValue="form.instance_id" :options="networkOptions"
                        optionLabel="label" optionValue="value" optionDisabled="disabled" filter
                        :placeholder="t('assets.publication.chooseNetwork')" :loading="networksLoading"
                        :disabled="disabled || !form.machine_id || networksLoading" :aria-invalid="!!errors.instance_id"
                        :aria-describedby="fieldId('network-status')" @update:modelValue="selectNetwork">
                        <template #option="{ option }">
                            <div class="publication-network-option">
                                <span>{{ option.label }}</span>
                                <small v-if="option.reason">{{ option.reason }}</small>
                            </div>
                        </template>
                    </Select>
                    <Button :label="t('assets.services.refresh')" type="button" icon="pi pi-refresh" text size="small"
                        :disabled="disabled || !form.machine_id || networksLoading" :loading="networksLoading" @click="loadNetworks" />
                </div>
                <div :id="fieldId('network-status')" class="publication-status" aria-live="polite">
                    <small v-if="networksLoading">{{ t('assets.publication.networksLoading') }}</small>
                    <small v-if="networkError" class="field-error">{{ networkError }}</small>
                    <small v-else-if="networksLoaded && !networks.length">{{ t('assets.publication.noNetworks') }}</small>
                    <small v-if="errors.instance_id" class="field-error">{{ errors.instance_id }}</small>
                    <small v-else-if="networksLoaded && form.instance_id && !selectedNetwork?.selectable" class="field-error">
                        {{ selectedNetwork ? unavailableReason(selectedNetwork) : t('assets.publication.networkUnavailable') }}
                    </small>
                </div>
            </div>
            <div class="publication-field">
                <label :for="fieldId('protocols')">{{ t('assets.services.protocols') }}</label>
                <InputText :id="fieldId('protocols')" :modelValue="protocolLabel" readonly :aria-describedby="fieldId('protocol-help')" />
                <small :id="fieldId('protocol-help')">{{ t('assets.publication.protocolHint') }}</small>
                <small v-if="errors.protocols" class="field-error">{{ errors.protocols }}</small>
            </div>
            <div class="publication-fields-row">
                <div class="publication-field">
                    <label :for="fieldId('bind-ip')">{{ t('assets.publication.bindIp') }} *</label>
                    <InputText :id="fieldId('bind-ip')" v-model="form.bind_ip" :disabled="disabled" autocomplete="off"
                        placeholder="0.0.0.0" :aria-invalid="!!errors.bind_ip" :aria-describedby="fieldId('bind-ip-error')" />
                    <small v-if="errors.bind_ip" :id="fieldId('bind-ip-error')" class="field-error">{{ errors.bind_ip }}</small>
                </div>
                <div class="publication-field">
                    <label :for="fieldId('bind-port')">{{ t('assets.publication.bindPort') }} *</label>
                    <InputNumber :inputId="fieldId('bind-port')" v-model="form.bind_port" :min="1" :max="65535"
                        :maxFractionDigits="0" :useGrouping="false" :disabled="disabled"
                        :aria-invalid="!!errors.bind_port" :aria-describedby="fieldId('bind-port-error')" />
                    <small v-if="errors.bind_port" :id="fieldId('bind-port-error')" class="field-error">{{ errors.bind_port }}</small>
                </div>
            </div>
            <div class="publication-fields-row">
                <div class="publication-field">
                    <label :for="fieldId('target-ip')">{{ t('assets.publication.targetIp') }}</label>
                    <InputText :id="fieldId('target-ip')" :modelValue="targetIp || t('assets.services.addressMissing')" readonly
                        :aria-describedby="fieldId('target-ip-help')" />
                    <small :id="fieldId('target-ip-help')">{{ t('assets.publication.targetHint') }}</small>
                    <small v-if="errors.dst_ip" class="field-error">{{ errors.dst_ip }}</small>
                </div>
                <div class="publication-field">
                    <label :for="fieldId('target-port')">{{ t('assets.publication.targetPort') }} *</label>
                    <InputNumber :inputId="fieldId('target-port')" v-model="form.dst_port" :min="1" :max="65535"
                        :maxFractionDigits="0" :useGrouping="false" :disabled="disabled"
                        :aria-invalid="!!errors.dst_port" :aria-describedby="fieldId('target-port-error')" />
                    <small v-if="errors.dst_port" :id="fieldId('target-port-error')" class="field-error">{{ errors.dst_port }}</small>
                </div>
            </div>
        </form>
        <template #footer>
            <Button :label="t('assets.services.cancel')" type="button" severity="secondary" :disabled="disabled" @click="cancel" />
            <Button :label="t('assets.publication.confirm')" icon="pi pi-check" type="submit" :form="formId"
                :disabled="disabled || serversLoading || networksLoading || !!serverError || !!networkError" />
        </template>
    </Dialog>
</template>

<style scoped>
.publication-form, .publication-field, .publication-status { display: flex; flex-direction: column; min-width: 0; gap: 0.45rem; }
.publication-form { gap: 1rem; }
.publication-field label { font-weight: 600; }
.publication-fields-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 1rem; }
.publication-select-row { display: flex; align-items: center; gap: 0.35rem; min-width: 0; }
.publication-select-row > :first-child { flex: 1; min-width: 0; }
.publication-network-option { display: grid; gap: 0.25rem; max-width: min(30rem, calc(100vw - 5rem)); white-space: normal; overflow-wrap: anywhere; }
.publication-network-option small { color: var(--console-muted, var(--p-text-muted-color)); }
.publication-hint, .publication-field small { color: var(--console-muted, var(--p-text-muted-color)); }
.publication-hint { margin: 0; font-size: 0.875rem; }
.field-error, .publication-field small.field-error { color: var(--p-red-600, #dc2626); }
.publication-status { overflow-wrap: anywhere; }
@media (max-width: 480px) { .publication-fields-row { grid-template-columns: 1fr; } }
</style>
