<script setup lang="ts">
import { Button, Message, Skeleton } from 'primevue';
import { computed, onMounted, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Utils } from 'easytier-frontend-lib';
import ApiClient, { type Summary, type CentralNetworkSummary } from '../modules/api';
import { overviewMessages } from '../modules/overview-messages';
import networkAtmosphere from '../assets/console/network-atmosphere.webp';

const props = defineProps<{ api: ApiClient; centralEnabled: boolean }>();
const { t, locale } = useI18n();
const copy = computed(() => overviewMessages[locale.value === 'cn' ? 'cn' : 'en']);
const summary = ref<Summary>();
const devices = ref<Utils.DeviceInfo[]>();
const networks = ref<CentralNetworkSummary[]>();
const loadError = ref(false);
const refreshing = ref(false);
let requestInFlight = false;

const loadSummary = async (manual = false) => {
    // A manual refresh can show progress for an already running background request.
    if (manual) refreshing.value = true;
    if (requestInFlight) return;
    requestInFlight = true;
    try {
        const results = await Promise.allSettled([
            props.api.get_summary().then(value => { summary.value = value; }),
            props.api.list_machines().then(value => { devices.value = value.map(Utils.buildDeviceInfo); }),
            ...(props.centralEnabled ? [props.api.list_networks().then(value => { networks.value = value; })] : []),
        ]);
        loadError.value = results.some(result => result.status === 'rejected');
    } finally {
        requestInFlight = false;
        refreshing.value = false;
    }
};
watch(() => props.centralEnabled, () => loadSummary());
const periodFunc = new Utils.PeriodicTask(loadSummary, 2000);
onMounted(() => periodFunc.start());
onUnmounted(() => periodFunc.stop());

const devicePreview = computed(() => [...(devices.value ?? [])]
    .sort((a, b) => ((a.alias || a.hostname) ?? '').localeCompare((b.alias || b.hostname) ?? '')).slice(0, 5));
const networkPreview = computed(() => [...(networks.value ?? [])]
    .sort((a, b) => a.display_name.localeCompare(b.display_name)).slice(0, 5));
const onlineDevices = computed(() => devices.value?.filter(device => device.online).length);
const offlineDevices = computed(() => devices.value ? devices.value.length - (onlineDevices.value ?? 0) : undefined);
const onlineRatio = computed(() => devices.value?.length ? Math.round((onlineDevices.value ?? 0) / devices.value.length * 100) : null);
const stats = computed(() => [
    { label: 'device_count', note: 'device_note', icon: 'pi-server', value: summary.value?.device_count },
    { label: 'online_device_count', note: 'online_device_note', icon: 'pi-check-circle', value: devices.value?.filter(device => device.online).length },
    ...(props.centralEnabled ? [{ label: 'network_count', note: 'network_note', icon: 'pi-globe', value: networks.value?.length }] : []),
    { label: 'instance_count', note: 'instance_note', icon: 'pi-share-alt', value: devices.value?.reduce((sum, device) => sum + device.running_network_count, 0) },
]);
</script>

<template>
    <div class="console-page dashboard-page">
        <header class="page-heading">
            <div><p class="overview-eyebrow">{{ copy.overviewEyebrow }}</p><h1>{{ t('web.main.dashboard') }}</h1><p class="page-description">{{ copy.overviewDescription }}</p></div>
            <Button icon="pi pi-refresh" :label="t('web.console.refresh')" severity="secondary" outlined :loading="refreshing" @click="loadSummary(true)" />
        </header>
        <Message v-if="loadError" severity="warn" :closable="false">{{ t('web.console.load_failed') }}</Message>
        <section class="overview-hero" :aria-label="copy.deviceOverview">
            <img :src="networkAtmosphere" class="overview-hero-art" alt="" aria-hidden="true" />
            <div class="overview-hero-copy">
                <span class="overview-hero-mark"><i class="pi pi-sitemap" aria-hidden="true"></i> EasyTier</span>
                <h2>{{ copy.heroTitle }}</h2>
                <p>{{ centralEnabled ? copy.heroDescription : copy.heroExternalDescription }}</p>
                <div class="overview-hero-actions">
                    <RouterLink v-if="centralEnabled" :to="{ name: 'networkMap' }" class="overview-primary-link"><i class="pi pi-globe" aria-hidden="true"></i>{{ copy.openMap }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink>
                    <RouterLink v-else :to="{ name: 'deviceList' }" class="overview-primary-link"><i class="pi pi-server" aria-hidden="true"></i>{{ copy.openDevices }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></RouterLink>
                    <RouterLink v-if="centralEnabled" :to="{ name: 'serviceNavigation' }" class="overview-secondary-link">{{ copy.openServices }}<i class="pi pi-arrow-right" aria-hidden="true"></i></RouterLink>
                </div>
            </div>
            <div class="overview-presence">
                <div class="overview-presence-title"><i class="pi pi-wave-pulse" aria-hidden="true"></i><span>{{ copy.onlineCoverage }}</span></div>
                <div class="overview-presence-value"><strong>{{ onlineRatio == null ? '—' : onlineRatio }}</strong><span v-if="onlineRatio != null">%</span></div>
                <progress :value="onlineDevices ?? 0" :max="devices?.length || 1" :aria-label="copy.onlineCoverage"></progress>
                <div class="overview-presence-legend"><span><i class="pi pi-circle-fill presence-online" aria-hidden="true"></i>{{ copy.online }} <b>{{ onlineDevices ?? '—' }}</b></span><span><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ copy.offline }} <b>{{ offlineDevices ?? '—' }}</b></span></div>
                <p>{{ loadError ? copy.retainedSample : devices === undefined ? copy.noSample : devices.length ? copy.snapshot : copy.noDevicesYet }}</p>
            </div>
        </section>
        <section class="console-panel summary-strip" :aria-label="t('web.main.dashboard')">
            <div v-for="stat in stats" :key="stat.label" class="summary-stat">
                <div class="summary-label"><span>{{ t(`web.console.${stat.label}`) }}</span><i :class="['pi', stat.icon]" aria-hidden="true"></i></div>
                <div class="summary-value">
                    <Skeleton v-if="stat.value === undefined && !loadError" width="4rem" height="2.8rem" />
                    <template v-else>{{ stat.value ?? '—' }}</template>
                </div>
                <p class="summary-note">{{ t(`web.console.${stat.note}`) }}</p>
            </div>
        </section>
        <div class="overview-columns">
            <section class="console-panel">
                <header class="section-heading"><div class="overview-section-title"><span class="overview-section-icon"><i class="pi pi-server" aria-hidden="true"></i></span><div><h2>{{ t('web.main.device_list') }}</h2><p>{{ copy.devicePreviewHint }}</p></div></div>
                    <RouterLink :to="{ name: 'deviceList' }" class="entity-link text-xs">{{ t('web.console.view_all') }}<i class="pi pi-arrow-right" aria-hidden="true"></i></RouterLink>
                </header>
                <div v-if="devices === undefined && !loadError" class="loading-rows"><Skeleton v-for="i in 3" :key="i" height="2rem" /></div>
                <div v-else-if="devices?.length === 0" class="console-empty-state">
                    <i class="pi pi-server" aria-hidden="true"></i><h2>{{ t('web.console.devices_empty') }}</h2><p>{{ t('web.console.devices_empty_hint') }}</p>
                    <a href="https://easytier.cn" target="_blank" rel="noopener noreferrer" class="entity-link">{{ t('web.console.documentation') }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></a>
                </div>
                <RouterLink v-for="device in devicePreview" :key="device.machine_id" class="preview-row"
                    :to="{ name: 'deviceManagement', params: { deviceId: device.machine_id, instanceId: device.running_network_instances?.[0] } }">
                    <div class="entity-link"><span class="overview-device-icon"><i class="pi pi-server entity-icon" aria-hidden="true"></i><span class="status-dot" :class="device.online ? 'online' : 'offline'" :title="device.online ? copy.online : copy.offline"></span></span><div class="min-w-0"><div class="preview-name">{{ device.alias || device.hostname || device.machine_id }}</div><div class="preview-secondary mono-value">{{ device.public_ip || '—' }}</div></div></div>
                    <div class="preview-number">{{ device.running_network_count }}<div class="preview-secondary">{{ t('web.console.instances') }}</div></div>
                </RouterLink>
            </section>
            <section v-if="centralEnabled" class="console-panel">
                <header class="section-heading"><div class="overview-section-title"><span class="overview-section-icon"><i class="pi pi-globe" aria-hidden="true"></i></span><div><h2>{{ t('web.main.network_list') }}</h2><p>{{ copy.networkPreviewHint }}</p></div></div>
                    <RouterLink :to="{ name: 'networkList' }" class="entity-link text-xs">{{ t('web.console.view_all') }}<i class="pi pi-arrow-right" aria-hidden="true"></i></RouterLink>
                </header>
                <div v-if="networks === undefined && !loadError" class="loading-rows"><Skeleton v-for="i in 3" :key="i" height="2rem" /></div>
                <div v-else-if="networks?.length === 0" class="console-empty-state">
                    <i class="pi pi-globe" aria-hidden="true"></i><h2>{{ t('web.console.networks_empty') }}</h2><p>{{ t('web.console.networks_empty_hint') }}</p>
                    <RouterLink :to="{ name: 'networkList' }" class="entity-link">{{ t('web.network_list.create') }}<i class="pi pi-arrow-right" aria-hidden="true"></i></RouterLink>
                </div>
                <RouterLink v-for="network in networkPreview" :key="network.network_id" class="preview-row" :to="{ name: 'networkDetail', params: { networkId: network.network_id } }">
                    <div class="entity-link"><i class="pi pi-globe entity-icon" aria-hidden="true"></i><div class="min-w-0"><div class="preview-name">{{ network.display_name }}</div><div class="preview-secondary">{{ network.network_name }}</div></div></div>
                    <div class="preview-number overview-network-presence"><span>{{ network.online_member_count }} <span class="muted">/ {{ network.member_count }}</span></span><progress :value="network.online_member_count" :max="network.member_count || 1" :aria-label="t('web.console.online_members')"></progress></div>
                </RouterLink>
            </section>
        </div>
        <p class="page-footnote"><i class="pi pi-sync" aria-hidden="true"></i>{{ t('web.console.automatic_refresh') }}</p>
    </div>
</template>

<style scoped>
.overview-eyebrow { margin: 0 0 8px; color: var(--p-primary-color); font-size: 10px; font-weight: 650; letter-spacing: .14em; }
.overview-hero { display: grid; grid-template-columns: minmax(0, 1fr) 250px; gap: 32px; align-items: center; border: 1px solid color-mix(in srgb, var(--p-primary-color) 30%, var(--console-border)); border-radius: 16px; padding: 32px 36px; background-color: var(--console-panel); box-shadow: inset 0 1px 0 color-mix(in srgb, var(--p-primary-color) 9%, transparent), 0 14px 45px rgb(0 0 0 / 4%); position: relative; overflow: hidden; isolation: isolate; }
.overview-hero { --console-panel: #0b1727; --console-border: #294056; --console-muted: #a8bfd2; --p-primary-color: #67e8f9; --p-primary-hover-color: #a5f3fc; --p-primary-contrast-color: #06252e; --p-text-color: #eff8ff; color: #eff8ff; }
.overview-hero-art { position: absolute; z-index: -1; inset: 0; width: 100%; height: 100%; object-fit: cover; object-position: center; opacity: .67; pointer-events: none; }
.overview-hero-copy { min-width: 0; position: relative; }
.overview-hero-mark { display: inline-flex; gap: 9px; align-items: center; font-size: 12px; font-weight: 600; color: var(--p-primary-color); margin-bottom: 18px; }
.overview-hero-mark .pi { font-size: 16px; }
.overview-hero h2 { font-size: clamp(24px, 2.3vw, 34px); font-weight: 600; line-height: 1.3; letter-spacing: -.04em; }
.overview-hero-copy > p { max-width: 500px; margin-top: 12px; color: var(--console-muted); font-size: 13px; line-height: 1.8; }
.overview-hero-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 18px; margin-top: 26px; }
.overview-primary-link, .overview-secondary-link { display: inline-flex; align-items: center; justify-content: center; gap: 10px; font-size: 12px; font-weight: 600; min-height: 40px; border-radius: 8px; transition: background-color .16s, box-shadow .16s; }
.overview-primary-link { background: var(--p-primary-color); color: var(--p-primary-contrast-color); padding: 10px 15px; border: 1px solid var(--p-primary-color); box-shadow: 0 3px 18px color-mix(in srgb, var(--p-primary-color) 14%, transparent); }
.overview-primary-link:hover { background: var(--p-primary-hover-color); box-shadow: 0 3px 22px color-mix(in srgb, var(--p-primary-color) 24%, transparent); }
.overview-secondary-link { color: var(--p-text-color); padding: 8px 0; }
.overview-secondary-link:hover { color: var(--p-primary-color); }
.overview-primary-link:focus-visible, .overview-secondary-link:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 4px; }
.overview-presence { position: relative; padding: 22px 24px; border: 1px solid var(--console-border); border-radius: 12px; background: color-mix(in srgb, var(--console-panel) 92%, transparent); backdrop-filter: blur(12px); }
.overview-presence-title { display: flex; gap: 9px; align-items: center; color: var(--console-muted); font-size: 12px; }
.overview-presence-title .pi { color: var(--p-primary-color); }
.overview-presence-value { display: flex; align-items: baseline; gap: 6px; margin: 16px 0 12px; }
.overview-presence-value strong { font-size: 48px; font-weight: 550; line-height: 1; letter-spacing: -.05em; font-variant-numeric: tabular-nums; }
.overview-presence-value > span { color: var(--console-muted); font-size: 18px; }
progress { display: block; width: 100%; height: 5px; border: 0; border-radius: 8px; overflow: hidden; appearance: none; background: var(--console-border); }
progress::-webkit-progress-bar { background: var(--console-border); border-radius: 8px; }
progress::-webkit-progress-value { background: var(--p-primary-color); border-radius: 8px; }
progress::-moz-progress-bar { background: var(--p-primary-color); border-radius: 8px; }
.overview-presence-legend { display: flex; justify-content: space-between; gap: 12px; margin-top: 15px; font-size: 11px; color: var(--console-muted); }
.overview-presence-legend > span { display: inline-flex; align-items: center; gap: 5px; }
.overview-presence-legend .pi { font-size: 6px; }
.overview-presence-legend .presence-online { color: var(--p-green-400); }
.overview-presence-legend b { font-weight: 550; color: var(--p-text-color); }
.overview-presence > p { margin-top: 14px; color: var(--console-muted); font-size: 10px; line-height: 1.5; }
.dashboard-page .summary-strip { border: 0; border-radius: 0; gap: 16px; background: transparent; overflow: visible; }
.dashboard-page .summary-stat { background: var(--console-panel); border: 1px solid var(--console-border); border-radius: 12px; padding: 21px 22px; min-width: 0; }
.dashboard-page .summary-label { gap: 8px; }
.dashboard-page .summary-label > .pi { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 32px; border: 1px solid color-mix(in srgb, var(--p-primary-color) 18%, var(--console-border)); border-radius: 8px; color: var(--p-primary-color); background: color-mix(in srgb, var(--p-primary-color) 6%, var(--console-panel)); }
.dashboard-page .summary-value { font-size: 36px; font-weight: 550; margin-top: 7px; }
.dashboard-page .summary-note { line-height: 1.5; }
.dashboard-page .overview-columns { gap: 20px; grid-template-columns: 1fr 1fr; }
.overview-section-title { min-width: 0; display: flex; align-items: center; gap: 12px; }
.overview-section-title h2 { font-size: 14px; }
.overview-section-title p { font-size: 11px; color: var(--console-muted); margin-top: 4px; line-height: 1.5; }
.overview-section-icon { display: flex; align-items: center; justify-content: center; flex: none; width: 34px; height: 34px; border: 1px solid var(--console-border); border-radius: 9px; color: var(--p-primary-color); }
.dashboard-page .section-heading > .entity-link { flex: none; }
.dashboard-page .preview-row { min-height: 76px; }
.overview-device-icon { position: relative; flex: none; }
.overview-device-icon .status-dot { position: absolute; bottom: 0; right: 0; width: 6px; height: 6px; }
.overview-network-presence { min-width: 70px; }
.overview-network-presence progress { height: 3px; margin-top: 9px; }
@media (max-width: 1199px) { .overview-hero { gap: 24px; padding: 28px; grid-template-columns: minmax(0, 1fr) 220px; } .overview-presence { position: relative; padding: 20px; } .dashboard-page .overview-columns { grid-template-columns: 1fr; } }
@media (max-width: 767px) { .overview-hero { grid-template-columns: 1fr; padding: 24px; gap: 24px; } .overview-hero h2 { font-size: 26px; } .overview-presence { display: grid; grid-template-columns: 1fr 1fr; column-gap: 20px; align-items: center; } .overview-presence-title { grid-column: 1; grid-row: 1; } .overview-presence-value { grid-column: 2; grid-row: 1 / 3; justify-content: flex-end; margin: 0; } .overview-presence-value strong { font-size: 40px; } .overview-presence > progress { grid-column: 1; grid-row: 2; margin: 10px 0 0; } .overview-presence-legend { grid-column: 1 / -1; margin-top: 16px; } .overview-presence > p { grid-column: 1 / -1; margin-top: 12px; } .dashboard-page .summary-strip { gap: 10px; } .dashboard-page .summary-stat { padding: 16px; } .dashboard-page .summary-label > .pi { width: 26px; height: 26px; font-size: 12px; flex: none; } .dashboard-page .summary-value { font-size: 30px; } .overview-section-title p { display: none; } }
@media (prefers-reduced-motion: reduce) { .overview-primary-link, .overview-secondary-link { transition: none; } }
</style>
