<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Button } from 'primevue';
import type { ServiceEntry } from '../../modules/network-space';
import { serviceMessages } from '../../modules/service-messages';
import { safeServiceUrl, serviceIcons } from '../../modules/service-utils';

const props = defineProps<{ service: ServiceEntry; networkName?: string }>();
defineEmits<{ actions: [service: ServiceEntry, event: MouseEvent] }>();
const { t } = useI18n({ useScope: 'local', messages: serviceMessages });
const href = computed(() => safeServiceUrl(props.service.url));
</script>

<template>
    <article class="service-card" :data-service-id="service.id">
        <a class="service-card-link" :href="href || undefined" :aria-disabled="!href"
            :target="service.open_in_new_tab ? '_blank' : undefined" rel="noopener noreferrer"
            :aria-label="`${t('open')}: ${service.name}`" @click="!href && $event.preventDefault()">
            <span :class="['service-card-icon', `service-icon-${service.icon}`]"><i :class="['pi', serviceIcons[service.icon] || 'pi-globe']" aria-hidden="true"></i></span>
            <span class="service-card-copy">
                <strong>{{ service.name }}</strong>
                <span v-if="service.description" class="service-card-description">{{ service.description }}</span>
                <span v-else class="service-card-description">{{ networkName || service.group || t('ungrouped') }}</span>
                <span class="service-card-status" :title="t('uncheckedHint')"><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ t('unchecked') }}</span>
            </span>
            <i class="pi pi-arrow-up-right service-card-open" aria-hidden="true"></i>
        </a>
        <div class="service-card-footer">
            <span class="service-card-address" :title="service.url">{{ service.url }}</span>
            <Button icon="pi pi-ellipsis-h" text rounded size="small" severity="secondary"
                :aria-label="`${t('actions')}: ${service.name}`" aria-haspopup="menu"
                @click="$emit('actions', service, $event)" />
        </div>
    </article>
</template>

<style scoped>
.service-card { min-width: 0; border: 1px solid var(--console-border); border-radius: 10px; background: var(--console-panel); transition: border-color .16s, box-shadow .16s; overflow: hidden; }
.service-card:hover { border-color: var(--p-primary-color); box-shadow: 0 4px 18px rgb(0 0 0 / 5%); }
.service-card:focus-within { border-color: var(--p-primary-color); }
.service-card-link { display: flex; align-items: flex-start; gap: 18px; padding: 24px 22px 18px; min-height: 144px; color: var(--p-text-color); text-decoration: none; }
.service-card-link:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: -3px; }
.service-card-link[aria-disabled="true"] { cursor: not-allowed; }
.service-card-icon { width: 56px; height: 56px; border-radius: 12px; display: inline-flex; align-items: center; justify-content: center; flex: none; background: var(--p-primary-50); color: var(--p-primary-700); }
.service-card-icon .pi { font-size: 25px; }
.service-icon-home { background: #e0f2fe; color: #0284c7; }
.service-icon-photos { background: #fce7f3; color: #db2777; }
.service-icon-media { background: #ede9fe; color: #7c3aed; }
.service-icon-chart { background: #fff1dd; color: #b45309; }
.service-icon-code { background: #e0e7ff; color: #4f46e5; }
.service-card-copy { min-width: 0; flex: 1; display: flex; flex-direction: column; gap: 7px; }
.service-card-copy strong { font-size: 17px; font-weight: 600; line-height: 1.3; overflow-wrap: anywhere; }
.service-card-description { font-size: 13px; color: var(--console-muted); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
.service-card-status { display: flex; align-items: center; gap: 6px; color: var(--console-muted); font-size: 12px; margin-top: 3px; }
.service-card-status .pi { font-size: 7px; }
.service-card-open { color: var(--console-muted); font-size: 14px; margin-top: 3px; }
.service-card-footer { display: flex; align-items: center; gap: 12px; border-top: 1px solid var(--console-border); min-height: 42px; padding: 5px 10px 5px 22px; }
.service-card-address { color: var(--console-muted); min-width: 0; flex: 1; text-overflow: ellipsis; overflow: hidden; white-space: nowrap; font-size: 11px; }
html.app-dark .service-card-icon { background: #163b35; color: #62d9b4; }
html.app-dark .service-icon-home { background: #143b4b; color: #53cafa; }
html.app-dark .service-icon-photos { background: #41243c; color: #f78ac4; }
html.app-dark .service-icon-media { background: #302747; color: #b39aff; }
html.app-dark .service-icon-chart { background: #453523; color: #f2b566; }
html.app-dark .service-icon-code { background: #253051; color: #99b1ff; }
@media (max-width: 520px) { .service-card-link { padding: 20px 18px 16px; gap: 14px; min-height: 128px; } .service-card-icon { width: 48px; height: 48px; } }
@media (prefers-reduced-motion: reduce) { .service-card { transition: none; } }
</style>
