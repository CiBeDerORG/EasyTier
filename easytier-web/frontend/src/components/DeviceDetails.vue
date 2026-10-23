<script setup lang="ts">
import { Utils } from 'easytier-frontend-lib';
import { useI18n } from 'vue-i18n'
import { nodeLocationCoordinates, nodeLocationLabel } from '../modules/node-location';

const { t } = useI18n()


// 定义组件接收的 props
withDefaults(defineProps<{
  device: Utils.DeviceInfo;
  // 可以传入额外的样式类
  containerClass?: string;
  // 是否使用紧凑布局
  compact?: boolean;
  // 卡片恢复常显的六项设备信息，其他详情入口保持原样
  cardView?: boolean;
}>(), { cardView: false });

</script>

<template>
  <div :class="['device-details', containerClass, { 'compact': compact, 'card-details': cardView }]">
    <template v-if="cardView">
      <div class="detail-item hostname">
        <div class="detail-label">{{ t('web.device.hostname') }}</div>
        <div class="detail-value">{{ device.hostname || '—' }}</div>
      </div>
      <div class="detail-item public-ip">
        <div class="detail-label">{{ t('web.device.public_ip') }}</div>
        <div class="detail-value">{{ device.public_ip || '—' }}</div>
      </div>
      <div class="detail-item running-networks">
        <div class="detail-label">{{ t('web.device.networks') }}</div>
        <div class="detail-value">{{ device.running_network_count }}</div>
      </div>
      <div class="detail-item last-seen">
        <div class="detail-label">{{ t('web.device.last_seen') }}</div>
        <div class="detail-value">{{ device.last_seen || device.report_time || '—' }}</div>
      </div>
      <div class="detail-item version">
        <div class="detail-label">{{ t('web.device.version') }}</div>
        <div class="detail-value">{{ device.easytier_version || '—' }}</div>
      </div>
      <div class="detail-item machine-id">
        <div class="detail-label">{{ t('web.device.machine_id') }}</div>
        <div class="detail-value">{{ device.machine_id || '—' }}</div>
      </div>
    </template>
    <template v-else>
    <div class="detail-item hostname">
      <div class="detail-label"><i class="pi pi-desktop" aria-hidden="true"></i>{{ t('web.device.hostname') }}</div>
      <div class="detail-value">{{ device.hostname || '—' }}</div>
    </div>
    <div class="detail-item status">
      <div class="detail-label"><i class="pi pi-wave-pulse" aria-hidden="true"></i>{{ t('web.device.status') }}</div>
      <div class="detail-value"><span class="device-presence" :class="{ online: device.online }"><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ device.online ? t('web.device.online') : t('web.device.offline') }}</span></div>
    </div>
    <div v-if="!device.online && device.last_seen" class="detail-item last-seen">
      <div class="detail-label"><i class="pi pi-clock" aria-hidden="true"></i>{{ t('web.device.last_seen') }}</div>
      <div class="detail-value">{{ device.last_seen }}</div>
    </div>
    <div class="detail-item public-ip">
      <div class="detail-label"><i class="pi pi-globe" aria-hidden="true"></i>{{ t('web.device.public_ip') }}</div>
      <div class="detail-value mono-value">{{ device.public_ip || '—' }}</div>
    </div>
    <div class="detail-item running-networks">
      <div class="detail-label"><i class="pi pi-share-alt" aria-hidden="true"></i>{{ t('web.device.networks') }}</div>
      <div class="detail-value detail-number">{{ device.running_network_count }}</div>
    </div>
    <div class="detail-item location">
      <div class="detail-label"><i class="pi pi-map-marker" aria-hidden="true"></i>{{ t('web.device.ip_location') }}</div>
      <div class="detail-value">{{ device.location ? [device.location.country, device.location.region, device.location.city].filter(Boolean).join(' · ') : t('web.device.unknown_location') }}</div>
    </div>
    <div v-if="device.device_model" class="detail-item device-model">
      <div class="detail-label"><i class="pi pi-microchip" aria-hidden="true"></i>{{ t('web.device.device_model') }}</div>
      <div class="detail-value">{{ device.device_model }}</div>
    </div>
    <div v-if="nodeLocationLabel(device.geo_location)" class="detail-item geo-location">
      <div class="detail-label"><i class="pi pi-map" aria-hidden="true"></i>{{ t('web.node_location.title') }}</div>
      <div class="detail-value">{{ nodeLocationLabel(device.geo_location) }}</div>
    </div>
    <div v-if="nodeLocationCoordinates(device.geo_location)" class="detail-item geo-coordinates">
      <div class="detail-label"><i class="pi pi-compass" aria-hidden="true"></i>{{ t('web.node_location.coordinates') }}</div>
      <div class="detail-value mono-value">{{ nodeLocationCoordinates(device.geo_location) }}</div>
    </div>
    <div v-if="(device.networks?.length ?? 0) > 0" class="detail-item central-networks">
      <div class="detail-label"><i class="pi pi-sitemap" aria-hidden="true"></i>{{ t('web.device.central_networks') }}</div>
      <div class="detail-value">
        <span v-for="network in device.networks" :key="network.network_id" class="central-network-name"
          :title="network.network_name">{{ network.display_name }}</span>
      </div>
    </div>
    <div class="detail-item version">
      <div class="detail-label"><i class="pi pi-code" aria-hidden="true"></i>{{ t('web.device.version') }}</div>
      <div class="detail-value mono-value">{{ device.easytier_version || '—' }}</div>
    </div>
    <details class="more-details">
      <summary>{{ t('web.console.more_details') }}<i class="pi pi-chevron-down" aria-hidden="true"></i></summary>
      <div class="detail-item last-report">
        <div class="detail-label">{{ t('web.device.last_report') }}</div>
        <div class="detail-value">{{ device.report_time }}</div>
      </div>
      <div class="detail-item machine-id">
        <div class="detail-label">{{ t('web.device.machine_id') }}</div>
        <div class="detail-value">
          <span class="machine-id-value" :title="device.machine_id">{{ device.machine_id }}</span>
        </div>
      </div>
    </details>
    </template>
  </div>
</template>

<style scoped>
.device-details { gap: 12px; }
.detail-item { min-width: 0; padding: 14px 16px; border: 1px solid var(--console-border); border-radius: 9px; background: var(--console-panel-raised, var(--console-panel)); }
.detail-label { display: flex; align-items: center; gap: 8px; color: var(--console-muted); font-size: 11px; font-weight: 500; margin-bottom: 9px; }
.detail-label > .pi { color: var(--p-primary-color); font-size: 11px; opacity: .85; }
.detail-value { color: var(--p-text-color); font-size: 13px; line-height: 1.5; overflow-wrap: anywhere; }
.detail-value.detail-number { font-weight: 650; font-variant-numeric: tabular-nums; }
.device-presence { display: inline-flex; align-items: center; gap: 7px; color: var(--console-muted); font-size: 12px; }
.device-presence > .pi { font-size: 6px; }
.device-presence.online { color: var(--p-green-700); }
html.app-dark .device-presence.online { color: var(--p-green-300); }
.more-details { grid-column: 1 / -1; border-top: 1px solid var(--console-border); padding-top: 6px; }
.more-details summary { display: flex; align-items: center; justify-content: space-between; gap: 12px; list-style: none; cursor: pointer; color: var(--console-muted); font-size: 12px; padding: 10px 2px; }
.more-details summary::-webkit-details-marker { display: none; }
.more-details summary:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; border-radius: 4px; }
.more-details summary > .pi { font-size: 10px; transition: transform .18s; }
.more-details[open] summary > .pi { transform: rotate(180deg); }
.more-details .detail-item { display: grid; grid-template-columns: minmax(110px, .5fr) minmax(0, 1fr); align-items: center; gap: 12px; background: transparent; padding: 10px 0; border: 0; border-radius: 0; }
.more-details .detail-label { margin-bottom: 0; }
.central-network-name { display: inline-flex; background: var(--console-accent-soft, var(--console-ground)); border: 1px solid var(--console-border-strong, var(--console-border)); border-radius: 5px; padding: 3px 8px; margin: 2px 4px 2px 0; max-width: 100%; overflow-wrap: anywhere; font-size: 11px; }
.machine-id-value { display: block; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 11px; color: var(--console-muted); max-width: 100%; overflow-wrap: anywhere; }
.device-details.compact { gap: 0; }
.compact .detail-item { display: grid; grid-template-columns: minmax(100px, .7fr) minmax(0, 1fr); align-items: center; gap: 10px; padding: 10px 0; border: 0; border-bottom: 1px solid var(--console-border); border-radius: 0; background: transparent; }
.compact .detail-label { margin-bottom: 0; }
.compact .detail-value { padding: 0; font-size: 12px; }
.compact .more-details { border-top: 0; }
@media (max-width: 767px) {
    .detail-item { padding: 12px 14px; }
    .more-details .detail-item { grid-template-columns: minmax(90px, .55fr) minmax(0, 1fr); }
}
@media (prefers-reduced-motion: reduce) { .more-details summary > .pi { transition: none; } }

.device-details.card-details {
  display: block;
  min-width: 0;
}

.card-details .detail-item {
  display: grid;
  grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.35fr);
  gap: 0.75rem;
  align-items: start;
  padding: 0.55rem 0;
  border: 0;
  border-top: 1px solid var(--console-border, #e2e8f0);
  border-radius: 0;
  background: transparent;
}

.card-details .detail-item:first-child {
  border-top: 0;
}

.card-details .detail-label {
  margin: 0;
  min-width: 0;
}

.card-details .detail-value {
  min-width: 0;
  padding-left: 0;
  line-height: 1.4;
  white-space: normal;
  word-break: normal;
  overflow-wrap: anywhere;
}

.card-details .machine-id .detail-value {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}
</style>
