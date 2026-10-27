<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { Button, Dialog, InputText, Message, Select, Skeleton } from 'primevue';
import { useI18n } from 'vue-i18n';
import { onBeforeRouteLeave, useRouter } from 'vue-router';
import ApiClient from '../modules/api';
import type { AssetDevice, AssetSavedResource } from '../types/node-assets';
import { useNodeAssetsPage } from '../composables/useNodeAssetsPage';
import GatewayGroupDialog from './node-assets/GatewayGroupDialog.vue';
import DeviceTypeDialog from './node-assets/DeviceTypeDialog.vue';
import DiscoveryDialog from './node-assets/DiscoveryDialog.vue';
import AssetDeviceTable from './node-assets/AssetDeviceTable.vue';
import AssetDeviceDialog from './node-assets/AssetDeviceDialog.vue';
import ServerAgentEntryDialog from './node-assets/ServerAgentEntryDialog.vue';
import AssetServicePanel from './node-assets/AssetServicePanel.vue';

const props = defineProps<{ api: ApiClient }>();
const { t } = useI18n();
const router = useRouter();
const { context, accounts, ownerId, nodeId, ownerName, nodes, selectedNode, groups, types, rows,
    total, page, pageSize, expandedIds, draft, contextVersion, booting, loading, error, invalidNode, invalidDevice, scope,
    listError, hasLoadedDevices, retry, refreshPage, selectOwner, selectNode, query, loadDevices, refreshCatalogs,
    afterDeviceSaved, afterDeviceDeleted, expand, handleContextFailure } = useNodeAssetsPage(props.api);
const discovery = ref<{ device: AssetDevice; action: 'devices' | 'adb' } | null>(null);
const showGroups = ref(false), showTypes = ref(false), showDevice = ref(false);
const pageElement = ref<HTMLElement | null>(null);
const gatewayFocusId = ref<string | null>(null);
const deviceInitialMode = ref<'metadata' | 'binding' | 'address'>('metadata');
const editingDevice = ref<AssetDevice | null>(null);
const agentEntryDevice = ref<AssetDevice | null>(null);
const deleteDevice = ref<AssetDevice | null>(null);
const deleting = ref(false), actionError = ref('');
const serviceRequests = ref<Record<string, number>>({});
let deleteDialogVersion = 0;
watch(deleteDevice, () => { ++deleteDialogVersion; deleting.value = false; actionError.value = ''; }, { flush: 'sync' });
const errorMessage = (cause: unknown) => (cause as { response?: { data?: { message?: string } }; message?: string })?.response?.data?.message
    || (cause as { message?: string })?.message || t('assets.page.operation_failed');
const nodeOptions = computed(() => nodes.value.map(node => ({ ...node,
    label: `${node.alias || node.hostname || node.machine_id} · ${node.machine_id.slice(0, 8)}` })));
const groupOptions = computed(() => [{ id: null, name: t('assets.page.all_devices') }, ...groups.value.map(group => ({
    id: group.id,
    name: [group.name, group.id.slice(0, 8), group.binding.snapshot?.display_name,
        group.binding.instance_id?.slice(0, 8), group.subnets.join(', ')].filter(Boolean).join(' · '),
}))]);
const typeOptions = computed(() => [{ id: null, name: t('assets.page.all_types') },
    ...types.value.map(type => ({ id: type.id, name: `${type.name} · ${type.id.slice(0, 8)}` }))]);
watch(contextVersion, () => {
    discovery.value = null; showGroups.value = false; showTypes.value = false; showDevice.value = false;
    deleteDevice.value = null; editingDevice.value = null;
    agentEntryDevice.value = null;
    gatewayFocusId.value = null; deviceInitialMode.value = 'metadata';
    serviceRequests.value = {}; actionError.value = ''; deleting.value = false;
});
function editDevice(device: AssetDevice | null, mode: 'metadata' | 'binding' | 'address' = 'metadata') {
    editingDevice.value = device; deviceInitialMode.value = mode; showDevice.value = true;
}
function openDeviceBinding(device: AssetDevice) {
    editDevice(device, device.gateway_group_id ? 'address' : 'binding');
}
async function savedDevice(result: AssetSavedResource) {
    const current = scope.value, version = contextVersion.value;
    if (!current || result.record.owner_user_id !== current.owner_user_id || result.record.machine_id !== current.machine_id) return;
    const created = !editingDevice.value;
    showDevice.value = false; editingDevice.value = null;
    if (result.resource_kind === 'gateway') {
        await refreshCatalogs();
        if (version !== contextVersion.value) return;
        // A gateway has its own management result; it is never a device row.
        gatewayFocusId.value = result.record.id;
        showGroups.value = true;
    } else {
        await afterDeviceSaved(result.record, created);
    }
}
function scrollAncestors() {
    const ancestors: HTMLElement[] = [];
    for (let element = pageElement.value?.parentElement; element; element = element.parentElement) ancestors.push(element);
    return ancestors;
}
function resetScroll(ancestors: HTMLElement[]) {
    for (const element of ancestors) {
        if (element.isConnected) { element.scrollTop = 0; element.scrollLeft = 0; }
    }
    window.scrollTo(0, 0);
}
onBeforeRouteLeave(to => {
    // This also covers the sidebar and browser-back path to the node list.
    if (to.name === 'deviceList') resetScroll(scrollAncestors());
});
async function backToNodes() {
    const ancestors = scrollAncestors();
    const failure = await router.push({ name: 'deviceList' });
    if (failure) return;
    await nextTick();
    resetScroll(ancestors);
}
function addService(device: AssetDevice) {
    expand({ id: device.id, expanded: true });
    serviceRequests.value = { ...serviceRequests.value, [device.id]: (serviceRequests.value[device.id] ?? 0) + 1 };
}
async function confirmDelete() {
    if (!scope.value || !deleteDevice.value || deleting.value) return;
    const current = { ...scope.value }, version = contextVersion.value, dialogVersion = deleteDialogVersion;
    const deviceId = deleteDevice.value.id;
    const active = () => version === contextVersion.value && dialogVersion === deleteDialogVersion;
    deleting.value = true; actionError.value = '';
    try {
        await props.api.assetsDeleteDevice(current, deviceId);
        if (!active()) return;
        await afterDeviceDeleted();
        if (active()) deleteDevice.value = null;
    } catch (cause) {
        if (active() && !await handleContextFailure(cause) && active()) actionError.value = errorMessage(cause);
    }
    finally { if (active()) deleting.value = false; }
}
</script>

<template>
    <div ref="pageElement" class="node-assets-page console-page">
        <header class="assets-heading">
            <div><h1>{{ t('assets.page.title') }}</h1><p class="muted">{{ t('assets.page.subtitle') }}</p></div>
            <Button icon="pi pi-arrow-left" :label="t('assets.page.back')" text severity="secondary" @click="backToNodes" />
        </header>
        <Message v-if="context && !context.available" severity="info" :closable="false">{{ t('assets.page.unavailable') }}</Message>
        <template v-else>
            <Message v-if="error" severity="error" :closable="false" class="assets-alert">
                {{ error }} <Button :label="t('assets.page.retry')" size="small" text @click="retry" />
            </Message>
            <Message v-if="invalidNode" severity="warn" :closable="false">{{ t('assets.page.invalid_node') }}</Message>
            <Message v-if="invalidDevice" severity="warn" :closable="false">{{ t('assets.page.invalid_device') }}</Message>
            <section v-if="context" class="assets-filter-card">
                <div class="assets-context-grid">
                    <div v-if="context.can_manage_others" class="assets-field">
                        <label for="asset-account">{{ t('assets.page.account') }} *</label>
                        <Select inputId="asset-account" :modelValue="ownerId" :options="accounts" optionLabel="username" optionValue="id"
                            :aria-label="t('assets.page.account')" :placeholder="t('assets.page.select_account')" :disabled="booting" @update:modelValue="selectOwner" />
                    </div>
                    <div class="assets-field assets-node-field">
                        <label for="asset-node">{{ t('assets.page.node') }} *</label>
                        <Select inputId="asset-node" :modelValue="nodeId" :options="nodeOptions" optionLabel="label" optionValue="machine_id" filter
                            :aria-label="t('assets.page.node')" :placeholder="t('assets.page.select_node')" :disabled="ownerId === null || booting" @update:modelValue="selectNode" />
                    </div>
                </div>
                <div v-if="ownerName" class="assets-owner-context"><i class="pi pi-user" aria-hidden="true"></i> {{ t('assets.page.managing', { account: ownerName }) }}</div>
                <form v-if="scope" class="assets-query-grid" @submit.prevent="query">
                    <div class="assets-field"><label for="asset-group">{{ t('assets.page.group') }}</label>
                        <div class="assets-input-action"><Select inputId="asset-group" v-model="draft.group_id" :options="groupOptions" optionLabel="name" optionValue="id" :aria-label="t('assets.page.group')" :placeholder="t('assets.page.all_devices')" />
                            <Button icon="pi pi-cog" :label="t('assets.page.manage_groups')" size="small" :aria-label="t('assets.page.manage_groups')" :title="t('assets.page.manage_groups')" outlined severity="secondary" @click="gatewayFocusId = null; showGroups = true" /></div>
                    </div>
                    <div class="assets-field"><label for="asset-type">{{ t('assets.page.type') }}</label>
                        <div class="assets-input-action"><Select inputId="asset-type" v-model="draft.type_id" :options="typeOptions" optionLabel="name" optionValue="id" :aria-label="t('assets.page.type')" :placeholder="t('assets.page.all_types')" />
                            <Button icon="pi pi-cog" :label="t('assets.page.manage_types')" size="small" :aria-label="t('assets.page.manage_types')" :title="t('assets.page.manage_types')" outlined severity="secondary" @click="showTypes = true" /></div>
                    </div>
                    <div class="assets-field"><label for="asset-keyword">{{ t('assets.page.keyword') }}</label>
                        <InputText inputId="asset-keyword" id="asset-keyword" v-model="draft.keyword" :placeholder="t('assets.page.keyword_hint')" /></div>
                    <Button type="submit" icon="pi pi-search" :label="t('assets.page.query')" :loading="loading" />
                </form>
            </section>
            <div v-if="booting" class="assets-loading"><Skeleton height="5rem" /><Skeleton height="12rem" /></div>
            <section v-else-if="scope" class="assets-list-card">
                <header class="assets-list-heading"><h2>{{ t('assets.page.devices') }} <span class="muted">{{ hasLoadedDevices ? total : '—' }}</span></h2>
                    <div class="assets-actions"><Button icon="pi pi-refresh" text severity="secondary" :aria-label="t('assets.page.refresh')" :loading="loading" @click="refreshPage" />
                        <Button icon="pi pi-plus" :label="t('assets.page.add_device')" @click="editDevice(null)" /></div>
                </header>
                <Message v-if="listError && hasLoadedDevices" severity="warn" :closable="false">{{ t('assets.page.stale_results') }}</Message>
                <AssetDeviceTable v-if="!listError || hasLoadedDevices" :rows="rows" :loading="loading" :total="total" :page="page" :page-size="pageSize" :expanded-ids="expandedIds"
                    :groups="groups" :types="types" @discovery="(device, action) => discovery = { device, action }" @page="loadDevices" @expand="expand" @edit="editDevice"
                    @delete="deleteDevice = $event; actionError = ''" @add-service="addService" @agent="agentEntryDevice = $event" @binding="openDeviceBinding" @address="editDevice($event, 'address')">
                    <template #services="{ device }"><AssetServicePanel :key="`${scope.owner_user_id}/${scope.machine_id}/${device.id}`"
                        :api="api" :scope="scope" :device="device" :add-request="serviceRequests[device.id] ?? 0"
                        @add-request-consumed="serviceRequests[device.id] = 0" @context-error="handleContextFailure" /></template>
                </AssetDeviceTable>
            </section>
            <div v-else-if="!booting && context?.available" class="console-empty-state"><i class="pi pi-server" aria-hidden="true"></i><p>{{ t('assets.page.choose_node') }}</p></div>
        </template>

        <template v-if="scope">
            <ServerAgentEntryDialog v-if="agentEntryDevice" :key="agentEntryDevice.id" :api="api"
                :scope="scope" :device="agentEntryDevice" @close="agentEntryDevice = null" />
            <DiscoveryDialog v-if="discovery" :key="discovery.device.id" :gateway="discovery.device" :owner-id="scope.owner_user_id" :initial-action="discovery.action" @close="discovery = null" @changed="refreshPage" />
            <GatewayGroupDialog v-if="showGroups" :visible="true" :api="api" :scope="scope" :focus-id="gatewayFocusId" :owner-label="ownerName" :node-label="selectedNode?.alias || selectedNode?.hostname || nodeId || ''" @close="showGroups = false" @changed="refreshCatalogs" @context-error="handleContextFailure" />
            <DeviceTypeDialog v-if="showTypes" :visible="true" :api="api" :owner-user-id="scope.owner_user_id" :owner-label="ownerName" @close="showTypes = false" @changed="refreshCatalogs" @context-error="handleContextFailure" />
            <AssetDeviceDialog v-if="showDevice" :visible="true" :api="api" :scope="scope" :groups="groups" :types="types" :device="editingDevice" :initial-mode="deviceInitialMode"
                @close="showDevice = false; editingDevice = null" @saved="savedDevice" @context-error="handleContextFailure" />
        </template>
        <Dialog :visible="!!deleteDevice" modal :header="t('assets.page.confirm_delete')" :style="{ width: '29rem', maxWidth: 'calc(100vw - 2rem)' }"
            :closable="!deleting" :closeOnEscape="!deleting" :dismissableMask="!deleting"
            @update:visible="value => { if (!value && !deleting) deleteDevice = null; }">
            <p>{{ t('assets.page.delete_device_hint', { name: deleteDevice?.name || '' }) }}</p>
            <Message v-if="actionError" severity="error" :closable="false">{{ actionError }}</Message>
            <template #footer><Button :label="t('assets.page.cancel')" severity="secondary" text :disabled="deleting" @click="deleteDevice = null" /><Button :label="t('assets.page.delete')" severity="danger" :loading="deleting" @click="confirmDelete" /></template>
        </Dialog>
    </div>
</template>

<style scoped>
.node-assets-page { display: grid; grid-template-columns: minmax(0, 1fr); gap: 1rem; min-width: 0; width: 100%; max-width: 100%; }
.node-assets-page > * { min-width: 0; max-width: 100%; }
.assets-heading,.assets-list-heading { display: flex; justify-content: space-between; gap: 1rem; align-items: center; }
.assets-heading > div { min-width: 0; overflow-wrap: anywhere; }
.assets-heading h1 { margin: 0; font-size: 1.3rem; }.assets-heading p { margin: .35rem 0 0; }
.assets-filter-card,.assets-list-card { min-width: 0; max-width: 100%; border: 1px solid var(--console-border, var(--p-content-border-color)); border-radius: .75rem; padding: 1rem; background: var(--console-surface, var(--p-content-background)); }
.assets-context-grid { display: grid; grid-template-columns: minmax(0,1fr) minmax(0,2fr); gap: 1rem; }.assets-node-field:last-child:first-child { grid-column: 1 / -1; }
.assets-field { display: flex; flex-direction: column; gap: .4rem; min-width: 0; }.assets-field label { font-size: .875rem; font-weight: 600; }
.assets-owner-context { margin-top: .65rem; font-size: .85rem; color: var(--console-muted); }
.assets-query-grid { display: grid; grid-template-columns: minmax(0,1fr) minmax(0,1fr) minmax(0,1.3fr) auto; align-items: end; gap: .8rem; margin-top: 1rem; }
.assets-input-action,.assets-actions { display: flex; align-items: center; gap: .45rem; min-width: 0; }.assets-input-action > :first-child { flex: 1; min-width: 0; }
.assets-list-heading { margin-bottom: .75rem; flex-wrap: wrap; }.assets-list-heading h2 { font-size: 1rem; margin: 0; }.assets-list-heading h2 span { margin-left: .4rem; font-weight: 400; }
.assets-loading { display: grid; gap: 1rem; }.assets-alert { overflow-wrap: anywhere; }
@media(max-width: 1200px) { .assets-query-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media(max-width: 600px) { .assets-context-grid,.assets-query-grid { grid-template-columns: minmax(0, 1fr); }.assets-heading { align-items: flex-start; flex-direction: column; }.assets-filter-card,.assets-list-card { padding: .8rem; }.assets-actions { flex-wrap: wrap; } }
</style>
