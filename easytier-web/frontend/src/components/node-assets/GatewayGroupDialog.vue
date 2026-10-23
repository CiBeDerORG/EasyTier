<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Dialog } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useAssetBindingLabels } from '../../composables/useAssetBindingLabels';
import type ApiClient from '../../modules/api';
import type { AssetScope, GatewayGroup } from '../../types/node-assets';
import { formatAssetError, getAssetError } from '../../composables/useAssetMutation';
import GatewayBindingEditor from './GatewayBindingEditor.vue';

const props = defineProps<{
    visible: boolean; api: ApiClient; scope: AssetScope;
    nodeLabel?: string; ownerLabel?: string; focusId?: string | null;
}>();
const emit = defineEmits<{ close: []; changed: []; 'context-error': [cause: unknown] }>();
const { t } = useI18n();
const { bindingReason } = useAssetBindingLabels();
const groups = ref<GatewayGroup[]>([]);
const loading = ref(false);
const loadError = ref('');
const actionError = ref('');
const editorOpen = ref(false);
const editing = ref<GatewayGroup | null>(null);
const editorMode = ref<'metadata' | 'binding'>('metadata');
const editorKey = ref(0);
const deleting = ref(false);
const deleteTarget = ref<GatewayGroup | null>(null);
const highlightedId = ref<string | null>(null);
let contextVersion = 0;
let listVersion = 0;
let disposed = false;
const contextKey = computed(() => `${props.scope.owner_user_id}:${props.scope.machine_id}`);
const current = (version: number) => !disposed && props.visible && version === contextVersion;
function contextError(cause: unknown) {
    if ([401, 403, 404].includes(getAssetError(cause).status ?? 0)) emit('context-error', cause);
}
function clearEditor() { editorOpen.value = false; editing.value = null; editorKey.value += 1; }
function startEditor(group: GatewayGroup | null = null, mode: 'metadata' | 'binding' = 'metadata') {
    clearEditor();
    deleteTarget.value = null;
    editing.value = group;
    editorMode.value = mode;
    editorOpen.value = true;
}
function close() { contextVersion += 1; listVersion += 1; clearEditor(); emit('close'); }
async function loadGroups() {
    const context = contextVersion;
    const request = ++listVersion;
    loading.value = true;
    loadError.value = '';
    try {
        const response = await props.api.assetsGroups({ ...props.scope });
        if (current(context) && request === listVersion) groups.value = response.items;
    } catch (cause) {
        if (!current(context) || request !== listVersion) return;
        loadError.value = formatAssetError(cause, t('assets.groups.request_failed'), key => t(key));
        contextError(cause);
    } finally { if (current(context) && request === listVersion) loading.value = false; }
}
function saved(event: { resource_kind: 'gateway'; record: GatewayGroup }) {
    highlightedId.value = event.record.id;
    const index = groups.value.findIndex(group => group.id === event.record.id);
    if (index < 0) groups.value.unshift(event.record); else groups.value[index] = event.record;
    clearEditor();
    emit('changed');
    void loadGroups();
}
function requestDelete(group: GatewayGroup) {
    clearEditor(); deleteTarget.value = group; actionError.value = '';
}
async function confirmDelete() {
    if (!deleteTarget.value || deleting.value) return;
    const target = deleteTarget.value;
    const context = contextVersion;
    deleting.value = true;
    actionError.value = '';
    try {
        await props.api.assetsDeleteGroup({ ...props.scope }, target.id);
        if (!current(context)) return;
        deleteTarget.value = null;
        emit('changed');
        void loadGroups();
    } catch (cause) {
        if (!current(context)) return;
        actionError.value = formatAssetError(cause, t('assets.groups.request_failed'), key => t(key));
        contextError(cause);
    } finally { if (current(context)) deleting.value = false; }
}
watch(() => [props.visible, contextKey.value] as const, () => {
    contextVersion += 1; listVersion += 1; clearEditor();
    groups.value = []; deleteTarget.value = null; loadError.value = ''; actionError.value = '';
    loading.value = false; deleting.value = false; highlightedId.value = props.focusId ?? null;
    if (props.visible) void loadGroups();
}, { immediate: true });
watch(() => props.focusId, id => { highlightedId.value = id ?? null; });
const displayedGroups = computed(() => highlightedId.value
    ? [...groups.value].sort((a, b) => Number(b.id === highlightedId.value) - Number(a.id === highlightedId.value)) : groups.value);
onBeforeUnmount(() => { disposed = true; contextVersion += 1; listVersion += 1; });
</script>

<template>
    <Dialog :visible="visible" modal :header="t('assets.groups.title')"
        :style="{ width: '48rem', maxWidth: 'calc(100vw - 2rem)' }" :dismissableMask="false"
        @update:visible="value => { if (!value) close(); }">
        <div class="group-manager">
            <p class="scope-identity">{{ t('assets.groups.node_context') }} <strong>{{ nodeLabel || scope.machine_id }}</strong>
                <span v-if="ownerLabel" class="muted"> · {{ ownerLabel }}</span></p>
            <div class="toolbar">
                <p class="muted">{{ t('assets.groups.scope_hint') }}</p>
                <Button :label="t('assets.groups.add')" icon="pi pi-plus" size="small" :disabled="deleting || editorOpen" @click="startEditor()" />
            </div>
            <GatewayBindingEditor v-if="editorOpen && visible" :key="editorKey" class="editor" :api="api" :scope="scope"
                :group="editing" :initial-mode="editorMode" @cancel="clearEditor" @saved="saved" @context-error="emit('context-error', $event)" />
            <section v-if="deleteTarget" class="delete-confirm" role="alert">
                <p>{{ t('assets.groups.delete_prompt', { name: deleteTarget.name }) }}</p>
                <p class="muted">{{ t('assets.groups.delete_hint') }}</p>
                <p v-if="actionError" class="error">{{ actionError }}</p>
                <div class="toolbar actions">
                    <Button :label="t('assets.groups.cancel')" severity="secondary" :disabled="deleting" @click="deleteTarget = null" />
                    <Button :label="t('assets.groups.delete')" icon="pi pi-trash" severity="danger" :loading="deleting" :disabled="deleting" @click="confirmDelete" />
                </div>
            </section>
            <p v-if="loading" role="status">{{ t('assets.groups.loading') }}</p>
            <div v-else-if="loadError" role="alert">
                <p class="error">{{ loadError }}</p>
                <Button :label="t('assets.groups.retry')" icon="pi pi-refresh" size="small" severity="secondary" @click="loadGroups" />
            </div>
            <p v-else-if="!groups.length" class="muted">{{ t('assets.groups.empty') }}</p>
            <ul v-else class="group-list">
                <li v-for="group in displayedGroups" :key="group.id" class="group-item" :class="{ highlighted: group.id === highlightedId }">
                    <div class="information">
                        <div class="toolbar"><strong>{{ group.name }}</strong><code class="muted">{{ group.id.slice(0, 8) }}</code></div>
                        <span class="muted">{{ t(`assets.binding.state.${group.binding.state}`) }}</span>
                        <p v-if="group.binding.reason" class="notice">{{ bindingReason(group.binding.reason) }}</p>
                        <p v-if="group.binding.instance_id" class="muted">{{ group.binding.snapshot?.display_name }} · <code>{{ group.binding.instance_id.slice(0, 8) }}</code>
                            · {{ t('assets.binding.instanceIp') }}: {{ group.overlay_ip || '—' }}</p>
                        <template v-if="group.binding.snapshot">
                            <code v-for="rule in group.binding.snapshot.rules" :key="rule.rule_key">{{ rule.real_cidr }}<template v-if="rule.mapped_cidr"> → {{ rule.mapped_cidr }}</template></code>
                        </template>
                        <template v-else><code v-for="subnet in group.subnets" :key="subnet">{{ subnet }}</code></template>
                        <p v-if="group.remarks" class="muted remarks">{{ group.remarks }}</p>
                    </div>
                    <div class="row-actions">
                        <Button :label="t(group.binding.state === 'legacy_unbound' ? 'assets.binding.attach' : 'assets.binding.review')" text size="small"
                            :disabled="deleting || editorOpen" @click="startEditor(group, 'binding')" />
                        <Button icon="pi pi-pencil" text severity="secondary" :disabled="deleting || editorOpen"
                            :aria-label="t('assets.groups.edit_named', { name: group.name })" @click="startEditor(group)" />
                        <Button icon="pi pi-trash" text severity="danger" :disabled="deleting || editorOpen"
                            :aria-label="t('assets.groups.delete_named', { name: group.name })" @click="requestDelete(group)" />
                    </div>
                </li>
            </ul>
        </div>
        <template #footer><Button :label="t('assets.groups.close')" severity="secondary" @click="close" /></template>
    </Dialog>
</template>

<style scoped>
.group-manager,.information,.delete-confirm { display:grid; gap:.6rem; min-width:0; }
.group-manager { gap:1rem; } p { margin:0; overflow-wrap:anywhere; }
.toolbar,.row-actions { display:flex; align-items:center; gap:.5rem; flex-wrap:wrap; }
.actions { justify-content:flex-end; }
.editor,.delete-confirm { padding:1rem; border:1px solid var(--p-content-border-color,#e2e8f0); border-radius:.75rem; }
.group-list { margin:0; padding:0; list-style:none; }
.group-item { display:flex; justify-content:space-between; gap:.75rem; padding:.85rem .5rem; border-bottom:1px solid var(--p-content-border-color,#e2e8f0); border-radius:.5rem; }
.group-item.highlighted { background:var(--p-content-hover-background,#f1f5f9); outline:1px solid var(--p-primary-color,#3b82f6); }
.row-actions { align-self:flex-start; flex-shrink:0; }
.information { overflow-wrap:anywhere; }
.remarks { white-space:pre-wrap; }
.muted { color:var(--p-text-muted-color,#64748b); font-size:.8125rem; }
.notice { color:var(--p-orange-700,#9a3412); font-size:.8125rem; }
.error { color:var(--p-red-600,#dc2626); }
@media(max-width:560px) { .group-item { flex-direction:column; } }
</style>
