<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Button } from 'primevue';
import type { AssetServiceCatalogEntry } from '../../types/node-assets';
import { serviceMessages } from '../../modules/service-messages';
import { safeServiceUrl } from '../../modules/service-utils';
import { assetServiceIconClass } from '../../constants/asset-service-icons';

const props = defineProps<{ service: AssetServiceCatalogEntry }>();
defineEmits<{ actions: [service: AssetServiceCatalogEntry, event: MouseEvent] }>();
const { t } = useI18n({ useScope: 'local', messages: serviceMessages });
const href = computed(() => safeServiceUrl(props.service.access_url || ''));
</script>

<template>
    <article class="service-card" :data-service-id="service.id">
        <a class="service-card-link" :href="href || undefined" :aria-disabled="!href"
            target="_blank" rel="noopener noreferrer"
            :aria-label="`${t('open')}: ${service.name}`" @click="!href && $event.preventDefault()">
            <span :class="['service-card-icon', `service-icon-${service.icon}`]"><i :class="assetServiceIconClass(service.icon)" aria-hidden="true"></i></span>
            <span class="service-card-copy">
                <strong>{{ service.name }}</strong>
                <span v-if="service.remarks" class="service-card-description">{{ service.remarks }}</span>
                <span v-else class="service-card-description">{{ service.device_name }}</span>
                <span class="service-card-meta"><span class="service-card-status" :title="t('uncheckedHint')"><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ t('unchecked') }}</span><span class="service-card-network" :title="`${service.node_name} · ${service.device_name}`"><i class="pi pi-share-alt" aria-hidden="true"></i>{{ service.node_name }} · {{ service.device_name }}</span></span>
            </span>
            <span class="service-card-open"><i class="pi pi-arrow-up-right" aria-hidden="true"></i></span>
        </a>
        <div class="service-card-footer">
            <span class="service-card-address" :title="service.access_url || undefined">{{ service.access_url || '—' }}</span>
            <Button icon="pi pi-ellipsis-h" text rounded size="small" severity="secondary"
                :aria-label="`${t('actions')}: ${service.name}`" aria-haspopup="menu"
                @click="$emit('actions', service, $event)" />
        </div>
    </article>
</template>

<style scoped>
.service-card { min-width: 0; position: relative; border: 1px solid var(--console-border); border-radius: 13px; background: var(--console-panel); transition: border-color .18s, box-shadow .18s, transform .18s; overflow: hidden; box-shadow: inset 0 1px 0 color-mix(in srgb, var(--p-text-color) 3%, transparent); }
.service-card:hover { transform: translateY(-2px); border-color: color-mix(in srgb, var(--p-primary-color) 60%, var(--console-border)); box-shadow: var(--console-glow, 0 8px 24px rgb(0 0 0 / 7%)); }
.service-card:focus-within { border-color: var(--p-primary-color); box-shadow: var(--console-glow, none); }
.service-card-link { display: flex; align-items: flex-start; gap: 18px; padding: 25px 24px 22px; min-height: 144px; color: var(--p-text-color); text-decoration: none; }
.service-card-link:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: -3px; }
.service-card-link[aria-disabled="true"] { cursor: not-allowed; }
.service-card-icon { --service-color: var(--p-primary-color); width: 58px; height: 58px; border-radius: 15px; display: inline-flex; align-items: center; justify-content: center; flex: none; background: color-mix(in srgb, var(--service-color) 10%, var(--console-panel)); color: var(--service-color); border: 1px solid color-mix(in srgb, var(--service-color) 23%, transparent); box-shadow: inset 0 1px 0 color-mix(in srgb, var(--service-color) 12%, transparent); }
.service-card-icon .pi { font-size: 26px; }
.service-icon-home { --service-color: #0284c7; }
.service-icon-photos { --service-color: #db2777; }
.service-icon-media { --service-color: #7c3aed; }
.service-icon-chart { --service-color: #b45309; }
.service-icon-code { --service-color: #4f46e5; }
.service-card-copy { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 8px; }
.service-card-copy strong { font-size: 18px; font-weight: 600; line-height: 1.3; letter-spacing: -.025em; overflow-wrap: anywhere; }
.service-card-description { font-size: 13px; color: var(--console-muted); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; line-height: 1.6; }
.service-card-meta { display: flex; align-items: center; flex-wrap: wrap; gap: 7px 13px; margin-top: 5px; }
.service-card-status { display: inline-flex; align-items: center; gap: 6px; color: var(--console-muted); font-size: 11px; white-space: nowrap; }
.service-card-status .pi { font-size: 6px; }
.service-card-network { display: inline-flex; align-items: center; gap: 6px; color: var(--console-muted); font-size: 11px; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.service-card-network .pi { font-size: 10px; }
.service-card-open { display: inline-flex; align-items: center; justify-content: center; color: var(--console-muted); width: 29px; height: 29px; flex: none; border: 1px solid var(--console-border); border-radius: 8px; background: var(--console-panel-raised, var(--console-panel)); transition: color .18s, border-color .18s; }
.service-card-open > .pi { font-size: 12px; }
.service-card:hover .service-card-open { color: var(--p-primary-color); border-color: color-mix(in srgb, var(--p-primary-color) 30%, var(--console-border)); }
.service-card-footer { display: flex; align-items: center; gap: 12px; border-top: 1px solid var(--console-border); min-height: 42px; padding: 5px 12px 5px 24px; background: color-mix(in srgb, var(--console-panel-raised, var(--console-panel)) 40%, transparent); }
.service-card-address { color: var(--console-muted); min-width: 0; flex: 1; text-overflow: ellipsis; overflow: hidden; white-space: nowrap; font-size: 11px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
html.app-dark .service-icon-home { --service-color: #53cafa; }
html.app-dark .service-icon-photos { --service-color: #f78ac4; }
html.app-dark .service-icon-media { --service-color: #b39aff; }
html.app-dark .service-icon-chart { --service-color: #f2b566; }
html.app-dark .service-icon-code { --service-color: #99b1ff; }
@media (max-width: 600px) { .service-card-link { padding: 21px 18px 18px; gap: 14px; min-height: 132px; } .service-card-icon { width: 48px; height: 48px; border-radius: 13px; } .service-card-icon .pi { font-size: 23px; } .service-card-copy strong { font-size: 16px; } .service-card-footer { padding-left: 18px; } .service-card-open { width: 25px; height: 25px; } }
@media (prefers-reduced-motion: reduce) { .service-card, .service-card-open { transition: none; } .service-card:hover { transform: none; } }
</style>
