<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { Button, InputNumber, InputText } from 'primevue';
import { useI18n } from 'vue-i18n';
import type { Utils } from 'easytier-frontend-lib';
import type { CircleMarker, LatLngTuple, LeafletMouseEvent, Map as LeafletMap } from 'leaflet';
import 'leaflet/dist/leaflet.css';
import type ApiClient from '../modules/api';
import type { NodeGeocodingConfig, NodeGeocodingLanguage } from '../modules/api';
import { emptyNodeLocation, NODE_LOCATION_TEXT_FIELDS, nodeLocationErrors, normalizeNodeLocation } from '../modules/node-location';
import type { NodeLocationTextField } from '../modules/node-location';

const props = defineProps<{
    modelValue: Utils.NodeGeoLocation | null;
    api?: ApiClient;
    idPrefix: string;
    disabled?: boolean;
}>();
const emit = defineEmits<{
    'update:modelValue': [value: Utils.NodeGeoLocation | null];
    'lookup-pending': [value: boolean];
}>();
const { t, locale } = useI18n();
const errors = computed(() => nodeLocationErrors(props.modelValue, (key, values) => values ? t(key, values) : t(key)));
const hasLocation = computed(() => normalizeNodeLocation(props.modelValue) !== null);
const point = computed<LatLngTuple | null>(() => {
    const latitude = props.modelValue?.latitude, longitude = props.modelValue?.longitude;
    if (latitude == null || longitude == null || !Number.isFinite(latitude) || !Number.isFinite(longitude)
        || latitude < -90 || latitude > 90 || longitude < -180 || longitude > 180) return null;
    return [latitude, longitude];
});
const language = computed<NodeGeocodingLanguage>(() => {
    const base = locale.value.toLowerCase().split(/[-_]/)[0];
    return base === 'en' || base === 'de' || base === 'fr' ? base : 'local';
});
const mapExpanded = ref(false), mapLoading = ref(false), mapError = ref(''), tileError = ref(false);
const mapElement = ref<HTMLElement | null>(null);
const config = ref<NodeGeocodingConfig | null>(null);
const lookupBusy = ref(false), lookupRequested = ref(false), lookupMessage = ref(''), lookupFailed = ref(false);
let leaflet: typeof import('leaflet') | null = null;
let map: LeafletMap | null = null, marker: CircleMarker | null = null;
let resizeObserver: ResizeObserver | null = null;
let configRequest: Promise<NodeGeocodingConfig> | null = null;
let lookupTimer: ReturnType<typeof setTimeout> | undefined;
let mapVersion = 0, lookupVersion = 0;
let disposed = false;

const attributionUrl = computed(() => {
    try {
        const url = new URL(config.value?.attribution_url ?? '');
        return ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password ? url.href : '';
    } catch { return ''; }
});

function invalidateLookup() {
    ++lookupVersion;
    if (lookupTimer !== undefined) clearTimeout(lookupTimer);
    lookupTimer = undefined;
    lookupBusy.value = false;
    lookupMessage.value = '';
    lookupFailed.value = false;
}

function updateText(field: NodeLocationTextField, value: string | null | undefined) {
    if (props.disabled) return;
    invalidateLookup();
    emit('update:modelValue', { ...(props.modelValue ?? emptyNodeLocation()), [field]: value ?? null });
}

function updateCoordinate(field: 'latitude' | 'longitude', value: number | null) {
    if (props.disabled) return;
    invalidateLookup();
    emit('update:modelValue', { ...(props.modelValue ?? emptyNodeLocation()), [field]: value });
}

function clearLocation() {
    if (props.disabled) return;
    invalidateLookup();
    emit('update:modelValue', null);
}

async function loadConfig(): Promise<NodeGeocodingConfig> {
    if (!props.api) throw new Error('Geocoding API unavailable');
    if (!configRequest) configRequest = props.api.nodeGeocodingConfig();
    const request = configRequest;
    try {
        const result = await request;
        if (!disposed) config.value = result;
        return result;
    } finally {
        if (configRequest === request) configRequest = null;
    }
}

function updateMarker(center = false) {
    if (!map || !leaflet) return;
    if (!point.value) {
        marker?.remove();
        marker = null;
        return;
    }
    if (marker) marker.setLatLng(point.value);
    else marker = leaflet.circleMarker(point.value, {
        radius: 8, color: '#ffffff', weight: 3, fillColor: '#2563eb', fillOpacity: 1,
    }).addTo(map);
    if (center) map.setView(point.value, Math.min(12, map.getMaxZoom()), { animate: false });
}

function removeMap() {
    resizeObserver?.disconnect();
    resizeObserver = null;
    marker = null;
    map?.remove();
    map = null;
}

function selectMapPoint(event: LeafletMouseEvent) {
    if (props.disabled) return;
    invalidateLookup();
    const latitude = Number(event.latlng.lat.toFixed(6));
    // Leaflet can show adjacent world copies; persist one WGS84 longitude.
    const longitude = Number(event.latlng.wrap().lng.toFixed(6));
    emit('update:modelValue', { ...(props.modelValue ?? emptyNodeLocation()), latitude, longitude, coordinate_system: 'WGS84' });
    const version = lookupVersion;
    lookupBusy.value = true;
    lookupTimer = setTimeout(() => {
        lookupTimer = undefined;
        void reverseLocation(version, [latitude, longitude]);
    }, 550);
}

async function initializeMap() {
    const version = ++mapVersion;
    removeMap();
    mapLoading.value = true;
    mapError.value = '';
    tileError.value = false;
    try {
        await nextTick();
        const settings = await loadConfig();
        const library = await import('leaflet');
        if (disposed || version !== mapVersion || !mapExpanded.value || !mapElement.value) return;
        const tileUrl = new URL(settings.tile_url.replace(/\{[xyzs]\}/g, '0'));
        if (!['https:', 'http:'].includes(tileUrl.protocol) || tileUrl.username || tileUrl.password) {
            throw new Error('Invalid map tile URL');
        }
        leaflet = library;
        const maxZoom = Number.isInteger(settings.max_zoom) ? Math.max(0, Math.min(22, settings.max_zoom)) : 19;
        const currentMap = library.map(mapElement.value, { attributionControl: false, maxZoom });
        map = currentMap;
        currentMap.setView(point.value ?? [20, 0], Math.min(point.value ? 11 : 2, maxZoom));
        library.tileLayer(settings.tile_url, {
            maxZoom, noWrap: true, updateWhenIdle: true, updateWhenZooming: false, keepBuffer: 1,
        }).on('tileerror', () => {
            if (!disposed && version === mapVersion) tileError.value = true;
        }).addTo(currentMap);
        currentMap.on('click', selectMapPoint);
        updateMarker();
        if (typeof ResizeObserver !== 'undefined') {
            resizeObserver = new ResizeObserver(() => {
                if (!disposed && version === mapVersion) currentMap.invalidateSize({ pan: false, animate: false });
            });
            resizeObserver.observe(mapElement.value);
        }
        await nextTick();
        if (!disposed && version === mapVersion) currentMap.invalidateSize({ pan: false, animate: false });
    } catch {
        if (!disposed && version === mapVersion) {
            removeMap();
            mapError.value = t('web.node_location.map_failed');
        }
    } finally {
        if (!disposed && version === mapVersion) mapLoading.value = false;
    }
}

function toggleMap() {
    if (props.disabled) return;
    mapExpanded.value = !mapExpanded.value;
    if (mapExpanded.value) void initializeMap();
    else {
        ++mapVersion;
        invalidateLookup();
        removeMap();
        mapLoading.value = false;
    }
}

function lookupErrorKey(cause: unknown): string {
    const response = (cause as { response?: { status?: number; data?: { code?: string } } })?.response;
    const code = response?.data?.code;
    if (response?.status === 429 || code === 'geocoding_busy') return 'web.node_location.lookup_busy';
    if (code === 'geocoding_disabled') return 'web.node_location.lookup_disabled';
    if (code === 'geocoding_no_result' || response?.status === 404) return 'web.node_location.lookup_no_result';
    return 'web.node_location.lookup_failed';
}

async function reverseLocation(version: number, coordinates: LatLngTuple) {
    // Coordinates and the revision must both still match. A text edit also
    // changes the revision, so a late lookup never overwrites a correction.
    const active = () => !disposed && !props.disabled && version === lookupVersion
        && point.value?.[0] === coordinates[0] && point.value?.[1] === coordinates[1];
    if (!active()) return;
    lookupBusy.value = true;
    lookupMessage.value = '';
    lookupFailed.value = false;
    try {
        const settings = config.value ?? await loadConfig();
        if (!active()) return;
        if (!settings.reverse_enabled) {
            lookupFailed.value = true;
            lookupMessage.value = t('web.node_location.lookup_disabled');
            return;
        }
        if (!props.api) throw new Error('Geocoding API unavailable');
        lookupRequested.value = true;
        const address = await props.api.reverseNodeLocation(coordinates[0], coordinates[1], language.value);
        if (!active()) return;
        if (!NODE_LOCATION_TEXT_FIELDS.some(field => address[field]?.trim())) {
            lookupFailed.value = true;
            lookupMessage.value = t('web.node_location.lookup_no_result');
            return;
        }
        emit('update:modelValue', {
            ...(props.modelValue ?? emptyNodeLocation()),
            country: address.country, region: address.region, city: address.city, district: address.district,
        });
        lookupMessage.value = t('web.node_location.lookup_complete');
    } catch (cause) {
        if (active()) {
            lookupFailed.value = true;
            lookupMessage.value = t(lookupErrorKey(cause));
        }
    } finally {
        if (active()) lookupBusy.value = false;
    }
}

function resolveAddress() {
    if (props.disabled || !point.value) return;
    invalidateLookup();
    void reverseLocation(lookupVersion, [...point.value]);
}

watch(() => [props.modelValue?.latitude, props.modelValue?.longitude], () => updateMarker());
watch(() => props.disabled, disabled => { if (disabled) invalidateLookup(); });
watch(language, () => invalidateLookup());
// Include the map-click debounce so an immediate save cannot commit the new
// coordinates with the address from the previous point.
watch(lookupBusy, value => emit('lookup-pending', value), { immediate: true, flush: 'sync' });
onBeforeUnmount(() => {
    disposed = true;
    ++mapVersion;
    invalidateLookup();
    removeMap();
});
</script>

<template>
    <section class="node-location-editor" :aria-labelledby="`${idPrefix}-heading`">
        <div class="location-heading">
            <h3 :id="`${idPrefix}-heading`">{{ t('web.node_location.title') }}</h3>
            <Button type="button" :label="t('web.node_location.clear')" icon="pi pi-times" text size="small"
                severity="secondary" :disabled="disabled || !hasLocation" @click="clearLocation" />
        </div>
        <p class="location-hint">{{ t('web.node_location.hint') }}</p>
        <div class="location-fields">
            <div v-for="field in NODE_LOCATION_TEXT_FIELDS" :key="field" class="location-field">
                <label :for="`${idPrefix}-${field}`">{{ t(`web.node_location.${field}`) }}</label>
                <InputText :id="`${idPrefix}-${field}`" :modelValue="modelValue?.[field] ?? ''" :disabled="disabled"
                    autocomplete="off" :invalid="!!errors[field]" :aria-invalid="!!errors[field]"
                    :aria-describedby="errors[field] ? `${idPrefix}-${field}-error` : undefined"
                    @update:modelValue="value => updateText(field, value)" />
                <small v-if="errors[field]" :id="`${idPrefix}-${field}-error`" class="location-error">{{ errors[field] }}</small>
            </div>
            <div class="location-field">
                <label :for="`${idPrefix}-longitude`">{{ t('web.node_location.longitude') }}</label>
                <InputNumber :inputId="`${idPrefix}-longitude`" :modelValue="modelValue?.longitude ?? null" :disabled="disabled"
                    :useGrouping="false" :maxFractionDigits="6" :invalid="!!errors.longitude" :aria-invalid="!!errors.longitude"
                    :aria-describedby="`${idPrefix}-longitude-help`" @input="invalidateLookup" @focus="invalidateLookup"
                    @update:modelValue="value => updateCoordinate('longitude', value)" />
                <small :id="`${idPrefix}-longitude-help`" :class="{ 'location-error': errors.longitude }">
                    {{ errors.longitude || t('web.node_location.longitude_hint') }}
                </small>
            </div>
            <div class="location-field">
                <label :for="`${idPrefix}-latitude`">{{ t('web.node_location.latitude') }}</label>
                <InputNumber :inputId="`${idPrefix}-latitude`" :modelValue="modelValue?.latitude ?? null" :disabled="disabled"
                    :useGrouping="false" :maxFractionDigits="6" :invalid="!!errors.latitude" :aria-invalid="!!errors.latitude"
                    :aria-describedby="`${idPrefix}-latitude-help`" @input="invalidateLookup" @focus="invalidateLookup"
                    @update:modelValue="value => updateCoordinate('latitude', value)" />
                <small :id="`${idPrefix}-latitude-help`" :class="{ 'location-error': errors.latitude }">
                    {{ errors.latitude || t('web.node_location.latitude_hint') }}
                </small>
            </div>
        </div>
        <div class="location-actions">
            <Button type="button" :label="t(mapExpanded ? 'web.node_location.hide_map' : 'web.node_location.choose_map')"
                icon="pi pi-map-marker" size="small" outlined severity="secondary" :disabled="disabled"
                :aria-expanded="mapExpanded" :aria-controls="`${idPrefix}-map-panel`" @click="toggleMap" />
            <Button type="button" :label="t('web.node_location.resolve_address')" icon="pi pi-search" size="small" text
                :loading="lookupBusy" :disabled="disabled || !api || !point || lookupBusy" @click="resolveAddress" />
        </div>
        <p v-if="lookupBusy || lookupMessage" class="location-status" :class="{ 'location-warning': lookupFailed }" role="status" aria-live="polite">
            {{ lookupBusy ? t('web.node_location.looking_up') : lookupMessage }}
        </p>
        <p v-if="lookupRequested" class="location-attribution location-hint">
            {{ t('web.node_location.address_data') }}
            <a href="https://photon.komoot.io" target="_blank" rel="noopener noreferrer">Photon</a>
            · <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">© OpenStreetMap contributors</a>
        </p>
        <div v-if="mapExpanded" :id="`${idPrefix}-map-panel`" class="location-map-panel">
            <div class="location-map-instructions">
                <span>{{ t('web.node_location.map_hint') }}</span>
                <Button type="button" icon="pi pi-crosshairs" :label="t('web.node_location.center_point')" text size="small"
                    :disabled="disabled || mapLoading || !!mapError || !point" @click="updateMarker(true)" />
            </div>
            <div class="location-map-shell">
                <div ref="mapElement" class="location-map" :aria-label="t('web.node_location.map_label')" :aria-busy="mapLoading"></div>
                <div v-if="mapLoading || mapError" class="location-map-placeholder" role="status">
                    <i v-if="mapLoading" class="pi pi-spin pi-spinner" aria-hidden="true"></i>
                    <p>{{ mapLoading ? t('web.node_location.loading_map') : mapError }}</p>
                    <Button v-if="mapError" type="button" :label="t('web.node_location.retry_map')" icon="pi pi-refresh"
                        size="small" :disabled="disabled" @click="initializeMap" />
                </div>
                <div v-if="config && !mapError" class="location-map-attribution location-attribution">
                    <a href="https://leafletjs.com" target="_blank" rel="noopener noreferrer">Leaflet</a> |
                    <a v-if="attributionUrl" :href="attributionUrl" target="_blank" rel="noopener noreferrer">{{ config.attribution_text }}</a>
                    <span v-else>{{ config.attribution_text }}</span>
                </div>
            </div>
            <div v-if="tileError && !mapError" class="location-tile-error" role="status">
                <span>{{ t('web.node_location.tiles_failed') }}</span>
                <Button type="button" :label="t('web.node_location.retry_map')" text size="small" :disabled="disabled || mapLoading" @click="initializeMap" />
            </div>
        </div>
    </section>
</template>

<style scoped>
.node-location-editor { display: grid; gap: .75rem; padding-top: 1rem; border-top: 1px solid var(--console-border, #e2e8f0); min-width: 0; }
.location-heading,.location-actions,.location-map-instructions,.location-tile-error { display: flex; align-items: center; gap: .5rem; flex-wrap: wrap; }
.location-heading,.location-map-instructions { justify-content: space-between; }
.location-heading h3 { margin: 0; font-size: 1rem; font-weight: 600; }
.location-hint,.location-field small,.location-map-instructions,.location-status,.location-tile-error { margin: 0; color: var(--console-muted, #64748b); font-size: .8125rem; line-height: 1.5; }
.location-fields { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: .8rem; }
.location-field { display: flex; flex-direction: column; gap: .35rem; min-width: 0; }
.location-field label { font-size: .875rem; font-weight: 500; }
.location-field :deep(.p-inputnumber-input) { width: 100%; min-width: 0; }
.location-error,.location-field small.location-error { color: var(--p-red-600, #dc2626); }
.location-warning,.location-tile-error { color: var(--p-orange-700, #c2410c); }
.location-map-panel { display: grid; gap: .5rem; min-width: 0; }
.location-map-shell { position: relative; isolation: isolate; overflow: hidden; border: 1px solid var(--console-border, #e2e8f0); border-radius: .5rem; }
.location-map { height: 20rem; width: 100%; z-index: 0; background: #e2e8f0; }
.location-map-placeholder { position: absolute; inset: 0; z-index: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; gap: .5rem; padding: 1.5rem; text-align: center; font-size: .875rem; color: var(--console-muted, #64748b); background: var(--console-surface, #f8fafc); }
.location-map-placeholder p { margin: 0; }
.location-map-attribution { position: absolute; z-index: 1; bottom: 0; right: 0; max-width: 100%; padding: 2px 5px; background: rgba(255,255,255,.92); color: #334155; font-size: 11px; overflow-wrap: anywhere; }
.location-attribution a { color: #2563eb; text-decoration: underline; }
@media (max-width: 480px) { .location-fields { grid-template-columns: minmax(0,1fr); }.location-map { height: 17rem; } }
</style>
