<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue';
import { Button, Dialog, InputText } from 'primevue';
import { useI18n } from 'vue-i18n';
import ApiClient from '../../modules/api';
import type { DeviceType } from '../../types/node-assets';
import { formatAssetError, getAssetError, useAssetMutation } from '../../composables/useAssetMutation';

const props = defineProps<{ visible: boolean; api: ApiClient; ownerUserId: number; ownerLabel?: string }>();
const emit = defineEmits<{ close: []; changed: []; 'context-error': [cause: unknown] }>();
const { t } = useI18n();
const types = ref<DeviceType[]>([]);
const loading = ref(false);
const loadError = ref('');
const editorOpen = ref(false);
const editingId = ref<string | null>(null);
const name = ref('');
const fieldError = ref('');
const actionError = ref('');
const saving = ref(false);
const deleting = ref(false);
const deleteTarget = ref<DeviceType | null>(null);
const createAttempt = useAssetMutation();
let contextVersion = 0;
let listVersion = 0;
let editorVersion = 0;
let disposed = false;

const currentContext = (version: number) => !disposed && props.visible && contextVersion === version;
const currentEditor = (context: number, version: number) => currentContext(context)
    && editorOpen.value && editorVersion === version;
const errorText = (error: unknown, fallback = 'assets.types.request_failed') =>
    formatAssetError(error, t(fallback), key => t(key));
const reportContextError = (cause: unknown) => {
    if ([401, 403, 404].includes(getAssetError(cause).status ?? 0)) emit('context-error', cause);
};

function clearEditor() {
    editorVersion += 1;
    editorOpen.value = false;
    editingId.value = null;
    name.value = '';
    saving.value = false;
    fieldError.value = '';
    actionError.value = '';
    createAttempt.reset();
}

function startEditor(type?: DeviceType) {
    clearEditor();
    deleteTarget.value = null;
    editingId.value = type?.id ?? null;
    name.value = type?.name ?? '';
    editorOpen.value = true;
}

function close() {
    contextVersion += 1;
    listVersion += 1;
    clearEditor();
    deleteTarget.value = null;
    emit('close');
}

async function loadTypes() {
    const context = contextVersion;
    const request = ++listVersion;
    const owner = props.ownerUserId;
    loading.value = true;
    loadError.value = '';
    try {
        // The page initializes each owner's catalog explicitly. Reading this
        // dialog must never recreate deleted preset types.
        const response = await props.api.assetsTypes(owner);
        if (currentContext(context) && request === listVersion) types.value = response.items;
    } catch (error: unknown) {
        if (currentContext(context) && request === listVersion) {
            loadError.value = errorText(error);
            reportContextError(error);
        }
    } finally {
        if (currentContext(context) && request === listVersion) loading.value = false;
    }
}

async function save() {
    if (saving.value || !editorOpen.value) return;
    fieldError.value = '';
    actionError.value = '';
    if (!name.value.trim()) {
        fieldError.value = t('assets.types.name_required');
        return;
    }
    const context = contextVersion;
    const editor = editorVersion;
    const owner = props.ownerUserId;
    const id = editingId.value;
    const payload = { name: name.value.trim() };
    saving.value = true;
    try {
        if (id) {
            await props.api.assetsUpdateType(owner, id, payload);
        } else {
            await createAttempt.run(payload, request => props.api.assetsCreateType(owner, request),
                `${owner}:device-type`);
        }
        if (!currentEditor(context, editor)) return;
        clearEditor();
        emit('changed');
        void loadTypes();
    } catch (error: unknown) {
        if (!currentEditor(context, editor)) return;
        const detail = getAssetError(error);
        fieldError.value = detail.field_errors.name || '';
        actionError.value = errorText(error, 'assets.types.save_failed');
        reportContextError(error);
    } finally {
        if (currentEditor(context, editor)) saving.value = false;
    }
}

function requestDelete(type: DeviceType) {
    clearEditor();
    deleteTarget.value = type;
    actionError.value = '';
}

async function confirmDelete() {
    if (!deleteTarget.value || deleting.value) return;
    const context = contextVersion;
    const owner = props.ownerUserId;
    const target = deleteTarget.value;
    deleting.value = true;
    actionError.value = '';
    try {
        await props.api.assetsDeleteType(owner, target.id);
        if (!currentContext(context)) return;
        deleteTarget.value = null;
        emit('changed');
        void loadTypes();
    } catch (error: unknown) {
        if (currentContext(context)) {
            actionError.value = errorText(error);
            reportContextError(error);
        }
    } finally {
        if (currentContext(context)) deleting.value = false;
    }
}

watch(() => [props.visible, props.ownerUserId] as const, () => {
    contextVersion += 1;
    listVersion += 1;
    clearEditor();
    types.value = [];
    deleteTarget.value = null;
    loading.value = false;
    deleting.value = false;
    loadError.value = '';
    if (props.visible) void loadTypes();
}, { immediate: true });

onBeforeUnmount(() => {
    disposed = true;
    contextVersion += 1;
    listVersion += 1;
    editorVersion += 1;
    createAttempt.reset();
});
</script>

<template>
    <Dialog :visible="visible" modal :header="t('assets.types.title')"
        :style="{ width: '36rem', maxWidth: 'calc(100vw - 2rem)' }" :dismissableMask="false"
        @update:visible="value => { if (!value) close(); }">
        <div class="type-manager">
            <p class="scope-identity">{{ t('assets.types.owner_context') }} <strong>{{ ownerLabel || `#${ownerUserId}` }}</strong></p>
            <div class="manager-toolbar">
                <p class="muted">{{ t('assets.types.scope_hint') }}</p>
                <Button :label="t('assets.types.add')" icon="pi pi-plus" size="small"
                    :disabled="saving || deleting || editorOpen" @click="startEditor()" />
            </div>

            <section v-if="editorOpen" class="editor" :aria-label="t(editingId ? 'assets.types.edit' : 'assets.types.add')">
                <h3>{{ t(editingId ? 'assets.types.edit' : 'assets.types.add') }}</h3>
                <form class="editor-fields" @submit.prevent="save">
                    <div class="field">
                        <label for="asset-type-name">{{ t('assets.types.name') }} *</label>
                        <InputText id="asset-type-name" v-model="name" :disabled="saving || createAttempt.uncertain.value"
                            autocomplete="off" :aria-invalid="!!fieldError" aria-describedby="asset-type-name-error" />
                        <small v-if="fieldError" id="asset-type-name-error" class="error" role="alert">{{ fieldError }}</small>
                    </div>
                    <p v-if="createAttempt.uncertain.value" class="notice" role="status">{{ t('assets.types.uncertain') }}</p>
                    <p v-if="actionError" class="error" role="alert">{{ actionError }}</p>
                    <div class="editor-actions">
                        <Button :label="t('assets.types.cancel')" severity="secondary" type="button" @click="clearEditor" />
                        <Button :label="t(createAttempt.uncertain.value ? 'assets.types.retry_save' : 'assets.types.save')"
                            icon="pi pi-check" type="submit" :loading="saving" :disabled="saving" />
                    </div>
                </form>
            </section>

            <section v-if="deleteTarget" class="delete-confirm" role="alert" :aria-label="t('assets.types.delete')">
                <p>{{ t('assets.types.delete_prompt', { name: deleteTarget.name }) }}</p>
                <p class="muted">{{ t('assets.types.delete_hint') }}</p>
                <p v-if="actionError" class="error">{{ actionError }}</p>
                <div class="editor-actions">
                    <Button :label="t('assets.types.cancel')" severity="secondary" :disabled="deleting" @click="deleteTarget = null" />
                    <Button :label="t('assets.types.delete')" icon="pi pi-trash" severity="danger"
                        :loading="deleting" :disabled="deleting" @click="confirmDelete" />
                </div>
            </section>

            <div v-if="loading" class="list-status" role="status"><i class="pi pi-spinner pi-spin" aria-hidden="true" /> {{ t('assets.types.loading') }}</div>
            <div v-else-if="loadError" class="list-status" role="alert">
                <p class="error">{{ loadError }}</p>
                <Button :label="t('assets.types.retry')" icon="pi pi-refresh" size="small" severity="secondary" @click="loadTypes" />
            </div>
            <p v-else-if="!types.length" class="list-status muted">{{ t('assets.types.empty') }}</p>
            <ul v-else class="type-list">
                <li v-for="type in types" :key="type.id" class="type-item">
                    <div class="type-information"><strong>{{ type.name }}</strong><code>{{ type.id.slice(0, 8) }}</code></div>
                    <div class="row-actions">
                        <Button icon="pi pi-pencil" text severity="secondary" :disabled="saving || deleting"
                            :aria-label="t('assets.types.edit_named', { name: type.name })" @click="startEditor(type)" />
                        <Button icon="pi pi-trash" text severity="danger" :disabled="saving || deleting"
                            :aria-label="t('assets.types.delete_named', { name: type.name })" @click="requestDelete(type)" />
                    </div>
                </li>
            </ul>
        </div>
        <template #footer><Button :label="t('assets.types.close')" severity="secondary" @click="close" /></template>
    </Dialog>
</template>

<style scoped>
.type-manager, .editor-fields, .field { display: flex; flex-direction: column; gap: 0.5rem; }
.type-manager, .editor-fields { gap: 1rem; }
.scope-identity { margin: 0; overflow-wrap: anywhere; }
.manager-toolbar, .editor-actions, .row-actions, .type-information { display: flex; align-items: center; gap: 0.5rem; }
.manager-toolbar { justify-content: space-between; flex-wrap: wrap; }
.manager-toolbar p, .delete-confirm p { margin: 0; }
.editor, .delete-confirm { padding: 1rem; border: 1px solid var(--p-content-border-color, #e2e8f0); border-radius: 0.75rem; }
.editor h3 { margin: 0 0 1rem; font-size: 1rem; }
.field label { font-weight: 600; }
.editor-actions { justify-content: flex-end; flex-wrap: wrap; }
.type-list { margin: 0; padding: 0; list-style: none; }
.type-item { display: flex; justify-content: space-between; align-items: center; gap: 0.75rem; padding: 0.65rem 0; border-bottom: 1px solid var(--p-content-border-color, #e2e8f0); }
.type-item:last-child { border-bottom: 0; }
.type-information { min-width: 0; flex-wrap: wrap; }
.type-information strong { overflow-wrap: anywhere; }
.type-information code { color: var(--p-text-muted-color, #64748b); font-size: 0.75rem; }
.row-actions { flex-shrink: 0; }
.list-status { padding: 1rem 0; text-align: center; }
.muted { color: var(--p-text-muted-color, #64748b); font-size: 0.875rem; }
.error { color: var(--p-red-600, #dc2626); font-size: 0.875rem; overflow-wrap: anywhere; }
.notice { color: var(--p-yellow-700, #a16207); font-size: 0.875rem; }
.delete-confirm { display: grid; gap: 0.75rem; border-color: var(--p-red-300, #fca5a5); }
:global(.app-dark) .notice { color: var(--p-yellow-300, #fde047); }
@media (max-width: 460px) { .type-item { flex-wrap: wrap; } .row-actions { margin-left: auto; } }
</style>
