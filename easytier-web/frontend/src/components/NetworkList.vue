<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Button, Column, DataTable, Dialog, InputSwitch, InputText, Message, Skeleton, Tag, useToast } from 'primevue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { NetworkTypes, Polling, UrlListInput } from 'easytier-frontend-lib';
import ApiClient, { type CentralNetworkSettings, type CentralNetworkSummary, type GatewayInfo } from '../modules/api';
import { normalizeVirtualSubnet } from '../modules/virtual-subnet';
import { overviewMessages } from '../modules/overview-messages';

const { t, locale } = useI18n()
const copy = computed(() => overviewMessages[locale.value === 'cn' ? 'cn' : 'en']);
const route = useRoute();
const router = useRouter();
const toast = useToast();

const props = defineProps({
    api: ApiClient,
});

const api = props.api;

const networks = ref<CentralNetworkSummary[] | undefined>(undefined);
const gatewayInfo = ref<GatewayInfo | undefined>(undefined);
const advancedMode = ref(false);
const search = ref('');
const loadError = ref(false);
const refreshing = ref(false);
const filteredNetworks = computed(() => {
    const query = search.value.trim().toLocaleLowerCase();
    return (networks.value ?? []).filter(network =>
        `${network.display_name} ${network.network_name}`.toLocaleLowerCase().includes(query)
    ).sort((a, b) => a.display_name.localeCompare(b.display_name));
});

const gatewayEnabled = computed(() => gatewayInfo.value?.enabled === true);
const memberCount = computed(() => networks.value?.reduce((sum, network) => sum + network.member_count, 0));
const onlineMemberCount = computed(() => networks.value?.reduce((sum, network) => sum + network.online_member_count, 0));

const protos = NetworkTypes.PEER_PROTOCOLS;

const networkingMethodLabels: { [method: string]: () => string } = {
    PublicServer: () => t('public_server'),
    Manual: () => t('manual'),
    Standalone: () => t('standalone'),
    Gateway: () => t('web.network_list.gateway_mode'),
};

const networkingMethodLabel = (method: string) => {
    return networkingMethodLabels[method]?.() ?? method;
};

const createVisible = ref(false);
const creating = ref(false);
const createForm = ref({
    display_name: '',
    virtual_cidr: '',
    secure_mode: false,
});
// Initial nodes in advanced mode, same model as the GUI config form.
const initialNodes = ref<string[]>([]);

const loadNetworks = async () => {
    if (refreshing.value) return;
    refreshing.value = true;
    try {
        networks.value = await api?.list_networks();
        loadError.value = false;
    } catch (e) {
        loadError.value = true;
        console.error(e);
    } finally {
        refreshing.value = false;
    }
};

Polling.useAutoRefreshTask(loadNetworks, 2000);

onMounted(async () => {
    try {
        gatewayInfo.value = await api?.get_gateway_info();
    } catch (e) {
        gatewayInfo.value = { enabled: false, relay_data: false };
    }
});

const openCreate = () => {
    createForm.value = {
        display_name: '',
        virtual_cidr: '',
        // The built-in gateway speaks the Noise handshake; third-party
        // peers of advanced networking may not, so secure mode defaults on
        // for gateway networks only.
        secure_mode: gatewayEnabled.value,
    };
    advancedMode.value = !gatewayEnabled.value;
    initialNodes.value =
        gatewayEnabled.value && gatewayInfo.value?.peer_url ? [gatewayInfo.value.peer_url] : [];
    createVisible.value = true;
};

const createNetwork = async () => {
    creating.value = true;
    try {
        const virtual_cidr = normalizeVirtualSubnet(createForm.value.virtual_cidr) || null;
        let settings: CentralNetworkSettings;
        if (!advancedMode.value && gatewayEnabled.value) {
            settings = {
                display_name: createForm.value.display_name,
                network_name: null,
                networking_method: 'Gateway',
                public_server_url: null,
                peer_urls: [],
                virtual_cidr,
                secure_mode: createForm.value.secure_mode,
            };
        } else {
            settings = {
                display_name: createForm.value.display_name,
                network_name: null,
                networking_method: initialNodes.value.length > 0 ? 'Manual' : 'Standalone',
                public_server_url: null,
                peer_urls: initialNodes.value,
                virtual_cidr,
                secure_mode: createForm.value.secure_mode,
            };
        }
        const detail = await api?.create_network(settings);
        createVisible.value = false;
        await loadNetworks();
        if (detail) {
            router.push({ name: 'networkDetail', params: { ...route.params, networkId: detail.network_id } });
        }
    } catch (e: any) {
        toast.add({ severity: 'error', summary: t('web.network.create'), detail: e?.response?.data?.message ?? String(e), life: 5000 });
    } finally {
        creating.value = false;
    }
};

const toggleAdvancedMode = () => {
    advancedMode.value = !advancedMode.value;
    // Seed the gateway URL only when the list has not been edited, so
    // toggling back and forth does not drop user input.
    if (advancedMode.value && initialNodes.value.length === 0 && gatewayInfo.value?.peer_url) {
        initialNodes.value = [gatewayInfo.value.peer_url];
    }
    // Secure mode follows the networking method's default; users can flip
    // it again afterwards.
    createForm.value.secure_mode = !advancedMode.value;
};

const openNetwork = (network: CentralNetworkSummary) => {
    router.push({ name: 'networkDetail', params: { ...route.params, networkId: network.network_id } });
};
</script>

<template>
    <div class="console-page network-list-page">
        <header class="page-heading">
            <div><p class="network-eyebrow">{{ copy.networkEyebrow }}</p><h1>{{ t('web.network_list.title') }}</h1><p class="page-description">{{ copy.networkDescription }}</p></div>
            <div class="network-heading-actions"><RouterLink :to="{ name: 'networkMap' }" class="network-map-link"><i class="pi pi-globe" aria-hidden="true"></i>{{ copy.openMap }}</RouterLink><Button :label="t('web.network_list.create')" icon="pi pi-plus" @click="openCreate" /></div>
        </header>
        <Message v-if="loadError" severity="warn" :closable="false">{{ t('web.console.load_failed') }}</Message>
        <section class="network-summary" :aria-label="copy.networkOverview">
            <div><span class="network-summary-icon"><i class="pi pi-globe" aria-hidden="true"></i></span><div><span>{{ copy.totalNetworks }}</span><strong>{{ networks?.length ?? '—' }}</strong></div></div>
            <div :title="copy.networkMembershipHint"><span class="network-summary-icon"><i class="pi pi-sitemap" aria-hidden="true"></i></span><div><span>{{ copy.networkMembers }}</span><strong>{{ memberCount ?? '—' }}</strong></div></div>
            <div><span class="network-summary-icon is-online"><i class="pi pi-check-circle" aria-hidden="true"></i></span><div><span>{{ copy.onlineMembers }}</span><strong>{{ onlineMemberCount ?? '—' }}</strong></div></div>
        </section>
        <section class="console-panel network-inventory-panel">
            <div class="list-toolbar">
                <div class="search-field"><i class="pi pi-search" aria-hidden="true"></i><InputText v-model="search" :placeholder="t('web.console.search_networks')" :aria-label="t('web.console.search_networks')" /></div>
                <div class="flex items-center gap-3"><span v-if="networks" class="table-count">{{ t('web.console.items', { count: filteredNetworks.length }) }}</span><Button icon="pi pi-refresh" text severity="secondary" :aria-label="t('web.console.refresh')" :loading="refreshing" @click="loadNetworks" /></div>
            </div>
            <div v-if="networks === undefined && !loadError" class="loading-rows"><Skeleton v-for="i in 4" :key="i" height="2rem" /></div>
            <div v-else-if="networks === undefined" class="console-empty-state"><Button :label="t('web.console.retry')" severity="secondary" @click="loadNetworks" /></div>
            <div v-else-if="networks.length === 0" class="console-empty-state"><i class="pi pi-globe" aria-hidden="true"></i><h2>{{ t('web.console.networks_empty') }}</h2><p>{{ t('web.console.networks_empty_hint') }}</p><Button :label="t('web.network_list.create')" icon="pi pi-plus" @click="openCreate" /></div>
            <div v-else-if="filteredNetworks.length === 0" class="console-empty-state"><h2>{{ t('web.console.no_results') }}</h2><Button :label="t('web.console.clear_search')" text @click="search = ''" /></div>
            <template v-else>
                <DataTable :value="filteredNetworks" dataKey="network_id" class="console-table desktop-list" size="small" scrollable :paginator="filteredNetworks.length > 20" :rows="20">
                    <Column field="display_name" :header="t('web.network_list.display_name')" sortable><template #body="{ data }"><div class="network-table-identity"><i class="pi pi-globe entity-icon" aria-hidden="true"></i><div><button class="entity-link" @click="openNetwork(data)">{{ data.display_name }}</button><p class="preview-secondary mono-value">{{ data.network_name }}</p></div></div></template></Column>
                    <Column field="networking_method" :header="t('networking_method')"><template #body="{ data }"><Tag :value="networkingMethodLabel(data.networking_method)" severity="secondary" /></template></Column>
                    <Column field="online_member_count" :header="t('web.console.online_members')" sortable><template #body="{ data }"><div class="network-member-presence"><span>{{ data.online_member_count }} <span class="muted">/ {{ data.member_count }}</span></span><progress :value="data.online_member_count" :max="data.member_count || 1" :aria-label="copy.onlineMembers"></progress></div></template></Column>
                    <Column :header="t('web.console.manage')"><template #body="{ data }"><div class="network-row-actions"><RouterLink :to="{ name: 'networkMap', query: { network: data.network_id } }" :aria-label="`${copy.viewOnMap}: ${data.display_name}`" :title="copy.viewOnMap" class="network-row-map"><i class="pi pi-map-marker" aria-hidden="true"></i></RouterLink><Button icon="pi pi-arrow-up-right" text severity="secondary" :aria-label="`${t('web.console.manage')} ${data.display_name}`" @click="openNetwork(data)" /></div></template></Column>
                </DataTable>
                <div class="mobile-list">
                    <article v-for="network in filteredNetworks" :key="network.network_id" class="mobile-list-item">
                        <div class="flex items-center justify-between gap-3"><button class="entity-link" @click="openNetwork(network)"><i class="pi pi-globe entity-icon" aria-hidden="true"></i>{{ network.display_name }}</button><Tag :value="networkingMethodLabel(network.networking_method)" severity="secondary" /></div>
                        <div class="network-mobile-presence"><p class="preview-secondary">{{ t('web.console.online_members') }} · {{ network.online_member_count }} / {{ network.member_count }}</p><RouterLink :to="{ name: 'networkMap', query: { network: network.network_id } }" :aria-label="`${copy.viewOnMap}: ${network.display_name}`"><i class="pi pi-map-marker" aria-hidden="true"></i>{{ copy.viewOnMap }}</RouterLink></div>
                        <progress class="network-mobile-progress" :value="network.online_member_count" :max="network.member_count || 1" :aria-label="copy.onlineMembers"></progress>
                        <details><summary>{{ t('web.console.details') }}</summary><p class="mono-value mt-2">{{ network.network_name }}</p></details>
                    </article>
                </div>
            </template>
        </section>

        <Dialog v-model:visible="createVisible" modal class="console-dialog" :header="t('web.network_list.create')" :style="{ width: '32rem' }">
            <div class="flex flex-col gap-3">
                <div v-if="gatewayEnabled && !advancedMode" class="flex items-start gap-2 p-3 surface-100 rounded-md">
                    <i class="pi pi-check-circle text-green-500 mt-0.5"></i>
                    <div class="text-sm">
                        {{ t('web.network_list.gateway_hint') }}
                        <span v-if="gatewayInfo?.peer_url" class="block font-mono text-xs mt-1">{{ gatewayInfo.peer_url }}</span>
                    </div>
                </div>
                <div class="flex flex-col gap-1">
                    <label for="network-display-name">{{ t('web.network_list.display_name') }}</label>
                    <InputText id="network-display-name" v-model="createForm.display_name" />
                </div>
                <div class="flex flex-col gap-1">
                    <div class="flex items-center">
                        <label for="network-cidr">{{ t('web.network_list.virtual_cidr') }}</label>
                        <span class="pi pi-question-circle ml-2 self-center" v-tooltip="t('web.network_list.virtual_cidr_hint')"></span>
                    </div>
                    <InputText id="network-cidr" v-model="createForm.virtual_cidr" class="font-mono"
                        @blur="createForm.virtual_cidr = normalizeVirtualSubnet(createForm.virtual_cidr)"
                        :placeholder="t('web.network_list.virtual_cidr_placeholder')" />
                </div>
                <div class="flex items-center gap-2">
                    <InputSwitch inputId="create-secure-mode" v-model="createForm.secure_mode" />
                    <label for="create-secure-mode" class="text-sm">{{ t('web.network_list.secure_mode') }}</label>
                    <span class="pi pi-question-circle text-sm" v-tooltip.top="t('web.network_list.secure_mode_hint')"></span>
                </div>
                <template v-if="advancedMode || !gatewayEnabled">
                    <div class="flex items-center">
                        <label for="initial-nodes">{{ t('initial_nodes') }}</label>
                        <span class="pi pi-question-circle ml-2 self-center" v-tooltip="t('initial_nodes_help')"></span>
                    </div>
                    <div class="items-center flex flex-col p-fluid gap-y-2">
                        <UrlListInput id="initial-nodes" v-model="initialNodes" :protos="protos"
                            defaultUrl="tcp://:11010" :add-label="t('add_initial_node')"
                            :placeholder="t('initial_node_placeholder')" />
                    </div>
                </template>
                <div v-if="gatewayEnabled" class="text-sm">
                    <button type="button" class="text-primary text-left" @click="toggleAdvancedMode">
                        {{ advancedMode ? t('web.network_list.use_gateway') : t('web.network_list.advanced_mode') }}
                    </button>
                </div>
                <div class="text-sm text-500">{{ t('web.network_list.create_hint') }}</div>
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" icon="pi pi-times" text severity="secondary" @click="createVisible = false" />
                <Button :label="t('web.common.confirm')" icon="pi pi-check" :loading="creating" @click="createNetwork" />
            </template>
        </Dialog>
    </div>
</template>

<style scoped>
.network-eyebrow { margin: 0 0 8px; color: var(--p-primary-color); font-size: 10px; font-weight: 650; letter-spacing: .14em; }
.network-heading-actions { display: flex; align-items: center; gap: 12px; flex: none; }
.network-map-link { display: inline-flex; align-items: center; justify-content: center; gap: 8px; min-height: 40px; padding: 9px 12px; border: 1px solid var(--console-border); border-radius: 8px; color: var(--console-muted); background: var(--console-panel); font-size: 12px; transition: color .15s, border-color .15s; }
.network-map-link:hover { color: var(--p-primary-color); border-color: var(--p-primary-color); }
.network-map-link:focus-visible, .network-row-map:focus-visible, .network-mobile-presence > a:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; }
.network-summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
.network-summary > div { display: flex; align-items: center; gap: 16px; padding: 20px 22px; border: 1px solid var(--console-border); border-radius: 12px; background: var(--console-panel); min-width: 0; }
.network-summary-icon { display: inline-flex; width: 42px; height: 42px; align-items: center; justify-content: center; flex: none; border: 1px solid color-mix(in srgb, var(--p-primary-color) 20%, var(--console-border)); background: var(--console-accent-soft); border-radius: 11px; color: var(--p-primary-color); }
.network-summary-icon.is-online { color: var(--p-green-500); background: color-mix(in srgb, var(--p-green-500) 8%, var(--console-panel)); border-color: color-mix(in srgb, var(--p-green-500) 20%, var(--console-border)); }
.network-summary > div > div { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.network-summary span:not(.network-summary-icon) { font-size: 12px; color: var(--console-muted); }
.network-summary strong { font-size: 27px; font-weight: 550; line-height: 1.2; letter-spacing: -.035em; }
.network-table-identity { display: flex; align-items: center; gap: 12px; min-width: 0; }
.network-table-identity .entity-icon { width: 38px; height: 38px; border-radius: 10px; color: var(--p-primary-color); background: var(--console-accent-soft); }
.network-table-identity .preview-secondary { font-size: 10px; margin-top: 4px; }
.network-member-presence { max-width: 100px; min-width: 70px; display: flex; flex-direction: column; gap: 8px; font-variant-numeric: tabular-nums; }
progress { display: block; width: 100%; height: 4px; border: 0; border-radius: 8px; overflow: hidden; appearance: none; background: var(--console-border); }
progress::-webkit-progress-bar { background: var(--console-border); border-radius: 8px; }
progress::-webkit-progress-value { background: var(--p-primary-color); border-radius: 8px; }
progress::-moz-progress-bar { background: var(--p-primary-color); border-radius: 8px; }
.network-row-actions { display: flex; align-items: center; gap: 4px; }
.network-row-map { display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: 7px; color: var(--console-muted); }
.network-row-map:hover { background: var(--console-accent-soft); color: var(--p-primary-color); }
.network-mobile-presence { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-top: 12px; }
.network-mobile-presence .preview-secondary { margin: 0; }
.network-mobile-presence > a { display: inline-flex; gap: 6px; align-items: center; color: var(--p-primary-color); font-size: 11px; min-height: 32px; }
.network-mobile-progress { margin-top: 8px; height: 3px; }
@media (max-width: 1100px) { .network-list-page .page-heading { align-items: flex-start; } .network-heading-actions { flex-direction: column-reverse; align-items: stretch; } }
@media (max-width: 767px) { .network-list-page .page-heading { flex-direction: column; } .network-heading-actions { flex-direction: row; align-items: center; width: 100%; } .network-heading-actions > * { flex: 1; } .network-map-link { font-size: 11px; white-space: nowrap; } .network-summary { gap: 8px; } .network-summary > div { padding: 14px 12px; gap: 8px; flex-direction: column; align-items: flex-start; } .network-summary-icon { width: 28px; height: 28px; border-radius: 7px; font-size: 12px; } .network-summary strong { font-size: 24px; } .network-summary span:not(.network-summary-icon) { font-size: 11px; } }
@media (prefers-reduced-motion: reduce) { .network-map-link { transition: none; } }
</style>
