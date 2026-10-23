<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Dialog, InputText, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import ApiClient from '../../modules/api';
import type { AssetScope, AssetWarning, GatewayGroup } from '../../types/node-assets';
import { formatAssetError, getAssetError, useAssetMutation } from '../../composables/useAssetMutation';

const props = defineProps<{
    visible: boolean;
    api: ApiClient;
    scope: AssetScope;
    nodeLabel?: string;
    ownerLabel?: string;
}>();
const emit = defineEmits<{ close: []; changed: [] }>();
const { t } = useI18n();
const groups = ref<GatewayGroup[]>([]);
const loading = ref(false);
const loadError = ref('');
const actionError = ref('');
const editorOpen = ref(false);
const editingId = ref<string | null>(null);
const name = ref('');
const remarks = ref('');
const subnets = ref<Array<{ key: number; value: string }>>([]);
const fieldErrors = ref<Record<string, string>>({});
const previewErrors = ref<Record<string, string>>({});
const previewWarnings = ref<AssetWarning[]>([]);
const savedWarnings = ref<AssetWarning[]>([]);
const previewError = ref('');
const previewing = ref(false);
const saving = ref(false);
const deleting = ref(false);
const deleteTarget = ref<GatewayGroup | null>(null);
const createAttempt = useAssetMutation();
let rowKey = 0;
let contextVersion = 0;
let listVersion = 0;
let editorVersion = 0;
let previewVersion = 0;
let disposed = false;

const contextKey = computed(() => `${props.scope.owner_user_id}:${props.scope.machine_id}`);
const locked = computed(() => saving.value || createAttempt.uncertain.value);
const currentSubnets = () => subnets.value.map(row => row.value.trim());
const previewInput = () => ({ gateway_group_id: editingId.value, subnets: currentSubnets() });
const scopeSnapshot = (): AssetScope => ({ ...props.scope });
const currentContext = (version: number) => !disposed && props.visible && version === contextVersion;
const currentEditor = (context: number, version: number) => currentContext(context)
    && editorOpen.value && version === editorVersion;
const errorText = (error: unknown, fallback = 'assets.groups.request_failed') =>
    formatAssetError(error, t(fallback), key => t(key));

function invalidatePreview() {
    previewVersion += 1;
    previewing.value = false;
    previewErrors.value = {};
    previewWarnings.value = [];
    previewError.value = '';
}

function clearEditor() {
    editorVersion += 1;
    invalidatePreview();
    editorOpen.value = false;
    editingId.value = null;
    saving.value = false;
    fieldErrors.value = {};
    actionError.value = '';
    createAttempt.reset();
}

function startEditor(group?: GatewayGroup) {
    clearEditor();
    deleteTarget.value = null;
    savedWarnings.value = [];
    editingId.value = group?.id ?? null;
    name.value = group?.name ?? '';
    remarks.value = group?.remarks ?? '';
    subnets.value = (group?.subnets.length ? group.subnets : [''])
        .map(value => ({ key: ++rowKey, value }));
    previewWarnings.value = group?.warnings ?? [];
    editorOpen.value = true;
}

function close() {
    contextVersion += 1;
    listVersion += 1;
    clearEditor();
    deleteTarget.value = null;
    emit('close');
}

async function loadGroups() {
    const context = contextVersion;
    const request = ++listVersion;
    const scope = scopeSnapshot();
    loading.value = true;
    loadError.value = '';
    try {
        const response = await props.api.assetsGroups(scope);
        if (currentContext(context) && request === listVersion) groups.value = response.items;
    } catch (error: unknown) {
        if (currentContext(context) && request === listVersion) loadError.value = errorText(error);
    } finally {
        if (currentContext(context) && request === listVersion) loading.value = false;
    }
}

async function previewSubnets() {
    if (!props.visible || !editorOpen.value || locked.value) return;
    const context = contextVersion;
    const editor = editorVersion;
    const request = ++previewVersion;
    const scope = scopeSnapshot();
    const input = previewInput();
    const signature = JSON.stringify(input);
    previewing.value = true;
    previewError.value = '';
    try {
        const response = await props.api.assetsPreviewSubnets(scope, input);
        if (!currentEditor(context, editor) || request !== previewVersion
            || signature !== JSON.stringify(previewInput())) return;
        previewErrors.value = response.field_errors;
        previewWarnings.value = response.warnings;
    } catch (error: unknown) {
        if (!currentEditor(context, editor) || request !== previewVersion
            || signature !== JSON.stringify(previewInput())) return;
        const detail = getAssetError(error);
        previewErrors.value = detail.field_errors;
        previewError.value = errorText(error, 'assets.groups.preview_failed');
    } finally {
        if (currentEditor(context, editor) && request === previewVersion) previewing.value = false;
    }
}

function addSubnet() {
    if (locked.value) return;
    subnets.value.push({ key: ++rowKey, value: '' });
    invalidatePreview();
}

function removeSubnet(index: number) {
    if (locked.value || subnets.value.length <= 1) return;
    subnets.value.splice(index, 1);
    invalidatePreview();
    void previewSubnets();
}

function subnetError(index: number) {
    return fieldErrors.value[`subnets.${index}`] || fieldErrors.value[`subnets[${index}]`]
        || previewErrors.value[`subnets.${index}`] || previewErrors.value[`subnets[${index}]`] || '';
}

async function save() {
    if (saving.value || !editorOpen.value) return;
    fieldErrors.value = {};
    actionError.value = '';
    if (!name.value.trim()) {
        fieldErrors.value.name = t('assets.groups.name_required');
        return;
    }
    const context = contextVersion;
    const editor = editorVersion;
    const scope = scopeSnapshot();
    const id = editingId.value;
    const payload = {
        name: name.value.trim(),
        remarks: remarks.value.trim() || null,
        subnets: currentSubnets(),
    };
    saving.value = true;
    try {
        const group = id
            ? await props.api.assetsUpdateGroup(scope, id, payload)
            : await createAttempt.run(payload, request => props.api.assetsCreateGroup(scope, request),
                `${scope.owner_user_id}:${scope.machine_id}:group`);
        if (!currentEditor(context, editor)) return;
        const warnings = group.warnings;
        clearEditor();
        savedWarnings.value = warnings;
        emit('changed');
        void loadGroups();
    } catch (error: unknown) {
        if (!currentEditor(context, editor)) return;
        const detail = getAssetError(error);
        fieldErrors.value = detail.field_errors;
        actionError.value = errorText(error, 'assets.groups.save_failed');
    } finally {
        if (currentEditor(context, editor)) saving.value = false;
    }
}

function requestDelete(group: GatewayGroup) {
    clearEditor();
    savedWarnings.value = [];
    deleteTarget.value = group;
    actionError.value = '';
}

async function confirmDelete() {
    if (!deleteTarget.value || deleting.value) return;
    const target = deleteTarget.value;
    const context = contextVersion;
    const scope = scopeSnapshot();
    deleting.value = true;
    actionError.value = '';
    try {
        await props.api.assetsDeleteGroup(scope, target.id);
        if (!currentContext(context)) return;
        deleteTarget.value = null;
        emit('changed');
        void loadGroups();
    } catch (error: unknown) {
        if (currentContext(context)) actionError.value = errorText(error);
    } finally {
        if (currentContext(context)) deleting.value = false;
    }
}

watch(() => [props.visible, contextKey.value] as const, () => {
    contextVersion += 1;
    listVersion += 1;
    clearEditor();
    groups.value = [];
    deleteTarget.value = null;
    savedWarnings.value = [];
    loadError.value = '';
    loading.value = false;
    deleting.value = false;
    if (props.visible) void loadGroups();
}, { immediate: true });

onBeforeUnmount(() => {
    disposed = true;
    contextVersion += 1;
    listVersion += 1;
    editorVersion += 1;
    previewVersion += 1;
    createAttempt.reset();
});
</script>

<template>
    <Dialog :visible="visible" modal :header="t('assets.groups.title')"
        :style="{ width: '46rem', maxWidth: 'calc(100vw - 2rem)' }" :dismissableMask="false"
        @update:visible="value => { if (!value) close(); }">
        <div class="group-manager">
            <p class="scope-identity">{{ t('assets.groups.node_context') }}
                <strong>{{ nodeLabel || scope.machine_id }}</strong>
                <span v-if="ownerLabel" class="muted"> · {{ ownerLabel }}</span>
            </p>
            <div class="manager-toolbar">
                <p class="muted">{{ t('assets.groups.scope_hint') }}</p>
                <Button :label="t('assets.groups.add')" icon="pi pi-plus" size="small"
                    :disabled="saving || deleting || editorOpen" @click="startEditor()" />
            </div>

            <section v-if="editorOpen" class="editor" :aria-label="t(editingId ? 'assets.groups.edit' : 'assets.groups.add')">
                <h3>{{ t(editingId ? 'assets.groups.edit' : 'assets.groups.add') }}</h3>
                <form class="editor-fields" @submit.prevent="save">
                    <div class="field">
                        <label for="asset-group-name">{{ t('assets.groups.name') }} *</label>
                        <InputText id="asset-group-name" v-model="name" :disabled="locked" autocomplete="off"
                            :aria-invalid="!!fieldErrors.name" aria-describedby="asset-group-name-error" />
                        <small v-if="fieldErrors.name" id="asset-group-name-error" class="error" role="alert">{{ fieldErrors.name }}</small>
                    </div>
                    <fieldset class="subnet-fields">
                        <legend>{{ t('assets.groups.subnets') }} *</legend>
                        <p class="muted">{{ t('assets.groups.subnets_hint') }}</p>
                        <div v-for="(row, index) in subnets" :key="row.key" class="subnet-field">
                            <div class="subnet-row">
                                <InputText :id="`asset-subnet-${row.key}`" v-model="row.value" :disabled="locked"
                                    :aria-label="t('assets.groups.subnet_number', { number: index + 1 })"
                                    :aria-invalid="!!subnetError(index)" :aria-describedby="`asset-subnet-${row.key}-error`"
                                    placeholder="192.168.1.0/24 / fd00::/64" autocomplete="off" spellcheck="false"
                                    @update:model-value="invalidatePreview" @blur="previewSubnets" />
                                <Button icon="pi pi-times" text severity="secondary" :disabled="locked || subnets.length <= 1"
                                    :aria-label="t('assets.groups.remove_subnet', { number: index + 1 })" @click="removeSubnet(index)" />
                            </div>
                            <small v-if="subnetError(index)" :id="`asset-subnet-${row.key}-error`" class="error">{{ subnetError(index) }}</small>
                        </div>
                        <small v-if="fieldErrors.subnets || previewErrors.subnets" class="error" role="alert">{{ fieldErrors.subnets || previewErrors.subnets }}</small>
                        <div class="subnet-actions">
                            <Button :label="t('assets.groups.add_subnet')" icon="pi pi-plus" text size="small" :disabled="locked" @click="addSubnet" />
                            <Button :label="t('assets.groups.preview')" icon="pi pi-search" text size="small"
                                :loading="previewing" :disabled="locked || previewing" @click="previewSubnets" />
                        </div>
                        <p v-if="previewError" class="error" role="alert">{{ previewError }}</p>
                        <ul v-if="previewWarnings.length" class="warning-list" role="status">
                            <li v-for="(warning, index) in previewWarnings" :key="`${warning.device_id ?? warning.field}-${index}`">
                                <i class="pi pi-exclamation-triangle" aria-hidden="true" />
                                <span><strong v-if="warning.device_name">{{ warning.device_name }} — </strong>{{ warning.message }}
                                    <span v-if="warning.suggestion"> {{ warning.suggestion }}</span>
                                </span>
                            </li>
                        </ul>
                    </fieldset>
                    <div class="field">
                        <label for="asset-group-remarks">{{ t('assets.groups.remarks') }}</label>
                        <Textarea id="asset-group-remarks" v-model="remarks" :disabled="locked" rows="3" autoResize
                            :aria-invalid="!!fieldErrors.remarks" aria-describedby="asset-group-remarks-error" />
                        <small v-if="fieldErrors.remarks" id="asset-group-remarks-error" class="error">{{ fieldErrors.remarks }}</small>
                    </div>
                    <p v-if="createAttempt.uncertain.value" class="notice" role="status">{{ t('assets.groups.uncertain') }}</p>
                    <p v-if="actionError" class="error" role="alert">{{ actionError }}</p>
                    <div class="editor-actions">
                        <Button :label="t('assets.groups.cancel')" severity="secondary" type="button" @click="clearEditor" />
                        <Button :label="t(createAttempt.uncertain.value ? 'assets.groups.retry_save' : 'assets.groups.save')"
                            icon="pi pi-check" type="submit" :loading="saving" :disabled="saving" />
                    </div>
                </form>
            </section>

            <section v-if="deleteTarget" class="delete-confirm" role="alert" :aria-label="t('assets.groups.delete')">
                <p>{{ t('assets.groups.delete_prompt', { name: deleteTarget.name }) }}</p>
                <p class="muted">{{ t('assets.groups.delete_hint') }}</p>
                <p v-if="actionError" class="error">{{ actionError }}</p>
                <div class="editor-actions">
                    <Button :label="t('assets.groups.cancel')" severity="secondary" :disabled="deleting" @click="deleteTarget = null" />
                    <Button :label="t('assets.groups.delete')" icon="pi pi-trash" severity="danger"
                        :loading="deleting" :disabled="deleting" @click="confirmDelete" />
                </div>
            </section>

            <ul v-if="savedWarnings.length" class="warning-list" role="status">
                <li v-for="(warning, index) in savedWarnings" :key="`${warning.device_id ?? warning.field}-${index}`">
                    <i class="pi pi-exclamation-triangle" aria-hidden="true" />
                    <span><strong v-if="warning.device_name">{{ warning.device_name }} — </strong>{{ warning.message }}</span>
                </li>
            </ul>

            <div v-if="loading" class="list-status" role="status"><i class="pi pi-spinner pi-spin" aria-hidden="true" /> {{ t('assets.groups.loading') }}</div>
            <div v-else-if="loadError" class="list-status" role="alert">
                <p class="error">{{ loadError }}</p>
                <Button :label="t('assets.groups.retry')" icon="pi pi-refresh" size="small" severity="secondary" @click="loadGroups" />
            </div>
            <p v-else-if="!groups.length" class="list-status muted">{{ t('assets.groups.empty') }}</p>
            <ul v-else class="group-list">
                <li v-for="group in groups" :key="group.id" class="group-item">
                    <div class="group-information">
                        <div class="group-heading"><strong>{{ group.name }}</strong><code>{{ group.id.slice(0, 8) }}</code></div>
                        <div class="subnet-list"><code v-for="subnet in group.subnets" :key="subnet">{{ subnet }}</code></div>
                        <p v-if="group.remarks" class="muted group-remarks">{{ group.remarks }}</p>
                    </div>
                    <div class="row-actions">
                        <Button icon="pi pi-pencil" text severity="secondary" :disabled="saving || deleting"
                            :aria-label="t('assets.groups.edit_named', { name: group.name })" @click="startEditor(group)" />
                        <Button icon="pi pi-trash" text severity="danger" :disabled="saving || deleting"
                            :aria-label="t('assets.groups.delete_named', { name: group.name })" @click="requestDelete(group)" />
                    </div>
                </li>
            </ul>
        </div>
        <template #footer><Button :label="t('assets.groups.close')" severity="secondary" @click="close" /></template>
    </Dialog>
</template>

<style scoped>
.group-manager, .editor-fields, .field, .subnet-field { display: flex; flex-direction: column; gap: 0.5rem; }
.group-manager { gap: 1rem; }
.scope-identity { margin: 0; overflow-wrap: anywhere; }
.manager-toolbar, .group-heading, .subnet-row, .subnet-actions, .row-actions, .editor-actions { display: flex; align-items: center; gap: 0.5rem; }
.manager-toolbar { justify-content: space-between; flex-wrap: wrap; }
.manager-toolbar p, .group-item p, .delete-confirm p { margin: 0; }
.editor, .delete-confirm { padding: 1rem; border: 1px solid var(--p-content-border-color, #e2e8f0); border-radius: 0.75rem; }
.editor h3 { margin: 0 0 1rem; font-size: 1rem; }
.editor-fields { gap: 1rem; }
.field label, .subnet-fields legend { font-weight: 600; }
.subnet-fields { display: flex; flex-direction: column; gap: 0.65rem; min-width: 0; margin: 0; padding: 0; border: 0; }
.subnet-fields legend { padding: 0; margin-bottom: 0.5rem; }
.subnet-fields p { margin: 0; }
.subnet-row :first-child { min-width: 0; flex: 1; }
.subnet-actions { flex-wrap: wrap; }
.editor-actions { justify-content: flex-end; flex-wrap: wrap; }
.group-list { margin: 0; padding: 0; list-style: none; }
.group-item { display: flex; justify-content: space-between; align-items: flex-start; gap: 0.75rem; padding: 0.85rem 0; border-bottom: 1px solid var(--p-content-border-color, #e2e8f0); }
.group-item:last-child { border-bottom: 0; }
.group-information { min-width: 0; display: grid; gap: 0.5rem; }
.group-heading { flex-wrap: wrap; }
.group-heading strong, .group-remarks { overflow-wrap: anywhere; }
.group-heading > code { font-size: 0.75rem; color: var(--p-text-muted-color, #64748b); }
.group-remarks { white-space: pre-wrap; }
.subnet-list { display: flex; flex-wrap: wrap; gap: 0.35rem; }
.subnet-list code { padding: 0.2rem 0.4rem; border-radius: 0.3rem; background: var(--p-content-hover-background, #f1f5f9); color: var(--p-text-color, #334155); overflow-wrap: anywhere; font-size: 0.8rem; }
.row-actions { flex-shrink: 0; }
.list-status { padding: 1rem 0; text-align: center; }
.muted { color: var(--p-text-muted-color, #64748b); font-size: 0.875rem; }
.error { color: var(--p-red-600, #dc2626); font-size: 0.875rem; overflow-wrap: anywhere; }
.notice, .warning-list { color: var(--p-yellow-700, #a16207); font-size: 0.875rem; }
.warning-list { list-style: none; margin: 0; padding: 0.75rem; border: 1px solid var(--p-yellow-300, #fde047); border-radius: 0.5rem; display: grid; gap: 0.5rem; }
.warning-list li { display: flex; align-items: flex-start; gap: 0.5rem; overflow-wrap: anywhere; }
.warning-list i { margin-top: 0.15rem; }
.delete-confirm { display: grid; gap: 0.75rem; border-color: var(--p-red-300, #fca5a5); }
:global(.app-dark) .notice, :global(.app-dark) .warning-list { color: var(--p-yellow-300, #fde047); }
@media (max-width: 520px) { .group-item { flex-wrap: wrap; } .row-actions { margin-left: auto; } }
</style>
