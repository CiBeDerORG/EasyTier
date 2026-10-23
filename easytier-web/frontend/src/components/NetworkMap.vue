<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import { Button, Dialog, Drawer, Message, Skeleton, useToast } from 'primevue';
import { Polling } from 'easytier-frontend-lib';
import { v4 as uuidv4 } from 'uuid';
import type ApiClient from '../modules/api';
import type { CentralNetworkMember, CentralNetworkSummary } from '../modules/api';
import type { MapLanDevice, MapNodeConfig, MapSceneLink, MapSceneNode, NetworkMapConfig, ServiceEntry } from '../modules/network-space';
import { buildMapTopology, type MapCounter, type MapRuntimeSample, type MapTopologyLink, type MapTopologyRoute } from '../modules/map-topology';
import { mapMessages } from '../modules/map-messages';

const NetworkGlobe = defineAsyncComponent(() => import('./map/NetworkGlobe.vue'));
const NodeSpace = defineAsyncComponent(() => import('./map/NodeSpace.vue'));
const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t, locale } = useI18n({ useScope: 'local', messages: mapMessages });
const route = useRoute();
const router = useRouter();
const toast = useToast();
const autoRefreshEnabled = Polling.useAutoRefresh();
const networks = ref<CentralNetworkSummary[]>([]);
const networkId = ref('');
const members = ref<CentralNetworkMember[]>([]);
const configuration = ref<NetworkMapConfig>({ revision: 0, nodes: [], devices: [] });
const configurationReady = ref(false);
const services = ref<ServiceEntry[]>([]);
const loadingNetworks = ref(false);
const loading = ref(false);
const collecting = ref(false);
const loadError = ref('');
const configurationError = ref(false);
const serviceError = ref(false);
const runtimeError = ref(false);
const membersStale = ref(false);
const selectedId = ref<string | null>(null);
const selectedDeviceId = ref<string | null>(null);
const selectedLinkId = ref<string | null>(null);
const selectedRouteId = ref<string | null>(null);
const mode = ref<'globe' | 'space'>('globe');
const search = ref('');
const onlyUnlocated = ref(false);
const rendererError = ref(false);
const links = ref<MapTopologyLink[]>([]);
const paths = ref<MapTopologyRoute[]>([]);
const unknownPeers = ref(0);
const lastUpdate = ref<number | null>(null);
let samples = new Map<string, MapRuntimeSample>();
let counters = new Map<string, MapCounter>();
let generation = 0;
let networkListGeneration = 0;
let mounted = true;
let polling: number | undefined;
let pollInFlight = false;
let runtimeGeneration: number | null = null;
const queryString = (value: unknown) => typeof value === 'string' ? value : '';
const errorText = (error: unknown, fallback: string) => (error as { response?: { data?: { message?: string } } })?.response?.data?.message || fallback;

const configFor = (deviceId: string): MapNodeConfig => configuration.value.nodes.find(node => node.device_id === deviceId) ?? {
    device_id: deviceId, latitude: null, longitude: null, label: '', role: 'node', layout: 'topology',
};
const nameOf = (deviceId: string) => {
    const member = members.value.find(item => item.device_id === deviceId);
    return configFor(deviceId).label || member?.alias || member?.hostname_override || member?.hostname || deviceId;
};
const hasPosition = (node: MapNodeConfig) => node.latitude != null && node.longitude != null && Number.isFinite(node.latitude) && Number.isFinite(node.longitude);
const selectedMember = computed(() => members.value.find(member => member.device_id === selectedId.value));
const selectedConfig = computed(() => selectedId.value ? configFor(selectedId.value) : null);
const isGateway = computed(() => selectedConfig.value?.role === 'gateway');
const localDevices = computed(() => configuration.value.devices.filter(device => device.gateway_device_id === selectedId.value));
const selectedDevice = computed(() => localDevices.value.find(device => device.id === selectedDeviceId.value));
const associatedServices = computed(() => services.value.filter(service => service.device_id === selectedId.value && (!selectedDeviceId.value || service.lan_device_id === selectedDeviceId.value)));
const selectedLink = computed(() => links.value.find(link => link.id === selectedLinkId.value));
const selectedPath = computed(() => paths.value.find(path => path.id === selectedRouteId.value));
const nodeLinks = computed(() => links.value.filter(link => link.source === selectedId.value || link.target === selectedId.value));
const nodePaths = computed(() => paths.value.filter(path => path.source === selectedId.value));
const unlocatedCount = computed(() => members.value.filter(member => !hasPosition(configFor(member.device_id))).length);
const filteredMembers = computed(() => members.value.filter(member => {
    const words = `${nameOf(member.device_id)} ${member.runtime_virtual_ipv4 || member.virtual_ipv4 || ''}`.toLocaleLowerCase();
    return (!onlyUnlocated.value || !hasPosition(configFor(member.device_id))) && words.includes(search.value.trim().toLocaleLowerCase());
}));
const sceneNodes = computed<MapSceneNode[]>(() => members.value.flatMap(member => {
    const node = configFor(member.device_id);
    return hasPosition(node) ? [{ id: member.device_id, name: nameOf(member.device_id), latitude: node.latitude!, longitude: node.longitude!, online: member.online && member.running !== false, role: node.role }] : [];
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
    const query = { ...route.query, network: networkId.value || undefined, node: selectedId.value || undefined, device: selectedDeviceId.value || undefined };
    if (queryString(route.query.network) !== networkId.value || queryString(route.query.node) !== (selectedId.value || '') || queryString(route.query.device) !== (selectedDeviceId.value || '')) void router.replace({ query });
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
function selectLink(id: string) {
    selectedLinkId.value = id;
    selectedRouteId.value = null;
}
function expireTelemetry(force = false) {
    const now = Date.now();
    links.value = links.value.map(link => force || now - link.sampledAt >= 45_000 ? { ...link, stale: true, rx_bps: 0, tx_bps: 0 } : link);
    paths.value = paths.value.map(path => ({ ...path, stale: path.stale || force || path.path.some(id => {
        const sample = samples.get(id);
        return !sample || sample.failed || now - sample.sampledAt >= 45_000;
    }) }));
}
function visibilityChanged() {
    expireTelemetry(true);
    counters.clear();
    if (canRefreshAutomatically()) void collectRuntime(generation, canRefreshAutomatically);
}
function canRefreshAutomatically() {
    return mounted && autoRefreshEnabled.value && !document.hidden && props.centralEnabled;
}

async function loadNetworks() {
    const current = ++networkListGeneration;
    if (!props.centralEnabled) return;
    loadingNetworks.value = true;
    loadError.value = '';
    try {
        const result = await props.api.list_networks();
        if (!mounted || current !== networkListGeneration || !props.centralEnabled) return;
        networks.value = result;
        const requested = queryString(route.query.network) || networkId.value;
        const next = result.find(network => network.network_id === requested)?.network_id || result[0]?.network_id || '';
        if (networkId.value !== next) networkId.value = next;
        else if (next) await loadNetwork();
    } catch (error) {
        if (current === networkListGeneration) loadError.value = errorText(error, t('loadFailed'));
    } finally {
        if (current === networkListGeneration) loadingNetworks.value = false;
    }
}

async function loadNetwork() {
    const id = networkId.value;
    const current = ++generation;
    if (!id || !props.centralEnabled) return;
    loading.value = true;
    loadError.value = '';
    configurationError.value = false;
    serviceError.value = false;
    const results = await Promise.allSettled([
        props.api.list_network_members(id), props.api.get_network_map(id), props.api.list_services(id),
    ]);
    if (!mounted || current !== generation) return;
    const [memberResult, configResult, serviceResult] = results;
    if (memberResult.status === 'fulfilled') { members.value = memberResult.value.members; membersStale.value = false; }
    else { loadError.value = errorText(memberResult.reason, t('membersFailed')); membersStale.value = true; }
    if (configResult.status === 'fulfilled') {
        configuration.value = configResult.value;
        configurationReady.value = true;
    } else {
        configurationError.value = true;
        configurationReady.value = false;
    }
    if (serviceResult.status === 'fulfilled') services.value = serviceResult.value;
    else serviceError.value = true;
    const requested = queryString(route.query.node) || selectedId.value;
    const next = members.value.find(member => member.device_id === requested)?.device_id || members.value.find(member => member.online)?.device_id || members.value[0]?.device_id;
    if (next) selectNode(next, false);
    else selectedId.value = null;
    const requestedDevice = queryString(route.query.device);
    if (requestedDevice && localDevices.value.some(device => device.id === requestedDevice)) selectedDeviceId.value = requestedDevice;
    else if (!localDevices.value.some(device => device.id === selectedDeviceId.value)) selectedDeviceId.value = null;
    loading.value = false;
    syncQuery();
    if (memberResult.status === 'fulfilled') void collectRuntime(current);
}

async function collectRuntime(current = generation, canContinue = () => true) {
    if (!canContinue() || !networkId.value || !props.centralEnabled || loading.value || runtimeGeneration === current || document.hidden) return;
    runtimeGeneration = current;
    collecting.value = true;
    try {
        const id = networkId.value;
        const queue = members.value.filter(member => member.online && member.running !== false);
        const next = new Map<string, MapRuntimeSample>();
        let failed = false;
        let cursor = 0;
        const worker = async () => {
            while (cursor < queue.length && current === generation && mounted && canContinue()) {
                const member = queue[cursor++];
                const results = await Promise.allSettled([
                    props.api.get_node_info(member.device_id, id),
                    props.api.get_node_routes(member.device_id, id),
                    props.api.get_node_peers(member.device_id, id),
                ]);
                if (current !== generation || !mounted || !canContinue()) return;
                const [info, routes, peers] = results;
                const previous = samples.get(member.device_id);
                const partial = results.some(result => result.status === 'rejected');
                failed ||= partial;
                next.set(member.device_id, {
                    deviceId: member.device_id,
                    peerId: info.status === 'fulfilled' ? info.value.peer_id : previous?.peerId,
                    routes: routes.status === 'fulfilled' ? routes.value : previous?.routes ?? [],
                    peers: peers.status === 'fulfilled' ? peers.value : previous?.peers ?? [],
                    sampledAt: partial ? previous?.sampledAt ?? Date.now() : Date.now(),
                    failed: partial,
                });
            }
        };
        await Promise.all(Array.from({ length: Math.min(3, queue.length) }, worker));
        if (!mounted || current !== generation || !canContinue()) return;
        for (const member of members.value.filter(member => !member.online || member.running === false)) {
            const previous = samples.get(member.device_id);
            if (previous) next.set(member.device_id, { ...previous, failed: true });
        }
        samples = next;
        const topology = buildMapTopology(members.value, [...samples.values()], counters);
        links.value = topology.links;
        paths.value = topology.routes;
        counters = topology.counters;
        unknownPeers.value = topology.unresolvedPeers;
        runtimeError.value = failed || membersStale.value;
        lastUpdate.value = queue.length && !failed ? Date.now() : lastUpdate.value;
        if (document.hidden) expireTelemetry(true);
    } catch {
        if (mounted && current === generation && canContinue()) { runtimeError.value = true; expireTelemetry(true); }
    } finally {
        if (runtimeGeneration === current) {
            runtimeGeneration = null;
            if (current === generation) collecting.value = false;
        }
    }
}

watch(networkId, () => {
    generation++;
    members.value = [];
    configuration.value = { revision: 0, nodes: [], devices: [] };
    configurationReady.value = false;
    services.value = [];
    links.value = [];
    paths.value = [];
    samples.clear();
    counters.clear();
    lastUpdate.value = null;
    selectedId.value = null;
    selectedDeviceId.value = null;
    selectedLinkId.value = null;
    selectedRouteId.value = null;
    mode.value = 'globe';
    rendererError.value = false;
    runtimeError.value = false;
    membersStale.value = false;
    nodeEditor.value = false;
    deviceEditor.value = false;
    removeCandidate.value = null;
    unknownPeers.value = 0;
    void loadNetwork();
});
watch(() => props.centralEnabled, enabled => {
    if (enabled) void loadNetworks();
    else { generation++; networkListGeneration++; }
}, { immediate: true });
watch(() => [route.query.network, route.query.node, route.query.device], () => {
    const requestedNetwork = queryString(route.query.network);
    if (requestedNetwork && requestedNetwork !== networkId.value && networks.value.some(network => network.network_id === requestedNetwork)) {
        networkId.value = requestedNetwork;
        return;
    }
    const requestedNode = queryString(route.query.node);
    if (requestedNode && requestedNode !== selectedId.value) selectNode(requestedNode, false);
    const requestedDevice = queryString(route.query.device);
    selectedDeviceId.value = localDevices.value.some(device => device.id === requestedDevice) ? requestedDevice : null;
});
onMounted(() => {
    document.addEventListener('visibilitychange', visibilityChanged);
    polling = window.setInterval(async () => {
        expireTelemetry();
        if (!canRefreshAutomatically() || !networkId.value || loading.value || runtimeGeneration === generation || pollInFlight) return;
        pollInFlight = true;
        const current = generation;
        try {
            const result = await props.api.list_network_members(networkId.value);
            if (current !== generation || !canRefreshAutomatically()) return;
            members.value = result.members;
            membersStale.value = false;
            if (selectedId.value && !result.members.some(member => member.device_id === selectedId.value)) {
                selectedId.value = null;
                selectedDeviceId.value = null;
                selectedLinkId.value = null;
                selectedRouteId.value = null;
                mode.value = 'globe';
                syncQuery();
            }
        } catch { if (current === generation && canRefreshAutomatically()) { runtimeError.value = true; membersStale.value = true; } }
        finally { pollInFlight = false; }
        if (current === generation && canRefreshAutomatically()) await collectRuntime(current, canRefreshAutomatically);
    }, 15_000);
});
onBeforeUnmount(() => { mounted = false; generation++; networkListGeneration++; window.clearInterval(polling); document.removeEventListener('visibilitychange', visibilityChanged); });

const nodeEditor = ref(false);
const deviceEditor = ref(false);
const saving = ref(false);
const formError = ref('');
const conflict = ref(false);
const serviceConflict = ref(false);
const nodeDraft = ref<MapNodeConfig>({ device_id: '', latitude: null, longitude: null, label: '', role: 'node', layout: 'topology' });
const latitudeDraft = ref<string | number>('');
const longitudeDraft = ref<string | number>('');
const deviceDraft = ref<MapLanDevice>({ id: '', gateway_device_id: '', name: '', address: '', kind: 'server', room: 'study' });
const removeCandidate = ref<MapLanDevice | null>(null);
const deviceKinds: MapLanDevice['kind'][] = ['server', 'desktop', 'camera', 'router', 'other'];
const rooms: MapLanDevice['room'][] = ['living', 'study', 'bedroom', 'entry'];
function editNode(id = selectedId.value) {
    if (!id || !configurationReady.value) return;
    selectNode(id);
    nodeDraft.value = { ...configFor(id) };
    latitudeDraft.value = nodeDraft.value.latitude?.toString() ?? '';
    longitudeDraft.value = nodeDraft.value.longitude?.toString() ?? '';
    formError.value = '';
    conflict.value = false;
    serviceConflict.value = false;
    nodeEditor.value = true;
}
function editDevice(device?: MapLanDevice) {
    if (!selectedId.value || !isGateway.value || !configurationReady.value) return;
    deviceDraft.value = device ? { ...device } : { id: uuidv4(), gateway_device_id: selectedId.value, name: '', address: '', kind: 'server', room: 'study' };
    formError.value = '';
    conflict.value = false;
    serviceConflict.value = false;
    deviceEditor.value = true;
}
const cloneConfiguration = (): NetworkMapConfig => ({ revision: configuration.value.revision, nodes: configuration.value.nodes.map(node => ({ ...node })), devices: configuration.value.devices.map(device => ({ ...device })) });
async function persist(next: NetworkMapConfig) {
    if (saving.value) return false;
    if (!configurationReady.value) { formError.value = t('configurationFailed'); return false; }
    const id = networkId.value;
    saving.value = true;
    formError.value = '';
    conflict.value = false;
    serviceConflict.value = false;
    try {
        const result = await props.api.save_network_map(id, next);
        if (id !== networkId.value || !mounted) return false;
        configuration.value = result;
        toast.add({ severity: 'success', summary: t('saved'), detail: t('savedHint'), life: 2500 });
        return true;
    } catch (error) {
        if (id !== networkId.value || !mounted) return false;
        const response = (error as { response?: { status?: number; data?: { code?: string; message?: string } } })?.response;
        const message = errorText(error, t('saveFailed'));
        conflict.value = response?.status === 409 && (response.data?.code === 'map_revision_conflict' || message.includes('map has changed'));
        serviceConflict.value = response?.status === 409 && (response.data?.code === 'map_device_in_use' || message.includes('associated services'));
        formError.value = conflict.value ? t('conflict') : message;
        return false;
    } finally { saving.value = false; }
}
async function saveNode() {
    const lat = String(latitudeDraft.value ?? '').trim();
    const lon = String(longitudeDraft.value ?? '').trim();
    if (!!lat !== !!lon) { formError.value = t('coordinatePair'); return; }
    const latitude = lat ? Number(lat) : null;
    const longitude = lon ? Number(lon) : null;
    if ((latitude != null && (!Number.isFinite(latitude) || Math.abs(latitude) > 90)) || (longitude != null && (!Number.isFinite(longitude) || Math.abs(longitude) > 180))) { formError.value = t('coordinateRange'); return; }
    if (nodeDraft.value.role === 'node' && configuration.value.devices.some(device => device.gateway_device_id === nodeDraft.value.device_id)) { formError.value = t('gatewayChildren'); return; }
    const next = cloneConfiguration();
    const updated = { ...nodeDraft.value, label: nodeDraft.value.label.trim(), latitude, longitude };
    next.nodes = [...next.nodes.filter(node => node.device_id !== updated.device_id), updated];
    if (await persist(next)) nodeEditor.value = false;
}
async function saveDevice() {
    if (!deviceDraft.value.name.trim() || !deviceDraft.value.address.trim()) { formError.value = t('nameRequired'); return; }
    const next = cloneConfiguration();
    const updated = { ...deviceDraft.value, name: deviceDraft.value.name.trim(), address: deviceDraft.value.address.trim() };
    next.devices = [...next.devices.filter(device => device.id !== updated.id), updated];
    if (await persist(next)) {
        deviceEditor.value = false;
        selectedDeviceId.value = updated.id;
        syncQuery();
    }
}
async function removeDevice() {
    if (!removeCandidate.value) return;
    const next = cloneConfiguration();
    const id = removeCandidate.value.id;
    next.devices = next.devices.filter(device => device.id !== id);
    if (await persist(next)) {
        removeCandidate.value = null;
        if (selectedDeviceId.value === id) selectedDeviceId.value = null;
        syncQuery();
    }
}
async function reloadConfiguration() {
    nodeEditor.value = false;
    deviceEditor.value = false;
    removeCandidate.value = null;
    await loadNetwork();
}
function manageAssociatedServices() {
    const deviceId = removeCandidate.value?.id || deviceDraft.value.id;
    const linked = services.value.find(service => service.lan_device_id === deviceId);
    nodeEditor.value = false;
    deviceEditor.value = false;
    removeCandidate.value = null;
    void router.push({ name: 'serviceManagement', query: linked ? { edit: linked.id } : { network: networkId.value } });
}
function openService(service: ServiceEntry) {
    try {
        const url = new URL(service.url);
        if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) throw new Error('invalid');
        if (service.open_in_new_tab) window.open(url.href, '_blank', 'noopener,noreferrer');
        else window.location.assign(url.href);
    } catch { toast.add({ severity: 'error', summary: t('invalidService'), life: 4000 }); }
}
const serviceIcons: Record<string, string> = { home: 'pi-home', photos: 'pi-images', media: 'pi-play-circle', chart: 'pi-chart-line', server: 'pi-server', globe: 'pi-globe', code: 'pi-code' };
const deviceIcons: Record<string, string> = { server: 'pi-server', desktop: 'pi-desktop', camera: 'pi-video', router: 'pi-wifi', other: 'pi-box' };
const serviceIcon = (icon: string) => serviceIcons[icon] || 'pi-globe';
const deviceIcon = (kind: string) => deviceIcons[kind] || 'pi-box';
const formatRate = (value: number) => value >= 1_000_000 ? `${(value / 1_000_000).toFixed(1)} MB/s` : value >= 1000 ? `${(value / 1000).toFixed(1)} kB/s` : `${Math.round(value)} B/s`;
const formatLatency = (value: number | null) => value == null ? '—' : `${value.toFixed(1)} ms`;
const serviceQuery = computed(() => ({ network: networkId.value, node: selectedId.value || undefined, device: selectedDeviceId.value || undefined }));
</script>

<template>
    <div class="console-page network-map-page">
        <header class="page-heading map-heading">
            <div class="map-heading-identity"><span class="map-heading-mark"><i class="pi pi-globe" aria-hidden="true"></i></span><div><h1>{{ t('title') }}</h1><p class="page-description">{{ t('subtitle') }}</p></div></div>
            <div class="map-header-actions">
                <label class="sr-only" for="map-network">{{ t('network') }}</label>
                <select v-if="centralEnabled" id="map-network" v-model="networkId" :disabled="loadingNetworks || saving" class="map-native-select">
                    <option v-if="!networks.length" value="">{{ t('chooseNetwork') }}</option>
                    <option v-for="network in networks" :key="network.network_id" :value="network.network_id">{{ network.display_name || network.network_name }}</option>
                </select>
                <Button v-if="centralEnabled" icon="pi pi-refresh" :aria-label="t('refresh')" :label="t('refresh')" severity="secondary" outlined :loading="loading || loadingNetworks" :disabled="saving" @click="loadNetworks" />
            </div>
        </header>
        <Message v-if="!centralEnabled" severity="info" :closable="false">{{ t('centralOnly') }}</Message>
        <template v-else>
            <Message v-if="loadError" severity="error" :closable="false"><div class="map-error-row">{{ loadError }}<Button :label="t('retry')" text size="small" @click="loadNetworks" /></div></Message>
            <Message v-if="configurationError" severity="warn" :closable="false">{{ t('configurationFailed') }}</Message>
            <Message v-if="runtimeError" severity="warn" :closable="false">{{ t('runtimePartial') }}</Message>
            <div v-if="loadingNetworks && !networks.length" class="map-loading"><Skeleton width="100%" height="28rem" /></div>
            <section v-else-if="!networks.length && !loadError" class="console-empty-state console-panel">
                <i class="pi pi-globe" aria-hidden="true"></i><h2>{{ t('noNetworks') }}</h2>
                <RouterLink :to="{ name: 'networkList' }" class="entity-link">{{ t('createNetwork') }}<i class="pi pi-arrow-right" aria-hidden="true"></i></RouterLink>
            </section>
            <template v-else-if="networkId">
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
                                <button :class="{ active: mode === 'space' }" :aria-pressed="mode === 'space'" :disabled="!isGateway" @click="mode = 'space'"><i class="pi pi-box" aria-hidden="true"></i>{{ t('space') }}</button>
                            </div>
                            <span class="map-live-label"><i :class="['pi', collecting ? 'pi-spin pi-spinner' : 'pi-circle-fill']" aria-hidden="true"></i>{{ collecting ? t('collecting') : t('nodeCount', { count: members.length }) }}</span>
                        </div>
                        <NetworkGlobe v-show="mode === 'globe'" class="map-scene" :nodes="sceneNodes" :links="sceneLinks" :selected-node-id="selectedId" :selected-link-id="selectedLinkId" :locale="locale" @select-node="selectNode" @select-link="selectLink" @unavailable="rendererError = true" />
                        <NodeSpace v-if="mode === 'space' && selectedId" class="map-scene" :devices="localDevices" :gateway-name="nameOf(selectedId)" :selected-device-id="selectedDeviceId" :layout="selectedConfig?.layout || 'topology'" :locale="locale" @select-device="selectDevice" />
                        <div v-if="mode === 'space'" class="map-space-caption"><strong>{{ selectedId ? nameOf(selectedId) : '' }}</strong><span>{{ selectedConfig?.layout === 'home' ? t('template') : t('configuredRelation') }}</span></div>
                        <div v-if="loading" class="map-scene-notice" role="status"><i class="pi pi-spin pi-spinner" aria-hidden="true"></i>{{ t('loading') }}</div>
                        <div v-else-if="rendererError && mode === 'globe'" class="map-scene-notice" role="status">{{ t('rendererFailed') }}</div>
                        <div v-else-if="!sceneNodes.length && mode === 'globe'" class="map-scene-notice"><i class="pi pi-map-marker" aria-hidden="true"></i><strong>{{ members.length ? t('noLocated') : t('noMembers') }}</strong><p>{{ members.length ? t('locationHint') : '' }}</p><Button v-if="selectedId" :label="t('setPosition')" icon="pi pi-map-marker" size="small" :disabled="!configurationReady" @click="editNode()" /><RouterLink v-else :to="{ name: 'networkDetail', params: { networkId } }">{{ t('addNodes') }}</RouterLink></div>
                        <div class="map-canvas-footer">
                            <div class="map-legend"><span><i class="legend-direct"></i>{{ t('direct') }}</span><span><i class="legend-relay"></i>{{ t('relay') }}</span><span><i class="legend-stale"></i>{{ t('stale') }}</span></div>
                        </div>
                    </section>

                    <aside class="map-inspector" :aria-label="t('selectNode')">
                        <template v-if="selectedMember && selectedConfig">
                            <header class="map-inspector-heading"><span class="map-node-avatar"><i :class="['pi', isGateway ? 'pi-wifi' : 'pi-server']" aria-hidden="true"></i></span><div class="map-node-identity"><span class="map-eyebrow">{{ isGateway ? t('gateway') : t('node') }}</span><h2>{{ nameOf(selectedMember.device_id) }}</h2></div><span class="map-presence" :class="{ online: selectedMember.online && selectedMember.running !== false }"><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ selectedMember.online && selectedMember.running !== false ? t('online') : t('offline') }}</span></header>
                            <div class="map-coordinate-line"><i class="pi pi-map-marker" aria-hidden="true"></i><span v-if="hasPosition(selectedConfig)">{{ selectedConfig.latitude?.toFixed(4) }}°, {{ selectedConfig.longitude?.toFixed(4) }}°</span><span v-else>{{ t('unset') }}</span><button :disabled="!configurationReady" :title="t('editNode')" :aria-label="t('editNode')" @click="editNode()"><i class="pi pi-pencil" aria-hidden="true"></i></button></div>
                            <dl class="map-node-facts"><div><dt>{{ t('virtualIp') }}</dt><dd>{{ selectedMember.runtime_virtual_ipv4 || selectedMember.virtual_ipv4 || selectedMember.allocated_ipv4 || '—' }}</dd></div></dl>
                            <template v-if="isGateway">
                                <div v-if="mode === 'globe'" class="map-space-preview"><NodeSpace :devices="localDevices" :gateway-name="nameOf(selectedMember.device_id)" :selected-device-id="selectedDeviceId" :layout="selectedConfig.layout" :locale="locale" @select-device="selectDevice" /></div>
                                <Button v-if="mode === 'globe'" :label="t('viewSpace')" icon="pi pi-arrow-up-right" severity="secondary" outlined fluid @click="mode = 'space'" />
                                <Button v-else :label="t('back')" icon="pi pi-arrow-left" severity="secondary" outlined fluid @click="mode = 'globe'" />
                            </template>
                            <section v-if="isGateway" class="map-detail-section">
                                <header><h3><i class="pi pi-sitemap" aria-hidden="true"></i>{{ t('devices') }}</h3><button :disabled="!configurationReady" :aria-label="t('addDevice')" :title="t('addDevice')" @click="editDevice()"><i class="pi pi-plus" aria-hidden="true"></i></button></header>
                                <p class="map-section-note">{{ t('configuredRelation') }}</p>
                                <p v-if="!localDevices.length" class="map-empty-note">{{ t('noDevices') }}</p>
                                <div v-for="device in localDevices" :key="device.id" class="map-device-row" :class="{ selected: device.id === selectedDeviceId }">
                                    <button class="map-device-select" :aria-pressed="device.id === selectedDeviceId" @click="selectDevice(device.id)"><i :class="['pi', deviceIcon(device.kind)]" aria-hidden="true"></i><span><strong>{{ device.name }}</strong><small>{{ t(`room_${device.room}`) }} · {{ device.address }}</small></span></button>
                                    <button class="map-icon-button" :aria-label="`${t('edit')} ${device.name}`" :disabled="!configurationReady" @click="editDevice(device)"><i class="pi pi-pencil" aria-hidden="true"></i></button>
                                    <button class="map-icon-button" :aria-label="`${t('remove')} ${device.name}`" :disabled="!configurationReady" @click="formError = ''; conflict = false; serviceConflict = false; removeCandidate = device"><i class="pi pi-trash" aria-hidden="true"></i></button>
                                </div>
                                <button v-if="selectedDevice" class="map-text-link" @click="selectedDeviceId = null; syncQuery()">{{ t('backToGateway') }}</button>
                            </section>
                            <section class="map-detail-section">
                                <header><h3><i class="pi pi-th-large" aria-hidden="true"></i>{{ selectedDevice ? `${selectedDevice.name} · ${t('services')}` : t('services') }}</h3><RouterLink :to="{ name: 'serviceNavigation' }" :aria-label="t('allServices')"><i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink></header>
                                <Message v-if="serviceError" severity="warn" :closable="false">{{ t('servicesFailed') }}</Message>
                                <p v-else-if="!associatedServices.length" class="map-empty-note">{{ t('noServices') }}</p>
                                <div class="map-service-grid"><button v-for="service in associatedServices" :key="service.id" class="map-service-card" @click="openService(service)"><i :class="['pi', serviceIcon(service.icon), 'map-service-icon']" aria-hidden="true"></i><i class="pi pi-arrow-up-right map-service-arrow" aria-hidden="true"></i><strong>{{ service.name }}</strong><small>{{ t('unknown') }}</small></button></div>
                                <RouterLink :to="{ name: 'serviceManagement', query: serviceQuery }" class="map-text-link"><i class="pi pi-plus" aria-hidden="true"></i>{{ t('addService') }}</RouterLink>
                            </section>
                            <section class="map-detail-section">
                                <header><h3><i class="pi pi-share-alt" aria-hidden="true"></i>{{ t('links') }}</h3><span class="map-section-count">{{ nodeLinks.length }}</span></header>
                                <p v-if="!nodeLinks.length" class="map-empty-note">{{ collecting ? t('collecting') : t('noLinks') }}</p>
                                <button v-for="link in nodeLinks" :key="link.id" class="map-link-row" :class="{ selected: selectedLinkId === link.id }" @click="selectLink(link.id)"><span><i class="pi pi-share-alt" aria-hidden="true"></i>{{ nameOf(link.source === selectedId ? link.target : link.source) }}</span><small>{{ link.stale ? t('stale') : formatLatency(link.latency_ms) }}</small></button>
                                <template v-if="nodePaths.length"><h4 class="map-route-heading">{{ t('routes') }}</h4><button v-for="path in nodePaths" :key="path.id" class="map-link-row" :class="{ selected: selectedRouteId === path.id }" @click="selectedRouteId = path.id; selectedLinkId = null"><span><i class="pi pi-directions" aria-hidden="true"></i>{{ nameOf(path.target) }}</span><small>{{ path.stale ? t('stale') : formatLatency(path.latency_ms) }}</small></button></template>
                            </section>
                            <footer class="map-inspector-actions"><Button :label="t('editNode')" icon="pi pi-sliders-h" severity="secondary" outlined :disabled="!configurationReady" @click="editNode()" /><RouterLink :to="{ name: 'deviceManagement', params: { deviceId: selectedMember.device_id, instanceId: networkId } }" class="map-text-link">{{ t('details') }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink></footer>
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
                    <div class="map-node-list-items"><button v-for="member in filteredMembers" :key="member.device_id" class="map-node-list-item" :class="{ selected: selectedId === member.device_id }" :aria-pressed="selectedId === member.device_id" @click="selectNode(member.device_id)"><i :class="['pi', configFor(member.device_id).role === 'gateway' ? 'pi-wifi' : 'pi-server']" aria-hidden="true"></i><span><strong>{{ nameOf(member.device_id) }}</strong><small>{{ hasPosition(configFor(member.device_id)) ? (member.runtime_virtual_ipv4 || member.virtual_ipv4 || t('node')) : t('unlocated') }}</small></span><span class="map-node-state" :class="{ online: member.online && member.running !== false }" :aria-label="member.online && member.running !== false ? t('online') : t('offline')"></span></button><p v-if="!filteredMembers.length" class="map-empty-note">{{ loading ? t('loading') : t('noMatches') }}</p></div>
                </section>
                <footer class="map-page-footnote"><span>{{ t('staticNote') }}</span><span v-if="unknownPeers">{{ t('unknownPeers', { count: unknownPeers }) }}</span><span>{{ lastUpdate ? t('updated', { time: new Date(lastUpdate).toLocaleTimeString(locale === 'cn' ? 'zh-CN' : 'en', { hour: '2-digit', minute: '2-digit', second: '2-digit' }) }) : t('noTelemetry') }}</span></footer>
            </template>
        </template>

        <Drawer v-model:visible="nodeEditor" position="right" :header="t('editNode')" class="map-edit-drawer" :style="{ width: 'min(440px, 100vw)' }" :dismissable="!saving" :show-close-icon="!saving">
            <form class="map-form" @submit.prevent="saveNode">
                <Message v-if="formError" severity="error" :closable="false">{{ formError }}<Button v-if="conflict" :label="t('reload')" text size="small" @click="reloadConfiguration" /><Button v-if="serviceConflict" :label="t('manageServices')" text size="small" @click="manageAssociatedServices" /></Message>
                <label>{{ t('name') }}<input v-model="nodeDraft.label" maxlength="100" :placeholder="nameOf(nodeDraft.device_id)" /></label>
                <div class="map-form-pair"><label>{{ t('latitude') }}<input v-model="latitudeDraft" inputmode="decimal" type="number" min="-90" max="90" step="any" placeholder="31.2304" /></label><label>{{ t('longitude') }}<input v-model="longitudeDraft" inputmode="decimal" type="number" min="-180" max="180" step="any" placeholder="121.4737" /></label></div>
                <button type="button" class="map-text-link" @click="latitudeDraft = ''; longitudeDraft = ''">{{ t('clearPosition') }}</button>
                <label>{{ t('role') }}<select v-model="nodeDraft.role" :aria-label="t('role')"><option value="node">{{ t('node') }}</option><option value="gateway">{{ t('gateway') }}</option></select></label><p class="map-form-help">{{ t('roleHint') }}</p>
                <label v-if="nodeDraft.role === 'gateway'">{{ t('layout') }}<select v-model="nodeDraft.layout" :aria-label="t('layout')"><option value="topology">{{ t('topology') }}</option><option value="home">{{ t('home') }}</option></select></label>
                <p v-if="nodeDraft.role === 'gateway' && nodeDraft.layout === 'home'" class="map-form-help">{{ t('spatialHint') }}</p>
                <div class="map-form-actions"><Button type="button" :label="t('cancel')" severity="secondary" outlined :disabled="saving" @click="nodeEditor = false" /><Button type="submit" :label="t('save')" :loading="saving" :disabled="conflict" /></div>
            </form>
        </Drawer>
        <Drawer v-model:visible="deviceEditor" position="right" :header="t('deviceEditor')" class="map-edit-drawer" :style="{ width: 'min(440px, 100vw)' }" :dismissable="!saving" :show-close-icon="!saving">
            <form class="map-form" @submit.prevent="saveDevice">
                <Message v-if="formError" severity="error" :closable="false">{{ formError }}<Button v-if="conflict" :label="t('reload')" text size="small" @click="reloadConfiguration" /><Button v-if="serviceConflict" :label="t('manageServices')" text size="small" @click="manageAssociatedServices" /></Message>
                <p class="map-form-help">{{ t('parentGateway') }} · {{ nameOf(deviceDraft.gateway_device_id) }}</p>
                <label>{{ t('name') }}<input v-model="deviceDraft.name" maxlength="100" required /></label>
                <label>{{ t('address') }}<input v-model="deviceDraft.address" maxlength="255" required placeholder="192.168.31.20" /></label>
                <label>{{ t('kind') }}<select v-model="deviceDraft.kind" :aria-label="t('kind')"><option v-for="kind in deviceKinds" :key="kind" :value="kind">{{ t(`kind_${kind}`) }}</option></select></label>
                <label>{{ t('room') }}<select v-model="deviceDraft.room" :aria-label="t('room')"><option v-for="room in rooms" :key="room" :value="room">{{ t(`room_${room}`) }}</option></select></label>
                <p class="map-form-help">{{ t('configuredRelation') }}</p>
                <div class="map-form-actions"><Button type="button" :label="t('cancel')" severity="secondary" outlined :disabled="saving" @click="deviceEditor = false" /><Button type="submit" :label="t('save')" :loading="saving" :disabled="conflict" /></div>
            </form>
        </Drawer>
        <Dialog :visible="!!removeCandidate" modal :header="t('removeDevice')" :style="{ width: 'min(430px, calc(100vw - 24px))' }" :closable="!saving" @update:visible="value => { if (!value) removeCandidate = null }">
            <p>{{ removeCandidate?.name }}</p><p class="map-form-help">{{ t('removeHint') }}</p><Message v-if="formError" severity="error" :closable="false">{{ formError }}<Button v-if="conflict" :label="t('reload')" text size="small" @click="reloadConfiguration" /><Button v-if="serviceConflict" :label="t('manageServices')" text size="small" @click="manageAssociatedServices" /></Message>
            <template #footer><Button :label="t('cancel')" severity="secondary" outlined :disabled="saving" @click="removeCandidate = null" /><Button :label="t('remove')" severity="danger" :loading="saving" :disabled="conflict" @click="removeDevice" /></template>
        </Dialog>
    </div>
</template>

<style scoped>
.network-map-page { gap: 18px; }
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
.map-legend { display: flex; flex-wrap: wrap; gap: 12px; color: #c4d1dc; font-size: 11px; padding: 9px 12px; border-radius: 6px; background: #101a24cc; }.map-legend span { display: flex; align-items: center; gap: 7px; }.map-legend i { width: 20px; border-top: 2px solid #55e8ed; }.map-legend .legend-relay { border-color: #fbbf24; border-top-style: dashed; }.map-legend .legend-stale { border-color: #768596; border-top-style: dashed; }
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
