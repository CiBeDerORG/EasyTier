<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Button, InputText, Menu, Message, Select, Skeleton, useToast } from 'primevue';
import type ApiClient from '../modules/api';
import type { AssetServiceCatalogEntry } from '../types/node-assets';
import { serviceMessages } from '../modules/service-messages';
import { serviceError } from '../modules/service-utils';
import { assetServiceScope, matchesAssetService, sortAssetServices, useServiceCatalog } from '../modules/service-catalog';
import ServiceCard from './services/ServiceCard.vue';

const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t } = useI18n({ useScope: 'local', messages: serviceMessages });
const router = useRouter();
const toast = useToast();
const { context, accounts, ownerId, ownerQuery, services, loading, error, available, load: loadCatalog, selectOwner, isCurrentOwner } = useServiceCatalog(props.api, () => t('loadFailed'));
const search = ref('');
const selectedNode = ref<string | null>(null);
const selectedDevice = ref<string | null>(null);
const menu = ref<InstanceType<typeof Menu>>();
const actionService = ref<AssetServiceCatalogEntry>();
const updating = ref(false);
let actionGeneration = 0;
const busy = computed(() => loading.value || updating.value);
const visibleServices = computed(() => sortAssetServices((services.value ?? []).filter(service => service.show_in_navigation)));
const nodeOptions = computed(() => [{ label: t('allNodes'), value: null }, ...[...new Map(visibleServices.value.map(service => [service.machine_id, service.node_name])).entries()].map(([value, label]) => ({ value, label }))]);
const linkedNodeCount = computed(() => new Set(visibleServices.value.map(service => service.machine_id)).size);
const groups = computed(() => [...new Map(visibleServices.value.filter(service => !selectedNode.value || service.machine_id === selectedNode.value)
    .map(service => [service.device_id, { id: service.device_id, label: `${service.device_name} · ${service.node_name}` }])).values()]);
const deviceCount = computed(() => new Set(visibleServices.value.map(service => service.device_id)).size);
const filtered = computed(() => visibleServices.value.filter(service =>
    (!selectedNode.value || service.machine_id === selectedNode.value) && (!selectedDevice.value || service.device_id === selectedDevice.value)
    && matchesAssetService(service, search.value)));
const menuItems = computed(() => [
    { label: t('edit'), icon: 'pi pi-pencil', disabled: busy.value, command: () => actionService.value && router.push({ name: 'serviceManagement', query: { owner: String(actionService.value.owner_user_id), edit: actionService.value.id } }) },
    { label: t('locateDevice'), icon: 'pi pi-desktop', command: () => {
        const service = actionService.value;
        if (service) void router.push({ name: 'nodeAssets', query: { owner: String(service.owner_user_id), node: service.machine_id, device: service.device_id } });
    } },
    ...(actionService.value?.owner_user_id === context.value?.actor.id ? [{ label: t('locate'), icon: 'pi pi-map-marker', command: () => {
        const service = actionService.value;
        if (service) void router.push({ name: 'networkMap', query: { node: service.machine_id, device: service.device_id } });
    } }] : []),
    { separator: true },
    { label: t('remove'), icon: 'pi pi-eye-slash', disabled: busy.value, command: () => removeFromNavigation() },
]);
const load = () => loadCatalog();
watch(ownerId, () => { actionGeneration++; clearFilters(); menu.value?.hide(); actionService.value = undefined; updating.value = false; });
onBeforeUnmount(() => { actionGeneration++; });
watch(selectedNode, () => { selectedDevice.value = null; });
function openActions(service: AssetServiceCatalogEntry, event: MouseEvent) { actionService.value = service; menu.value?.toggle(event); }
async function removeFromNavigation() {
    const service = actionService.value;
    if (!service || busy.value || !isCurrentOwner(service.owner_user_id)) return;
    const current = ++actionGeneration;
    const active = () => current === actionGeneration && isCurrentOwner(service.owner_user_id);
    updating.value = true;
    error.value = '';
    try {
        const updated = await props.api.assetsSetServiceNavigation(assetServiceScope(service), service.id, false);
        if (!active()) return;
        services.value = services.value?.map(entry => entry.id === updated.id ? { ...entry, ...updated } : entry);
        toast.add({ severity: 'success', summary: t('removed'), detail: t('removedHint'), life: 4000 });
        await load();
    } catch (cause) {
        if (active()) {
            error.value = serviceError(cause, t('mutationFailed'));
            const status = (cause as { response?: { status?: number } })?.response?.status;
            if (status === 401 || status === 403) await loadCatalog(true);
        }
    }
    finally { if (active()) updating.value = false; }
}
function clearFilters() { search.value = ''; selectedNode.value = null; selectedDevice.value = null; }
</script>

<template>
    <div class="console-page service-navigation-page">
        <header class="page-heading service-page-heading">
            <div class="service-heading-identity"><span class="service-page-mark"><i class="pi pi-th-large" aria-hidden="true"></i></span><div><h1>{{ t('navigation') }}</h1><p class="page-description">{{ t('navigationHint') }}</p></div></div>
            <div class="service-heading-actions">
                <Button icon="pi pi-refresh" text severity="secondary" :aria-label="t('refresh')" :loading="loading" :disabled="busy" @click="load" />
                <RouterLink v-if="available" :to="{ name: 'serviceManagement', query: ownerQuery }" class="service-manage-link"><i class="pi pi-sliders-h" aria-hidden="true"></i>{{ t('management') }}</RouterLink>
            </div>
        </header>
        <Message v-if="context && !available" severity="info" :closable="false">{{ t('external') }}</Message>
        <Message v-if="error" severity="error" :closable="false"><div class="service-error">{{ error }}<Button :label="t('retry')" text size="small" :disabled="busy" @click="load" /></div></Message>
        <template v-if="available || loading">
            <Select v-if="context?.can_manage_others" :model-value="ownerId" :options="accounts" option-label="username" option-value="id" :aria-label="t('owner')" :disabled="busy" @update:model-value="selectOwner" />
            <div v-if="services !== undefined && visibleServices.length" class="service-navigation-overview">
                <div><span>{{ t('entryCount') }}</span><strong>{{ visibleServices.length }}</strong></div>
                <div><span>{{ t('linkedNodes') }}</span><strong>{{ linkedNodeCount }}</strong></div>
                <div><span>{{ t('linkedDevices') }}</span><strong>{{ deviceCount }}</strong></div>
                <Button :label="t('addOnDevice')" icon="pi pi-plus" text size="small" @click="router.push({ name: 'nodeAssets', query: ownerQuery })" />
            </div>
            <div v-if="visibleServices.length" class="service-navigation-toolbar">
                <div class="search-field"><i class="pi pi-search" aria-hidden="true"></i><InputText v-model="search" :placeholder="t('search')" :aria-label="t('search')" /></div>
                <Select v-model="selectedNode" :placeholder="t('allNodes')" :options="nodeOptions" option-label="label" option-value="value" :aria-label="t('node')" class="service-network-filter" />
            </div>
            <div v-if="groups.length > 1" class="service-group-filters" role="group" :aria-label="t('assetDevice')">
                <button type="button" :class="{ active: selectedDevice === null }" :aria-pressed="selectedDevice === null" @click="selectedDevice = null">{{ t('allDevices') }}</button>
                <button v-for="group in groups" :key="group.id" type="button" :class="{ active: selectedDevice === group.id }" :aria-pressed="selectedDevice === group.id" @click="selectedDevice = group.id">{{ group.label }}</button>
            </div>
            <div v-if="loading && services === undefined" class="service-navigation-grid" :aria-label="t('loading')"><Skeleton v-for="i in 6" :key="i" height="190px" /></div>
            <section v-else-if="services !== undefined && visibleServices.length === 0" class="service-empty">
                <i class="pi pi-th-large" aria-hidden="true"></i><h2>{{ t('empty') }}</h2><p>{{ t('emptyHint') }}</p>
                <Button :label="t('management')" icon="pi pi-sliders-h" @click="router.push({ name: 'serviceManagement', query: ownerQuery })" />
            </section>
            <section v-else-if="services !== undefined && filtered.length === 0" class="service-empty">
                <i class="pi pi-search" aria-hidden="true"></i><h2>{{ t('noResults') }}</h2><p>{{ t('noResultsHint') }}</p><Button :label="t('clear')" outlined @click="clearFilters" />
            </section>
            <section v-else-if="filtered.length" class="service-navigation-grid" :aria-label="t('navigation')">
                <ServiceCard v-for="service in filtered" :key="service.id" :service="service" @actions="openActions" />
            </section>
            <RouterLink v-if="ownerId === context?.actor.id" :to="{ name: 'networkMap' }" class="service-map-link">
                <span class="service-map-icon"><i class="pi pi-globe" aria-hidden="true"></i></span><span><strong>{{ t('map') }}</strong><span class="service-map-hint">{{ t('mapHint') }}</span></span><span class="service-map-action">{{ t('openMap') }}<i class="pi pi-arrow-right" aria-hidden="true"></i></span>
            </RouterLink>
            <Menu ref="menu" :model="menuItems" popup />
        </template>
    </div>
</template>

<style scoped>
.service-navigation-page { gap: 26px; }
.service-heading-identity { display: flex; align-items: center; gap: 16px; min-width: 0; }
.service-page-mark { display: inline-flex; width: 48px; height: 48px; align-items: center; justify-content: center; flex: none; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 13px; color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); box-shadow: var(--console-glow, none); }
.service-page-mark > .pi { font-size: 23px; }
.service-heading-actions { display: flex; gap: 8px; align-items: center; flex: none; }
.service-manage-link { display: inline-flex; align-items: center; gap: 9px; min-height: 40px; padding: 9px 14px; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 9px; font-size: 13px; background: var(--console-panel-raised, var(--console-panel)); color: var(--p-text-color); white-space: nowrap; transition: border-color .18s, background .18s; }
.service-manage-link:hover { border-color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); }
.service-error { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.service-navigation-overview { display: flex; align-items: center; gap: 28px; padding: 18px 0 20px; border-top: 1px solid var(--console-border); border-bottom: 1px solid var(--console-border); }
.service-navigation-overview > div { display: flex; align-items: baseline; gap: 12px; padding-right: 28px; border-right: 1px solid var(--console-border); }
.service-navigation-overview > div > span { color: var(--console-muted); font-size: 12px; }
.service-navigation-overview strong { font-size: 23px; font-weight: 600; font-variant-numeric: tabular-nums; letter-spacing: -.04em; line-height: 1; }
.service-navigation-overview > .p-button { margin-left: auto; }
.service-navigation-toolbar { display: flex; gap: 14px; align-items: center; }
.service-navigation-toolbar .search-field { max-width: 520px; }
.service-navigation-toolbar :deep(.p-inputtext), .service-network-filter { background: var(--console-panel); }
.service-network-filter { min-width: 200px; margin-left: auto; }
.service-group-filters { display: flex; flex-wrap: wrap; gap: 8px; margin-top: -10px; }
.service-group-filters button { padding: 8px 15px; border: 1px solid transparent; border-radius: 8px; color: var(--console-muted); font-size: 12px; background: transparent; transition: color .18s, background .18s, border-color .18s; }
.service-group-filters button:hover { color: var(--p-text-color); background: var(--console-panel-raised, var(--console-stripe)); }
.service-group-filters button.active { background: var(--console-accent-soft, var(--p-highlight-background)); color: var(--p-primary-color); border-color: color-mix(in srgb, var(--p-primary-color) 30%, var(--console-border)); }
.service-group-filters button:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 2px; }
.service-navigation-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 280px), 1fr)); gap: 16px; }
.service-empty { padding: 76px 24px; text-align: center; background: var(--console-panel); border: 1px solid var(--console-border); border-radius: 14px; box-shadow: var(--console-shadow, none); }
.service-empty > .pi { display: inline-flex; align-items: center; justify-content: center; width: 76px; height: 76px; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 22px; font-size: 30px; color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); margin-bottom: 26px; }
.service-empty h2 { font-size: 21px; font-weight: 600; margin-bottom: 10px; }
.service-empty p { color: var(--console-muted); max-width: 410px; margin: 0 auto 26px; line-height: 1.8; }
.service-map-link { display: flex; align-items: center; gap: 18px; padding: 22px 24px; border: 1px solid var(--console-border); border-radius: 12px; background: var(--console-panel); margin-top: 8px; color: var(--p-text-color); transition: border-color .18s, box-shadow .18s; }
.service-map-link:hover { border-color: color-mix(in srgb, var(--p-primary-color) 50%, var(--console-border)); box-shadow: var(--console-glow, none); }
.service-map-icon { display: flex; align-items: center; justify-content: center; color: var(--p-primary-color); width: 48px; height: 48px; flex: none; border: 1px solid var(--console-border-strong, var(--console-border)); background: var(--console-accent-soft, var(--p-highlight-background)); border-radius: 12px; }
.service-map-icon > .pi { font-size: 24px; }
.service-map-link strong { font-size: 15px; font-weight: 550; }
.service-map-hint { display: block; font-size: 12px; color: var(--console-muted); margin-top: 6px; }
.service-map-action { margin-left: auto; color: var(--p-primary-color); display: flex; align-items: center; gap: 10px; white-space: nowrap; font-size: 12px; }
@media (max-width: 1000px) { .service-navigation-overview { gap: 18px; } .service-navigation-overview > div { padding-right: 18px; flex-direction: column-reverse; gap: 7px; } }
@media (max-width: 600px) { .service-page-heading { flex-wrap: wrap; } .service-page-mark { width: 42px; height: 42px; } .service-heading-identity { gap: 12px; } .service-navigation-overview { gap: 12px; flex-wrap: wrap; } .service-navigation-overview > div { padding-right: 12px; flex: 1; } .service-navigation-overview > div:last-of-type { border-right: 0; } .service-navigation-overview > .p-button { margin-left: 0; padding-left: 0; } .service-navigation-grid { grid-template-columns: 1fr; } .service-navigation-toolbar { flex-wrap: wrap; } .service-network-filter { margin-left: 0; width: 100%; } .service-navigation-toolbar .search-field { max-width: none; width: 100%; } .service-map-action { display: none; } .service-map-hint { max-width: 240px; } .service-map-link { align-items: flex-start; padding: 20px; } }
@media (prefers-reduced-motion: reduce) { .service-group-filters button, .service-map-link, .service-manage-link { transition: none; } }
</style>
