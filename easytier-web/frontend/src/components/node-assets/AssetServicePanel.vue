<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Dialog, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AssetDevice, AssetScope, AssetService } from '../../types/node-assets';
import { formatAssetError, getAssetError } from '../../composables/useAssetMutation';
import { assetServiceIconClass } from '../../constants/asset-service-icons';
import { publicationFeedback } from '../../modules/publication-sync';
import AssetServiceDialog from './AssetServiceDialog.vue';
import { serviceMessages } from '../../modules/service-messages';
import { safeServiceUrl } from '../../modules/service-utils';

const props = withDefaults(defineProps<{
    api: ApiClient;
    scope: AssetScope;
    device: AssetDevice;
    addRequest?: number;
}>(), { addRequest: 0 });
const emit = defineEmits<{ changed: []; 'add-request-consumed': []; 'context-error': [error: unknown] }>();
const { t, locale } = useI18n();
const navigationCopy = computed(() => locale.value === 'cn' || locale.value.startsWith('zh') ? serviceMessages.cn : serviceMessages.en);
const rows = ref<AssetService[]>([]);
const loading = ref(false);
const loadError = ref('');
const feedback = ref('');
const feedbackWarning = ref(false);
const unavailable = ref(false);
const editorVisible = ref(false);
const editingService = ref<AssetService | null>(null);
const deleteTarget = ref<AssetService | null>(null);
const deleting = ref(false);
const deleteError = ref('');
const contextKey = computed(() => `${props.scope.owner_user_id}:${props.scope.machine_id}:${props.device.id}`);
const contextValid = computed(() => props.scope.owner_user_id === props.device.owner_user_id
    && props.scope.machine_id === props.device.machine_id);
let contextEpoch = 0;
let loadEpoch = 0;

async function loadServices() {
    const currentContext = contextEpoch;
    const currentLoad = ++loadEpoch;
    if (!contextValid.value) {
        rows.value = [];
        unavailable.value = true;
        loadError.value = t('assets.services.contextChanged');
        loading.value = false;
        return;
    }
    const scope = { ...props.scope };
    const deviceId = props.device.id;
    loading.value = true;
    loadError.value = '';
    try {
        const response = await props.api.assetsServices(scope, deviceId);
        if (currentContext !== contextEpoch || currentLoad !== loadEpoch) return;
        rows.value = response.items;
        unavailable.value = false;
    } catch (error: unknown) {
        if (currentContext !== contextEpoch || currentLoad !== loadEpoch) return;
        const status = getAssetError(error).status;
        if (status === 401 || status === 403 || status === 404) {
            rows.value = [];
            unavailable.value = true;
            editorVisible.value = false;
            deleteTarget.value = null;
            emit('context-error', error);
        }
        loadError.value = formatAssetError(error, t('assets.services.loadFailed'), key => t(key));
    } finally {
        if (currentContext === contextEpoch && currentLoad === loadEpoch) loading.value = false;
    }
}

watch(contextKey, () => {
    contextEpoch++;
    rows.value = [];
    unavailable.value = false;
    feedback.value = '';
    feedbackWarning.value = false;
    editorVisible.value = false;
    editingService.value = null;
    deleteTarget.value = null;
    deleting.value = false;
    deleteError.value = '';
    void loadServices();
}, { immediate: true });
watch(() => props.addRequest, (value, previous) => {
    if (value > 0 && value !== previous) {
        openCreate();
        emit('add-request-consumed');
    }
}, { immediate: true });
onBeforeUnmount(() => { contextEpoch++; loadEpoch++; });

function openCreate() {
    if (!contextValid.value || unavailable.value) return;
    editingService.value = null;
    editorVisible.value = true;
    feedback.value = '';
}

function openEdit(service: AssetService) {
    if (unavailable.value) return;
    editingService.value = service;
    editorVisible.value = true;
    feedback.value = '';
}

function saved(service: AssetService) {
    if (service.owner_user_id !== props.scope.owner_user_id || service.machine_id !== props.scope.machine_id
        || service.device_id !== props.device.id) return;
    const wasPublished = !!editingService.value?.publication;
    editorVisible.value = false;
    editingService.value = null;
    const result = publicationFeedback(service, locale.value,
        t(service.publication ? 'assets.publication.saved' : wasPublished ? 'assets.publication.removed' : 'assets.services.saved'),
        service.publication ? 'save' : 'remove');
    feedback.value = result.message;
    feedbackWarning.value = result.warning;
    emit('changed');
    void loadServices();
}

function askDelete(service: AssetService) {
    deleteTarget.value = service;
    deleteError.value = '';
    feedback.value = '';
}

async function confirmDelete() {
    if (deleting.value || !deleteTarget.value) return;
    const service = deleteTarget.value;
    const currentContext = contextEpoch;
    const scope = { ...props.scope };
    deleting.value = true;
    deleteError.value = '';
    try {
        const response = await props.api.assetsDeleteService(scope, service.id);
        if (currentContext !== contextEpoch) return;
        deleteTarget.value = null;
        const result = publicationFeedback(response, locale.value, t('assets.services.deleted'), 'delete');
        feedback.value = result.message;
        feedbackWarning.value = result.warning;
        emit('changed');
        await loadServices();
    } catch (error: unknown) {
        if (currentContext !== contextEpoch) return;
        emit('context-error', error);
        if (getAssetError(error).status === 404) {
            deleteTarget.value = null;
            feedback.value = t('assets.services.alreadyRemoved');
            feedbackWarning.value = false;
            emit('changed');
            await loadServices();
        } else {
            deleteError.value = formatAssetError(error, t('assets.services.deleteFailed'), key => t(key));
        }
    } finally {
        if (currentContext === contextEpoch) deleting.value = false;
    }
}

function safeAccessUrl(value: string | null): string | null {
    return safeServiceUrl(value || '');
}
</script>

<template>
    <section class="service-panel" :aria-label="t('assets.services.panelFor', { name: device.name })" :aria-busy="loading">
        <header class="service-panel-header">
            <h3>{{ t('assets.services.title') }}</h3>
            <div class="service-panel-actions">
                <Button :label="t('assets.services.refresh')" icon="pi pi-refresh" text size="small" :loading="loading"
                    :disabled="loading" @click="loadServices" />
                <Button :label="t('assets.services.add')" icon="pi pi-plus" size="small"
                    :disabled="unavailable || !contextValid" @click="openCreate" />
            </div>
        </header>
        <p v-if="feedback" class="service-feedback" :class="{ 'service-sync-warning': feedbackWarning }"
            :role="feedbackWarning ? 'alert' : 'status'">{{ feedback }}</p>
        <div v-if="loadError" class="service-error-row" role="alert">
            <span>{{ loadError }}</span>
            <Button :label="t('assets.services.retry')" severity="secondary" size="small" :disabled="loading" @click="loadServices" />
        </div>
        <p v-if="loading && !rows.length" class="service-placeholder" role="status">{{ t('assets.services.loading') }}</p>
        <p v-else-if="!rows.length && !loadError" class="service-placeholder">{{ t('assets.services.empty') }}</p>
        <div v-if="rows.length" class="service-table-scroll">
            <table class="service-table">
                <caption class="sr-only">{{ t('assets.services.panelFor', { name: device.name }) }}</caption>
                <thead><tr>
                    <th scope="col">{{ t('assets.services.name') }}</th>
                    <th scope="col">{{ t('assets.services.protocols') }}</th>
                    <th scope="col">{{ t('assets.services.port') }}</th>
                    <th scope="col">{{ t('assets.publication.column') }}</th>
                    <th scope="col">{{ t('assets.services.accessUrl') }}</th>
                    <th scope="col">{{ navigationCopy.navigation }}</th>
                    <th scope="col">{{ t('assets.services.remarks') }}</th>
                    <th scope="col">{{ t('assets.services.actions') }}</th>
                </tr></thead>
                <tbody>
                    <tr v-for="service in rows" :key="service.id">
                        <td><span class="service-name"><i :class="assetServiceIconClass(service.icon)" aria-hidden="true"></i>{{ service.name }}</span></td>
                        <td class="service-protocol">{{ service.protocols.map(protocol => protocol.toUpperCase()).join(' / ') }}</td>
                        <td class="service-port">{{ service.port }}</td>
                        <td>
                            <div v-if="service.publication" class="service-publication-info">
                                <span>{{ t('assets.publication.configured') }}</span>
                                <small>{{ service.publication.server_name || service.publication.machine_id }}</small>
                                <small>{{ service.publication.network_name || service.publication.instance_id }}</small>
                                <code>{{ service.publication.bind_ip.includes(':') ? `[${service.publication.bind_ip}]` : service.publication.bind_ip }}:{{ service.publication.bind_port }}</code>
                            </div>
                            <span v-else class="service-muted">{{ t('assets.publication.notPublished') }}</span>
                        </td>
                        <td>
                            <a v-if="safeAccessUrl(service.access_url)" :href="safeAccessUrl(service.access_url) || undefined"
                                target="_blank" rel="noopener noreferrer" class="service-link"
                                :title="service.access_url ?? undefined"
                                :aria-label="t('assets.services.openNamed', { name: service.name })">
                                {{ t('assets.services.open') }} <i class="pi pi-external-link" aria-hidden="true"></i>
                            </a>
                            <span v-else class="service-muted">—</span>
                        </td>
                        <td><Tag :value="service.show_in_navigation ? navigationCopy.enabled : navigationCopy.hidden"
                            :severity="service.show_in_navigation ? 'success' : 'secondary'" /></td>
                        <td class="service-remarks">{{ service.remarks || '—' }}</td>
                        <td><div class="service-row-actions">
                            <Button :label="t('assets.services.edit')" icon="pi pi-pencil" text size="small"
                                :disabled="unavailable" @click="openEdit(service)" />
                            <Button :label="t('assets.services.delete')" icon="pi pi-trash" text severity="danger" size="small"
                                :disabled="unavailable || deleting" @click="askDelete(service)" />
                        </div></td>
                    </tr>
                </tbody>
            </table>
        </div>
        <AssetServiceDialog v-if="editorVisible" :visible="true" :api="api" :scope="scope" :device="device"
            :service="editingService" @close="editorVisible = false" @saved="saved"
            @context-error="emit('context-error', $event)" />
        <Dialog :visible="!!deleteTarget" modal :header="t('assets.services.deleteTitle')"
            :style="{ width: '28rem', maxWidth: 'calc(100vw - 2rem)' }" :closable="!deleting"
            :closeOnEscape="!deleting" :dismissableMask="false"
            @update:visible="value => { if (!value && !deleting) deleteTarget = null; }">
            <p class="delete-description">{{ t('assets.services.deleteConfirm', { name: deleteTarget?.name ?? '' }) }}</p>
            <p v-if="deleteTarget?.publication" class="delete-description">{{ t('assets.publication.deleteHint') }}</p>
            <p v-if="deleteError" class="service-error" role="alert">{{ deleteError }}</p>
            <template #footer>
                <Button :label="t('assets.services.cancel')" severity="secondary" :disabled="deleting" @click="deleteTarget = null" />
                <Button :label="t('assets.services.delete')" icon="pi pi-trash" severity="danger" :loading="deleting"
                    :disabled="deleting" @click="confirmDelete" />
            </template>
        </Dialog>
    </section>
</template>

<style scoped>
.service-panel { padding: 0.85rem 1rem 1rem; border: 1px solid var(--console-border, var(--p-content-border-color)); border-radius: 0.5rem; background: var(--p-content-background); min-width: 0; max-width: 100%; }
.service-panel-header, .service-panel-actions, .service-row-actions, .service-error-row { display: flex; align-items: center; gap: 0.5rem; }
.service-panel-header { justify-content: space-between; flex-wrap: wrap; margin-bottom: 0.65rem; }
.service-panel-header h3 { margin: 0; font-size: 0.95rem; font-weight: 600; }
.service-panel-actions { flex-wrap: wrap; }
.service-table-scroll { min-width: 0; max-width: 100%; overflow-x: auto; }
.service-table { border-collapse: collapse; width: 100%; font-size: 0.875rem; }
.service-table th, .service-table td { padding: 0.65rem 0.75rem; text-align: start; vertical-align: middle; border-bottom: 1px solid var(--console-border, var(--p-content-border-color)); }
.service-table th { color: var(--console-muted, var(--p-text-muted-color)); font-weight: 600; white-space: nowrap; }
.service-table tbody tr:last-child td { border-bottom: 0; }
.service-name { display: flex; align-items: center; gap: 0.5rem; overflow-wrap: anywhere; min-width: 7rem; }
.service-name i { color: var(--p-primary-color); flex-shrink: 0; }
.service-protocol, .service-port { white-space: nowrap; }
.service-publication-info { display: grid; gap: 0.25rem; min-width: 8rem; max-width: 18rem; overflow-wrap: anywhere; }
.service-publication-info small { color: var(--console-muted, var(--p-text-muted-color)); }
.service-remarks { min-width: 6rem; max-width: 22rem; overflow-wrap: anywhere; white-space: pre-wrap; }
.service-link { display: inline-flex; align-items: center; gap: 0.35rem; color: var(--p-primary-color); white-space: nowrap; }
.service-link:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; border-radius: 2px; }
.service-row-actions { flex-wrap: nowrap; }
.service-placeholder, .service-muted, .service-feedback { color: var(--console-muted, var(--p-text-muted-color)); }
.service-placeholder { margin: 0; padding-block: 0.75rem; font-size: 0.875rem; }
.service-feedback { font-size: 0.875rem; margin: 0.4rem 0; }
.service-feedback.service-sync-warning { color: var(--p-orange-700, #c2410c); }
.service-error-row { justify-content: space-between; flex-wrap: wrap; padding: 0.6rem 0; }
.service-error-row, .service-error { color: var(--p-red-600, #dc2626); }
.delete-description { overflow-wrap: anywhere; }
.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; }
@media (max-width: 480px) { .service-panel { padding: 0.6rem; } .service-table th, .service-table td { padding-inline: 0.5rem; } }
</style>
