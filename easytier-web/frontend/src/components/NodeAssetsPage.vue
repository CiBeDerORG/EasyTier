<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { Button, Checkbox, Dialog, InputText, Message, Select, Skeleton } from 'primevue';
import { useI18n } from 'vue-i18n';
import { onBeforeRouteLeave, useRouter } from 'vue-router';
import ApiClient from '../modules/api';
import type { AssetDevice, AssetNode, AssetSavedResource } from '../types/node-assets';
import { useNodeAssetsPage } from '../composables/useNodeAssetsPage';
import GatewayGroupDialog from './node-assets/GatewayGroupDialog.vue';
import DeviceTypeDialog from './node-assets/DeviceTypeDialog.vue';
import AssetDeviceTable from './node-assets/AssetDeviceTable.vue';
import AssetDeviceDialog from './node-assets/AssetDeviceDialog.vue';
import AssetServicePanel from './node-assets/AssetServicePanel.vue';

const props = defineProps<{ api: ApiClient }>();
const { t } = useI18n();
const router = useRouter();
const { context, accounts, ownerId, nodeId, ownerName, nodes, selectedNode, groups, types, rows,
    total, page, pageSize, expandedIds, draft, contextVersion, booting, loading, error, invalidNode, scope,
    listError, hasLoadedDevices, retry, refreshPage, selectOwner, selectNode, query, loadDevices, refreshCatalogs, refreshNodes,
    afterDeviceSaved, afterDeviceDeleted, expand, handleContextFailure } = useNodeAssetsPage(props.api);
const showGroups = ref(false), showTypes = ref(false), showDevice = ref(false), showNode = ref(false);
const pageElement = ref<HTMLElement | null>(null);
const gatewayFocusId = ref<string | null>(null);
const deviceInitialMode = ref<'metadata' | 'binding' | 'address'>('metadata');
const editingDevice = ref<AssetDevice | null>(null);
const deleteDevice = ref<AssetDevice | null>(null), deleteNode = ref<AssetNode | null>(null);
const deleting = ref(false), actionError = ref(''), nodeSaving = ref(false);
const nodeLocation = ref(''), nodeModel = ref(''), nodePublic = ref(false);
const nodeFieldError = (value: string, limit: number) => {
    if (/[\u0000-\u001f\u007f-\u009f]/u.test(value)) return t('web.device.attributes_invalid_control');
    if (Array.from(value.trim()).length > limit) return t('web.device.attributes_too_long', { limit });
    return '';
};
const nodeLocationError = computed(() => nodeFieldError(nodeLocation.value, 256));
const nodeModelError = computed(() => nodeFieldError(nodeModel.value, 128));
const serviceRequests = ref<Record<string, number>>({});
let nodeDialogVersion = 0, deleteDialogVersion = 0;
watch(showNode, () => { ++nodeDialogVersion; nodeSaving.value = false; actionError.value = ''; }, { flush: 'sync' });
watch([deleteDevice, deleteNode], () => { ++deleteDialogVersion; deleting.value = false; actionError.value = ''; }, { flush: 'sync' });
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
    showGroups.value = false; showTypes.value = false; showDevice.value = false; showNode.value = false;
    deleteDevice.value = null; deleteNode.value = null; editingDevice.value = null;
    gatewayFocusId.value = null; deviceInitialMode.value = 'metadata';
    serviceRequests.value = {}; actionError.value = ''; deleting.value = false; nodeSaving.value = false;
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
function editNode() {
    const node = selectedNode.value;
    if (!node) return;
    nodeLocation.value = node.site_location ?? ''; nodeModel.value = node.device_model ?? '';
    nodePublic.value = node.is_public_server; actionError.value = ''; showNode.value = true;
}
async function saveNode() {
    if (!scope.value || nodeSaving.value || nodeLocationError.value || nodeModelError.value) return;
    const version = contextVersion.value, dialogVersion = nodeDialogVersion;
    const current = { ...scope.value };
    const active = () => version === contextVersion.value && dialogVersion === nodeDialogVersion;
    nodeSaving.value = true; actionError.value = '';
    try {
        await props.api.assetsUpdateNode(current, { is_public_server: nodePublic.value,
            site_location: nodeLocation.value.trim() || null, device_model: nodeModel.value.trim() || null });
        if (!active()) return;
        await refreshNodes();
        if (active()) showNode.value = false;
    } catch (cause) {
        if (active() && !await handleContextFailure(cause) && active()) actionError.value = errorMessage(cause);
    }
    finally { if (active()) nodeSaving.value = false; }
}
async function confirmDelete() {
    if (!scope.value || deleting.value) return;
    const current = { ...scope.value }, version = contextVersion.value, dialogVersion = deleteDialogVersion;
    const deviceId = deleteDevice.value?.id, node = deleteNode.value;
    const active = () => version === contextVersion.value && dialogVersion === deleteDialogVersion;
    deleting.value = true; actionError.value = '';
    try {
        if (deviceId) {
            await props.api.assetsDeleteDevice(current, deviceId);
            if (!active()) return;
            await afterDeviceDeleted();
            if (active()) deleteDevice.value = null;
        } else if (node) {
            await props.api.assetsDeleteNode(current);
            if (!active()) return;
            deleteNode.value = null; await selectNode(null);
        }
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
                <div v-if="selectedNode" class="assets-node-summary">
                    <div><strong>{{ selectedNode.alias || selectedNode.hostname }}</strong>
                        <span v-if="selectedNode.is_public_server" class="assets-public-tag"><i class="pi pi-cloud" aria-hidden="true"></i> {{ t('assets.page.public_node') }}</span>
                        <p class="muted">{{ selectedNode.site_location || t('assets.page.no_location') }} · {{ selectedNode.device_model || t('assets.page.no_model') }}</p>
                        <code>{{ selectedNode.machine_id }}</code>
                    </div>
                    <div class="assets-actions">
                        <Button icon="pi pi-pencil" :label="t('assets.page.edit_node')" text size="small" @click="editNode" />
                        <Button icon="pi pi-trash" :label="t('assets.page.delete_node')" text severity="danger" size="small" @click="deleteNode = selectedNode; actionError = ''" />
                    </div>
                </div>
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
                    :groups="groups" :types="types" @page="loadDevices" @expand="expand" @edit="editDevice"
                    @delete="deleteDevice = $event; actionError = ''" @add-service="addService" @binding="openDeviceBinding" @address="editDevice($event, 'address')">
                    <template #services="{ device }"><AssetServicePanel :key="`${scope.owner_user_id}/${scope.machine_id}/${device.id}`"
                        :api="api" :scope="scope" :device="device" :add-request="serviceRequests[device.id] ?? 0"
                        @add-request-consumed="serviceRequests[device.id] = 0" @context-error="handleContextFailure" /></template>
                </AssetDeviceTable>
            </section>
            <div v-else-if="!booting && context?.available" class="console-empty-state"><i class="pi pi-server" aria-hidden="true"></i><p>{{ t('assets.page.choose_node') }}</p></div>
        </template>

        <template v-if="scope">
            <GatewayGroupDialog v-if="showGroups" :visible="true" :api="api" :scope="scope" :focus-id="gatewayFocusId" :owner-label="ownerName" :node-label="selectedNode?.alias || selectedNode?.hostname || nodeId || ''" @close="showGroups = false" @changed="refreshCatalogs" @context-error="handleContextFailure" />
            <DeviceTypeDialog v-if="showTypes" :visible="true" :api="api" :owner-user-id="scope.owner_user_id" :owner-label="ownerName" @close="showTypes = false" @changed="refreshCatalogs" @context-error="handleContextFailure" />
            <AssetDeviceDialog v-if="showDevice" :visible="true" :api="api" :scope="scope" :groups="groups" :types="types" :device="editingDevice" :initial-mode="deviceInitialMode"
                @close="showDevice = false; editingDevice = null" @saved="savedDevice" @context-error="handleContextFailure" />
        </template>
        <Dialog v-model:visible="showNode" modal :header="t('assets.page.edit_node')" :style="{ width: '30rem', maxWidth: 'calc(100vw - 2rem)' }">
            <form id="asset-node-metadata" class="assets-node-form" @submit.prevent="saveNode">
                <p class="muted">{{ selectedNode?.alias || selectedNode?.hostname }} · {{ ownerName }}</p>
                <div class="assets-field"><label for="asset-node-location">{{ t('assets.page.location') }}</label>
                    <InputText id="asset-node-location" v-model="nodeLocation" :disabled="nodeSaving" :invalid="!!nodeLocationError" :aria-invalid="!!nodeLocationError" aria-describedby="asset-node-location-error" />
                    <small v-if="nodeLocationError" id="asset-node-location-error" class="assets-field-error" role="alert">{{ nodeLocationError }}</small></div>
                <div class="assets-field"><label for="asset-node-model">{{ t('assets.page.model') }}</label>
                    <InputText id="asset-node-model" v-model="nodeModel" :disabled="nodeSaving" :invalid="!!nodeModelError" :aria-invalid="!!nodeModelError" aria-describedby="asset-node-model-error" />
                    <small v-if="nodeModelError" id="asset-node-model-error" class="assets-field-error" role="alert">{{ nodeModelError }}</small></div>
                <div class="assets-actions"><Checkbox inputId="asset-public-node" v-model="nodePublic" binary :disabled="nodeSaving" /><label for="asset-public-node">{{ t('assets.page.public_node') }}</label></div>
                <p class="muted">{{ t('assets.page.public_hint') }}</p>
                <Message v-if="actionError" severity="error" :closable="false">{{ actionError }}</Message>
            </form>
            <template #footer><Button :label="t('assets.page.cancel')" severity="secondary" text @click="showNode = false" /><Button type="submit" form="asset-node-metadata" :label="t('assets.page.save')" :loading="nodeSaving" :disabled="!!nodeLocationError || !!nodeModelError" /></template>
        </Dialog>
        <Dialog :visible="!!deleteDevice || !!deleteNode" modal :header="t('assets.page.confirm_delete')" :style="{ width: '29rem', maxWidth: 'calc(100vw - 2rem)' }"
            @update:visible="value => { if (!value) { deleteDevice = null; deleteNode = null; } }">
            <p>{{ deleteDevice ? t('assets.page.delete_device_hint', { name: deleteDevice.name }) : t('assets.page.delete_node_hint', { name: deleteNode?.alias || deleteNode?.hostname }) }}</p>
            <Message v-if="actionError" severity="error" :closable="false">{{ actionError }}</Message>
            <template #footer><Button :label="t('assets.page.cancel')" severity="secondary" text @click="deleteDevice = null; deleteNode = null" /><Button :label="t('assets.page.delete')" severity="danger" :loading="deleting" @click="confirmDelete" /></template>
        </Dialog>
    </div>
</template>

<style scoped>
.node-assets-page { display: grid; grid-template-columns: minmax(0, 1fr); gap: 1rem; min-width: 0; width: 100%; max-width: 100%; }
.node-assets-page > * { min-width: 0; max-width: 100%; }
.assets-heading,.assets-list-heading,.assets-node-summary { display: flex; justify-content: space-between; gap: 1rem; align-items: center; }
.assets-heading > div,.assets-node-summary > div { min-width: 0; overflow-wrap: anywhere; }
.assets-heading h1 { margin: 0; font-size: 1.3rem; }.assets-heading p { margin: .35rem 0 0; }
.assets-filter-card,.assets-list-card { min-width: 0; max-width: 100%; border: 1px solid var(--console-border, var(--p-content-border-color)); border-radius: .75rem; padding: 1rem; background: var(--console-surface, var(--p-content-background)); }
.assets-context-grid { display: grid; grid-template-columns: minmax(0,1fr) minmax(0,2fr); gap: 1rem; }.assets-node-field:last-child:first-child { grid-column: 1 / -1; }
.assets-field { display: flex; flex-direction: column; gap: .4rem; min-width: 0; }.assets-field label { font-size: .875rem; font-weight: 600; }
.assets-owner-context { margin-top: .65rem; font-size: .85rem; color: var(--console-muted); }
.assets-node-summary { padding: .9rem 0; border-bottom: 1px solid var(--console-border, var(--p-content-border-color)); }.assets-node-summary p { margin: .4rem 0; font-size: .875rem; }.assets-node-summary code { overflow-wrap: anywhere; font-size: .75rem; }
.assets-public-tag { display: inline-flex; gap: .3rem; margin-left: .6rem; color: var(--p-primary-color); font-size: .8rem; }
.assets-query-grid { display: grid; grid-template-columns: minmax(0,1fr) minmax(0,1fr) minmax(0,1.3fr) auto; align-items: end; gap: .8rem; margin-top: 1rem; }
.assets-input-action,.assets-actions { display: flex; align-items: center; gap: .45rem; min-width: 0; }.assets-input-action > :first-child { flex: 1; min-width: 0; }
.assets-list-heading { margin-bottom: .75rem; flex-wrap: wrap; }.assets-list-heading h2 { font-size: 1rem; margin: 0; }.assets-list-heading h2 span { margin-left: .4rem; font-weight: 400; }
.assets-node-form,.assets-loading { display: grid; gap: 1rem; }.assets-alert { overflow-wrap: anywhere; }
.assets-field-error { color: var(--p-red-600, #dc2626); }
@media(max-width: 1200px) { .assets-query-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media(max-width: 600px) { .assets-context-grid,.assets-query-grid { grid-template-columns: minmax(0, 1fr); }.assets-heading,.assets-node-summary { align-items: flex-start; flex-direction: column; }.assets-filter-card,.assets-list-card { padding: .8rem; }.assets-actions { flex-wrap: wrap; } }
</style>
