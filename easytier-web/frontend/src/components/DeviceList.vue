<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { Button, Checkbox, Column, DataTable, Dialog, Drawer, InputText, SelectButton, Skeleton, useToast } from 'primevue';
import Tooltip from 'primevue/tooltip';
import { useRoute, useRouter } from 'vue-router';
import { Polling, Utils } from 'easytier-frontend-lib';
import DeviceDetails from './DeviceDetails.vue';
import DeviceAttributesDialog from './DeviceAttributesDialog.vue';
import { useI18n } from 'vue-i18n'
import ApiClient from '../modules/api';
import { overviewMessages } from '../modules/overview-messages';
import { nodeLocationCoordinates, nodeLocationLabel } from '../modules/node-location';
import type { AssetContextResponse, AssetNode, AssetNodeAttributeChanges } from '../types/node-assets';

const { t, locale } = useI18n()
const copy = computed(() => overviewMessages[locale.value === 'cn' ? 'cn' : 'en']);

// 注册 Tooltip 指令
const vTooltip = Tooltip;

const props = defineProps({
    api: ApiClient,
    centralEnabled: Boolean,
});

const api = props.api;
const route = useRoute();
const router = useRouter();
const toast = useToast();

const deviceList = ref<Array<Utils.DeviceInfo> | undefined>(undefined);
const search = ref('');
const statusFilter = ref<'all' | 'online' | 'offline'>('all');
const onlineCount = computed(() => deviceList.value?.filter(device => device.online).length);
const offlineCount = computed(() => deviceList.value ? deviceList.value.length - (onlineCount.value ?? 0) : undefined);
const instanceCount = computed(() => deviceList.value?.reduce((sum, device) => sum + device.running_network_count, 0));
const statusOptions = computed(() => [
    { value: 'all' as const, label: copy.value.allDevices, count: deviceList.value?.length },
    { value: 'online' as const, label: copy.value.online, count: onlineCount.value },
    { value: 'offline' as const, label: copy.value.offline, count: offlineCount.value },
]);
const loadError = ref(false);
const refreshing = ref(false);
let requestInFlight: Promise<Array<any>> | undefined;
const expandedRows = ref({});
const assetContext = ref<AssetContextResponse | null>(null);
const assetCounts = ref<Record<string, number>>({});
const assetNodes = ref<Record<string, AssetNode>>({});
const assetCountsFailed = ref(false);
let assetCountRequest = 0;
let disposed = false;
const loadAssetCounts = async () => {
    if (!api) return;
    const request = ++assetCountRequest;
    try {
        const context = assetContext.value ?? await api.assetsContext();
        if (disposed || request !== assetCountRequest) return;
        assetContext.value = context;
        if (!context.available) {
            assetNodes.value = {};
            assetCounts.value = {};
            return;
        }
        // This single owner-scoped read provides both counts and the explicit
        // public-server role. Do not infer that role from an IP address.
        const result = await api.assetsNodes(context.actor.id);
        if (disposed || request !== assetCountRequest) return;
        assetNodes.value = Object.fromEntries(result.items.map(item => [item.machine_id, item]));
        assetCounts.value = Object.fromEntries(result.items.map(item => [item.machine_id, item.device_count]));
        assetCountsFailed.value = false;
    } catch {
        if (!disposed && request === assetCountRequest) {
            assetCountsFailed.value = true;
            assetCounts.value = {};
            assetNodes.value = {};
        }
    }
};
const assetCountLabel = (machineId: string) => t('assets.page.device_count', { count: assetCounts.value[machineId] ?? '—' });
const isPublicNode = (machineId: string) => assetNodes.value[machineId]?.is_public_server === true;
const openAssets = (machineId: string) => {
    if (!assetContext.value?.available) return;
    router.push({ name: 'nodeAssets', query: { owner: String(assetContext.value.actor.id), node: machineId } });
};

// 桌面端可在表格与卡片视图之间切换，选择持久化；移动端始终使用紧凑列表
const viewMode = ref<'table' | 'card'>(localStorage.getItem('deviceList.viewMode') === 'card' ? 'card' : 'table');
const viewModeOptions = [
    { value: 'table', icon: 'pi pi-table' },
    { value: 'card', icon: 'pi pi-th-large' },
];
const viewModeLabel = (value: string) => t(value === 'card' ? 'web.console.view_card' : 'web.console.view_table');
watch(viewMode, (mode) => localStorage.setItem('deviceList.viewMode', mode));

// Zero-device state is where users look for "how do I add one": surface
// the enrollment command right there.
const enrollInfo = ref<Awaited<ReturnType<ApiClient['get_console_info']>> | null>(null);
const enrollCommand = computed(() => {
    if (!enrollInfo.value) return '';
    const host = window.location.hostname;
    const { config_server_protocol: proto, config_server_port: port, username } = enrollInfo.value;
    return `easytier-core --config-server ${proto}://${host}:${port}/${username}`;
});

const copyEnrollCommand = async () => {
    try {
        await navigator.clipboard.writeText(enrollCommand.value);
        toast.add({ severity: 'success', summary: t('web.common.confirm'), life: 1500 });
    } catch (e) {
        console.error(e);
    }
};

const loadDevices = async (manual = false) => {
    // Refresh owner-scoped asset metadata explicitly, not on every heartbeat poll.
    if (manual) void loadAssetCounts();
    // A manual refresh can show progress for an already running background request.
    if (manual) refreshing.value = true;
    while (requestInFlight) {
        await requestInFlight.catch(() => {});
        if (!manual || disposed) return;
    }
    if (disposed) return;
    requestInFlight = api?.list_machines() ?? Promise.resolve([]);
    try {
        const resp = await requestInFlight;
        if (disposed) return;
        deviceList.value = (resp || []).map(Utils.buildDeviceInfo);
        loadError.value = false;
    } catch (e) {
        if (!disposed) loadError.value = true;
        console.error(e);
    } finally {
        requestInFlight = undefined;
        refreshing.value = false;
    }
};

Polling.useAutoRefreshTask(() => loadDevices(), 1000);

onMounted(() => {
    void loadAssetCounts();
    api?.get_console_info().then(info => { enrollInfo.value = info; }).catch(() => {});
});

onUnmounted(() => {
    disposed = true;
    ++assetCountRequest;
});

const selectedDeviceId = computed<string | undefined>(() => route.params.deviceId as string);

const deviceManageVisible = computed<boolean>({
    get: () => !!selectedDeviceId.value,
    set: (value) => {
        if (!value) {
            router.push({ name: 'deviceList', params: { deviceId: undefined } });
        }
    }
});

const selectedDeviceHostname = computed<string | undefined>(() => {
    const device = deviceList.value?.find((device) => device.machine_id === selectedDeviceId.value);
    return device ? deviceName(device) : undefined;
});

// 处理设备管理
const handleDeviceManagement = (device: Utils.DeviceInfo) => {
    const instanceId = device.running_network_instances?.[0];
    router.push({
        name: 'deviceManagement',
        params: {
            deviceId: device.machine_id,
            instanceId: instanceId
        }
    });
};

// 删除设备：若还是网络成员会连带移出并回收托管配置；可选同时拉黑
const deleteVisible = ref(false);
const deleting = ref(false);
const deleteTarget = ref<Utils.DeviceInfo | null>(null);
const deleteAlsoBlock = ref(true);

const handleDeleteDevice = (device: Utils.DeviceInfo) => {
    deleteTarget.value = device;
    deleteAlsoBlock.value = true;
    deleteVisible.value = true;
};

// 别名仅改变控制台内的显示名，不影响设备上报的主机名
const deviceName = (device: Utils.DeviceInfo) => device.alias || device.hostname || device.machine_id;

const attributesTarget = ref<Utils.DeviceInfo | null>(null);
const attributesNode = ref<AssetNode | null>(null);
const openingAttributes = ref(false);
const handleEditAttributes = async (device: Utils.DeviceInfo) => {
    if (attributesTarget.value || openingAttributes.value) return;
    openingAttributes.value = true;
    try {
        // Load the saved public role before editing so an unknown value cannot
        // silently clear a server's role when only its location/model changes.
        await loadAssetCounts();
        if (disposed) return;
        const node = assetNodes.value[device.machine_id];
        if (!node || !assetContext.value?.available) {
            toast.add({ severity: 'error', summary: t('web.console.load_failed'), life: 4000 });
            return;
        }
        attributesNode.value = { ...node };
        attributesTarget.value = { ...device };
    } finally {
        if (!disposed) openingAttributes.value = false;
    }
};
const closeAttributes = () => { attributesTarget.value = null; attributesNode.value = null; };
const handleAttributesUpdated = async (machineId: string, changes: AssetNodeAttributeChanges) => {
    // The asset's manually maintained public_ip is distinct from DeviceInfo.public_ip
    // (the observed management connection URL); never write the former into the latter.
    ++assetCountRequest;
    const { public_ip: _manualPublicIp, is_public_server: _publicRole, ...machineChanges } = changes;
    if (assetNodes.value[machineId]) Object.assign(assetNodes.value[machineId], changes);
    if (attributesNode.value?.machine_id === machineId) Object.assign(attributesNode.value, changes);
    if (attributesTarget.value?.machine_id === machineId) Object.assign(attributesTarget.value, machineChanges);
    // Finish any pre-save list response before applying confirmed changes.
    await requestInFlight?.catch(() => {});
    if (disposed) return;
    const device = deviceList.value?.find(device => device.machine_id === machineId);
    if (device) Object.assign(device, machineChanges);
};
const handleAttributesSaved = async () => {
    closeAttributes();
    await requestInFlight?.catch(() => {});
    if (disposed) return;
    toast.add({ severity: 'success', summary: t('web.device.attributes_saved'), life: 2000 });
    await Promise.all([loadDevices(), loadAssetCounts()]);
};

const confirmDeleteDevice = async () => {
    if (!deleteTarget.value || deleting.value) return;
    deleting.value = true;
    try {
        await api?.delete_machine(deleteTarget.value.machine_id, deleteAlsoBlock.value);
        deleteVisible.value = false;
        await loadDevices(true);
    } catch (e: any) {
        toast.add({ severity: 'error', summary: t('web.console.delete_device'), detail: e?.response?.data?.message ?? String(e), life: 5000 });
    } finally {
        deleting.value = false;
    }
};

// 搜索过滤 + 预排序：离线设备始终排在在线设备之后；列排序交给 DataTable
const sortedDeviceList = computed(() => {
    const query = search.value.trim().toLocaleLowerCase();
    return (deviceList.value ?? [])
        .filter(device => `${device.alias ?? ''} ${device.hostname ?? ''} ${device.public_ip ?? ''} ${device.device_model ?? ''} ${nodeLocationLabel(device.geo_location)}`.toLocaleLowerCase().includes(query) &&
            (statusFilter.value === 'all' || (statusFilter.value === 'online' ? device.online : !device.online)))
        .sort((a, b) => {
            const online = Number(b.online === true) - Number(a.online === true);
            if (online !== 0) return online;
            return (deviceName(a) ?? '').localeCompare(deviceName(b) ?? '');
        });
});

// User-selected geography, never the connection IP's GeoIP result.
const locationText = (device: Utils.DeviceInfo) => nodeLocationLabel(device.geo_location)
    || nodeLocationCoordinates(device.geo_location) || t('web.device.unknown_location');
</script>

<template>
    <div class="console-page device-list-page">
        <header class="page-heading">
            <div>
                <p class="inventory-eyebrow">{{ copy.deviceEyebrow }}</p><h1>{{ t('web.device.list') }}</h1>
                <p class="page-description">{{ copy.deviceDescription }}</p>
            </div>
        </header>
        <section class="inventory-summary" :aria-label="copy.deviceOverview">
            <div><span class="inventory-summary-icon"><i class="pi pi-server" aria-hidden="true"></i></span><div><span>{{ copy.registered }}</span><strong>{{ deviceList?.length ?? '—' }}</strong></div></div>
            <div><span class="inventory-summary-icon is-online"><i class="pi pi-check-circle" aria-hidden="true"></i></span><div><span>{{ copy.online }}</span><strong>{{ onlineCount ?? '—' }}</strong></div></div>
            <div><span class="inventory-summary-icon"><i class="pi pi-share-alt" aria-hidden="true"></i></span><div><span>{{ copy.runningInstances }}</span><strong>{{ instanceCount ?? '—' }}</strong></div></div>
        </section>
        <section class="console-panel inventory-panel">
            <div class="inventory-status-tabs" :aria-label="copy.matchingDevices">
                <button v-for="option in statusOptions" :key="option.value" type="button" :aria-pressed="statusFilter === option.value" :class="{ 'is-selected': statusFilter === option.value }" @click="statusFilter = option.value">
                    <i v-if="option.value !== 'all'" class="pi pi-circle-fill" :class="{ 'is-online': option.value === 'online' }" aria-hidden="true"></i>{{ option.label }}<span>{{ option.count ?? '—' }}</span>
                </button>
            </div>
            <div class="list-toolbar">
                <span class="search-field"><i class="pi pi-search" aria-hidden="true" /><InputText v-model="search"
                        :placeholder="t('web.console.search_devices')"
                        :aria-label="t('web.console.search_devices')" /></span>
                <div class="flex items-center gap-3">
                    <span v-if="deviceList" class="table-count">{{ t('web.console.items', { count: sortedDeviceList.length }) }}</span>
                    <SelectButton v-model="viewMode" :options="viewModeOptions" optionValue="value"
                        :allow-empty="false" class="view-toggle">
                        <template #option="{ option }">
                            <i :class="option.icon" v-tooltip.top="viewModeLabel(option.value)"
                                :aria-label="viewModeLabel(option.value)"></i>
                        </template>
                    </SelectButton>
                    <Button icon="pi pi-refresh" text severity="secondary" :aria-label="t('web.console.refresh')"
                        :loading="refreshing" @click="loadDevices(true)" />
                </div>
            </div>
            <div v-if="deviceList === undefined && !loadError" class="loading-rows">
                <Skeleton v-for="i in 4" :key="i" height="2rem" />
            </div>
            <div v-else-if="loadError" class="console-empty-state">
                <i class="pi pi-exclamation-triangle" aria-hidden="true"></i>
                <h2>{{ t('web.console.load_failed') }}</h2>
                <Button :label="t('web.console.retry')" icon="pi pi-refresh" @click="loadDevices(true)" />
            </div>
            <div v-else-if="sortedDeviceList.length === 0" class="console-empty-state">
                <i class="pi pi-server" aria-hidden="true"></i>
                <h2>{{ search || statusFilter !== 'all' ? copy.noFilterResults : t('web.console.devices_empty') }}</h2>
                <p v-if="!search && statusFilter === 'all'">{{ t('web.console.devices_empty_hint') }}</p>
                <Button v-if="search || statusFilter !== 'all'" :label="copy.clearFilters" text @click="search = ''; statusFilter = 'all'" />
                <div v-if="!search && statusFilter === 'all' && enrollCommand" class="flex items-center gap-2 justify-center mt-2 flex-wrap">
                    <code class="mono-value p-2 rounded" style="background: var(--console-ground); word-break: break-all">{{ enrollCommand }}</code>
                    <Button icon="pi pi-copy" severity="secondary" outlined size="small"
                        :aria-label="t('web.console.enroll_copy')" @click="copyEnrollCommand" />
                </div>
            </div>
            <template v-else>
                <p v-if="assetCountsFailed" class="muted" role="status">{{ t('web.device.asset_metadata_failed') }}</p>
                <DataTable v-if="viewMode === 'table'" :value="sortedDeviceList" class="console-table desktop-list" size="small"
                    v-model:expandedRows="expandedRows" dataKey="machine_id" scrollable
                    :paginator="sortedDeviceList.length > 20" :rows="20">
                    <Column expander style="width: 2.5rem" />
                    <Column field="hostname" :header="t('web.device.hostname')" sortable>
                        <template #body="{ data }">
                            <button class="entity-link" @click="handleDeviceManagement(data)">
                                <span class="status-dot" :class="data.online ? 'online' : 'offline'"></span>
                                <span class="min-w-0">
                                    <span class="node-name-line"><span class="preview-name">{{ deviceName(data) }}</span>
                                        <span v-if="isPublicNode(data.machine_id)" class="node-public-badge" :title="t('assets.page.public_node')"><i class="pi pi-globe" aria-hidden="true"></i>{{ t('web.device.public_badge') }}</span></span>
                                    <span v-if="data.alias && data.hostname" class="preview-secondary mono-value">{{ data.hostname }}</span>
                                </span>
                            </button>
                        </template>
                    </Column>
                    <Column :header="t('web.node_location.title')" style="min-width: 12rem">
                        <template #body="{ data }"><span class="node-location-value" :title="[locationText(data), nodeLocationCoordinates(data.geo_location)].filter(Boolean).join(' / ')">{{ locationText(data) }}</span></template>
                    </Column>
                    <Column field="device_model" :header="t('web.device.device_model')" sortable style="min-width: 9rem">
                        <template #body="{ data }"><span class="node-model-value" :title="data.device_model || undefined">{{ data.device_model || '—' }}</span></template>
                    </Column>
                    <Column v-if="assetContext?.available" :header="t('assets.page.devices')" style="min-width: 7rem">
                        <template #body="{ data }"><Button class="asset-count-button" :label="assetCountLabel(data.machine_id)" icon="pi pi-desktop" text size="small" @click="openAssets(data.machine_id)" /></template>
                    </Column>
                    <Column field="running_network_count" :header="t('web.device.networks')" sortable style="width: 7rem" />
                    <Column field="easytier_version" :header="t('web.device.version')" sortable style="width: 7rem">
                        <template #body="{ data }"><span class="mono-value muted">v{{ (data.easytier_version || '?').split('-')[0] }}</span></template>
                    </Column>
                    <Column :header="t('web.device.last_seen')" style="width: 10rem">
                        <template #body="{ data }"><span v-if="!data.online && data.last_seen" class="muted">{{ data.last_seen.replace('T', ' ').slice(5, 16) }}</span><span v-else class="muted">—</span></template>
                    </Column>
                    <Column :header="t('web.console.manage')" style="width: 7rem">
                        <template #body="{ data }">
                            <div class="flex items-center gap-1">
                                <Button v-if="centralEnabled" v-tooltip.top="t('web.console.delete_device')" icon="pi pi-trash" severity="danger"
                                    text rounded :aria-label="t('web.console.delete_device')" @click="handleDeleteDevice(data)" />
                                <Button icon="pi pi-cog" severity="secondary" text rounded
                                    :aria-label="`${t('web.console.manage')} ${data.hostname}`"
                                    @click="handleDeviceManagement(data)" />
                                <Button v-if="centralEnabled" v-tooltip.top="t('web.device.edit_attributes')" icon="pi pi-file-edit"
                                    severity="secondary" text rounded :aria-label="t('web.device.edit_attributes')" :disabled="openingAttributes"
                                    @click="handleEditAttributes(data)" />
                            </div>
                        </template>
                    </Column>
                    <template #expansion="{ data }">
                        <DeviceDetails :device="data" containerClass="row-details-content" :compact="true" />
                    </template>
                </DataTable>
                <div v-if="viewMode === 'card'" class="device-card-grid">
                    <article v-for="device in sortedDeviceList" :key="device.machine_id" class="device-card"
                        :class="{ offline: !device.online }">
                        <div class="device-card-title">
                            <button class="entity-link" @click="handleDeviceManagement(device)">
                                <span class="status-dot" :class="device.online ? 'online' : 'offline'"></span>
                                <span class="min-w-0">
                                    <span class="node-name-line"><span class="preview-name">{{ deviceName(device) }}</span>
                                        <span v-if="isPublicNode(device.machine_id)" class="node-public-badge" :title="t('assets.page.public_node')"><i class="pi pi-globe" aria-hidden="true"></i>{{ t('web.device.public_badge') }}</span></span>
                                    <span v-if="device.alias && device.hostname" class="preview-secondary mono-value">{{ device.hostname }}</span>
                                </span>
                            </button>
                            <span class="device-version-badge" :title="device.easytier_version">v{{ (device.easytier_version || '?').split('-')[0] }}</span>
                        </div>
                        <div class="device-card-presence"><span :class="{ 'is-online': device.online }">{{ device.online ? copy.online : copy.offline }}</span></div>
                        <div class="device-card-actions">
                            <p class="device-card-location" :title="locationText(device)">
                                <i class="pi pi-map-marker" aria-hidden="true"></i>
                                <span>{{ locationText(device) }}</span>
                            </p>
                            <div class="device-card-controls">
                                <Button v-if="centralEnabled" v-tooltip.top="t('web.console.delete_device')" icon="pi pi-trash" severity="danger"
                                    text rounded class="w-8 h-8" :aria-label="t('web.console.delete_device')" @click="handleDeleteDevice(device)" />
                                <Button v-if="assetContext?.available" class="asset-count-button" :label="assetCountLabel(device.machine_id)" icon="pi pi-desktop"
                                    text size="small" @click="openAssets(device.machine_id)" />
                                <span class="device-network-count"
                                    :title="`${t('web.device.networks')}: ${device.running_network_count}`">{{ device.running_network_count }}</span>
                                <Button icon="pi pi-cog" severity="secondary" text rounded class="w-8 h-8"
                                    :aria-label="`${t('web.console.manage')} ${device.hostname}`"
                                    @click="handleDeviceManagement(device)" />
                                <Button v-if="centralEnabled" v-tooltip.top="t('web.device.edit_attributes')" icon="pi pi-file-edit"
                                    severity="secondary" text rounded class="w-8 h-8" :aria-label="t('web.device.edit_attributes')" :disabled="openingAttributes"
                                    @click="handleEditAttributes(device)" />
                            </div>
                        </div>
                        <DeviceDetails :device="device" :card-view="true" />
                        <dl class="device-attributes-summary">
                            <div><dt>{{ t('web.device.device_model') }}</dt><dd :title="device.device_model || undefined">{{ device.device_model || '—' }}</dd></div>
                            <div v-if="nodeLocationCoordinates(device.geo_location)"><dt>{{ t('web.node_location.coordinates') }}</dt><dd :title="nodeLocationCoordinates(device.geo_location)">{{ nodeLocationCoordinates(device.geo_location) }}</dd></div>
                        </dl>
                    </article>
                </div>
                <div class="mobile-list">
                    <article v-for="device in sortedDeviceList" :key="device.machine_id" class="mobile-list-item">
                        <div class="flex items-center justify-between gap-3">
                            <button class="entity-link" @click="handleDeviceManagement(device)">
                                <span class="status-dot" :class="device.online ? 'online' : 'offline'"></span>
                                <span class="node-name-line"><span class="preview-name">{{ deviceName(device) }}</span>
                                    <span v-if="isPublicNode(device.machine_id)" class="node-public-badge" :title="t('assets.page.public_node')"><i class="pi pi-globe" aria-hidden="true"></i>{{ t('web.device.public_badge') }}</span></span>
                            </button>
                            <span class="mono-value muted">v{{ (device.easytier_version || '?').split('-')[0] }}</span>
                        </div>
                        <p class="preview-secondary">{{ locationText(device) }} · {{ device.running_network_count }} {{ t('web.console.instances') }}</p>
                        <p class="preview-secondary">{{ t('web.device.device_model') }}：{{ device.device_model || '—' }}</p>
                        <Button v-if="assetContext?.available" class="asset-count-button" :label="assetCountLabel(device.machine_id)" icon="pi pi-desktop"
                            text size="small" @click="openAssets(device.machine_id)" />
                        <details>
                            <summary>{{ t('web.console.details') }}</summary>
                            <DeviceDetails :device="device" :compact="true" />
                        </details>
                        <Button v-if="centralEnabled" class="attributes-edit" :label="t('web.device.edit_attributes')" :disabled="openingAttributes"
                            icon="pi pi-file-edit" text size="small" severity="secondary" @click="handleEditAttributes(device)" />
                    </article>
                </div>
            </template>
        </section>

        <DeviceAttributesDialog v-if="attributesTarget && attributesNode" :device="attributesTarget" :node="attributesNode" :api="api"
            @close="closeAttributes" @updated="handleAttributesUpdated" @saved="handleAttributesSaved" />

        <!-- 删除确认对话框 -->
        <Dialog v-model:visible="deleteVisible" modal :header="t('web.console.delete_device')" :style="{ width: '26rem' }"
            :closable="!deleting" :closeOnEscape="!deleting" :dismissableMask="!deleting">
            <div class="flex flex-col gap-3">
                <div>{{ t('web.console.delete_device_confirm', { device: deleteTarget?.hostname || deleteTarget?.machine_id }) }}</div>
                <div class="flex items-center gap-2">
                    <Checkbox inputId="delete-also-block" v-model="deleteAlsoBlock" :binary="true" :disabled="deleting" />
                    <label for="delete-also-block">{{ t('web.console.block_device') }}</label>
                </div>
                <div class="text-xs text-500">{{ deleteAlsoBlock ? t('web.console.block_device_hint') : t('web.console.no_block_device_hint') }}</div>
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" icon="pi pi-times" text severity="secondary" :disabled="deleting" @click="deleteVisible = false" />
                <Button :label="t('web.common.confirm')" icon="pi pi-check" :loading="deleting" severity="danger" @click="confirmDeleteDevice" />
            </template>
        </Dialog>

        <Drawer v-model:visible="deviceManageVisible" position="right" class="console-device-drawer" :baseZIndex="1000" :dismissable="true">
            <template #header>
                <div class="drawer-heading flex items-baseline gap-3 min-w-0">
                    <h2 class="truncate">{{ selectedDeviceHostname }}</h2>
                    <p class="mono-value truncate">{{ selectedDeviceId }}</p>
                </div>
            </template>
            <RouterView v-slot="{ Component }">
                <component :is="Component" :api="api" :deviceList="deviceList" @update="loadDevices(true)" />
            </RouterView>
        </Drawer>
    </div>
</template>

<style scoped>
.inventory-eyebrow { margin: 0 0 8px; color: var(--p-primary-color); font-size: 10px; font-weight: 650; letter-spacing: .14em; }
.inventory-summary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
.inventory-summary > div { display: flex; align-items: center; gap: 16px; padding: 20px 22px; border: 1px solid var(--console-border); border-radius: 12px; background: var(--console-panel); min-width: 0; }
.inventory-summary-icon { display: inline-flex; width: 42px; height: 42px; align-items: center; justify-content: center; flex: none; border: 1px solid color-mix(in srgb, var(--p-primary-color) 20%, var(--console-border)); background: var(--console-accent-soft); border-radius: 11px; color: var(--p-primary-color); }
.inventory-summary-icon.is-online { color: var(--p-green-500); background: color-mix(in srgb, var(--p-green-500) 8%, var(--console-panel)); border-color: color-mix(in srgb, var(--p-green-500) 20%, var(--console-border)); }
.inventory-summary > div > div { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.inventory-summary span:not(.inventory-summary-icon) { font-size: 12px; color: var(--console-muted); }
.inventory-summary strong { font-size: 27px; font-weight: 550; line-height: 1.2; letter-spacing: -.035em; }
.inventory-status-tabs { display: flex; flex-wrap: wrap; gap: 6px; padding: 16px 18px 0; }
.inventory-status-tabs > button { display: inline-flex; align-items: center; gap: 8px; padding: 8px 13px; font-size: 12px; min-height: 36px; border-radius: 7px; color: var(--console-muted); border: 1px solid transparent; transition: color .15s, background-color .15s; }
.inventory-status-tabs > button:hover { color: var(--p-text-color); background: var(--p-content-hover-background); }
.inventory-status-tabs > button.is-selected { color: var(--p-primary-color); border-color: color-mix(in srgb, var(--p-primary-color) 22%, var(--console-border)); background: var(--console-accent-soft); }
.inventory-status-tabs > button:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; }
.inventory-status-tabs button > span { font-size: 10px; min-width: 18px; padding: 1px 5px; border-radius: 4px; background: color-mix(in srgb, var(--p-text-color) 6%, transparent); font-variant-numeric: tabular-nums; }
.inventory-status-tabs .pi { font-size: 6px; }
.inventory-status-tabs .is-online { color: var(--p-green-500); }
.device-list-page .list-toolbar { border-top: 0; padding-top: 14px; }
.device-card-presence { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-top: 4px; font-size: 11px; color: var(--console-muted); }
.device-card-presence > span:first-child { padding: 2px 7px; border: 1px solid var(--console-border); border-radius: 5px; flex: none; }
.device-card-presence > span.is-online { color: var(--p-green-700); border-color: color-mix(in srgb, var(--p-green-500) 24%, var(--console-border)); background: color-mix(in srgb, var(--p-green-500) 7%, transparent); }
html.app-dark .device-card-presence > span.is-online { color: var(--p-green-300); }
@media (max-width: 767px) { .inventory-summary { gap: 8px; } .inventory-summary > div { padding: 14px 12px; gap: 8px; flex-direction: column; align-items: flex-start; } .inventory-summary-icon { width: 28px; height: 28px; border-radius: 7px; font-size: 12px; } .inventory-summary strong { font-size: 24px; } .inventory-summary span:not(.inventory-summary-icon) { font-size: 11px; } .inventory-status-tabs { padding: 12px 12px 0; gap: 4px; } .inventory-status-tabs > button { padding: 7px 9px; gap: 6px; } .device-card-grid { display: none; } }
@media (prefers-reduced-motion: reduce) { .inventory-status-tabs > button, .device-card { transition: none; } }

.device-card-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 16px;
    padding: 18px;
}

.device-card {
    background: var(--console-panel);
    border: 1px solid var(--console-border);
    border-radius: 12px;
    padding: 20px;
    transition: border-color .16s, box-shadow .16s;
    display: flex;
    flex-direction: column;
    gap: 8px;
}

.device-card:hover { border-color: var(--console-border-strong); box-shadow: var(--console-shadow); }
.device-card.offline { border-style: dashed; }

.device-card-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 0;
}

.device-card-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
}
.device-version-badge {
    flex-shrink: 0;
    background-color: var(--console-accent-soft);
    color: var(--p-primary-color);
    padding: 0.1rem 0.4rem;
    border-radius: 0.75rem;
    font-size: 0.65rem;
    font-weight: 500;
    letter-spacing: 0.02em;
    white-space: nowrap;
}

.device-card-location {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    margin: 0;
    color: var(--console-muted, #64748b);
    font-size: 0.875rem;
    line-height: 1.25rem;
}

.device-card-location .pi-map-marker {
    flex-shrink: 0;
    font-size: 0.9rem;
}

.device-card-location span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.device-card-controls {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    gap: 0.25rem;
}

.device-network-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    min-width: 1.5rem;
    height: 1.5rem;
    padding: 0 0.25rem;
    border-radius: 9999px;
    background-color: var(--console-accent-soft);
    color: var(--p-primary-color);
    font-size: 0.75rem;
    font-weight: 500;
}

.node-name-line { display: inline-flex; align-items: center; flex-wrap: wrap; gap: .45rem; min-width: 0; }
.node-public-badge { display: inline-flex; align-items: center; gap: .25rem; flex-shrink: 0; padding: .1rem .45rem; border-radius: .35rem; font-size: .75rem; font-weight: 500; white-space: nowrap; color: var(--p-primary-color); background: var(--console-accent-soft); }
.node-public-badge i { font-size: .7rem; }
.node-location-value, .node-model-value { display: block; max-width: 20rem; overflow-wrap: anywhere; }
.asset-count-button, .asset-count-button :deep(.p-button-label) { white-space: nowrap; }
.device-attributes-summary { margin: 0; padding-top: 10px; border-top: 1px solid var(--console-border); font-size: 0.875rem; }
.device-attributes-summary > div { display: flex; gap: 0.5rem; min-width: 0; }
.device-attributes-summary dt { color: var(--console-muted, #64748b); flex-shrink: 0; }
.device-attributes-summary dd { margin: 0; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.attributes-edit { align-self: flex-start; }
@media (max-width: 767px) { .device-card-grid { display: none; } }
</style>
