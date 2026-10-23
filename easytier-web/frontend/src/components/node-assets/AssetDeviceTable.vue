<script setup lang="ts">
import { computed } from 'vue';
import { Button, Column, DataTable, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import type { AssetDevice, DeviceType, GatewayGroup } from '../../types/node-assets';

const props = defineProps<{
    rows: AssetDevice[];
    loading: boolean;
    total: number;
    page: number;
    pageSize: number;
    expandedIds: string[];
    groups: GatewayGroup[];
    types: DeviceType[];
}>();
const emit = defineEmits<{
    page: [page: number];
    expand: [event: { id: string; expanded: boolean }];
    edit: [device: AssetDevice];
    delete: [device: AssetDevice];
    'add-service': [device: AssetDevice];
}>();
defineSlots<{ services(props: { device: AssetDevice }): unknown }>();
const { t, locale } = useI18n();

const expandedRows = computed(() => Object.fromEntries(props.expandedIds.map(id => [id, true])));
const groupNames = computed(() => new Map(props.groups.map(group => [group.id, group.name])));
const typeNames = computed(() => new Map(props.types.map(type => [type.id, type.name])));
const first = computed(() => Math.max(0, props.page - 1) * props.pageSize);
const groupName = (device: AssetDevice) => device.gateway_group_id
    ? groupNames.value.get(device.gateway_group_id) || device.gateway_group_id
    : t('assets.deviceDirectNode');
const typeName = (device: AssetDevice) => device.device_type_id
    ? typeNames.value.get(device.device_type_id) || device.device_type_id
    : t('assets.deviceUnclassified');
const lanWarnings = (device: AssetDevice) => device.warnings.filter(warning => warning.field === 'lan_ip');
const createdAt = (value: string) => {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return new Intl.DateTimeFormat(locale.value === 'cn' ? 'zh-CN' : 'en-US', {
        year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit',
    }).format(date);
};
</script>

<template>
    <section class="asset-device-table" :aria-label="t('assets.deviceList')">
        <DataTable :value="rows" :loading="loading" dataKey="id" :expandedRows="expandedRows"
            :paginator="total > pageSize" :rows="pageSize" :first="first" :totalRecords="total" lazy
            scrollable size="small" class="console-table asset-table" :tableStyle="{ minWidth: '65rem' }"
            @page="event => emit('page', event.page + 1)"
            @row-expand="event => emit('expand', { id: event.data.id, expanded: true })"
            @row-collapse="event => emit('expand', { id: event.data.id, expanded: false })">
            <template #empty>
                <div class="device-empty">{{ t('assets.deviceEmpty') }}</div>
            </template>
            <Column expander style="width: 3rem" />
            <Column field="name" :header="t('assets.deviceName')" style="min-width: 11rem">
                <template #body="{ data }">
                    <div class="device-identity" :data-asset-device-id="data.id">
                        <span class="device-name">{{ data.name }}</span>
                        <Tag v-if="data.is_gateway_self" severity="info" :value="t('assets.deviceGatewaySelf')" />
                        <span class="device-record-id" :title="data.id">{{ data.id.slice(0, 8) }}</span>
                    </div>
                </template>
            </Column>
            <Column field="model" :header="t('assets.deviceModel')" style="min-width: 9rem">
                <template #body="{ data }"><span class="wrap-value">{{ data.model || '—' }}</span></template>
            </Column>
            <Column :header="t('assets.deviceType')" style="min-width: 8rem">
                <template #body="{ data }"><span class="wrap-value">{{ typeName(data) }}</span></template>
            </Column>
            <Column :header="t('assets.deviceGroup')" style="min-width: 9rem">
                <template #body="{ data }"><span class="wrap-value">{{ groupName(data) }}</span></template>
            </Column>
            <Column field="overlay_ip" :header="t('assets.deviceOverlayIp')" style="min-width: 11rem">
                <template #body="{ data }"><code class="ip-value">{{ data.overlay_ip }}</code></template>
            </Column>
            <Column field="lan_ip" :header="t('assets.deviceLanIp')" style="min-width: 11rem">
                <template #body="{ data }">
                    <code v-if="data.lan_ip" class="ip-value">{{ data.lan_ip }}</code>
                    <span v-else>—</span>
                    <span v-if="lanWarnings(data).length" class="lan-warning" tabindex="0"
                        :title="lanWarnings(data).map(warning => warning.message).join('\n')">
                        <i class="pi pi-exclamation-triangle" aria-hidden="true"></i>
                        {{ t('assets.deviceLanWarning') }}
                    </span>
                </template>
            </Column>
            <Column :header="t('assets.deviceRemarks')" style="min-width: 10rem">
                <template #body="{ data }"><span class="wrap-value device-remarks">{{ data.remarks || '—' }}</span></template>
            </Column>
            <Column :header="t('assets.deviceCreatedAt')" style="min-width: 10rem">
                <template #body="{ data }"><time :datetime="data.created_at">{{ createdAt(data.created_at) }}</time></template>
            </Column>
            <Column :header="t('assets.deviceActions')" style="min-width: 13rem">
                <template #body="{ data }">
                    <div class="device-actions">
                        <Button :label="t('assets.deviceAddService')" icon="pi pi-plus" text size="small"
                            :aria-label="t('assets.deviceAddServiceFor', { name: data.name })"
                            @click="emit('add-service', data)" />
                        <Button icon="pi pi-pencil" text rounded severity="secondary"
                            :aria-label="t('assets.deviceEditNamed', { name: data.name })"
                            :title="t('assets.deviceEdit')" @click="emit('edit', data)" />
                        <Button icon="pi pi-trash" text rounded severity="danger"
                            :aria-label="t('assets.deviceDeleteNamed', { name: data.name })"
                            :title="t('assets.deviceDelete')" @click="emit('delete', data)" />
                    </div>
                </template>
            </Column>
            <template #expansion="{ data }">
                <div class="device-services"><slot name="services" :device="data" /></div>
            </template>
        </DataTable>
    </section>
</template>

<style scoped>
.asset-device-table,.asset-table { min-width: 0; width: 100%; max-width: 100%; }
.asset-table :deep(.p-datatable-table-container) { min-width: 0; max-width: 100%; overflow-x: auto; }
.device-record-id { color: var(--console-muted, #64748b); font-size: 0.8125rem; }
.device-identity { display: flex; align-items: flex-start; flex-direction: column; gap: 0.35rem; }
.device-name { font-weight: 600; overflow-wrap: anywhere; }
.wrap-value { display: block; max-width: 18rem; overflow-wrap: anywhere; }
.device-record-id, .ip-value { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.ip-value { display: block; max-width: 18rem; overflow-wrap: anywhere; font-size: 0.8125rem; background: transparent; }
.lan-warning { display: flex; align-items: flex-start; gap: 0.3rem; margin-top: 0.35rem; font-size: 0.75rem; color: var(--p-orange-700, #9a3412); }
.lan-warning .pi { margin-top: 0.125rem; }
.device-remarks { white-space: pre-wrap; font-size: 0.8125rem; }
.device-actions { display: flex; align-items: center; gap: 0.125rem; flex-wrap: wrap; }
.device-services { padding: 0.75rem; border-left: 3px solid var(--p-primary-color, #3b82f6); background: var(--console-ground, var(--p-content-background)); }
.device-empty { padding: 1.5rem; text-align: center; color: var(--console-muted, #64748b); }
time { font-size: 0.8125rem; white-space: nowrap; }
@media (max-width: 640px) {
    .device-services { padding: 0.5rem; }
}
</style>
