<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Button, InputText, Menu, Message, Select, Skeleton, useToast } from 'primevue';
import ApiClient, { type CentralNetworkSummary } from '../modules/api';
import type { ServiceEntry } from '../modules/network-space';
import { serviceMessages } from '../modules/service-messages';
import { serviceError, serviceInput, sortServices } from '../modules/service-utils';
import ServiceCard from './services/ServiceCard.vue';

const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t } = useI18n({ useScope: 'local', messages: serviceMessages });
const router = useRouter();
const toast = useToast();
const services = ref<ServiceEntry[]>();
const networks = ref<CentralNetworkSummary[]>([]);
const search = ref('');
const selectedNetwork = ref<string | null>(null);
const selectedGroup = ref<string | null>(null);
const loading = ref(false);
const error = ref('');
let requestVersion = 0;
const menu = ref<InstanceType<typeof Menu>>();
const actionService = ref<ServiceEntry>();
const updating = ref(false);
const networkNames = computed(() => new Map(networks.value.map(network => [network.network_id, network.display_name || network.network_name])));
const networkOptions = computed(() => [{ label: t('allNetworks'), value: null }, ...networks.value.map(network => ({ label: network.display_name || network.network_name, value: network.network_id }))]);
const visibleServices = computed(() => sortServices((services.value ?? []).filter(service => service.show_in_navigation)));
const linkedNetworkCount = computed(() => new Set(visibleServices.value.map(service => service.network_id)).size);
const groups = computed(() => [...new Set(visibleServices.value.map(service => service.group))].sort((a, b) => a.localeCompare(b)));
const filtered = computed(() => {
    const query = search.value.trim().toLocaleLowerCase();
    return visibleServices.value.filter(service =>
        (!selectedNetwork.value || service.network_id === selectedNetwork.value) &&
        (selectedGroup.value === null || service.group === selectedGroup.value) &&
        (!query || `${service.name} ${service.description} ${service.group} ${service.url}`.toLocaleLowerCase().includes(query)));
});
const sections = computed(() => {
    const grouped = new Map<string, ServiceEntry[]>();
    for (const service of filtered.value) {
        const items = grouped.get(service.group) ?? [];
        items.push(service);
        grouped.set(service.group, items);
    }
    return [...grouped.entries()].map(([name, entries]) => ({ name, entries }));
});
const menuItems = computed(() => [
    { label: t('edit'), icon: 'pi pi-pencil', command: () => actionService.value && router.push({ name: 'serviceManagement', query: { edit: actionService.value.id } }) },
    { label: t('locate'), icon: 'pi pi-map-marker', command: () => { const service = actionService.value; if (service) router.push({ name: 'networkMap', query: { network: service.network_id, node: service.device_id, ...(service.lan_device_id ? { device: service.lan_device_id } : {}) } }); } },
    { separator: true },
    { label: t('remove'), icon: 'pi pi-eye-slash', disabled: updating.value, command: () => removeFromNavigation() },
]);
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
}
watch(() => props.centralEnabled, load, { immediate: true });
onUnmounted(() => { requestVersion++; });
function openActions(service: ServiceEntry, event: MouseEvent) { actionService.value = service; menu.value?.toggle(event); }
async function removeFromNavigation() {
    const service = actionService.value;
    if (!service || updating.value) return;
    updating.value = true;
    try {
        const updated = await props.api.update_service(service.id, { ...serviceInput(service), show_in_navigation: false });
        services.value = services.value?.map(entry => entry.id === updated.id ? updated : entry);
        toast.add({ severity: 'success', summary: t('removed'), detail: t('removedHint'), life: 4000 });
    } catch (cause) { error.value = serviceError(cause, t('mutationFailed')); }
    finally { updating.value = false; }
}
function clearFilters() { search.value = ''; selectedNetwork.value = null; selectedGroup.value = null; }
</script>

<template>
    <div class="console-page service-navigation-page">
        <header class="page-heading service-page-heading">
            <div class="service-heading-identity"><span class="service-page-mark"><i class="pi pi-th-large" aria-hidden="true"></i></span><div><h1>{{ t('navigation') }}</h1><p class="page-description">{{ t('navigationHint') }}</p></div></div>
            <div class="service-heading-actions">
                <Button icon="pi pi-refresh" text severity="secondary" :aria-label="t('refresh')" :loading="loading" @click="load" />
                <RouterLink v-if="centralEnabled" :to="{ name: 'serviceManagement' }" class="service-manage-link"><i class="pi pi-sliders-h" aria-hidden="true"></i>{{ t('management') }}</RouterLink>
            </div>
        </header>
        <Message v-if="!centralEnabled" severity="info" :closable="false">{{ t('external') }}</Message>
        <template v-else>
            <Message v-if="error" severity="error" :closable="false"><div class="service-error">{{ error }}<Button :label="t('retry')" text size="small" @click="load" /></div></Message>
            <div v-if="services !== undefined && visibleServices.length" class="service-navigation-overview">
                <div><span>{{ t('entryCount') }}</span><strong>{{ visibleServices.length }}</strong></div>
                <div><span>{{ t('linkedNetworks') }}</span><strong>{{ linkedNetworkCount }}</strong></div>
                <div><span>{{ t('customGroups') }}</span><strong>{{ groups.length }}</strong></div>
                <Button :label="t('add')" icon="pi pi-plus" text size="small" @click="router.push({ name: 'serviceManagement', query: { create: '1' } })" />
            </div>
            <div v-if="visibleServices.length" class="service-navigation-toolbar">
                <div class="search-field"><i class="pi pi-search" aria-hidden="true"></i><InputText v-model="search" :placeholder="t('search')" :aria-label="t('search')" /></div>
                <Select :placeholder="t('allNetworks')" v-model="selectedNetwork" :options="networkOptions" option-label="label" option-value="value" :aria-label="t('network')" class="service-network-filter" />
            </div>
            <div v-if="groups.length > 1" class="service-group-filters" role="group" :aria-label="t('group')">
                <button type="button" :class="{ active: selectedGroup === null }" :aria-pressed="selectedGroup === null" @click="selectedGroup = null">{{ t('allGroups') }}</button>
                <button v-for="group in groups" :key="group" type="button" :class="{ active: selectedGroup === group }" :aria-pressed="selectedGroup === group" @click="selectedGroup = group">{{ group || t('ungrouped') }}</button>
            </div>
            <div v-if="loading && services === undefined" class="service-navigation-grid" :aria-label="t('loading')"><Skeleton v-for="i in 4" :key="i" height="190px" /></div>
            <section v-else-if="services !== undefined && visibleServices.length === 0" class="service-empty">
                <i class="pi pi-th-large" aria-hidden="true"></i><h2>{{ t('empty') }}</h2><p>{{ t('emptyHint') }}</p>
                <Button :label="t('add')" icon="pi pi-plus" @click="router.push({ name: 'serviceManagement', query: { create: '1' } })" />
            </section>
            <section v-else-if="services !== undefined && filtered.length === 0" class="service-empty">
                <i class="pi pi-search" aria-hidden="true"></i><h2>{{ t('noResults') }}</h2><p>{{ t('noResultsHint') }}</p><Button :label="t('clear')" outlined @click="clearFilters" />
            </section>
            <section v-for="section in sections" :key="section.name" class="service-group-section">
                <header><i class="pi pi-folder" aria-hidden="true"></i><h2>{{ section.name || t('ungrouped') }}</h2><span>{{ section.entries.length }}</span></header>
                <div class="service-navigation-grid"><ServiceCard v-for="service in section.entries" :key="service.id" :service="service" :network-name="networkNames.get(service.network_id)" @actions="openActions" /></div>
            </section>
            <RouterLink :to="{ name: 'networkMap' }" class="service-map-link">
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
.service-group-section { display: flex; flex-direction: column; gap: 15px; }
.service-group-section > header { display: flex; align-items: center; gap: 9px; }
.service-group-section > header > .pi { font-size: 13px; color: var(--console-muted); }
.service-group-section h2 { font-size: 14px; font-weight: 600; }
.service-group-section > header > span { font-size: 11px; color: var(--console-muted); padding: 1px 7px; border-radius: 5px; background: var(--console-panel-raised, var(--console-panel)); border: 1px solid var(--console-border); }
.service-navigation-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 290px), 1fr)); gap: 18px; }
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
