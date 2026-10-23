<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Button, ConfirmDialog, InputText, Menu, Message, Select, Skeleton, Tag, useConfirm, useToast } from 'primevue';
import ApiClient, { type CentralNetworkSummary } from '../modules/api';
import type { ServiceEntry } from '../modules/network-space';
import { serviceMessages } from '../modules/service-messages';
import { safeServiceUrl, serviceError, serviceIcons, serviceInput, sortServices } from '../modules/service-utils';
import ServiceEditor from './services/ServiceEditor.vue';

const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t } = useI18n({ useScope: 'local', messages: serviceMessages });
const route = useRoute();
const router = useRouter();
const confirm = useConfirm();
const toast = useToast();
const services = ref<ServiceEntry[]>();
const networks = ref<CentralNetworkSummary[]>([]);
const search = ref('');
const selectedNetwork = ref<string | null>(null);
const loading = ref(false);
const updatingId = ref<string | null>(null);
const error = ref('');
const lastSaved = ref<ServiceEntry>();
const editorVisible = ref(false);
const editing = ref<ServiceEntry | null>(null);
const editorDefaults = ref<{ network?: string; node?: string; device?: string }>({});
const menu = ref<InstanceType<typeof Menu>>();
const actionService = ref<ServiceEntry>();
let requestVersion = 0;
let handledQuery = '';
const networkNames = computed(() => new Map(networks.value.map(network => [network.network_id, network.display_name || network.network_name])));
const networkOptions = computed(() => [{ label: t('allNetworks'), value: null }, ...networks.value.map(network => ({ label: network.display_name || network.network_name, value: network.network_id }))]);
const navigationCount = computed(() => (services.value ?? []).filter(service => service.show_in_navigation).length);
const filtered = computed(() => {
    const query = search.value.trim().toLocaleLowerCase();
    return sortServices((services.value ?? []).filter(service =>
        (!selectedNetwork.value || service.network_id === selectedNetwork.value) &&
        (!query || `${service.name} ${service.description} ${service.group} ${service.url}`.toLocaleLowerCase().includes(query))));
});
const menuItems = computed(() => [
    { label: t('edit'), icon: 'pi pi-pencil', command: () => actionService.value && openEdit(actionService.value) },
    { label: t('locate'), icon: 'pi pi-map-marker', command: () => { const service = actionService.value; if (service) router.push({ name: 'networkMap', query: { network: service.network_id, node: service.device_id, ...(service.lan_device_id ? { device: service.lan_device_id } : {}) } }); } },
    { separator: true },
    { label: t('delete'), icon: 'pi pi-trash', command: () => actionService.value && deleteService(actionService.value) },
]);
function queryValue(key: string): string | undefined { const value = route.query[key]; return typeof value === 'string' ? value : undefined; }
function queryAction() {
    if (!props.centralEnabled || services.value === undefined || editorVisible.value) return;
    const signature = JSON.stringify(route.query);
    if (signature === handledQuery) return;
    const editId = queryValue('edit');
    if (editId) {
        const entry = services.value.find(service => service.id === editId);
        handledQuery = signature;
        if (entry) openEdit(entry);
        else error.value = t('serviceMissing');
    } else if (queryValue('create') || queryValue('node')) {
        handledQuery = signature;
        openNew({ network: queryValue('network'), node: queryValue('node'), device: queryValue('device') });
    }
}
async function load() {
    const version = ++requestVersion;
    if (!props.centralEnabled) { services.value = undefined; loading.value = false; return; }
    loading.value = true;
    error.value = '';
    const results = await Promise.allSettled([props.api.list_services(), props.api.list_networks()]);
    if (version !== requestVersion) return;
    if (results[0].status === 'fulfilled') services.value = results[0].value;
    else error.value = serviceError(results[0].reason, t('loadFailed'));
    if (results[1].status === 'fulfilled') networks.value = results[1].value;
    else if (!error.value) error.value = serviceError(results[1].reason, t('loadFailed'));
    loading.value = false;
    queryAction();
}
watch(() => props.centralEnabled, load, { immediate: true });
watch(() => route.query, queryAction);
watch(editorVisible, visible => {
    if (!visible && (route.query.edit || route.query.create || route.query.node)) {
        const query = { ...route.query };
        delete query.edit; delete query.create; delete query.node; delete query.device;
        router.replace({ query });
        handledQuery = '';
    }
});
onUnmounted(() => { requestVersion++; });
function openNew(defaults: { network?: string; node?: string; device?: string } = {}) {
    editing.value = null;
    editorDefaults.value = { network: defaults.network || selectedNetwork.value || undefined, node: defaults.node, device: defaults.device };
    editorVisible.value = true;
}
function openEdit(service: ServiceEntry) { editing.value = service; editorDefaults.value = {}; editorVisible.value = true; }
function openActions(service: ServiceEntry, event: MouseEvent) { actionService.value = service; menu.value?.toggle(event); }
function saved(service: ServiceEntry) {
    services.value = [...(services.value ?? []).filter(entry => entry.id !== service.id), service];
    lastSaved.value = service;
    toast.add({ severity: 'success', summary: t('saved'), life: 2500 });
}
async function toggleNavigation(service: ServiceEntry) {
    if (updatingId.value) return;
    updatingId.value = service.id;
    error.value = '';
    try {
        const updated = await props.api.update_service(service.id, { ...serviceInput(service), show_in_navigation: !service.show_in_navigation });
        services.value = services.value?.map(entry => entry.id === updated.id ? updated : entry);
        lastSaved.value = updated;
        toast.add({ severity: 'success', summary: updated.show_in_navigation ? t('savedNav') : t('removed'), life: 2500 });
    } catch (cause) { error.value = serviceError(cause, t('mutationFailed')); }
    finally { updatingId.value = null; }
}
function deleteService(service: ServiceEntry) {
    confirm.require({
        group: 'service-delete', header: t('deleteTitle'), message: t('deleteHint', { name: service.name }), icon: 'pi pi-exclamation-triangle',
        rejectProps: { label: t('cancel'), severity: 'secondary', outlined: true }, acceptProps: { label: t('delete'), severity: 'danger' },
        accept: async () => {
            if (updatingId.value) return;
            updatingId.value = service.id;
            error.value = '';
            try {
                await props.api.delete_service(service.id);
                services.value = services.value?.filter(entry => entry.id !== service.id);
                if (lastSaved.value?.id === service.id) lastSaved.value = undefined;
                toast.add({ severity: 'success', summary: t('deleted'), life: 2500 });
            } catch (cause) { error.value = serviceError(cause, t('mutationFailed')); }
            finally { updatingId.value = null; }
        },
    });
}
function clearFilters() { search.value = ''; selectedNetwork.value = null; }
</script>

<template>
    <div class="console-page service-management-page">
        <header class="page-heading service-page-heading"><div class="service-heading-identity"><span class="service-page-mark"><i class="pi pi-sliders-h" aria-hidden="true"></i></span><div><h1>{{ t('management') }}</h1><p class="page-description">{{ t('managementHint') }}</p></div></div><Button v-if="centralEnabled" :label="t('add')" icon="pi pi-plus" @click="openNew()" /></header>
        <Message v-if="!centralEnabled" severity="info" :closable="false">{{ t('external') }}</Message>
        <template v-else>
            <div class="service-management-back"><RouterLink :to="{ name: 'serviceNavigation' }"><i class="pi pi-arrow-left" aria-hidden="true"></i>{{ t('viewNavigation') }}</RouterLink><Button :aria-label="t('refresh')" icon="pi pi-refresh" text severity="secondary" :loading="loading" @click="load" /></div>
            <Message v-if="error" severity="error" :closable="false"><div class="service-message-content">{{ error }}<Button :label="t('retry')" text size="small" @click="load" /></div></Message>
            <Message v-if="lastSaved?.show_in_navigation" severity="success" :closable="true" @close="lastSaved = undefined"><div class="service-message-content"><span>{{ t('savedNav') }}</span><RouterLink :to="{ name: 'serviceNavigation' }">{{ t('viewNavigation') }} <i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink></div></Message>
            <div v-if="services !== undefined && services.length" class="service-management-overview">
                <span><i class="pi pi-box" aria-hidden="true"></i>{{ t('serviceCount') }}<strong>{{ services.length }}</strong></span>
                <span><i class="pi pi-th-large" aria-hidden="true"></i>{{ t('entryCount') }}<strong>{{ navigationCount }}</strong></span>
                <span><i class="pi pi-eye-slash" aria-hidden="true"></i>{{ t('hidden') }}<strong>{{ services.length - navigationCount }}</strong></span>
            </div>
            <section class="console-panel service-list-panel">
                <div class="list-toolbar service-list-toolbar"><div class="search-field"><i class="pi pi-search" aria-hidden="true"></i><InputText v-model="search" :placeholder="t('search')" :aria-label="t('search')" /></div><Select :placeholder="t('allNetworks')" v-model="selectedNetwork" :options="networkOptions" option-label="label" option-value="value" :aria-label="t('network')" class="service-management-filter" /><span class="table-count">{{ t('filterCount', { count: filtered.length }) }}</span></div>
                <div v-if="loading && services === undefined" class="loading-rows" :aria-label="t('loading')"><Skeleton v-for="i in 4" :key="i" height="64px" /></div>
                <div v-else-if="services !== undefined && !services.length" class="service-management-empty"><i class="pi pi-box" aria-hidden="true"></i><h2>{{ t('noServices') }}</h2><p>{{ t('noServicesHint') }}</p><Button :label="t('add')" icon="pi pi-plus" @click="openNew()" /></div>
                <div v-else-if="services !== undefined && !filtered.length" class="service-management-empty"><i class="pi pi-search" aria-hidden="true"></i><h2>{{ t('noResults') }}</h2><p>{{ t('noResultsHint') }}</p><Button :label="t('clear')" outlined @click="clearFilters" /></div>
                <ul v-else class="service-management-list" :aria-label="t('management')">
                    <li v-for="service in filtered" :key="service.id" :data-service-id="service.id" class="service-management-row">
                        <span class="service-management-icon"><i :class="['pi', serviceIcons[service.icon] || 'pi-globe']" aria-hidden="true"></i></span>
                        <div class="service-management-identity"><button type="button" class="service-name-button" @click="openEdit(service)">{{ service.name }}</button><a :href="safeServiceUrl(service.url) || undefined" :aria-disabled="!safeServiceUrl(service.url)" :target="service.open_in_new_tab ? '_blank' : undefined" rel="noopener noreferrer" class="service-url" :title="service.url" @click="!safeServiceUrl(service.url) && $event.preventDefault()">{{ service.url }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></a><span class="service-network-name">{{ networkNames.get(service.network_id) || service.network_id }}<template v-if="service.group"> · {{ service.group }}</template></span></div>
                        <div class="service-management-state"><button type="button" class="service-navigation-toggle" role="switch" :aria-checked="service.show_in_navigation" :aria-label="`${t('show')}: ${service.name}`" :disabled="!!updatingId" @click="toggleNavigation(service)"><Tag :value="service.show_in_navigation ? t('enabled') : t('hidden')" :severity="service.show_in_navigation ? 'success' : 'secondary'" :icon="service.show_in_navigation ? 'pi pi-check' : 'pi pi-minus'" /></button><span class="service-unchecked" :title="t('uncheckedHint')">{{ t('unchecked') }}</span></div>
                        <Button icon="pi pi-ellipsis-h" text rounded severity="secondary" :aria-label="`${t('actions')}: ${service.name}`" aria-haspopup="menu" :disabled="updatingId === service.id" @click="openActions(service, $event)" />
                    </li>
                </ul>
            </section>
            <ServiceEditor :api="api" v-model:visible="editorVisible" :entry="editing" :defaults="editorDefaults" @saved="saved" />
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
.service-management-list { margin: 0; padding: 0; list-style: none; }
.service-management-row { display: flex; align-items: center; gap: 18px; padding: 22px; border-bottom: 1px solid var(--console-border); transition: background .18s; }
.service-management-row:hover { background: color-mix(in srgb, var(--console-panel-raised, var(--console-panel)) 70%, transparent); }
.service-management-row:last-child { border-bottom: 0; }
.service-management-icon { display: inline-flex; align-items: center; justify-content: center; width: 48px; height: 48px; flex: none; border: 1px solid color-mix(in srgb, var(--p-primary-color) 20%, var(--console-border)); border-radius: 13px; background: var(--console-accent-soft, var(--p-highlight-background)); color: var(--p-primary-color); }
.service-management-icon .pi { font-size: 22px; }
.service-management-identity { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
.service-name-button { text-align: left; align-self: flex-start; font-size: 15px; font-weight: 600; background: transparent; color: var(--p-text-color); overflow-wrap: anywhere; }
.service-name-button:hover { color: var(--p-primary-color); }
.service-name-button:focus-visible, .service-navigation-toggle:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; border-radius: 3px; }
.service-url { display: inline-flex; align-items: center; gap: 7px; color: var(--console-muted); font-size: 12px; overflow-wrap: anywhere; }
.service-url:hover { color: var(--p-primary-color); }
.service-url .pi { font-size: 10px; flex: none; }
.service-network-name { color: var(--console-muted); font-size: 11px; overflow-wrap: anywhere; }
.service-management-state { display: flex; flex-direction: column; gap: 9px; align-items: flex-end; flex: none; }
.service-navigation-toggle { background: transparent; cursor: pointer; border-radius: 6px; }
.service-navigation-toggle :deep(.p-tag) { padding: 5px 9px; border: 1px solid color-mix(in srgb, currentColor 18%, transparent); font-size: 11px; font-weight: 500; }
.service-navigation-toggle:hover { box-shadow: 0 0 0 3px var(--console-accent-soft, var(--p-highlight-background)); }
.service-navigation-toggle:disabled { opacity: .6; cursor: wait; }
.service-unchecked { font-size: 11px; color: var(--console-muted); }
.service-management-empty { padding: 68px 24px; text-align: center; }
.service-management-empty > .pi { display: inline-flex; align-items: center; justify-content: center; width: 68px; height: 68px; font-size: 28px; color: var(--p-primary-color); margin-bottom: 24px; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 19px; background: var(--console-accent-soft, var(--p-highlight-background)); }
.service-management-empty h2 { font-size: 19px; font-weight: 600; margin-bottom: 10px; }
.service-management-empty p { color: var(--console-muted); margin-bottom: 24px; line-height: 1.7; }
@media (max-width: 650px) { .service-page-heading { flex-wrap: wrap; align-items: flex-start; } .service-heading-identity { gap: 12px; } .service-page-mark { width: 42px; height: 42px; } .service-management-row { flex-wrap: wrap; gap: 12px; padding: 18px 16px; } .service-management-identity { flex-basis: calc(100% - 109px); } .service-management-row > .p-button { order: 2; } .service-management-state { order: 3; flex-direction: row; align-items: center; width: 100%; padding-left: 60px; } .service-management-filter { width: 100%; } .service-list-toolbar .search-field { max-width: none; width: 100%; } .service-list-toolbar .table-count { margin-left: 0; } .service-management-overview { gap: 13px 20px; } }
@media (prefers-reduced-motion: reduce) { .service-management-row { transition: none; } }
</style>
