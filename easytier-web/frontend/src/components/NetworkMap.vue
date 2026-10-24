<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter, type LocationQueryRaw } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Button, Message, Skeleton, useToast } from 'primevue';
import { Polling, Utils } from 'easytier-frontend-lib';
import type ApiClient from '../modules/api';
import type { MapLanDevice, MapNodeConfig, MapSceneLink, MapSceneNode } from '../modules/network-space';
import type { MapCounter, MapTopologyLink, MapTopologyRoute } from '../modules/map-topology';
import { buildRegisteredMapTopology, type RegisteredMapRuntimeSample } from '../modules/registered-map-topology';
import { buildNodeSpaceDevices } from '../modules/node-map-assets';
import { mapMessages } from '../modules/map-messages';
import type { AssetContextResponse, AssetDevice, AssetNode, AssetServiceCatalogEntry, DeviceType, GatewayGroup } from '../types/node-assets';
import DeviceAttributesDialog from './DeviceAttributesDialog.vue';

const NetworkGlobe = defineAsyncComponent(() => import('./map/NetworkGlobe.vue'));
const NodeSpace = defineAsyncComponent(() => import('./map/NodeSpace.vue'));
const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t, locale } = useI18n({ useScope: 'local', messages: mapMessages });
const route = useRoute();
const router = useRouter();
const toast = useToast();
const autoRefreshEnabled = Polling.useAutoRefresh();
const machines = ref<Utils.DeviceInfo[]>([]);
const context = ref<AssetContextResponse | null>(null);
const assetNodes = ref<AssetNode[]>([]);
const assetDevices = ref<AssetDevice[]>([]);
const groups = ref<GatewayGroup[]>([]);
const types = ref<DeviceType[]>([]);
const services = ref<AssetServiceCatalogEntry[]>([]);
const loading = ref(false);
const loaded = ref(false);
const assetsLoading = ref(false);
const collecting = ref(false);
const loadError = ref('');
const configurationError = ref(false);
const assetError = ref(false);
const serviceError = ref(false);
const runtimeError = ref(false);
const membersStale = ref(false);
const selectedId = ref<string | null>(null);
const selectedDeviceId = ref<string | null>(null);
const selectedLinkId = ref<string | null>(null);
const selectedRouteId = ref<string | null>(null);
const mode = ref<'globe' | 'space'>('globe');
const spaceLayout = ref<'topology' | 'home'>('topology');
const search = ref('');
const onlyUnlocated = ref(false);
const rendererError = ref(false);
const links = ref<MapTopologyLink[]>([]);
const paths = ref<MapTopologyRoute[]>([]);
const unknownPeers = ref(0);
const lastUpdate = ref<number | null>(null);
const virtualIps = ref<Record<string, string>>({});
const assetGatewayIds = ref<Set<string>>(new Set());
const runtimeGatewayIds = ref<Set<string>>(new Set());
const gatewayIds = computed(() => new Set([...assetGatewayIds.value, ...runtimeGatewayIds.value]));
const attributesTarget = ref<Utils.DeviceInfo | null>(null);
const attributesNode = ref<AssetNode | null>(null);
let samples = new Map<string, RegisteredMapRuntimeSample>();
let counters = new Map<string, MapCounter>();
let generation = 0;
let assetGeneration = 0;
let metadataGeneration = 0;
let mounted = true;
let polling: number | undefined;
let pollInFlight = false;
let runtimeGeneration: number | null = null;
const queryString = (value: unknown) => typeof value === 'string' ? value : '';
const errorText = (error: unknown, fallback: string) => (error as { response?: { data?: { message?: string } } })?.response?.data?.message || fallback;
const sampleKey = (machineId: string, instanceId: string) => `${machineId}:${instanceId}`;
const members = computed(() => machines.value.map(machine => ({ ...machine, device_id: machine.machine_id,
    online: machine.online === true, runtime_virtual_ipv4: virtualIps.value[machine.machine_id] || '' })));
const selectedMember = computed(() => members.value.find(member => member.device_id === selectedId.value));
const nameOf = (deviceId: string) => {
    const member = members.value.find(item => item.device_id === deviceId);
    return member?.alias || member?.hostname || deviceId;
};
const configFor = (deviceId: string): MapNodeConfig => {
    const member = members.value.find(item => item.device_id === deviceId);
    return { device_id: deviceId, latitude: member?.geo_location?.latitude ?? null,
        longitude: member?.geo_location?.longitude ?? null, label: nameOf(deviceId),
        role: gatewayIds.value.has(deviceId) ? 'gateway' : 'node', layout: spaceLayout.value };
};
const hasPosition = (node: MapNodeConfig) => node.latitude != null && node.longitude != null
    && Number.isFinite(node.latitude) && Math.abs(node.latitude) <= 90
    && Number.isFinite(node.longitude) && Math.abs(node.longitude) <= 180;
const selectedConfig = computed(() => selectedId.value ? configFor(selectedId.value) : null);
const isGateway = computed(() => selectedConfig.value?.role === 'gateway');
const configurationReady = computed(() => !!context.value?.available && assetNodes.value.some(node => node.machine_id === selectedId.value));
const canViewSpace = computed(() => !!selectedId.value && !!context.value?.available);
const spaceDevices = computed(() => buildNodeSpaceDevices(selectedId.value || '', assetDevices.value, groups.value, types.value));
const localDevices = computed(() => spaceDevices.value.filter(device => !!device.asset_device_id));
const selectedDevice = computed(() => localDevices.value.find(device => device.id === selectedDeviceId.value));
const associatedServices = computed(() => services.value.filter(service => service.machine_id === selectedId.value
    && (!selectedDeviceId.value || service.device_id === selectedDeviceId.value)));
const selectedLink = computed(() => links.value.find(link => link.id === selectedLinkId.value));
const selectedPath = computed(() => paths.value.find(path => path.id === selectedRouteId.value));
const nodeLinks = computed(() => links.value.filter(link => link.source === selectedId.value || link.target === selectedId.value));
const nodePaths = computed(() => paths.value.filter(path => path.source === selectedId.value));
const unlocatedCount = computed(() => members.value.filter(member => !hasPosition(configFor(member.device_id))).length);
const filteredMembers = computed(() => members.value.filter(member => {
    const words = `${nameOf(member.device_id)} ${member.runtime_virtual_ipv4} ${member.device_model || ''}`.toLocaleLowerCase();
    return (!onlyUnlocated.value || !hasPosition(configFor(member.device_id))) && words.includes(search.value.trim().toLocaleLowerCase());
}));
const sceneNodes = computed<MapSceneNode[]>(() => members.value.flatMap(member => {
    const node = configFor(member.device_id);
    return hasPosition(node) ? [{ id: member.device_id, name: nameOf(member.device_id), latitude: node.latitude!, longitude: node.longitude!, online: member.online, role: node.role }] : [];
}));
const sceneLinks = computed<MapSceneLink[]>(() => {
    const located = new Set(sceneNodes.value.map(node => node.id));
    const base = links.value.filter(link => located.has(link.source) && located.has(link.target));
    const path = selectedPath.value;
    if (!path) return base;
    const segments = new Set(path.path.slice(1).map((target, i) => [path.path[i], target].sort().join(':')));
    return base.map(link => segments.has([link.source, link.target].sort().join(':')) ? { ...link, kind: 'relay', stale: link.stale || path.stale } : link);
});

function syncQuery() {
    const query: LocationQueryRaw = { ...route.query, node: selectedId.value || undefined, device: selectedDeviceId.value || undefined };
    // Older map links may contain a central-network filter. It does not scope registered nodes.
    delete query.network;
    delete query.owner;
    if (JSON.stringify(query) !== JSON.stringify(route.query)) void router.replace({ query });
}
function selectNode(id: string, updateQuery = true) {
    if (!members.value.some(member => member.device_id === id)) return;
    if (selectedId.value !== id) {
        selectedDeviceId.value = null;
        selectedLinkId.value = null;
        selectedRouteId.value = null;
        mode.value = 'globe';
    }
    selectedId.value = id;
    if (updateQuery) syncQuery();
}
function selectDevice(id: string) {
    if (!localDevices.value.some(device => device.id === id)) return;
    selectedDeviceId.value = selectedDeviceId.value === id ? null : id;
    syncQuery();
}
function selectLink(id: string) { selectedLinkId.value = id; selectedRouteId.value = null; }
function expireTelemetry(force = false) {
    const now = Date.now();
    links.value = links.value.map(link => force || now - link.sampledAt >= 45_000 ? { ...link, stale: true, rx_bps: 0, tx_bps: 0 } : link);
    if (force) paths.value = paths.value.map(path => ({ ...path, stale: true }));
    else {
        const previouslyStale = new Set(paths.value.filter(path => path.stale).map(path => path.id));
        paths.value = buildRegisteredMapTopology(members.value, [...samples.values()], counters, now).routes
            .map(path => ({ ...path, stale: path.stale || previouslyStale.has(path.id) }));
    }
}
function canRefreshAutomatically() { return mounted && autoRefreshEnabled.value && !document.hidden && props.centralEnabled; }
function visibilityChanged() {
    expireTelemetry(true);
    counters.clear();
    if (canRefreshAutomatically()) void poll();
}

async function loadMetadata() {
    const current = ++metadataGeneration;
    try {
        const nextContext = await props.api.assetsContext();
        if (!mounted || current !== metadataGeneration) return;
        context.value = nextContext;
        if (!nextContext.available) { assetNodes.value = []; services.value = []; types.value = []; return; }
        const results = await Promise.allSettled([
            props.api.assetsNodes(nextContext.actor.id), props.api.assetsTypes(nextContext.actor.id), props.api.assetsServiceCatalog(nextContext.actor.id),
        ]);
        if (!mounted || current !== metadataGeneration) return;
        const [nodesResult, typesResult, servicesResult] = results;
        configurationError.value = nodesResult.status === 'rejected';
        assetNodes.value = nodesResult.status === 'fulfilled' ? nodesResult.value.items : [];
        types.value = typesResult.status === 'fulfilled' ? typesResult.value.items : [];
        serviceError.value = servicesResult.status === 'rejected';
        services.value = servicesResult.status === 'fulfilled' ? servicesResult.value.items : [];
    } catch {
        if (mounted && current === metadataGeneration) {
            context.value = null; assetNodes.value = []; services.value = []; types.value = [];
            configurationError.value = true; serviceError.value = true;
        }
    }
}
async function loadSelectedAssets() {
    const current = ++assetGeneration;
    const nodeId = selectedId.value;
    const owner = context.value?.available ? context.value.actor.id : undefined;
    assetDevices.value = []; groups.value = []; assetError.value = false;
    if (!nodeId || owner === undefined) { assetsLoading.value = false; return; }
    assetsLoading.value = true;
    const active = () => mounted && current === assetGeneration && selectedId.value === nodeId && context.value?.actor.id === owner;
    const scope = { owner_user_id: owner, machine_id: nodeId };
    try {
        const [groupResult, firstPage] = await Promise.all([props.api.assetsGroups(scope), props.api.assetsDevices(scope, { page: 1, page_size: 100 })]);
        if (!active()) return;
        const devices = [...firstPage.items];
        // Respect server pagination; nodes with more than one page remain complete.
        const pages = Math.ceil(firstPage.total / firstPage.page_size);
        for (let page = 2; page <= pages; page++) {
            if (!active()) return;
            const result = await props.api.assetsDevices(scope, { page, page_size: firstPage.page_size });
            if (!active()) return;
            devices.push(...result.items);
        }
        groups.value = groupResult.items;
        assetDevices.value = [...new Map(devices.map(device => [device.id, device])).values()];
        const nextGateways = new Set(assetGatewayIds.value);
        if (groups.value.length) nextGateways.add(nodeId);
        else nextGateways.delete(nodeId);
        assetGatewayIds.value = nextGateways;
        const requested = queryString(route.query.device);
        selectedDeviceId.value = devices.some(device => device.id === requested) ? requested : null;
        syncQuery();
    } catch {
        if (active()) { assetError.value = true; selectedDeviceId.value = null; }
    } finally { if (active()) assetsLoading.value = false; }
}
function acceptMachines(result: unknown[]) {
    machines.value = result.map(Utils.buildDeviceInfo);
    membersStale.value = false;
    const requested = queryString(route.query.node) || selectedId.value;
    const next = members.value.find(member => member.device_id === requested)?.device_id
        || members.value.find(member => member.online)?.device_id || members.value[0]?.device_id || null;
    if (next !== selectedId.value) {
        selectedDeviceId.value = null; selectedLinkId.value = null; selectedRouteId.value = null; mode.value = 'globe';
        selectedId.value = next;
    }
    if (!next) syncQuery();
}
async function refresh() {
    if (!props.centralEnabled || loading.value) return;
    const current = ++generation;
    loading.value = true; loadError.value = '';
    const [nodesResult] = await Promise.allSettled([props.api.list_machines(), loadMetadata()]);
    if (!mounted || current !== generation) return;
    if (nodesResult.status === 'fulfilled') { acceptMachines(nodesResult.value); loaded.value = true; }
    else { loadError.value = errorText(nodesResult.reason, t('loadFailed')); membersStale.value = true; expireTelemetry(true); }
    loading.value = false;
    await loadSelectedAssets();
    if (nodesResult.status === 'fulfilled' && mounted && current === generation) void collectRuntime(current);
}

async function collectRuntime(current = generation, canContinue = () => true) {
    if (!canContinue() || loading.value || runtimeGeneration === current || document.hidden) return;
    runtimeGeneration = current; collecting.value = true;
    try {
        const queue = members.value.filter(member => member.online).flatMap(member =>
            [...new Set(member.running_network_instances || [])].map(instanceId => ({ member, instanceId })));
        const next = new Map<string, RegisteredMapRuntimeSample>();
        const addresses: Record<string, Set<string>> = {};
        const gateways = new Set<string>();
        let failed = false; let cursor = 0;
        const active = () => mounted && current === generation && canContinue();
        const worker = async () => {
            while (cursor < queue.length && active()) {
                const { member, instanceId } = queue[cursor++];
                const results = await Promise.allSettled([
                    props.api.get_node_info(member.device_id, instanceId), props.api.get_node_routes(member.device_id, instanceId), props.api.get_node_peers(member.device_id, instanceId),
                ]);
                if (!active()) return;
                const [info, routes, peers] = results;
                const key = sampleKey(member.device_id, instanceId);
                const previous = samples.get(key);
                const partial = results.some(result => result.status === 'rejected');
                failed ||= partial;
                if (info.status === 'fulfilled') {
                    if (info.value.ipv4_addr) (addresses[member.device_id] ??= new Set()).add(info.value.ipv4_addr);
                    if (info.value.proxy_cidrs?.length) gateways.add(member.device_id);
                }
                next.set(key, { deviceId: member.device_id, instanceId,
                    peerId: info.status === 'fulfilled' ? info.value.peer_id : previous?.peerId,
                    routes: routes.status === 'fulfilled' ? routes.value : previous?.routes ?? [],
                    peers: peers.status === 'fulfilled' ? peers.value : previous?.peers ?? [],
                    sampledAt: partial ? previous?.sampledAt ?? Date.now() : Date.now(), failed: partial });
            }
        };
        await Promise.all(Array.from({ length: Math.min(3, queue.length) }, worker));
        if (!active()) return;
        const knownNodes = new Set(members.value.map(member => member.device_id));
        for (const [key, previous] of samples) {
            if (knownNodes.has(previous.deviceId) && !next.has(key)) next.set(key, { ...previous, failed: true });
        }
        samples = next;
        const topology = buildRegisteredMapTopology(members.value, [...samples.values()], counters);
        links.value = topology.links; paths.value = topology.routes; counters = topology.counters;
        unknownPeers.value = topology.unresolvedPeers; runtimeGatewayIds.value = gateways;
        virtualIps.value = Object.fromEntries(Object.entries(addresses).map(([id, values]) => [id, [...values].join(' · ')]));
        runtimeError.value = failed || membersStale.value;
        if (queue.length && !failed) lastUpdate.value = Date.now();
        if (document.hidden) expireTelemetry(true);
    } catch {
        if (mounted && current === generation && canContinue()) { runtimeError.value = true; expireTelemetry(true); }
    } finally {
        if (runtimeGeneration === current) { runtimeGeneration = null; if (current === generation) collecting.value = false; }
    }
}
async function poll() {
    if (!canRefreshAutomatically() || loading.value || pollInFlight || runtimeGeneration === generation) return;
    const current = generation;
    pollInFlight = true;
    try {
        const result = await props.api.list_machines();
        if (!canRefreshAutomatically() || current !== generation) return;
        acceptMachines(result);
        await collectRuntime(current, canRefreshAutomatically);
    } catch {
        if (canRefreshAutomatically() && current === generation) { runtimeError.value = true; membersStale.value = true; expireTelemetry(true); }
    } finally { pollInFlight = false; }
}
watch(selectedId, () => { if (!loading.value) void loadSelectedAssets(); }, { flush: 'sync' });
watch(() => props.centralEnabled, enabled => {
    if (enabled) void refresh();
    else {
        generation++; metadataGeneration++; assetGeneration++;
        machines.value = []; assetNodes.value = []; services.value = []; assetDevices.value = []; groups.value = [];
        context.value = null; samples.clear(); counters.clear(); links.value = []; paths.value = [];
        assetGatewayIds.value = new Set(); runtimeGatewayIds.value = new Set();
        selectedId.value = null; selectedDeviceId.value = null; loading.value = false; collecting.value = false;
    }
}, { immediate: true });
watch(() => [route.query.node, route.query.device], () => {
    const node = queryString(route.query.node);
    if (node && node !== selectedId.value) { selectNode(node, false); return; }
    const device = queryString(route.query.device);
    if (!assetsLoading.value) selectedDeviceId.value = localDevices.value.some(item => item.id === device) ? device : null;
});
watch(autoRefreshEnabled, enabled => { if (!enabled) { expireTelemetry(true); counters.clear(); } else if (canRefreshAutomatically()) void poll(); });
onMounted(() => {
    try { spaceLayout.value = localStorage.getItem('easytier.map.space-view') === 'home' ? 'home' : 'topology'; } catch { /* View preferences are optional. */ }
    document.addEventListener('visibilitychange', visibilityChanged);
    polling = window.setInterval(() => { expireTelemetry(); void poll(); }, 15_000);
});
watch(spaceLayout, value => { try { localStorage.setItem('easytier.map.space-view', value); } catch { /* Optional preference. */ } });
onBeforeUnmount(() => { mounted = false; generation++; assetGeneration++; metadataGeneration++; window.clearInterval(polling); document.removeEventListener('visibilitychange', visibilityChanged); });

function editNode(id = selectedId.value) {
    const machine = machines.value.find(node => node.machine_id === id);
    const node = assetNodes.value.find(node => node.machine_id === id);
    if (!machine || !node) return;
    selectNode(machine.machine_id);
    attributesTarget.value = { ...machine }; attributesNode.value = { ...node };
}
function attributesUpdated(machineId: string, changes: Partial<Utils.DeviceInfo> & { is_public_server?: boolean }) {
    // Fence reads that started before this confirmed write.
    generation++; metadataGeneration++;
    const machine = machines.value.find(node => node.machine_id === machineId);
    if (machine) Object.assign(machine, changes);
    const node = assetNodes.value.find(node => node.machine_id === machineId);
    if (node) Object.assign(node, changes);
    collecting.value = false; loading.value = false;
}
function attributesSaved() { attributesTarget.value = null; attributesNode.value = null; void refresh(); }
function editDevice(device?: MapLanDevice) {
    if (!selectedId.value || !context.value?.available) return;
    void router.push({ name: 'nodeAssets', query: { owner: String(context.value.actor.id), node: selectedId.value,
        ...(device?.asset_device_id ? { device: device.asset_device_id } : {}) } });
}
function openService(service: AssetServiceCatalogEntry) {
    try {
        const url = new URL(service.access_url || '');
        if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('invalid');
        window.open(url.href, '_blank', 'noopener,noreferrer');
    } catch { toast.add({ severity: 'error', summary: t('invalidService'), life: 4000 }); }
}
const serviceIcons: Record<string, string> = { home: 'pi-home', server: 'pi-server', globe: 'pi-globe', database: 'pi-database', mobile: 'pi-mobile', desktop: 'pi-desktop', cloud: 'pi-cloud', cog: 'pi-cog', play: 'pi-play', link: 'pi-link', shield: 'pi-shield', wifi: 'pi-wifi' };
const deviceIcons: Record<string, string> = { server: 'pi-server', desktop: 'pi-desktop', camera: 'pi-video', router: 'pi-wifi', other: 'pi-box' };
const serviceIcon = (icon: string | null) => serviceIcons[icon || 'globe'] || 'pi-globe';
const deviceIcon = (kind: string) => deviceIcons[kind] || 'pi-box';
const formatRate = (value: number) => value >= 1_000_000 ? `${(value / 1_000_000).toFixed(1)} MB/s` : value >= 1000 ? `${(value / 1000).toFixed(1)} kB/s` : `${Math.round(value)} B/s`;
const formatLatency = (value: number | null) => value == null ? '—' : `${value.toFixed(1)} ms`;
const serviceQuery = computed(() => ({ node: selectedId.value || undefined, device: selectedDeviceId.value || undefined }));
</script>

<template>
    <div class="console-page network-map-page">
        <header class="page-heading map-heading">
            <div class="map-heading-identity"><span class="map-heading-mark"><i class="pi pi-globe" aria-hidden="true"></i></span><div><h1>{{ t('title') }}</h1><p class="page-description">{{ t('subtitle') }}</p></div></div>
            <div class="map-header-actions">
                <Button v-if="centralEnabled" icon="pi pi-refresh" :aria-label="t('refresh')" :label="t('refresh')" severity="secondary" outlined :loading="loading" :disabled="!!attributesTarget" @click="refresh" />
            </div>
        </header>
        <Message v-if="!centralEnabled" severity="info" :closable="false">{{ t('centralOnly') }}</Message>
        <template v-else>
            <Message v-if="loadError" severity="error" :closable="false"><div class="map-error-row">{{ loadError }}<Button :label="t('retry')" text size="small" @click="refresh" /></div></Message>
            <Message v-if="configurationError" severity="warn" :closable="false">{{ t('configurationFailed') }}</Message>
            <Message v-if="runtimeError" severity="warn" :closable="false">{{ t('runtimePartial') }}</Message>
            <div v-if="loading && !loaded" class="map-loading"><Skeleton width="100%" height="28rem" /></div>
            <section v-else-if="loaded && !members.length && !loadError" class="console-empty-state console-panel">
                <i class="pi pi-globe" aria-hidden="true"></i><h2>{{ t('noMembers') }}</h2>
                <RouterLink :to="{ name: 'deviceList' }" class="entity-link">{{ t('addNodes') }}<i class="pi pi-arrow-right" aria-hidden="true"></i></RouterLink>
            </section>
            <template v-else-if="members.length">
                <div class="map-network-summary" :aria-label="t('networkOverview')">
                    <span><i class="pi pi-map-marker" aria-hidden="true"></i>{{ t('locatedNodes') }}<strong>{{ sceneNodes.length }}<small>/ {{ members.length }}</small></strong></span>
                    <span><i class="pi pi-share-alt" aria-hidden="true"></i>{{ t('visibleConnections') }}<strong>{{ sceneLinks.length }}</strong></span>
                    <span v-if="unlocatedCount" class="map-unlocated-summary"><i class="pi pi-compass" aria-hidden="true"></i>{{ t('unlocated') }}<strong>{{ unlocatedCount }}</strong></span>
                </div>
                <div class="map-workbench">
                    <section class="map-stage" :aria-label="mode === 'globe' ? t('globe') : t('space')">
                        <div class="map-scene-toolbar">
                            <div class="map-view-tabs" role="group" :aria-label="t('layout')">
                                <button :class="{ active: mode === 'globe' }" :aria-pressed="mode === 'globe'" @click="mode = 'globe'"><i class="pi pi-globe" aria-hidden="true"></i>{{ t('globe') }}</button>
                                <button :class="{ active: mode === 'space' }" :aria-pressed="mode === 'space'" :disabled="!canViewSpace" @click="mode = 'space'"><i class="pi pi-box" aria-hidden="true"></i>{{ t('space') }}</button>
                            </div>
                            <span class="map-live-label"><i :class="['pi', collecting ? 'pi-spin pi-spinner' : 'pi-circle-fill']" aria-hidden="true"></i>{{ collecting ? t('collecting') : t('nodeCount', { count: members.length }) }}</span>
                        </div>
                        <NetworkGlobe v-show="mode === 'globe'" class="map-scene" :nodes="sceneNodes" :links="sceneLinks" :selected-node-id="selectedId" :selected-link-id="selectedLinkId" :locale="locale" @select-node="selectNode" @select-link="selectLink" @unavailable="rendererError = true" />
                        <NodeSpace v-if="mode === 'space' && selectedId" class="map-scene" :devices="spaceDevices" :gateway-name="nameOf(selectedId)" :selected-device-id="selectedDeviceId" :layout="selectedConfig?.layout || 'topology'" :locale="locale" @select-device="selectDevice" />
                        <div v-if="mode === 'space'" class="map-space-caption"><strong>{{ selectedId ? nameOf(selectedId) : '' }}</strong><span>{{ selectedConfig?.layout === 'home' ? t('template') : t('configuredRelation') }}</span></div>
                        <div v-if="loading" class="map-scene-notice" role="status"><i class="pi pi-spin pi-spinner" aria-hidden="true"></i>{{ t('loading') }}</div>
                        <div v-else-if="rendererError && mode === 'globe'" class="map-scene-notice" role="status">{{ t('rendererFailed') }}</div>
                        <div v-else-if="!sceneNodes.length && mode === 'globe'" class="map-scene-notice"><i class="pi pi-map-marker" aria-hidden="true"></i><strong>{{ members.length ? t('noLocated') : t('noMembers') }}</strong><p>{{ members.length ? t('locationHint') : '' }}</p><Button v-if="selectedId" :label="t('setPosition')" icon="pi pi-map-marker" size="small" :disabled="!configurationReady" @click="editNode()" /><RouterLink v-else :to="{ name: 'deviceList' }">{{ t('addNodes') }}</RouterLink></div>
                        <div class="map-canvas-footer">
                            <div class="map-legend"><span><i class="legend-direct"></i>{{ t('direct') }}</span><span><i class="legend-relay"></i>{{ t('relay') }}</span><span><i class="legend-stale"></i>{{ t('stale') }}</span></div>
                        </div>
                    </section>

                    <aside class="map-inspector" :aria-label="t('selectNode')">
                        <template v-if="selectedMember && selectedConfig">
                            <header class="map-inspector-heading"><span class="map-node-avatar"><i :class="['pi', isGateway ? 'pi-wifi' : 'pi-server']" aria-hidden="true"></i></span><div class="map-node-identity"><span class="map-eyebrow">{{ isGateway ? t('gateway') : t('node') }}</span><h2>{{ nameOf(selectedMember.device_id) }}</h2></div><span class="map-presence" :class="{ online: selectedMember.online }"><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ selectedMember.online ? t('online') : t('offline') }}</span></header>
                            <div class="map-coordinate-line"><i class="pi pi-map-marker" aria-hidden="true"></i><span v-if="hasPosition(selectedConfig)">{{ selectedConfig.latitude?.toFixed(4) }}°, {{ selectedConfig.longitude?.toFixed(4) }}°</span><span v-else>{{ t('unset') }}</span><button :disabled="!configurationReady" :title="t('editNode')" :aria-label="t('editNode')" @click="editNode()"><i class="pi pi-pencil" aria-hidden="true"></i></button></div>
                            <dl class="map-node-facts"><div><dt>{{ t('virtualIp') }}</dt><dd>{{ selectedMember.runtime_virtual_ipv4 || '—' }}</dd></div></dl>
                            <template v-if="canViewSpace">
                                <div v-if="mode === 'globe'" class="map-space-preview"><NodeSpace :devices="spaceDevices" :gateway-name="nameOf(selectedMember.device_id)" :selected-device-id="selectedDeviceId" :layout="selectedConfig.layout" :locale="locale" @select-device="selectDevice" /></div>
                                <Button v-if="mode === 'globe'" :label="t('viewSpace')" icon="pi pi-arrow-up-right" severity="secondary" outlined fluid @click="mode = 'space'" />
                                <Button v-else :label="t('back')" icon="pi pi-arrow-left" severity="secondary" outlined fluid @click="mode = 'globe'" />
                            </template>
                            <section v-if="canViewSpace" class="map-detail-section">
                                <header><h3><i class="pi pi-sitemap" aria-hidden="true"></i>{{ t('devices') }}</h3><button :disabled="!configurationReady" :aria-label="t('addDevice')" :title="t('addDevice')" @click="editDevice()"><i class="pi pi-plus" aria-hidden="true"></i></button></header>
                                <p class="map-section-note">{{ t('configuredRelation') }}</p>
                                <Message v-if="assetError" severity="warn" :closable="false">{{ t('assetsFailed') }}<Button :label="t('retry')" text size="small" @click="loadSelectedAssets" /></Message>
                                <p v-if="assetsLoading" class="map-empty-note" role="status">{{ t('loadingDevices') }}</p>
                                <label class="map-space-layout">{{ t('layout') }}<select v-model="spaceLayout" class="map-native-select"><option value="topology">{{ t('topology') }}</option><option value="home">{{ t('home') }}</option></select></label>
                                <p v-if="!assetsLoading && !assetError && !localDevices.length" class="map-empty-note">{{ t('noDevices') }}</p>
                                <div v-for="device in localDevices" :key="device.id" class="map-device-row" :class="{ selected: device.id === selectedDeviceId }">
                                    <button class="map-device-select" :aria-pressed="device.id === selectedDeviceId" @click="selectDevice(device.id)"><i :class="['pi', deviceIcon(device.kind)]" aria-hidden="true"></i><span><strong>{{ device.name }}</strong><small>{{ device.group_name || t('directDevice') }} · {{ device.address }}</small></span></button>
                                    <button class="map-icon-button" :aria-label="`${t('edit')} ${device.name}`" :disabled="!configurationReady" @click="editDevice(device)"><i class="pi pi-pencil" aria-hidden="true"></i></button>

                                </div>
                                <button v-if="selectedDevice" class="map-text-link" @click="selectedDeviceId = null; syncQuery()">{{ t('backToGateway') }}</button>
                            </section>
                            <section class="map-detail-section">
                                <header><h3><i class="pi pi-th-large" aria-hidden="true"></i>{{ selectedDevice ? `${selectedDevice.name} · ${t('services')}` : t('services') }}</h3><RouterLink :to="{ name: 'serviceManagement', query: serviceQuery }" :aria-label="t('allServices')"><i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink></header>
                                <Message v-if="serviceError" severity="warn" :closable="false">{{ t('servicesFailed') }}</Message>
                                <p v-else-if="!associatedServices.length" class="map-empty-note">{{ t('noServices') }}</p>
                                <div class="map-service-grid"><button v-for="service in associatedServices" :key="service.id" class="map-service-card" :disabled="!service.access_url" @click="openService(service)"><i :class="['pi', serviceIcon(service.icon), 'map-service-icon']" aria-hidden="true"></i><i class="pi pi-arrow-up-right map-service-arrow" aria-hidden="true"></i><strong>{{ service.name }}</strong><small>{{ service.device_name }}</small></button></div>
                                <RouterLink :to="{ name: 'serviceManagement', query: serviceQuery }" class="map-text-link"><i class="pi pi-plus" aria-hidden="true"></i>{{ t('manageServices') }}</RouterLink>
                            </section>
                            <section class="map-detail-section">
                                <header><h3><i class="pi pi-share-alt" aria-hidden="true"></i>{{ t('links') }}</h3><span class="map-section-count">{{ nodeLinks.length }}</span></header>
                                <p v-if="!nodeLinks.length" class="map-empty-note">{{ collecting ? t('collecting') : t('noLinks') }}</p>
                                <button v-for="link in nodeLinks" :key="link.id" class="map-link-row" :class="{ selected: selectedLinkId === link.id }" @click="selectLink(link.id)"><span><i class="pi pi-share-alt" aria-hidden="true"></i>{{ nameOf(link.source === selectedId ? link.target : link.source) }}</span><small>{{ link.stale ? t('stale') : formatLatency(link.latency_ms) }}</small></button>
                                <template v-if="nodePaths.length"><h4 class="map-route-heading">{{ t('routes') }}</h4><button v-for="path in nodePaths" :key="path.id" class="map-link-row" :class="{ selected: selectedRouteId === path.id }" @click="selectedRouteId = path.id; selectedLinkId = null"><span><i class="pi pi-directions" aria-hidden="true"></i>{{ nameOf(path.target) }}</span><small>{{ path.stale ? t('stale') : formatLatency(path.latency_ms) }}</small></button></template>
                            </section>
                            <footer class="map-inspector-actions"><Button :label="t('editNode')" icon="pi pi-sliders-h" severity="secondary" outlined :disabled="!configurationReady" @click="editNode()" /><RouterLink :to="{ name: 'deviceManagement', params: { deviceId: selectedMember.device_id, instanceId: selectedMember.running_network_instances?.[0] } }" class="map-text-link">{{ t('details') }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink></footer>
                        </template>
                        <div v-else class="map-inspector-empty"><i class="pi pi-map-marker" aria-hidden="true"></i><h2>{{ t('selectNode') }}</h2><p>{{ t('selectHint') }}</p></div>
                    </aside>
                </div>

                <section v-if="selectedLink || selectedPath" class="map-path-details" aria-live="polite">
                    <div class="map-path-heading"><strong v-if="selectedPath">{{ selectedPath.path.map(nameOf).join(' → ') }}<template v-if="!selectedPath.complete"> → … → {{ nameOf(selectedPath.target) }}</template></strong><strong v-else-if="selectedLink">{{ nameOf(selectedLink.source) }} ↔ {{ nameOf(selectedLink.target) }}</strong><button class="map-icon-button" :aria-label="t('close')" @click="selectedLinkId = null; selectedRouteId = null"><i class="pi pi-times" aria-hidden="true"></i></button></div>
                    <p v-if="selectedPath && !selectedPath.complete" class="map-path-warning">{{ t('pathMissing') }}</p>
                    <dl><div><dt>{{ t('latency') }}</dt><dd>{{ formatLatency(selectedPath?.latency_ms ?? selectedLink?.latency_ms ?? null) }}</dd></div><template v-if="selectedLink"><div><dt>{{ t('traffic') }}</dt><dd>↓ {{ formatRate(selectedLink.rx_bps) }} · ↑ {{ formatRate(selectedLink.tx_bps) }}</dd></div><div><dt>{{ t('loss') }}</dt><dd>{{ selectedLink.lossRate == null ? '—' : `${(selectedLink.lossRate * 100).toFixed(1)}%` }}</dd></div><div><dt>{{ t('protocol') }}</dt><dd>{{ selectedLink.protocols.join(' / ') || '—' }}</dd></div></template><div v-if="selectedLink?.stale || selectedPath?.stale"><dt>{{ t('stale') }}</dt><dd>—</dd></div></dl>
                </section>

                <section class="map-node-list console-panel" :aria-label="t('allNodes')">
                    <header class="map-node-list-toolbar"><div class="map-node-filters"><button :class="{ active: !onlyUnlocated }" :aria-pressed="!onlyUnlocated" @click="onlyUnlocated = false">{{ t('allNodes') }} <span>{{ members.length }}</span></button><button :class="{ active: onlyUnlocated }" :aria-pressed="onlyUnlocated" @click="onlyUnlocated = true">{{ t('unlocated') }} <span>{{ unlocatedCount }}</span></button></div><label class="map-search"><i class="pi pi-search" aria-hidden="true"></i><input v-model="search" type="search" :aria-label="t('search')" :placeholder="t('search')" /></label></header>
                    <div class="map-node-list-items"><button v-for="member in filteredMembers" :key="member.device_id" class="map-node-list-item" :class="{ selected: selectedId === member.device_id }" :aria-pressed="selectedId === member.device_id" @click="selectNode(member.device_id)"><i :class="['pi', configFor(member.device_id).role === 'gateway' ? 'pi-wifi' : 'pi-server']" aria-hidden="true"></i><span><strong>{{ nameOf(member.device_id) }}</strong><small>{{ hasPosition(configFor(member.device_id)) ? (member.runtime_virtual_ipv4 || t('node')) : t('unlocated') }}</small></span><span class="map-node-state" :class="{ online: member.online }" :aria-label="member.online ? t('online') : t('offline')"></span></button><p v-if="!filteredMembers.length" class="map-empty-note">{{ loading ? t('loading') : t('noMatches') }}</p></div>
                </section>
                <footer class="map-page-footnote"><span>{{ t('staticNote') }}</span><span v-if="unknownPeers">{{ t('unknownPeers', { count: unknownPeers }) }}</span><span>{{ lastUpdate ? t('updated', { time: new Date(lastUpdate).toLocaleTimeString(locale === 'cn' ? 'zh-CN' : 'en', { hour: '2-digit', minute: '2-digit', second: '2-digit' }) }) : t('noTelemetry') }}</span></footer>
            </template>
        </template>

        <DeviceAttributesDialog v-if="attributesTarget && attributesNode" :device="attributesTarget" :node="attributesNode" :api="api"
            @updated="attributesUpdated" @saved="attributesSaved" @close="attributesTarget = null; attributesNode = null" />
    </div>
</template>

<style scoped>
.network-map-page { gap: 18px; }
.map-space-layout { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; margin: 12px 0; color: var(--console-muted); font-size: 12px; }
.map-space-layout .map-native-select { min-width: 0; max-width: 180px; }
.map-heading { padding: 24px 24px 0; }
.map-workbench,.map-path-details,.map-node-list,.map-loading,.network-map-page > .console-empty-state,.network-map-page > .p-message { margin-inline: 24px; }
.map-page-footnote { margin: 0 24px 24px; }
.map-header-actions { display: flex; align-items: center; gap: 10px; }
.map-header-actions :deep(.p-button) { flex-shrink: 0; white-space: nowrap; }
.map-native-select { max-width: 240px; min-width: 160px; padding: 10px 34px 10px 12px; border: 1px solid var(--console-border); border-radius: 7px; color: var(--p-text-color); background: var(--console-panel); }
.map-error-row { display: flex; align-items: center; gap: 12px; }
.map-workbench { display: grid; grid-template-columns: minmax(0, 1fr) 340px; border: 1px solid var(--console-border); border-radius: 12px; overflow: hidden; min-height: 650px; }
.map-stage { position: relative; min-height: 650px; background: #080f17; overflow: hidden; }
.map-scene { position: absolute; inset: 0; width: 100%; height: 100%; }
.map-scene-toolbar { position: absolute; z-index: 3; top: 20px; left: 20px; right: 20px; display: flex; align-items: center; justify-content: space-between; gap: 14px; pointer-events: none; }
.map-view-tabs { display: flex; gap: 4px; padding: 4px; border: 1px solid #33414d; border-radius: 8px; background: #101a24e8; pointer-events: auto; }
.map-view-tabs button { display: flex; align-items: center; gap: 7px; padding: 7px 12px; border-radius: 5px; color: #aab8c6; font-size: 12px; font-weight: 550; }
.map-view-tabs button.active { color: #5eead4; background: #123d3c; }
.map-view-tabs button:disabled { opacity: .35; cursor: not-allowed; }
.map-live-label { display: inline-flex; align-items: center; gap: 7px; color: #a6b8c5; font-size: 11px; }
.map-live-label .pi-circle-fill { font-size: 6px; color: #5eead4; }
.map-space-caption { position: absolute; left: 24px; top: 90px; display: flex; flex-direction: column; gap: 4px; color: #e1eaf2; pointer-events: none; }
.map-space-caption strong { font-size: 20px; }.map-space-caption span { color: #a6b8c5; font-size: 12px; }
.map-scene-notice { position: absolute; top: 38%; left: 50%; transform: translateX(-50%); z-index: 4; width: min(85%, 340px); display: flex; flex-direction: column; align-items: center; gap: 14px; padding: 22px; border: 1px solid #33414d; background: #101a24e8; color: #e4edf5; border-radius: 10px; text-align: center; }
.map-scene-notice p { font-size: 12px; color: #adc0d1; }.map-scene-notice > .pi { font-size: 24px; color: #5eead4; }.map-scene-notice a { color: #5eead4; }
.map-canvas-footer { position: absolute; left: 22px; right: 22px; bottom: 22px; display: flex; align-items: flex-end; justify-content: space-between; pointer-events: none; }
.map-legend { display: flex; flex-wrap: wrap; gap: 12px; color: #c4d1dc; font-size: 11px; padding: 9px 12px; border-radius: 6px; background: #101a24cc; }.map-legend span { display: flex; align-items: center; gap: 7px; }.map-legend i { width: 20px; border-top: 2px solid #72edfa; }.map-legend .legend-relay { border-color: #ffd074; border-top-style: dashed; }.map-legend .legend-stale { border-color: #a5b6c7; border-top-style: dashed; }
.map-inspector { min-width: 0; padding: 24px 20px 20px; background: var(--console-panel); border-left: 1px solid var(--console-border); display: flex; flex-direction: column; gap: 16px; max-height: 780px; overflow-y: auto; }
.map-inspector > * { flex-shrink: 0; }
.map-inspector-heading { display: flex; gap: 10px; justify-content: space-between; align-items: center; }.map-eyebrow { color: var(--console-muted); font-size: 11px; }.map-inspector-heading h2 { font-size: 20px; font-weight: 600; letter-spacing: -.4px; overflow-wrap: anywhere; margin-top: 3px; }
.map-presence { display: flex; align-items: center; gap: 5px; flex: none; color: var(--console-muted); font-size: 11px; }.map-presence .pi { font-size: 7px; }.map-presence.online { color: var(--p-primary-color); }
.map-coordinate-line { display: flex; align-items: center; gap: 8px; color: var(--console-muted); font-size: 12px; font-variant-numeric: tabular-nums; }.map-coordinate-line button { margin-left: auto; padding: 4px; }
.map-space-preview { position: relative; height: 235px; margin: 0 -20px; overflow: hidden; background: #10171e; }.map-space-preview > :first-child { width: 100%; height: 100%; }
.map-space-preview :deep(.node-space) { min-height: 0; border-radius: 0; }
.map-space-preview :deep(.space-unavailable) { min-height: 0; height: 100%; padding: 20px 12px; gap: 8px; font-size: 11px; }
.map-stage :deep(.node-space .space-caption) { display: none; }.map-template-label { position: absolute; top: 12px; left: 20px; padding: 3px 7px; color: #b2c3d0; background: #18212ae6; border: 1px solid #3b4853; border-radius: 4px; font-size: 10px; pointer-events: none; }
.map-node-facts { display: flex; flex-wrap: wrap; gap: 12px; }.map-node-facts div { width: 100%; display: flex; justify-content: space-between; gap: 12px; font-size: 12px; }.map-node-facts dt { color: var(--console-muted); }.map-node-facts dd { font-family: ui-monospace, monospace; overflow-wrap: anywhere; }
.map-detail-section { padding-top: 17px; border-top: 1px solid var(--console-border); }.map-detail-section > header { display: flex; justify-content: space-between; align-items: center; gap: 10px; margin-bottom: 10px; }.map-detail-section h3 { font-size: 13px; font-weight: 600; }.map-detail-section header button,.map-detail-section header a { color: var(--p-primary-color); padding: 3px; }.map-section-count { font-size: 11px; color: var(--console-muted); }.map-section-note { font-size: 10px; color: var(--console-muted); margin-bottom: 8px; }.map-empty-note { font-size: 12px; line-height: 1.65; color: var(--console-muted); padding: 10px 0; }
.map-device-row { display: flex; gap: 4px; align-items: center; padding: 7px 0; border-bottom: 1px solid var(--console-border); }.map-device-row:last-of-type { border-bottom: 0; }.map-device-row.selected { color: var(--p-primary-color); }.map-device-select { min-width: 0; flex: 1; display: flex; gap: 10px; align-items: center; text-align: left; }.map-device-select > .pi { font-size: 18px; }.map-device-select span { min-width: 0; }.map-device-select strong { display: block; font-weight: 550; font-size: 12px; overflow-wrap: anywhere; }.map-device-select small { display: block; color: var(--console-muted); font-size: 10px; overflow-wrap: anywhere; margin-top: 2px; }
.map-icon-button { width: 28px; height: 28px; border-radius: 5px; flex: none; color: var(--console-muted); }.map-icon-button:hover { background: var(--console-ground); color: var(--p-primary-color); }
.map-service-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }.map-service-card { position: relative; text-align: left; padding: 12px; border: 1px solid var(--console-border); border-radius: 8px; display: flex; flex-direction: column; align-items: flex-start; gap: 5px; min-width: 0; transition: border-color .15s, background .15s; }.map-service-card:hover { border-color: var(--p-primary-color); background: var(--console-ground); }.map-service-card strong { font-size: 12px; font-weight: 550; max-width: 100%; overflow-wrap: anywhere; }.map-service-card small { font-size: 10px; color: var(--console-muted); }.map-service-icon { color: var(--p-primary-color); font-size: 24px; padding: 5px 0 9px; }.map-service-arrow { position: absolute; top: 13px; right: 10px; font-size: 10px; color: var(--console-muted); }
.map-text-link { display: inline-flex; align-items: center; gap: 6px; color: var(--p-primary-color); font-size: 12px; margin-top: 10px; }.map-inspector-actions { display: flex; flex-direction: column; gap: 5px; margin-top: auto; padding-top: 8px; }.map-inspector-actions > .map-text-link { justify-content: center; }
.map-link-row { width: 100%; display: flex; gap: 10px; justify-content: space-between; align-items: center; text-align: left; padding: 9px 7px; border-radius: 5px; }.map-link-row > span { display: inline-flex; align-items: center; gap: 8px; min-width: 0; font-size: 12px; overflow-wrap: anywhere; }.map-link-row small { white-space: nowrap; font-size: 10px; color: var(--console-muted); }.map-link-row:hover,.map-link-row.selected { background: var(--console-ground); }.map-link-row.selected { color: var(--p-primary-color); }.map-route-heading { color: var(--console-muted); font-size: 11px; margin: 14px 0 4px; }
.map-inspector-empty { margin: auto; text-align: center; color: var(--console-muted); padding: 30px 10px; }.map-inspector-empty > .pi { font-size: 28px; color: var(--p-primary-color); }.map-inspector-empty h2 { font-size: 17px; color: var(--p-text-color); margin: 18px 0 10px; }.map-inspector-empty p { font-size: 12px; line-height: 1.7; }
.map-path-details { padding: 16px 20px; border: 1px solid var(--console-border); border-radius: 8px; background: var(--console-panel); }.map-path-heading { display: flex; justify-content: space-between; align-items: center; gap: 10px; }.map-path-heading strong { font-weight: 550; font-size: 13px; overflow-wrap: anywhere; }.map-path-details dl { display: flex; flex-wrap: wrap; gap: 16px 30px; margin-top: 12px; }.map-path-details dt { color: var(--console-muted); font-size: 10px; }.map-path-details dd { margin-top: 3px; font-size: 12px; font-variant-numeric: tabular-nums; }.map-path-warning { font-size: 12px; color: var(--p-orange-500); margin-top: 8px; }
.map-node-list-toolbar { display: flex; flex-wrap: wrap; justify-content: space-between; align-items: center; gap: 12px; padding: 12px 16px; border-bottom: 1px solid var(--console-border); }.map-node-filters { display: flex; gap: 18px; }.map-node-filters button { font-size: 12px; color: var(--console-muted); }.map-node-filters button.active { color: var(--p-primary-color); }.map-node-filters span { font-size: 10px; margin-left: 5px; padding: 2px 5px; border-radius: 4px; background: var(--console-ground); }.map-search { display: flex; align-items: center; gap: 8px; color: var(--console-muted); font-size: 12px; }.map-search input { min-width: 0; width: 180px; border: 0; background: transparent; padding: 5px; color: var(--p-text-color); }.map-node-list-items { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; padding: 12px; max-height: 230px; overflow-y: auto; }.map-node-list-item { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 10px; border-radius: 7px; border: 1px solid transparent; text-align: left; }.map-node-list-item:hover { background: var(--console-ground); }.map-node-list-item.selected { background: var(--p-highlight-background); border-color: var(--p-primary-color); }.map-node-list-item > .pi { font-size: 17px; color: var(--console-muted); }.map-node-list-item > span:nth-child(2) { min-width: 0; flex: 1; }.map-node-list-item strong { display: block; font-size: 12px; font-weight: 550; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }.map-node-list-item small { display: block; font-size: 10px; color: var(--console-muted); margin-top: 3px; }.map-node-state { width: 7px; height: 7px; background: var(--console-muted); border-radius: 50%; flex: none; }.map-node-state.online { background: var(--p-primary-color); }.map-page-footnote { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px 18px; font-size: 10px; color: var(--console-muted); line-height: 1.6; }
.map-form { display: flex; flex-direction: column; gap: 20px; padding-top: 8px; }.map-form label { display: flex; flex-direction: column; gap: 8px; font-size: 13px; }.map-form input,.map-form select { padding: 10px 12px; width: 100%; border: 1px solid var(--p-content-border-color); border-radius: 7px; color: var(--p-text-color); background: var(--p-content-background); font-size: 14px; }.map-form input:focus,.map-form select:focus { outline: 2px solid var(--p-primary-color); outline-offset: 2px; }.map-form-pair { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }.map-form-help { color: var(--p-text-muted-color); font-size: 12px; line-height: 1.7; }.map-form-actions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 14px; }.map-form .map-text-link { align-self: flex-start; margin-top: -8px; }
.network-map-page button:focus-visible,.network-map-page a:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; }.network-map-page button:disabled { cursor: not-allowed; opacity: .45; }
@media (min-width: 1600px) { .map-workbench { grid-template-columns: minmax(0, 1fr) 380px; }.map-stage { min-height: 760px; }.map-inspector { max-height: 850px; }.map-space-preview { height: 270px; } }
@media (max-width: 1200px) { .map-workbench { grid-template-columns: minmax(0, 1fr) 300px; }.map-inspector { padding: 20px 16px; }.map-space-preview { margin-inline: -16px; height: 210px; }.map-live-label { display: none; }.map-node-list-items { grid-template-columns: repeat(3, minmax(0, 1fr)); }.map-inspector-heading { align-items: flex-start; flex-direction: column; gap: 5px; } }
@media (max-width: 900px) { .map-workbench { grid-template-columns: minmax(0, 1fr); }.map-stage { min-height: 510px; }.map-inspector { border-left: 0; border-top: 1px solid var(--console-border); max-height: none; }.map-inspector-heading { flex-direction: row; align-items: center; }.map-space-preview { height: 300px; }.map-node-list-items { grid-template-columns: repeat(2, minmax(0, 1fr)); }.map-inspector-actions { flex-direction: row; align-items: center; justify-content: space-between; }.map-service-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); } }
@media (max-width: 600px) { .map-heading { padding: 16px 16px 0; }.map-workbench,.map-path-details,.map-node-list,.map-loading,.network-map-page > .console-empty-state,.network-map-page > .p-message { margin-inline: 16px; }.map-page-footnote { margin: 0 16px 16px; }.map-heading { flex-direction: column; align-items: stretch; }.map-header-actions { justify-content: space-between; }.map-native-select { max-width: none; min-width: 0; width: 100%; }.map-stage { min-height: 430px; }.map-scene-toolbar { left: 12px; top: 12px; right: 12px; }.map-canvas-footer { left: 12px; right: 12px; bottom: 12px; }.map-legend { max-width: 220px; gap: 8px; font-size: 10px; }.map-view-tabs button { padding: 7px 10px; }.map-node-list-toolbar { align-items: stretch; flex-direction: column; }.map-search input { width: 100%; }.map-node-list-items { grid-template-columns: minmax(0, 1fr); }.map-service-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }.map-inspector-actions { align-items: stretch; flex-direction: column; }.map-path-details { padding: 14px; } }

/* Shared console surfaces frame the real-time scene without changing renderer data. */
.map-heading { padding-top: 28px; }
.map-heading-identity { display: flex; align-items: center; gap: 15px; min-width: 0; }
.map-heading-mark { display: inline-flex; align-items: center; justify-content: center; width: 48px; height: 48px; flex: none; border-radius: 13px; border: 1px solid var(--console-border-strong, var(--console-border)); color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); box-shadow: var(--console-glow, none); }
.map-heading-mark > .pi { font-size: 24px; }
.map-header-actions { gap: 12px; }
.map-native-select { border-color: var(--console-border-strong, var(--console-border)); border-radius: 9px; background: var(--console-panel-raised, var(--console-panel)); min-height: 40px; font-size: 13px; }
.map-network-summary { display: flex; align-items: center; flex-wrap: wrap; gap: 22px; margin: 0 24px; padding: 0 2px 1px; }
.map-network-summary > span { display: inline-flex; align-items: center; gap: 8px; color: var(--console-muted); font-size: 11px; }
.map-network-summary > span > .pi { font-size: 11px; color: var(--p-primary-color); }
.map-network-summary strong { color: var(--p-text-color); font-weight: 600; font-size: 16px; letter-spacing: -.02em; font-variant-numeric: tabular-nums; }
.map-network-summary strong small { color: var(--console-muted); font-size: 11px; font-weight: 400; margin-left: 5px; }
.map-network-summary .map-unlocated-summary { margin-left: auto; }
.map-workbench { grid-template-columns: minmax(0, 1fr) 360px; border-radius: 15px; border-color: var(--console-border-strong, var(--console-border)); box-shadow: var(--console-shadow, none); }
.map-scene-toolbar { top: 18px; left: 18px; right: 18px; }
.map-view-tabs { padding: 5px; border-color: #29435b; border-radius: 10px; background: #101d2ded; box-shadow: 0 6px 20px rgb(0 0 0 / 16%); }
.map-view-tabs button { border: 1px solid transparent; border-radius: 6px; padding: 7px 12px; color: #aabed4; transition: border-color .18s, background .18s, color .18s; }
.map-view-tabs button.active { color: #88e9f3; border-color: #2c6874; background: #153c49; }
.map-live-label { padding: 7px 10px; border: 1px solid #29435b; border-radius: 7px; background: #101d2dcc; color: #aabed4; }
.map-legend { border: 1px solid #29435b; background: #101d2dcc; border-radius: 8px; padding: 10px 12px; }
.map-inspector { background: var(--console-panel); gap: 16px; padding: 23px 20px 20px; border-left-color: var(--console-border-strong, var(--console-border)); scrollbar-width: thin; scrollbar-color: var(--console-border-strong, var(--console-border)) transparent; }
.map-inspector-heading { gap: 11px; align-items: center; justify-content: flex-start; }
.map-node-avatar { display: inline-flex; align-items: center; justify-content: center; flex: none; width: 39px; height: 39px; border-radius: 11px; color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); border: 1px solid var(--console-border-strong, var(--console-border)); }
.map-node-avatar > .pi { font-size: 19px; }
.map-node-identity { min-width: 0; flex: 1; }
.map-eyebrow { font-size: 10px; letter-spacing: .03em; }
.map-inspector-heading h2 { font-size: 18px; font-weight: 600; line-height: 1.35; margin-top: 4px; }
.map-presence { border: 1px solid var(--console-border); border-radius: 5px; padding: 3px 6px; background: var(--console-panel-raised, var(--console-panel)); font-size: 10px; }
.map-presence.online { color: var(--p-green-700); background: color-mix(in srgb, var(--p-green-500) 8%, var(--console-panel)); border-color: color-mix(in srgb, var(--p-green-500) 23%, var(--console-border)); }
html.app-dark .map-presence.online { color: var(--p-green-300); }
.map-coordinate-line { padding: 0 1px; font-size: 11px; }
.map-coordinate-line > .pi { color: var(--p-primary-color); }
.map-coordinate-line button { display: inline-flex; width: 28px; height: 28px; align-items: center; justify-content: center; border-radius: 6px; }
.map-coordinate-line button:hover { color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); }
.map-node-facts { padding: 11px 12px; border: 1px solid var(--console-border); border-radius: 8px; background: var(--console-panel-raised, var(--console-panel)); margin-top: -4px; }
.map-node-facts div { font-size: 11px; }
.map-node-facts dd { color: var(--p-text-color); font-size: 12px; }
.map-space-preview { margin-top: 2px; border-top: 1px solid var(--console-border); border-bottom: 1px solid var(--console-border); }
.map-detail-section { padding-top: 18px; }
.map-detail-section > header { margin-bottom: 12px; }
.map-detail-section h3 { display: flex; align-items: center; gap: 8px; font-size: 13px; min-width: 0; }
.map-detail-section h3 > .pi { font-size: 12px; color: var(--p-primary-color); }
.map-detail-section header button, .map-detail-section header a { display: inline-flex; align-items: center; justify-content: center; width: 26px; height: 26px; flex: none; border: 1px solid var(--console-border); border-radius: 6px; background: var(--console-panel-raised, var(--console-panel)); }
.map-detail-section header button:hover, .map-detail-section header a:hover { border-color: var(--p-primary-color); }
.map-section-note { font-size: 11px; line-height: 1.6; }
.map-empty-note { padding: 12px 0; font-size: 12px; line-height: 1.7; }
.map-device-row { padding: 9px 0; }
.map-device-select > .pi { display: inline-flex; justify-content: center; align-items: center; width: 31px; height: 34px; flex: none; border-radius: 8px; background: var(--console-panel-raised, var(--console-panel)); color: var(--console-muted); border: 1px solid var(--console-border); font-size: 16px; }
.map-device-row.selected .map-device-select > .pi { color: var(--p-primary-color); background: var(--console-accent-soft, var(--p-highlight-background)); border-color: color-mix(in srgb, var(--p-primary-color) 35%, var(--console-border)); }
.map-device-select small { font-size: 10px; line-height: 1.5; margin-top: 4px; }
.map-icon-button { border: 1px solid transparent; width: 28px; height: 30px; }
.map-icon-button:hover { background: var(--console-accent-soft, var(--p-highlight-background)); border-color: var(--console-border-strong, var(--console-border)); }
.map-service-grid { gap: 10px; }
.map-service-card { padding: 13px; border-radius: 10px; background: var(--console-panel-raised, var(--console-panel)); gap: 7px; transition: border-color .18s, box-shadow .18s, transform .18s; }
.map-service-card:hover { border-color: var(--p-primary-color); background: var(--console-panel-raised, var(--console-panel)); box-shadow: var(--console-glow, none); transform: translateY(-1px); }
.map-service-icon { display: inline-flex; align-items: center; justify-content: center; width: 35px; height: 35px; border-radius: 9px; background: var(--console-accent-soft, var(--p-highlight-background)); border: 1px solid var(--console-border-strong, var(--console-border)); font-size: 18px; padding: 0; margin-bottom: 5px; }
.map-service-card strong { font-size: 12px; line-height: 1.4; }
.map-service-card small { font-size: 10px; }
.map-link-row { padding: 10px 8px; border: 1px solid transparent; border-radius: 7px; }
.map-link-row:hover, .map-link-row.selected { background: var(--console-panel-raised, var(--console-panel)); border-color: var(--console-border); }
.map-link-row.selected { border-color: color-mix(in srgb, var(--p-primary-color) 35%, var(--console-border)); }
.map-link-row > span > .pi { font-size: 11px; color: var(--p-primary-color); }
.map-path-details { border-radius: 12px; border-color: var(--console-border-strong, var(--console-border)); box-shadow: var(--console-glow, none); padding: 20px; }
.map-path-details dd { font-size: 14px; font-weight: 550; }
.map-node-list { border-radius: 12px; }
.map-node-list-toolbar { padding: 14px 16px; background: var(--console-panel-raised, var(--console-panel)); }
.map-node-filters { gap: 8px; }
.map-node-filters button { padding: 6px 9px; border: 1px solid transparent; border-radius: 6px; }
.map-node-filters button.active { background: var(--console-accent-soft, var(--p-highlight-background)); border-color: color-mix(in srgb, var(--p-primary-color) 25%, var(--console-border)); }
.map-node-filters span { background: var(--console-panel); }
.map-search { padding: 3px 9px; border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 7px; background: var(--console-panel); }
.map-search > .pi { font-size: 12px; }
.map-node-list-item { padding: 12px; border-radius: 9px; gap: 11px; }
.map-node-list-item:hover { background: var(--console-panel-raised, var(--console-panel)); border-color: var(--console-border); }
.map-node-list-item.selected { background: var(--console-accent-soft, var(--p-highlight-background)); border-color: color-mix(in srgb, var(--p-primary-color) 50%, var(--console-border)); }
.map-node-list-item.selected > .pi { color: var(--p-primary-color); }
.map-node-list-item small { font-size: 11px; }
.map-node-state.online { background: var(--p-green-400); box-shadow: 0 0 0 3px color-mix(in srgb, var(--p-green-400) 12%, transparent); }
.map-page-footnote { font-size: 11px; }
:global(.map-edit-drawer.p-drawer) { background: var(--console-panel, var(--p-content-background)); border-left-color: var(--console-border-strong, var(--p-content-border-color)); }
:global(.map-edit-drawer .p-drawer-header) { background: var(--console-panel-raised, var(--p-content-background)); border-bottom: 1px solid var(--console-border, var(--p-content-border-color)); }
.map-form input, .map-form select { background: var(--console-ground, var(--p-content-background)); border-color: var(--console-border-strong, var(--p-content-border-color)); border-radius: 8px; }
.map-form-actions { padding-top: 18px; border-top: 1px solid var(--console-border, var(--p-content-border-color)); }
@media (min-width: 1600px) { .map-workbench { grid-template-columns: minmax(0, 1fr) 390px; } }
@media (max-width: 1200px) { .map-workbench { grid-template-columns: minmax(0, 1fr) 310px; } .map-inspector { padding-inline: 16px; } .map-inspector-heading { flex-direction: row; align-items: flex-start; flex-wrap: wrap; } .map-node-avatar { width: 33px; height: 33px; border-radius: 9px; } .map-node-identity { min-width: 150px; } .map-presence { margin-left: 44px; } }
@media (max-width: 900px) { .map-workbench { grid-template-columns: minmax(0, 1fr); } .map-inspector { border-top-color: var(--console-border-strong, var(--console-border)); } .map-inspector-heading { align-items: center; } .map-node-identity { min-width: 0; } .map-presence { margin-left: 0; } }
@media (max-width: 600px) { .map-heading { padding-top: 20px; } .map-heading-mark { width: 42px; height: 42px; } .map-heading-identity { gap: 12px; } .map-network-summary { margin-inline: 16px; gap: 10px 18px; } .map-network-summary > span { gap: 6px; font-size: 10px; } .map-network-summary strong { font-size: 14px; } .map-network-summary .map-unlocated-summary { margin-left: 0; } .map-native-select { width: 100%; } .map-header-actions { width: 100%; } .map-inspector-heading h2 { font-size: 18px; } .map-path-details { padding: 16px; } }
@media (prefers-reduced-motion: reduce) { .map-view-tabs button, .map-service-card { transition: none; } .map-service-card:hover { transform: none; } }
</style>
