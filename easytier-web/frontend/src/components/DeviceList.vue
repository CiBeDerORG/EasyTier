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
import { nodeLocationCoordinates, nodeLocationLabel } from '../modules/node-location';
import type { AssetContextResponse } from '../types/node-assets';

const { t } = useI18n()

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
const loadError = ref(false);
const refreshing = ref(false);
let requestInFlight: Promise<Array<any>> | undefined;
const expandedRows = ref({});
const assetContext = ref<AssetContextResponse | null>(null);
const assetCounts = ref<Record<string, number>>({});
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
        if (!context.available) return;
        const result = await api.assetsDeviceCounts(context.actor.id);
        if (disposed || request !== assetCountRequest) return;
        assetCounts.value = Object.fromEntries(result.items.map(item => [item.machine_id, item.device_count]));
        assetCountsFailed.value = false;
    } catch {
        if (!disposed && request === assetCountRequest) { assetCountsFailed.value = true; assetCounts.value = {}; }
    }
};
const assetCountLabel = (machineId: string) => t('assets.page.device_count', { count: assetCounts.value[machineId] ?? '—' });
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
    // Only the user's refresh reloads manual asset counts; the existing one-
    // second instance poll never calls any asset API.
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
const handleEditAttributes = (device: Utils.DeviceInfo) => {
    if (!attributesTarget.value) attributesTarget.value = { ...device };
};
const handleAttributesUpdated = async (machineId: string, changes: Partial<Utils.DeviceAttributes> & { alias?: string }) => {
    // Reflect each confirmed write even when another field fails or editing is cancelled.
    if (attributesTarget.value?.machine_id === machineId) Object.assign(attributesTarget.value, changes);
    // Finish any pre-save list response before applying confirmed changes.
    await requestInFlight?.catch(() => {});
    if (disposed) return;
    const device = deviceList.value?.find(device => device.machine_id === machineId);
    if (device) Object.assign(device, changes);
};
const handleAttributesSaved = async () => {
    attributesTarget.value = null;
    await requestInFlight?.catch(() => {});
    if (disposed) return;
    toast.add({ severity: 'success', summary: t('web.device.attributes_saved'), life: 2000 });
    await loadDevices();
};

const confirmDeleteDevice = async () => {
    if (!deleteTarget.value) return;
    deleting.value = true;
    try {
        await api?.delete_machine(deleteTarget.value.machine_id, deleteAlsoBlock.value);
        deleteVisible.value = false;
        await loadDevices();
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
        .filter(device => `${device.alias ?? ''} ${device.hostname ?? ''} ${device.public_ip ?? ''}`.toLocaleLowerCase().includes(query))
        .sort((a, b) => {
            const online = Number(b.online === true) - Number(a.online === true);
            if (online !== 0) return online;
            return (deviceName(a) ?? '').localeCompare(deviceName(b) ?? '');
        });
});

const locationText = (device: Utils.DeviceInfo) => {
    const parts = [device.location?.country, device.location?.region, device.location?.city].filter(Boolean);
    return parts.length ? parts.join(' · ') : t('web.device.unknown_location');
};
</script>

<template>
    <div class="console-page">
        <header class="page-heading">
            <div>
                <h1>{{ t('web.device.list') }}</h1>
            </div>
        </header>
        <section class="console-panel">
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
                <h2>{{ search ? t('web.console.no_results') : t('web.console.devices_empty') }}</h2>
                <p v-if="!search">{{ t('web.console.devices_empty_hint') }}</p>
                <div v-if="!search && enrollCommand" class="flex items-center gap-2 justify-center mt-2 flex-wrap">
                    <code class="mono-value p-2 rounded" style="background: var(--console-ground); word-break: break-all">{{ enrollCommand }}</code>
                    <Button icon="pi pi-copy" severity="secondary" outlined size="small"
                        :aria-label="t('web.console.enroll_copy')" @click="copyEnrollCommand" />
                </div>
            </div>
            <template v-else>
                <p v-if="assetCountsFailed" class="muted" role="status">{{ t('assets.page.count_failed') }}</p>
                <DataTable v-if="viewMode === 'table'" :value="sortedDeviceList" class="console-table desktop-list" size="small"
                    v-model:expandedRows="expandedRows" dataKey="machine_id" scrollable
                    :paginator="sortedDeviceList.length > 20" :rows="20">
                    <Column expander style="width: 2.5rem" />
                    <Column field="hostname" :header="t('web.device.hostname')" sortable>
                        <template #body="{ data }">
                            <button class="entity-link" @click="handleDeviceManagement(data)">
                                <span class="status-dot" :class="data.online ? 'online' : 'offline'"></span>
                                <span class="min-w-0">
                                    <span class="preview-name">{{ deviceName(data) }}</span>
                                    <span v-if="data.alias && data.hostname" class="preview-secondary mono-value">{{ data.hostname }}</span>
                                </span>
                            </button>
                        </template>
                    </Column>
                    <Column :header="t('web.console.location')">
                        <template #body="{ data }">{{ locationText(data) }}</template>
                    </Column>
                    <Column v-if="assetContext?.available" :header="t('assets.page.devices')" style="width: 7rem">
                        <template #body="{ data }"><Button :label="assetCountLabel(data.machine_id)" icon="pi pi-desktop" text size="small" @click="openAssets(data.machine_id)" /></template>
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
                                    severity="secondary" text rounded :aria-label="t('web.device.edit_attributes')"
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
                                    <span class="preview-name">{{ deviceName(device) }}</span>
                                    <span v-if="device.alias && device.hostname" class="preview-secondary mono-value">{{ device.hostname }}</span>
                                </span>
                            </button>
                            <span class="device-version-badge" :title="device.easytier_version">v{{ (device.easytier_version || '?').split('-')[0] }}</span>
                        </div>
                        <div class="device-card-actions">
                            <p class="device-card-location" :title="locationText(device)">
                                <i class="pi pi-map-marker" aria-hidden="true"></i>
                                <span>{{ locationText(device) }}</span>
                            </p>
                            <div class="device-card-controls">
                                <Button v-if="centralEnabled" v-tooltip.top="t('web.console.delete_device')" icon="pi pi-trash" severity="danger"
                                    text rounded class="w-8 h-8" :aria-label="t('web.console.delete_device')" @click="handleDeleteDevice(device)" />
                                <Button v-if="assetContext?.available" :label="assetCountLabel(device.machine_id)" icon="pi pi-desktop"
                                    text size="small" @click="openAssets(device.machine_id)" />
                                <span class="device-network-count"
                                    :title="`${t('web.device.networks')}: ${device.running_network_count}`">{{ device.running_network_count }}</span>
                                <Button icon="pi pi-cog" severity="secondary" text rounded class="w-8 h-8"
                                    :aria-label="`${t('web.console.manage')} ${device.hostname}`"
                                    @click="handleDeviceManagement(device)" />
                                <Button v-if="centralEnabled" v-tooltip.top="t('web.device.edit_attributes')" icon="pi pi-file-edit"
                                    severity="secondary" text rounded class="w-8 h-8" :aria-label="t('web.device.edit_attributes')"
                                    @click="handleEditAttributes(device)" />
                            </div>
                        </div>
                        <DeviceDetails :device="device" :card-view="true" />
                        <dl v-if="device.site_location || device.device_model || device.geo_location" class="device-attributes-summary">
                            <div v-if="device.site_location"><dt>{{ t('web.device.site_location') }}</dt><dd :title="device.site_location">{{ device.site_location }}</dd></div>
                            <div v-if="device.device_model"><dt>{{ t('web.device.device_model') }}</dt><dd :title="device.device_model">{{ device.device_model }}</dd></div>
                            <div v-if="nodeLocationLabel(device.geo_location)"><dt>{{ t('web.node_location.title') }}</dt><dd :title="nodeLocationLabel(device.geo_location)">{{ nodeLocationLabel(device.geo_location) }}</dd></div>
                            <div v-if="nodeLocationCoordinates(device.geo_location)"><dt>{{ t('web.node_location.coordinates') }}</dt><dd :title="nodeLocationCoordinates(device.geo_location)">{{ nodeLocationCoordinates(device.geo_location) }}</dd></div>
                        </dl>
                    </article>
                </div>
                <div class="mobile-list">
                    <article v-for="device in sortedDeviceList" :key="device.machine_id" class="mobile-list-item">
                        <div class="flex items-center justify-between gap-3">
                            <button class="entity-link" @click="handleDeviceManagement(device)">
                                <span class="status-dot" :class="device.online ? 'online' : 'offline'"></span>
                                <span class="preview-name">{{ deviceName(device) }}</span>
                            </button>
                            <span class="mono-value muted">v{{ (device.easytier_version || '?').split('-')[0] }}</span>
                        </div>
                        <p class="preview-secondary">{{ locationText(device) }} · {{ device.running_network_count }} {{ t('web.console.instances') }}</p>
                        <Button v-if="assetContext?.available" :label="assetCountLabel(device.machine_id)" icon="pi pi-desktop"
                            text size="small" @click="openAssets(device.machine_id)" />
                        <details>
                            <summary>{{ t('web.console.details') }}</summary>
                            <DeviceDetails :device="device" :compact="true" />
                        </details>
                        <Button v-if="centralEnabled" class="attributes-edit" :label="t('web.device.edit_attributes')"
                            icon="pi pi-file-edit" text size="small" severity="secondary" @click="handleEditAttributes(device)" />
                    </article>
                </div>
            </template>
        </section>

        <DeviceAttributesDialog v-if="attributesTarget" :device="attributesTarget" :api="api"
            @close="attributesTarget = null" @updated="handleAttributesUpdated" @saved="handleAttributesSaved" />

        <!-- 删除确认对话框 -->
        <Dialog v-model:visible="deleteVisible" modal :header="t('web.console.delete_device')" :style="{ width: '26rem' }">
            <div class="flex flex-col gap-3">
                <div>{{ t('web.console.delete_device_confirm', { device: deleteTarget?.hostname || deleteTarget?.machine_id }) }}</div>
                <div class="flex items-center gap-2">
                    <Checkbox inputId="delete-also-block" v-model="deleteAlsoBlock" :binary="true" />
                    <label for="delete-also-block">{{ t('web.console.block_device') }}</label>
                </div>
                <div class="text-xs text-500">{{ deleteAlsoBlock ? t('web.console.block_device_hint') : t('web.console.no_block_device_hint') }}</div>
            </div>
            <template #footer>
                <Button :label="t('web.common.cancel')" icon="pi pi-times" text severity="secondary" @click="deleteVisible = false" />
                <Button :label="t('web.common.confirm')" icon="pi pi-check" :loading="deleting" severity="danger" @click="confirmDeleteDevice" />
            </template>
        </Dialog>

        <Drawer v-model:visible="deviceManageVisible" position="right" class="console-device-drawer" :baseZIndex="1000">
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
.device-card-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 16px;
    padding: 18px;
}

.device-card {
    background: var(--console-panel);
    border: 1px solid var(--console-border);
    border-radius: 8px;
    padding: 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
}

.device-card.offline {
    opacity: .72;
}

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
    background-color: #3b82f6;
    color: #fff;
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
    background-color: #dbeafe;
    color: #1e40af;
    font-size: 0.75rem;
    font-weight: 500;
}

.device-attributes-summary { margin: 0; padding-top: 10px; border-top: 1px solid var(--console-border); font-size: 0.875rem; }
.device-attributes-summary > div { display: flex; gap: 0.5rem; min-width: 0; }
.device-attributes-summary dt { color: var(--console-muted, #64748b); flex-shrink: 0; }
.device-attributes-summary dd { margin: 0; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.attributes-edit { align-self: flex-start; }
</style>
