<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Button, Drawer, InputNumber, InputText, Message, Select, Textarea, ToggleSwitch } from 'primevue';
import ApiClient, { type CentralNetworkMember, type CentralNetworkSummary } from '../../modules/api';
import type { MapLanDevice, ServiceEntry, ServiceInput } from '../../modules/network-space';
import { serviceMessages } from '../../modules/service-messages';
import { safeServiceUrl, serviceError, serviceIcons, serviceInput } from '../../modules/service-utils';

const props = defineProps<{
    api: ApiClient;
    visible: boolean;
    entry: ServiceEntry | null;
    defaults?: { network?: string; node?: string; device?: string };
}>();
const emit = defineEmits<{ 'update:visible': [value: boolean]; saved: [service: ServiceEntry] }>();
const { t } = useI18n({ useScope: 'local', messages: serviceMessages });
const emptyForm = (): ServiceInput => ({ network_id: '', device_id: '', lan_device_id: null, name: '', description: '', url: '', icon: 'globe', group: '', show_in_navigation: true, open_in_new_tab: true, sort_order: 0 });
const form = ref<ServiceInput>(emptyForm());
const networks = ref<CentralNetworkSummary[]>([]);
const members = ref<CentralNetworkMember[]>([]);
const devices = ref<MapLanDevice[]>([]);
const loading = ref(false);
const destinationsLoading = ref(false);
const saving = ref(false);
const error = ref('');
const contextError = ref(false);
let sessionVersion = 0;
let destinationVersion = 0;
const networkOptions = computed(() => networks.value.map(network => ({ label: network.display_name || network.network_name, value: network.network_id })));
const memberOptions = computed(() => members.value.map(member => ({ label: member.alias || member.hostname_override || member.hostname || member.device_id, value: member.device_id })));
const downstream = computed(() => devices.value.filter(device => device.gateway_device_id === form.value.device_id));
const deviceOptions = computed(() => [{ label: t('nodeItself'), value: null }, ...downstream.value.map(device => ({ label: `${device.name}${device.address ? ` · ${device.address}` : ''}`, value: device.id }))]);
const iconOptions = computed(() => Object.keys(serviceIcons).map(value => ({ label: t(value), value })));
const previewUrl = computed(() => safeServiceUrl(form.value.url));
const busy = computed(() => loading.value || destinationsLoading.value || saving.value);
const missingMember = computed(() => !destinationsLoading.value && !!form.value.device_id && !members.value.some(member => member.device_id === form.value.device_id));

async function loadDestinations(networkId: string, desiredNode = '', desiredDevice: string | null = null) {
    const version = ++destinationVersion;
    contextError.value = false;
    members.value = [];
    devices.value = [];
    form.value.device_id = desiredNode;
    form.value.lan_device_id = desiredDevice;
    if (!networkId) { destinationsLoading.value = false; return; }
    destinationsLoading.value = true;
    try {
        const [result, map] = await Promise.all([props.api.list_network_members(networkId), props.api.get_network_map(networkId)]);
        if (version !== destinationVersion) return;
        members.value = result.members;
        devices.value = map.devices;
        if (!form.value.device_id && result.members.length === 1) form.value.device_id = result.members[0].device_id;
        if (form.value.lan_device_id && !downstream.value.some(device => device.id === form.value.lan_device_id)) form.value.lan_device_id = null;
    } catch {
        if (version === destinationVersion) contextError.value = true;
    } finally {
        if (version === destinationVersion) destinationsLoading.value = false;
    }
}
async function initialize(resetForm = true) {
    const version = ++sessionVersion;
    destinationVersion++;
    error.value = '';
    contextError.value = false;
    if (resetForm) form.value = props.entry ? serviceInput(props.entry) : { ...emptyForm(), network_id: props.defaults?.network ?? '', device_id: props.defaults?.node ?? '', lan_device_id: props.defaults?.device ?? null };
    networks.value = [];
    members.value = [];
    devices.value = [];
    loading.value = true;
    try {
        const result = await props.api.list_networks();
        if (version !== sessionVersion) return;
        networks.value = result;
        if (!form.value.network_id && result.length === 1) form.value.network_id = result[0].network_id;
        await loadDestinations(form.value.network_id, form.value.device_id, form.value.lan_device_id);
    } catch (cause) {
        if (version === sessionVersion) { contextError.value = true; error.value = serviceError(cause, t('contextFailed')); }
    } finally {
        if (version === sessionVersion) loading.value = false;
    }
}
watch(() => props.visible, visible => {
    if (visible) initialize();
    else { sessionVersion++; destinationVersion++; }
}, { immediate: true });
onUnmounted(() => { sessionVersion++; destinationVersion++; });
function changeNetwork(networkId: string) { error.value = ''; loadDestinations(networkId); }
function changeNode() { form.value.lan_device_id = null; }
function close() { if (!saving.value) emit('update:visible', false); }
async function save() {
    if (busy.value || contextError.value) return;
    error.value = '';
    if (!form.value.name.trim()) { error.value = t('nameRequired'); return; }
    if (!form.value.network_id || !form.value.device_id || missingMember.value) { error.value = t('nodeRequired'); return; }
    const url = safeServiceUrl(form.value.url);
    if (!url) { error.value = t('urlInvalid'); return; }
    const input: ServiceInput = { ...form.value, name: form.value.name.trim(), description: form.value.description.trim(), group: form.value.group.trim(), url, sort_order: Number.isFinite(form.value.sort_order) ? Math.trunc(form.value.sort_order) : 0 };
    saving.value = true;
    try {
        const result = props.entry ? await props.api.update_service(props.entry.id, input) : await props.api.create_service(input);
        emit('saved', result);
        emit('update:visible', false);
    } catch (cause) { error.value = serviceError(cause, t('mutationFailed')); }
    finally { saving.value = false; }
}
</script>

<template>
    <Drawer :visible="visible" position="right" :header="entry ? t('edit') : t('add')" class="service-editor-drawer"
        :closable="!saving" :dismissable="!saving" :close-on-escape="!saving" @update:visible="!saving && emit('update:visible', $event)">
        <form class="service-editor-form" @submit.prevent="save">
            <Message v-if="error" severity="error" :closable="false" role="alert">{{ error }}</Message>
            <div v-if="loading" class="service-editor-loading"><i class="pi pi-spin pi-spinner" aria-hidden="true"></i>{{ t('loading') }}</div>
            <Message v-if="contextError" severity="warn" :closable="false"><span>{{ t('contextFailed') }}</span><Button :label="t('retry')" text size="small" @click="networks.length ? loadDestinations(form.network_id, form.device_id, form.lan_device_id) : initialize(false)" /></Message>
            <Message v-if="!loading && !contextError && networks.length === 0" severity="info" :closable="false">{{ t('noNetworks') }} <RouterLink :to="{ name: 'networkList' }" class="service-editor-link">{{ t('createNetwork') }}</RouterLink></Message>
            <div class="service-editor-field"><label for="service-name">{{ t('name') }}</label><InputText id="service-name" v-model="form.name" maxlength="80" required autofocus :disabled="saving" /></div>
            <div class="service-editor-field"><label for="service-description">{{ t('description') }}</label><Textarea id="service-description" v-model="form.description" maxlength="500" rows="2" auto-resize :disabled="saving" /></div>
            <div class="service-editor-field"><label for="service-network">{{ t('network') }}</label><Select :aria-label="t('network')" input-id="service-network" v-model="form.network_id" :options="networkOptions" option-label="label" option-value="value" :placeholder="t('networkPlaceholder')" :disabled="loading || saving" @update:model-value="changeNetwork" /></div>
            <div class="service-editor-field"><label for="service-node">{{ t('node') }}</label><Select :aria-label="t('node')" input-id="service-node" v-model="form.device_id" :options="memberOptions" option-label="label" option-value="value" :placeholder="t('nodePlaceholder')" :disabled="busy || !form.network_id" filter :loading="destinationsLoading" @update:model-value="changeNode" /><small>{{ t('nodeHint') }}</small></div>
            <Message v-if="missingMember" severity="warn" :closable="false">{{ t('selectedMissing') }}</Message>
            <Message v-if="form.network_id && !busy && !contextError && members.length === 0" severity="info" :closable="false">{{ t('noNodes') }}</Message>
            <div v-if="downstream.length" class="service-editor-field"><label for="service-device">{{ t('device') }}</label><Select :aria-label="t('device')" input-id="service-device" :placeholder="t('nodeItself')" v-model="form.lan_device_id" :options="deviceOptions" option-label="label" option-value="value" :disabled="busy" /><small>{{ t('deviceHint') }}</small></div>
            <div class="service-editor-field"><label for="service-url">{{ t('url') }}</label><InputText id="service-url" v-model="form.url" type="url" placeholder="http://10.144.0.1:8123" maxlength="2048" required :disabled="saving" aria-describedby="service-url-hint" /><small id="service-url-hint">{{ t('urlHint') }}</small></div>
            <div class="service-editor-pair">
                <div class="service-editor-field"><label for="service-group">{{ t('group') }}</label><InputText id="service-group" v-model="form.group" :placeholder="t('groupPlaceholder')" maxlength="64" :disabled="saving" /></div>
                <div class="service-editor-field"><label for="service-order">{{ t('order') }}</label><InputNumber input-id="service-order" v-model="form.sort_order" :min="-1000000" :max="1000000" :use-grouping="false" :disabled="saving" /><small>{{ t('orderHint') }}</small></div>
            </div>
            <div class="service-editor-field"><label for="service-icon">{{ t('icon') }}</label><Select :aria-label="t('icon')" input-id="service-icon" v-model="form.icon" :options="iconOptions" option-label="label" option-value="value" :disabled="saving"><template #value="slotProps"><span class="service-icon-option"><i :class="['pi', serviceIcons[slotProps.value as keyof typeof serviceIcons] || 'pi-globe']" aria-hidden="true"></i>{{ t(slotProps.value || 'globe') }}</span></template><template #option="slotProps"><span class="service-icon-option"><i :class="['pi', serviceIcons[slotProps.option.value as keyof typeof serviceIcons]]" aria-hidden="true"></i>{{ slotProps.option.label }}</span></template></Select></div>
            <div class="service-editor-toggles">
                <div class="service-editor-toggle"><div><label for="service-navigation">{{ t('show') }}</label><small id="service-navigation-hint">{{ t('showHint') }}</small></div><ToggleSwitch input-id="service-navigation" v-model="form.show_in_navigation" :disabled="saving" aria-describedby="service-navigation-hint" /></div>
                <div class="service-editor-toggle"><label for="service-new-tab">{{ t('newTab') }}</label><ToggleSwitch input-id="service-new-tab" v-model="form.open_in_new_tab" :disabled="saving" /></div>
            </div>
            <a v-if="previewUrl" :href="previewUrl" target="_blank" rel="noopener noreferrer" class="service-editor-preview"><i class="pi pi-external-link" aria-hidden="true"></i>{{ t('preview') }}</a>
            <footer class="service-editor-footer"><Button type="button" :label="t('cancel')" outlined severity="secondary" :disabled="saving" @click="close" /><Button type="submit" :label="t('save')" icon="pi pi-check" :loading="saving" :disabled="loading || destinationsLoading || contextError || !networks.length || missingMember" /></footer>
        </form>
    </Drawer>
</template>

<style scoped>
:global(.service-editor-drawer.p-drawer) { width: min(100vw, 480px); }
:global(.service-editor-drawer .p-drawer-header) { border-bottom: 1px solid var(--p-content-border-color); padding: 22px 24px; }
:global(.service-editor-drawer .p-drawer-content) { padding: 24px; }
.service-editor-form { display: flex; flex-direction: column; gap: 22px; }
.service-editor-field { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.service-editor-field > label, .service-editor-toggle label { font-size: 13px; font-weight: 550; }
.service-editor-field > small, .service-editor-toggle small { color: var(--p-text-muted-color); font-size: 12px; line-height: 1.6; }
.service-editor-pair { display: grid; grid-template-columns: 1.6fr 1fr; gap: 16px; }
.service-editor-pair :deep(.p-inputnumber-input) { width: 100%; }
.service-icon-option { display: inline-flex; align-items: center; gap: 10px; }
.service-editor-toggles { border-top: 1px solid var(--p-content-border-color); padding-top: 22px; display: flex; flex-direction: column; gap: 24px; }
.service-editor-toggle { display: flex; align-items: center; justify-content: space-between; gap: 18px; }
.service-editor-toggle > div:first-child { display: flex; flex-direction: column; gap: 7px; max-width: 300px; }
.service-editor-preview, .service-editor-link { display: inline-flex; align-items: center; gap: 9px; color: var(--p-primary-color); font-size: 13px; align-self: flex-start; }
.service-editor-footer { display: flex; justify-content: flex-end; gap: 10px; border-top: 1px solid var(--p-content-border-color); padding-top: 20px; margin-top: 4px; }
.service-editor-loading { display: flex; align-items: center; gap: 9px; color: var(--p-text-muted-color); font-size: 13px; }
@media (max-width: 480px) { :global(.service-editor-drawer .p-drawer-content) { padding: 20px 16px; } .service-editor-pair { grid-template-columns: 1fr; } }
</style>
