<script setup lang="ts">
import { computed } from 'vue';
import { Button, Column, DataTable, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useAssetBindingLabels } from '../../composables/useAssetBindingLabels';
import type { AssetDevice, AssetDeviceRow, DeviceType, GatewayGroup } from '../../types/node-assets';

const props = defineProps<{
    rows: AssetDeviceRow[];
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
    edit: [device: AssetDeviceRow];
    delete: [device: AssetDeviceRow];
    'add-service': [device: AssetDevice];
    binding: [device: AssetDeviceRow];
    address: [device: AssetDevice];
}>();
defineSlots<{ services(props: { device: AssetDevice }): unknown }>();
const { t, locale } = useI18n();
const { bindingReason } = useAssetBindingLabels();

function isOrdinary(device: AssetDeviceRow): device is AssetDevice & { resource_kind: 'device' } {
    return device.resource_kind === 'device';
}
const expandedRows = computed(() => Object.fromEntries(props.rows
    .filter(isOrdinary).filter(device => props.expandedIds.includes(device.id)).map(device => [device.id, true])));
const groupNames = computed(() => new Map(props.groups.map(group => [group.id, `${group.name} · ${group.id.slice(0, 8)}`])));
const groupDetails = computed(() => new Map(props.groups.map(group => [group.id,
    [group.binding.snapshot?.display_name, group.binding.instance_id?.slice(0, 8), group.subnets.join(', ')].filter(Boolean).join(' · ')])));
const typeNames = computed(() => new Map(props.types.map(type => [type.id, `${type.name} · ${type.id.slice(0, 8)}`])));
const first = computed(() => Math.max(0, props.page - 1) * props.pageSize);
const groupName = (device: AssetDeviceRow) => isOrdinary(device) && device.gateway_group_id
    ? groupNames.value.get(device.gateway_group_id) || device.gateway_group_id
    : t('assets.deviceDirectNode');
const typeName = (device: AssetDeviceRow) => !isOrdinary(device) ? '—' : device.device_type_id
    ? typeNames.value.get(device.device_type_id) || device.device_type_id : t('assets.deviceUnclassified');
const lanWarnings = (device: AssetDeviceRow) => isOrdinary(device)
    ? device.warnings.filter(warning => warning.field === 'lan_ip') : [];
const bindingSeverity = (device: AssetDeviceRow): 'success' | 'danger' | 'warn' | 'info' | 'secondary' => {
    switch (device.binding.state) {
        case 'current': return 'success';
        case 'missing': return 'danger';
        case 'changed': case 'replaced': return 'warn';
        case 'unavailable': return 'info';
        default: return 'secondary';
    }
};
const bindingAction = (device: AssetDeviceRow) => {
    if (!isOrdinary(device)) {
        if (device.binding.state === 'legacy_unbound') return 'assets.binding_page.attach';
        return device.binding.can_refresh ? 'assets.binding_page.review' : 'assets.binding_page.view';
    }
    if (device.gateway_group_id) {
        if (device.address_mode === 'legacy_manual') return 'assets.binding_page.attach_address';
        return device.binding.can_edit_address ? 'assets.binding_page.review_address' : 'assets.binding_page.view_address';
    }
    if (device.address_mode === 'legacy_manual') return 'assets.binding_page.attach';
    return device.binding.can_refresh || device.binding.can_edit_address
        ? 'assets.binding_page.review' : 'assets.binding_page.view';
};
const ruleSummaries = (device: AssetDeviceRow) => {
    if (!isOrdinary(device)) {
        return device.binding.snapshot?.rules.map(rule => rule.mapped_cidr
            ? `${rule.real_cidr} → ${rule.mapped_cidr}` : rule.real_cidr) ?? device.subnets;
    }
    const rule = device.binding.snapshot?.rules.find(rule => rule.rule_key === device.rule_key);
    return rule ? [`${rule.real_cidr} → ${rule.mapped_cidr || rule.real_cidr}`] : [];
};
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
            @page="event => emit('page', event.page + 1)">
            <template #empty>
                <div class="device-empty">{{ t('assets.deviceEmpty') }}</div>
            </template>
            <Column style="width: 3rem">
                <template #body="{ data }">
                    <Button v-if="isOrdinary(data)" :icon="expandedIds.includes(data.id) ? 'pi pi-chevron-down' : 'pi pi-chevron-right'"
                        text rounded size="small" severity="secondary" :aria-expanded="expandedIds.includes(data.id)"
                        :aria-label="t('assets.services.panelFor', { name: data.name })" :aria-controls="`asset-services-${data.id}`"
                        @click="emit('expand', { id: data.id, expanded: !expandedIds.includes(data.id) })" />
                </template>
            </Column>
            <Column field="name" :header="t('assets.deviceName')" style="min-width: 11rem">
                <template #body="{ data }">
                    <div class="device-identity" :data-asset-device-id="data.id">
                        <span class="device-name">{{ data.name }}</span>
                        <Tag v-if="isOrdinary(data) && data.is_gateway_self" severity="info" :value="t('assets.deviceGatewaySelf')" />
                        <span class="device-record-id" :title="data.id">{{ data.id.slice(0, 8) }}</span>
                    </div>
                </template>
            </Column>
            <Column :header="t('assets.page.device_role')" style="min-width: 8rem">
                <template #body="{ data }"><Tag :severity="isOrdinary(data) ? 'secondary' : 'info'"
                    :value="t(isOrdinary(data) ? 'assets.binding.ordinary' : 'assets.binding.gateway')" /></template>
            </Column>
            <Column field="model" :header="t('assets.deviceModel')" style="min-width: 9rem">
                <template #body="{ data }"><span class="wrap-value">{{ isOrdinary(data) ? data.model || '—' : '—' }}</span></template>
            </Column>
            <Column :header="t('assets.deviceType')" style="min-width: 8rem">
                <template #body="{ data }"><span class="wrap-value">{{ typeName(data) }}</span></template>
            </Column>
            <Column :header="t('assets.deviceGroup')" style="min-width: 9rem">
                <template #body="{ data }"><span class="wrap-value">{{ groupName(data) }}</span>
                    <small v-if="isOrdinary(data) && data.gateway_group_id" class="group-detail wrap-value">{{ groupDetails.get(data.gateway_group_id) }}</small>
                </template>
            </Column>
            <Column :header="t('assets.binding_page.source')" style="min-width: 15rem">
                <template #body="{ data }">
                    <div class="binding-summary">
                        <Tag :severity="bindingSeverity(data)" :value="t(`assets.binding_page.states.${data.binding.state}`)" :title="bindingReason(data.binding.reason) || undefined" />
                        <span v-if="data.binding.snapshot" class="wrap-value">{{ data.binding.snapshot.display_name }}</span>
                        <code v-if="data.binding.instance_id" class="device-record-id" :title="data.binding.instance_id">{{ data.binding.instance_id.slice(0, 8) }}</code>
                        <small v-if="isOrdinary(data)" class="wrap-value">{{ t(`assets.binding_page.modes.${data.address_mode}`) }}</small>
                        <small v-for="rule in ruleSummaries(data)" :key="rule" class="wrap-value">{{ rule }}</small>
                        <small v-if="data.binding.state !== 'current' && data.binding.state !== 'legacy_unbound'" class="binding-note wrap-value">{{ t('assets.binding_page.saved_snapshot') }}</small>
                    </div>
                </template>
            </Column>
            <Column field="overlay_ip" :header="t('assets.deviceOverlayIp')" style="min-width: 11rem">
                <template #body="{ data }"><code class="ip-value">{{ data.overlay_ip || '—' }}</code></template>
            </Column>
            <Column field="lan_ip" :header="t('assets.deviceLanIp')" style="min-width: 11rem">
                <template #body="{ data }">
                    <code v-if="isOrdinary(data) && data.lan_ip" class="ip-value">{{ data.lan_ip }}</code>
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
            <Column :header="t('assets.deviceActions')" frozen alignFrozen="right" headerClass="device-action-column" bodyClass="device-action-column" style="width: 9rem; min-width: 9rem; max-width: 9rem">
                <template #body="{ data }">
                    <div class="device-actions">
                        <Button v-if="isOrdinary(data)" icon="pi pi-plus" text rounded size="small"
                            :aria-label="t('assets.deviceAddServiceFor', { name: data.name })"
                            :title="t('assets.deviceAddService')"
                            @click="emit('add-service', data)" />
                        <Button icon="pi pi-pencil" text rounded severity="secondary"
                            :aria-label="t('assets.deviceEditNamed', { name: data.name })"
                            :title="t('assets.deviceEdit')" @click="emit('edit', data)" />
                        <Button icon="pi pi-trash" text rounded severity="danger"
                            :aria-label="t('assets.deviceDeleteNamed', { name: data.name })"
                            :title="t('assets.deviceDelete')" @click="emit('delete', data)" />
                    </div>
                    <div class="device-binding-actions">
                        <Button :label="t(bindingAction(data))" icon="pi pi-link" text size="small"
                            :aria-label="t('assets.binding_page.review_named', { name: data.name })" @click="emit('binding', data)" />
                        <Button v-if="isOrdinary(data) && !data.gateway_group_id && data.address_mode !== 'legacy_manual' && data.binding.can_edit_address"
                            icon="pi pi-map-marker" text rounded size="small" severity="secondary"
                            :title="t('assets.binding_page.edit_address')" :aria-label="t('assets.binding_page.edit_address_named', { name: data.name })"
                            @click="emit('address', data)" />
                    </div>
                </template>
            </Column>
            <template #expansion="{ data }">
                <div v-if="isOrdinary(data)" :id="`asset-services-${data.id}`" class="device-services"><slot name="services" :device="data" /></div>
            </template>
        </DataTable>
    </section>
</template>

<style scoped>
.asset-device-table,.asset-table { min-width: 0; width: 100%; max-width: 100%; }
.asset-device-table { container-type: inline-size; }
.asset-table :deep(.p-datatable-table-container) { min-width: 0; max-width: 100%; overflow-x: auto; }
.asset-table :deep(.device-action-column) { background: var(--p-datatable-row-background, var(--p-content-background, #fff)); box-shadow: -4px 0 5px -4px var(--p-content-border-color, #94a3b8); }
.binding-summary { display: flex; align-items: flex-start; flex-direction: column; gap: .3rem; max-width: 20rem; }
.binding-note,.group-detail { color: var(--console-muted, #64748b); }
.group-detail { margin-top: .35rem; }
.device-record-id { color: var(--console-muted, #64748b); font-size: 0.8125rem; }
.device-identity { display: flex; align-items: flex-start; flex-direction: column; gap: 0.35rem; }
.device-name { font-weight: 600; overflow-wrap: anywhere; }
.wrap-value { display: block; max-width: 18rem; overflow-wrap: anywhere; }
.device-record-id, .ip-value { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
.ip-value { display: block; max-width: 18rem; overflow-wrap: anywhere; font-size: 0.8125rem; background: transparent; }
.lan-warning { display: flex; align-items: flex-start; gap: 0.3rem; margin-top: 0.35rem; font-size: 0.75rem; color: var(--p-orange-700, #9a3412); }
.lan-warning .pi { margin-top: 0.125rem; }
.device-remarks { white-space: pre-wrap; font-size: 0.8125rem; }
.device-actions,.device-binding-actions { display: flex; align-items: center; gap: 0.125rem; }
.device-binding-actions { flex-wrap: wrap; }
.device-actions :deep(.p-button) { flex-shrink: 0; }
.device-services { position: sticky; left: 0; width: calc(100cqw - 1rem); max-width: 100%; box-sizing: border-box; padding: 0.75rem; border-left: 3px solid var(--p-primary-color, #3b82f6); background: var(--console-ground, var(--p-content-background)); }
.device-empty { padding: 1.5rem; text-align: center; color: var(--console-muted, #64748b); }
time { font-size: 0.8125rem; white-space: nowrap; }
@media (max-width: 640px) {
    .device-services { padding: 0.5rem; }
}
</style>
