<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Button, ConfirmDialog, InputText, Menu, Message, Select, Skeleton, Tag, useConfirm, useToast } from 'primevue';
import type ApiClient from '../modules/api';
import type { AssetDevice, AssetService, AssetServiceCatalogEntry } from '../types/node-assets';
import { serviceMessages } from '../modules/service-messages';
import { safeServiceUrl, serviceError } from '../modules/service-utils';
import { assetServiceScope, matchesAssetService, sortAssetServices, useServiceCatalog } from '../modules/service-catalog';
import { assetServiceIconClass } from '../constants/asset-service-icons';
import { publicationFeedback } from '../modules/publication-sync';
import AssetServiceDialog from './node-assets/AssetServiceDialog.vue';

const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t, locale } = useI18n({ useScope: 'local', messages: serviceMessages });
const route = useRoute();
const router = useRouter();
const confirm = useConfirm();
const toast = useToast();
const { context, accounts, ownerId, ownerQuery, services, loading, error, available, load: loadCatalog, selectOwner, isCurrentOwner } = useServiceCatalog(props.api, () => t('loadFailed'));
const search = ref('');
const selectedNode = ref<string | null>(null);
const selectedDevice = ref<string | null>(null);
const updatingId = ref<string | null>(null);
const lastSaved = ref<AssetService>();
const editing = ref<AssetServiceCatalogEntry | null>(null);
const editingDevice = ref<AssetDevice | null>(null);
const editorVisible = ref(false);
const menu = ref<InstanceType<typeof Menu>>();
const actionService = ref<AssetServiceCatalogEntry>();
let editGeneration = 0;
let actionGeneration = 0;
let disposed = false;
let handledQuery = '';
const busy = computed(() => loading.value || !!updatingId.value);
const navigationCount = computed(() => (services.value ?? []).filter(service => service.show_in_navigation).length);
const nodeOptions = computed(() => [{ label: t('allNodes'), value: null }, ...[...new Map((services.value ?? []).map(service => [service.machine_id, service.node_name])).entries()].map(([value, label]) => ({ value, label }))]);
const deviceOptions = computed(() => [{ label: t('allDevices'), value: null }, ...[...new Map((services.value ?? []).filter(service => !selectedNode.value || service.machine_id === selectedNode.value).map(service => [service.device_id, `${service.device_name} · ${service.node_name}`])).entries()].map(([value, label]) => ({ value, label }))]);
const filtered = computed(() => sortAssetServices((services.value ?? []).filter(service =>
    (!selectedNode.value || service.machine_id === selectedNode.value) && (!selectedDevice.value || service.device_id === selectedDevice.value)
    && matchesAssetService(service, search.value))));
const menuItems = computed(() => [
    { label: t('edit'), icon: 'pi pi-pencil', disabled: busy.value, command: () => actionService.value && openEdit(actionService.value) },
    { label: t('locateDevice'), icon: 'pi pi-desktop', command: () => actionService.value && openDevice(actionService.value) },
    ...(actionService.value?.owner_user_id === context.value?.actor.id ? [{ label: t('locate'), icon: 'pi pi-map-marker', command: () => {
        const service = actionService.value;
        if (service) void router.push({ name: 'networkMap', query: { node: service.machine_id, device: service.device_id } });
    } }] : []),
    { separator: true },
    { label: t('delete'), icon: 'pi pi-trash', disabled: busy.value, command: () => actionService.value && deleteService(actionService.value) },
]);
const load = () => loadCatalog();
async function recoverAccess(cause: unknown) {
    const status = (cause as { response?: { status?: number } })?.response?.status;
    if (status === 401 || status === 403) await loadCatalog(true);
}
function queryValue(key: string): string | undefined { const value = route.query[key]; return typeof value === 'string' ? value : undefined; }
function openDevice(service?: AssetServiceCatalogEntry) {
    void router.push({ name: 'nodeAssets', query: { ...ownerQuery.value, ...(service ? { owner: String(service.owner_user_id) } : {}), node: service?.machine_id || selectedNode.value || undefined, device: service?.device_id || selectedDevice.value || undefined } });
}
function queryAction() {
    if (!available.value || services.value === undefined || loading.value || editorVisible.value || updatingId.value) return;
    const signature = JSON.stringify(route.query);
    if (signature === handledQuery) return;
    const editId = queryValue('edit');
    if (editId) {
        handledQuery = signature;
        const entry = services.value.find(service => service.id === editId);
        if (entry) void openEdit(entry);
        else error.value = t('serviceMissing');
    } else if (queryValue('create')) { handledQuery = signature; openDevice(); }
    else handledQuery = '';
}
watch(() => [route.query.node, route.query.device], () => { selectedNode.value = queryValue('node') || null; selectedDevice.value = queryValue('device') || null; }, { immediate: true });
watch([services, loading, () => route.query], queryAction);
watch(ownerId, () => { editGeneration++; actionGeneration++; editing.value = null; editingDevice.value = null; editorVisible.value = false; updatingId.value = null; lastSaved.value = undefined; handledQuery = ''; actionService.value = undefined; menu.value?.hide(); confirm.close(); });
onBeforeUnmount(() => { disposed = true; editGeneration++; actionGeneration++; confirm.close(); });
function closeEditor() {
    editGeneration++;
    editorVisible.value = false;
    editing.value = null;
    editingDevice.value = null;
    handledQuery = '';
    if (route.query.edit) {
        const query = { ...route.query }; delete query.edit;
        void router.replace({ query });
    }
}
async function openEdit(service: AssetServiceCatalogEntry) {
    if (busy.value || !isCurrentOwner(service.owner_user_id)) return;
    const current = ++editGeneration;
    updatingId.value = service.id;
    error.value = '';
    try {
        const device = await props.api.assetsDevice(assetServiceScope(service), service.device_id);
        if (disposed || current !== editGeneration || !isCurrentOwner(service.owner_user_id)) return;
        if (device.id !== service.device_id || device.owner_user_id !== service.owner_user_id || device.machine_id !== service.machine_id) throw new Error(t('serviceMissing'));
        editing.value = service;
        editingDevice.value = device;
        editorVisible.value = true;
    } catch (cause) {
        if (!disposed && current === editGeneration) {
            error.value = serviceError(cause, t('contextFailed'));
            await recoverAccess(cause);
        }
    } finally { if (current === editGeneration) updatingId.value = null; }
}
function openActions(service: AssetServiceCatalogEntry, event: MouseEvent) { actionService.value = service; menu.value?.toggle(event); }
async function saved(service: AssetService) {
    if (!isCurrentOwner(service.owner_user_id)) return;
    const wasPublished = !!editing.value?.publication;
    closeEditor();
    lastSaved.value = service;
    services.value = services.value?.map(entry => entry.id === service.id ? { ...entry, ...service } : entry);
    const result = publicationFeedback(service, locale.value, t('saved'), service.publication ? 'save' : wasPublished ? 'remove' : 'save');
    toast.add({ severity: result.warning ? 'warn' : 'success', summary: result.message, life: result.warning ? 6000 : 2500 });
    await load();
}
async function toggleNavigation(service: AssetServiceCatalogEntry) {
    if (busy.value || !isCurrentOwner(service.owner_user_id)) return;
    if (!service.show_in_navigation && !safeServiceUrl(service.access_url || '')) { await openEdit(service); return; }
    const current = ++actionGeneration;
    const active = () => current === actionGeneration && isCurrentOwner(service.owner_user_id);
    updatingId.value = service.id;
    error.value = '';
    try {
        const updated = await props.api.assetsSetServiceNavigation(assetServiceScope(service), service.id, !service.show_in_navigation);
        if (!active()) return;
        lastSaved.value = updated;
        services.value = services.value?.map(entry => entry.id === updated.id ? { ...entry, ...updated } : entry);
        toast.add({ severity: 'success', summary: updated.show_in_navigation ? t('savedNav') : t('removed'), life: 2500 });
        await load();
    } catch (cause) { if (active()) { error.value = serviceError(cause, t('mutationFailed')); await recoverAccess(cause); } }
    finally { if (active()) updatingId.value = null; }
}
function deleteService(service: AssetServiceCatalogEntry) {
    const confirmationGeneration = actionGeneration;
    confirm.require({
        group: 'service-delete', header: t('deleteTitle'), message: t('deleteHint', { name: service.name }), icon: 'pi pi-exclamation-triangle',
        rejectProps: { label: t('cancel'), severity: 'secondary', outlined: true }, acceptProps: { label: t('delete'), severity: 'danger' },
        accept: async () => {
            if (confirmationGeneration !== actionGeneration || busy.value || !isCurrentOwner(service.owner_user_id)) return;
            const current = ++actionGeneration;
            const active = () => current === actionGeneration && isCurrentOwner(service.owner_user_id);
            updatingId.value = service.id;
            error.value = '';
            try {
                const response = await props.api.assetsDeleteService(assetServiceScope(service), service.id);
                if (!active()) return;
                services.value = services.value?.filter(entry => entry.id !== service.id);
                if (lastSaved.value?.id === service.id) lastSaved.value = undefined;
                const result = publicationFeedback(response, locale.value, t('deleted'), 'delete');
                toast.add({ severity: result.warning ? 'warn' : 'success', summary: result.message, life: result.warning ? 6000 : 2500 });
                await load();
            } catch (cause) {
                if (active()) {
                    if ((cause as { response?: { status?: number } })?.response?.status === 404) await load();
                    else { error.value = serviceError(cause, t('mutationFailed')); await recoverAccess(cause); }
                }
            } finally { if (active()) updatingId.value = null; }
        },
    });
}
function clearFilters() { search.value = ''; selectedNode.value = null; selectedDevice.value = null; }
</script>

<template>
    <div class="console-page service-management-page">
        <header class="page-heading service-page-heading"><div class="service-heading-identity"><span class="service-page-mark"><i class="pi pi-sliders-h" aria-hidden="true"></i></span><div><h1>{{ t('management') }}</h1><p class="page-description">{{ t('managementHint') }}</p></div></div><Button v-if="available" :label="t('addOnDevice')" icon="pi pi-plus" @click="openDevice()" /></header>
        <Message v-if="context && !available" severity="info" :closable="false">{{ t('external') }}</Message>
        <Message v-if="error" severity="error" :closable="false"><div class="service-message-content">{{ error }}<Button :label="t('retry')" text size="small" :disabled="busy" @click="load" /></div></Message>
        <template v-if="available || loading">
            <div class="service-management-back"><RouterLink :to="{ name: 'serviceNavigation', query: ownerQuery }"><i class="pi pi-arrow-left" aria-hidden="true"></i>{{ t('viewNavigation') }}</RouterLink><Button :aria-label="t('refresh')" icon="pi pi-refresh" text severity="secondary" :loading="loading" :disabled="busy" @click="load" /></div>
            <Select v-if="context?.can_manage_others" :model-value="ownerId" :options="accounts" option-label="username" option-value="id" :aria-label="t('owner')" :disabled="busy" @update:model-value="selectOwner" />
            <Message v-if="lastSaved?.show_in_navigation" severity="success" :closable="true" @close="lastSaved = undefined"><div class="service-message-content"><span>{{ t('savedNav') }}</span><RouterLink :to="{ name: 'serviceNavigation', query: ownerQuery }">{{ t('viewNavigation') }} <i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink></div></Message>
            <div v-if="services !== undefined && services.length" class="service-management-overview">
                <span><i class="pi pi-box" aria-hidden="true"></i>{{ t('serviceCount') }}<strong>{{ services.length }}</strong></span>
                <span><i class="pi pi-th-large" aria-hidden="true"></i>{{ t('entryCount') }}<strong>{{ navigationCount }}</strong></span>
                <span><i class="pi pi-eye-slash" aria-hidden="true"></i>{{ t('hidden') }}<strong>{{ services.length - navigationCount }}</strong></span>
            </div>
            <section class="console-panel service-list-panel">
                <div class="list-toolbar service-list-toolbar"><div class="search-field"><i class="pi pi-search" aria-hidden="true"></i><InputText v-model="search" :placeholder="t('search')" :aria-label="t('search')" /></div><Select v-model="selectedNode" :placeholder="t('allNodes')" :options="nodeOptions" option-label="label" option-value="value" :aria-label="t('node')" class="service-management-filter" @update:model-value="selectedDevice = null" /><Select v-model="selectedDevice" :placeholder="t('allDevices')" :options="deviceOptions" option-label="label" option-value="value" :aria-label="t('assetDevice')" class="service-management-filter" /><span class="table-count">{{ t('filterCount', { count: filtered.length }) }}</span></div>
                <div v-if="loading && services === undefined" class="service-management-grid" :aria-label="t('loading')"><Skeleton v-for="i in 6" :key="i" height="190px" /></div>
                <div v-else-if="services !== undefined && !services.length" class="service-management-empty"><i class="pi pi-box" aria-hidden="true"></i><h2>{{ t('noServices') }}</h2><p>{{ t('noServicesHint') }}</p><Button :label="t('addOnDevice')" icon="pi pi-plus" @click="openDevice()" /></div>
                <div v-else-if="services !== undefined && !filtered.length" class="service-management-empty"><i class="pi pi-search" aria-hidden="true"></i><h2>{{ t('noResults') }}</h2><p>{{ t('noResultsHint') }}</p><Button :label="t('clear')" outlined @click="clearFilters" /></div>
                <ul v-else class="service-management-grid" :aria-label="t('management')">
                    <li v-for="service in filtered" :key="service.id" :data-service-id="service.id" class="service-management-card">
                        <div class="service-management-card-header">
                            <span class="service-management-icon"><i :class="assetServiceIconClass(service.icon)" aria-hidden="true"></i></span>
                            <button type="button" class="service-name-button" :disabled="busy" @click="openEdit(service)">{{ service.name }}</button>
                            <Button icon="pi pi-ellipsis-h" text rounded severity="secondary" :aria-label="`${t('actions')}: ${service.name}`" aria-haspopup="menu" :disabled="busy" @click="openActions(service, $event)" />
                        </div>
                        <div class="service-management-identity">
                            <a v-if="safeServiceUrl(service.access_url || '')" :href="safeServiceUrl(service.access_url || '') || undefined" target="_blank" rel="noopener noreferrer" class="service-url" :title="service.access_url || undefined">{{ service.access_url }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></a>
                            <span v-else class="service-network-name">{{ t('navigationUrlRequired') }}</span>
                            <button type="button" class="service-name-button service-network-name" @click="openDevice(service)">{{ service.node_name }} · {{ service.device_name }}</button>
                        </div>
                        <div class="service-management-state">
                            <button type="button" class="service-navigation-toggle" role="switch" :aria-checked="service.show_in_navigation" :aria-label="`${t('show')}: ${service.name}`" :disabled="busy" @click="toggleNavigation(service)"><Tag :value="service.show_in_navigation ? t('enabled') : t('hidden')" :severity="service.show_in_navigation ? 'success' : 'secondary'" :icon="service.show_in_navigation ? 'pi pi-check' : 'pi pi-minus'" /></button>
                            <span class="service-unchecked" :title="t('uncheckedHint')">{{ t('unchecked') }}</span>
                        </div>
                    </li>
                </ul>
            </section>
            <AssetServiceDialog v-if="editorVisible && editing && editingDevice" :api="api" :visible="true" :scope="{ owner_user_id: editing.owner_user_id, machine_id: editing.machine_id }" :device="editingDevice" :service="editing" @close="closeEditor" @saved="saved" @context-error="recoverAccess" />
            <Menu ref="menu" :model="menuItems" popup />
            <ConfirmDialog group="service-delete" class="console-dialog" />
        </template>
    </div>
</template>

<style scoped>
.service-page-heading { align-items: center; }
.service-heading-identity { display: flex; align-items: center; gap: 16px; min-width: 0; }
.service-page-mark { display: inline-flex; width: 48px; height: 48px; align-items: center; justify-content: center; flex: none; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 13px; color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); box-shadow: var(--console-glow, none); }
.service-page-mark > .pi { font-size: 23px; }
.service-page-heading > .p-button { flex: none; }
.service-management-back { display: flex; align-items: center; justify-content: space-between; margin-top: -8px; }
.service-management-back > a { display: inline-flex; align-items: center; gap: 9px; font-size: 12px; color: var(--p-primary-color); }
.service-message-content { display: flex; justify-content: space-between; align-items: center; gap: 18px; flex-wrap: wrap; }
.service-message-content a { text-decoration: underline; font-size: 13px; }
.service-management-overview { display: flex; flex-wrap: wrap; align-items: center; gap: 20px 28px; padding: 17px 0; border-top: 1px solid var(--console-border); border-bottom: 1px solid var(--console-border); margin-top: -6px; }
.service-management-overview > span { display: inline-flex; align-items: center; gap: 10px; color: var(--console-muted); font-size: 12px; }
.service-management-overview > span > .pi { color: var(--p-primary-color); font-size: 12px; }
.service-management-overview strong { color: var(--p-text-color); font-size: 18px; font-weight: 600; font-variant-numeric: tabular-nums; }
.service-list-panel { border-radius: 13px; box-shadow: var(--console-shadow, none); }
.service-list-toolbar { padding: 18px 20px; background: var(--console-panel-raised, var(--console-panel)); }
.service-management-filter { min-width: 190px; }
.service-list-toolbar .search-field { max-width: 480px; }
.service-list-toolbar .table-count { margin-left: auto; white-space: nowrap; font-variant-numeric: tabular-nums; }
.service-management-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr)); gap: 16px; margin: 0; padding: 20px; list-style: none; }
.service-management-card { display: flex; flex-direction: column; gap: 16px; min-width: 0; padding: 18px; border: 1px solid var(--console-border); border-radius: 12px; background: var(--console-panel); box-shadow: inset 0 1px 0 color-mix(in srgb, var(--p-text-color) 3%, transparent); transition: border-color .18s, box-shadow .18s; }
.service-management-card:hover, .service-management-card:focus-within { border-color: color-mix(in srgb, var(--p-primary-color) 60%, var(--console-border)); box-shadow: var(--console-glow, 0 8px 24px rgb(0 0 0 / 7%)); }
.service-management-card-header { display: flex; align-items: center; gap: 12px; min-width: 0; }
.service-management-card-header > .service-name-button { flex: 1; min-width: 0; align-self: center; }
.service-management-card-header > .p-button { flex: none; margin-right: -6px; }
.service-management-icon { display: inline-flex; align-items: center; justify-content: center; width: 48px; height: 48px; flex: none; border: 1px solid color-mix(in srgb, var(--p-primary-color) 20%, var(--console-border)); border-radius: 13px; background: var(--console-accent-soft, var(--p-highlight-background)); color: var(--p-primary-color); }
.service-management-icon .pi { font-size: 22px; }
.service-management-identity { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 8px; }
.service-name-button { text-align: left; align-self: flex-start; font-size: 15px; font-weight: 600; background: transparent; color: var(--p-text-color); overflow-wrap: anywhere; }
.service-name-button:hover { color: var(--p-primary-color); }
.service-name-button:focus-visible, .service-navigation-toggle:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; border-radius: 3px; }
.service-url { display: block; max-width: 100%; color: var(--console-muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.service-url:hover { color: var(--p-primary-color); }
.service-url .pi { font-size: 10px; margin-left: 7px; }
.service-network-name { color: var(--console-muted); font-size: 11px; overflow-wrap: anywhere; }
.service-management-state { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 9px; padding-top: 14px; border-top: 1px solid var(--console-border); }
.service-navigation-toggle { background: transparent; cursor: pointer; border-radius: 6px; }
.service-navigation-toggle :deep(.p-tag) { padding: 5px 9px; border: 1px solid color-mix(in srgb, currentColor 18%, transparent); font-size: 11px; font-weight: 500; }
.service-navigation-toggle:hover { box-shadow: 0 0 0 3px var(--console-accent-soft, var(--p-highlight-background)); }
.service-navigation-toggle:disabled { opacity: .6; cursor: wait; }
.service-unchecked { font-size: 11px; color: var(--console-muted); }
.service-management-empty { padding: 68px 24px; text-align: center; }
.service-management-empty > .pi { display: inline-flex; align-items: center; justify-content: center; width: 68px; height: 68px; font-size: 28px; color: var(--p-primary-color); margin-bottom: 24px; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 19px; background: var(--console-accent-soft, var(--p-highlight-background)); }
.service-management-empty h2 { font-size: 19px; font-weight: 600; margin-bottom: 10px; }
.service-management-empty p { color: var(--console-muted); margin-bottom: 24px; line-height: 1.7; }
@media (max-width: 650px) { .service-page-heading { flex-wrap: wrap; align-items: flex-start; } .service-heading-identity { gap: 12px; } .service-page-mark { width: 42px; height: 42px; } .service-management-grid { padding: 14px; gap: 14px; } .service-management-card { padding: 16px; } .service-management-filter { width: 100%; } .service-list-toolbar .search-field { max-width: none; width: 100%; } .service-list-toolbar .table-count { margin-left: 0; } .service-management-overview { gap: 13px 20px; } }
@media (prefers-reduced-motion: reduce) { .service-management-card { transition: none; } }
</style>
