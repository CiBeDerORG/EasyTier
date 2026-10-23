<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import { Button, Column, DataTable, Dialog, Message, Skeleton, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import ApiClient from '../modules/api';
import type { DeleteManagedUserResult, ManagedUser, UserAdminContext } from '../types/user-admin';

const props = defineProps<{ api: ApiClient }>();
const emit = defineEmits<{ 'user-admin-context': [context: UserAdminContext | null] }>();
const { t, locale } = useI18n();
const context = ref<UserAdminContext | null>(null);
const users = ref<ManagedUser[]>([]);
const hasLoaded = ref(false);
const loading = ref(false);
const loadError = ref('');
const accessProblem = ref<'forbidden' | 'unavailable' | null>(null);
const deleting = ref(false);
const deleteTarget = ref<ManagedUser | null>(null);
const deleteError = ref('');
const result = ref<{ username: string; outcome: DeleteManagedUserResult } | null>(null);
const canManage = computed(() => context.value?.available === true && context.value.can_manage_users === true);
const deletedCount = computed(() => users.value.filter(user => user.deleted_at !== null).length);
const dialogVisible = computed({
    get: () => deleteTarget.value !== null,
    set: value => { if (!value && !deleting.value) { deleteTarget.value = null; deleteError.value = ''; } },
});
const runtimePending = computed(() => result.value !== null && (result.value.outcome.runtime_error
    || result.value.outcome.runtime === null || result.value.outcome.runtime.pending_nodes > 0));
let disposed = false;
let listRequest = 0;
let mutationRequest = 0;

type RequestError = { response?: { status?: number; data?: { code?: string; message?: string } } };
const details = (cause: unknown) => (cause as RequestError | null)?.response;
const errorMessage = (cause: unknown) => {
    const response = details(cause);
    const key: Record<string, string> = {
        user_admin_unavailable: 'unavailable',
        user_admin_forbidden: 'forbidden',
        user_not_found: 'user_not_found',
        user_delete_self: 'cannot_delete_self',
        user_last_admin: 'cannot_delete_last_admin',
        user_admin_database_error: 'operation_failed',
    };
    const messageKey = key[response?.data?.code ?? ''];
    return messageKey ? t(`user_admin.${messageKey}`) : response?.data?.message || t('user_admin.load_failed');
};

function clearSensitiveState(problem: 'forbidden' | 'unavailable') {
    ++listRequest;
    ++mutationRequest;
    context.value = null;
    users.value = [];
    hasLoaded.value = false;
    deleteTarget.value = null;
    deleteError.value = '';
    result.value = null;
    accessProblem.value = problem;
    loadError.value = '';
    loading.value = false;
    deleting.value = false;
    emit('user-admin-context', null);
}

function handleAccessFailure(cause: unknown): boolean {
    const response = details(cause);
    if (response?.data?.code === 'user_admin_unavailable') {
        clearSensitiveState('unavailable');
        return true;
    }
    if (response?.status === 401 || response?.status === 403) {
        clearSensitiveState('forbidden');
        return true;
    }
    return false;
}

async function loadUsers() {
    if (loading.value || disposed) return;
    const request = ++listRequest;
    const current = () => !disposed && request === listRequest;
    loading.value = true;
    loadError.value = '';
    try {
        const access = await props.api.userAdminContext();
        if (!current()) return;
        if (!access.available || !access.can_manage_users) {
            clearSensitiveState(access.available ? 'forbidden' : 'unavailable');
            emit('user-admin-context', access);
            return;
        }
        // A changed signed-in account must never inherit the prior account's rows or result.
        if (context.value && context.value.actor.id !== access.actor.id) {
            users.value = []; hasLoaded.value = false;
            deleteTarget.value = null; deleteError.value = ''; result.value = null;
        }
        context.value = access;
        accessProblem.value = null;
        emit('user-admin-context', access);
        const response = await props.api.listManagedUsers();
        if (!current()) return;
        users.value = response.items;
        hasLoaded.value = true;
    } catch (cause) {
        if (current() && !handleAccessFailure(cause)) loadError.value = errorMessage(cause);
    } finally {
        if (current()) loading.value = false;
    }
}

function openDelete(user: ManagedUser) {
    if (!canManage.value || loading.value || deleting.value || user.is_current_user
        || (!user.deleted_at && !user.can_delete)) return;
    deleteTarget.value = { ...user };
    deleteError.value = '';
}

async function confirmDelete() {
    const target = deleteTarget.value;
    if (!target || !canManage.value || deleting.value || target.is_current_user
        || (!target.deleted_at && !target.can_delete)) return;
    const request = ++mutationRequest;
    const current = () => !disposed && request === mutationRequest;
    deleting.value = true;
    deleteError.value = '';
    result.value = null;
    try {
        const outcome = await props.api.deleteManagedUser(target.id);
        if (!current()) return;
        result.value = { username: target.username, outcome };
        // Commit the acknowledged state locally even if the subsequent refresh fails.
        users.value = users.value.map(user => user.id === outcome.user_id ? {
            ...user, deleted_at: outcome.deleted_at, can_delete: false,
            disabled_network_instance_count: outcome.disabled_network_instance_count,
            network_instance_count: Math.max(user.network_instance_count, outcome.disabled_network_instance_count),
        } : user);
        deleteTarget.value = null;
        await loadUsers();
    } catch (cause) {
        if (!current() || handleAccessFailure(cause)) return;
        const status = details(cause)?.status;
        deleteError.value = status === undefined || status >= 500
            ? t('user_admin.delete_uncertain') : errorMessage(cause);
    } finally {
        if (current()) deleting.value = false;
    }
}

const deletionDate = (value: string) => {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale.value === 'cn' ? 'zh-CN' : 'en-US');
};
const actionLabel = (user: ManagedUser) => t(user.deleted_at ? 'user_admin.retry_disable' : 'user_admin.delete');
const protectedReason = (user: ManagedUser) => user.is_current_user ? t('user_admin.cannot_delete_self')
    : !user.deleted_at && !user.can_delete ? t('user_admin.cannot_delete_last_admin') : undefined;

onMounted(() => { void loadUsers(); });
onUnmounted(() => { disposed = true; ++listRequest; ++mutationRequest; });
</script>

<template>
    <div class="console-page user-management">
        <header class="page-heading">
            <div><h1>{{ t('user_admin.title') }}</h1><p class="muted user-subtitle">{{ t('user_admin.subtitle') }}</p></div>
            <Button :label="t('user_admin.refresh')" icon="pi pi-refresh" severity="secondary" outlined
                :loading="loading" :disabled="deleting" @click="loadUsers" />
        </header>

        <Message v-if="accessProblem" severity="warn" :closable="false">{{ t(`user_admin.${accessProblem}`) }}</Message>
        <Message v-if="loadError" severity="error" :closable="false">
            {{ loadError }}
            <span v-if="hasLoaded"> {{ t('user_admin.retained_rows') }}</span>
        </Message>
        <Message v-if="result" :severity="runtimePending ? 'warn' : 'success'" :closable="false">
            <div class="result-lines" role="status">
                <strong>{{ t(result.outcome.already_deleted ? 'user_admin.retry_success' : 'user_admin.delete_success', { username: result.username }) }}</strong>
                <span>{{ t('user_admin.configurations_disabled', { count: result.outcome.disabled_network_instance_count }) }}</span>
                <span v-if="result.outcome.runtime && !result.outcome.runtime_error">
                    {{ t('user_admin.runtime_summary', { confirmed: result.outcome.runtime.confirmed_nodes, pending: result.outcome.runtime.pending_nodes }) }}
                </span>
                <span v-if="runtimePending">{{ t('user_admin.runtime_pending') }}</span>
                <span>{{ t('user_admin.data_retained') }}</span>
            </div>
        </Message>

        <section v-if="canManage" class="console-panel" :aria-label="t('user_admin.title')" :aria-busy="loading">
            <div class="list-toolbar">
                <span v-if="hasLoaded" class="table-count">{{ t('user_admin.count', { count: users.length, deleted: deletedCount }) }}</span>
                <span class="muted">{{ t('user_admin.manual_refresh') }}</span>
            </div>
            <div v-if="!hasLoaded && loading" class="loading-rows"><Skeleton v-for="i in 3" :key="i" height="2rem" /></div>
            <div v-else-if="!hasLoaded || users.length === 0" class="console-empty-state">
                <p>{{ t(loadError ? 'user_admin.load_failed' : 'user_admin.no_users') }}</p>
                <Button v-if="loadError" :label="t('user_admin.refresh')" icon="pi pi-refresh" severity="secondary" @click="loadUsers" />
            </div>
            <template v-else>
                <DataTable :value="users" dataKey="id" class="console-table desktop-list" size="small" scrollable
                    :paginator="users.length > 20" :rows="20" tableStyle="min-width: 60rem">
                    <Column field="id" :header="t('user_admin.id')" style="width: 5rem" />
                    <Column field="username" :header="t('user_admin.username')">
                        <template #body="{ data }"><div class="user-name">{{ data.username }}<Tag v-if="data.is_current_user" :value="t('user_admin.current_user')" severity="secondary" /></div></template>
                    </Column>
                    <Column :header="t('user_admin.role')"><template #body="{ data }"><Tag :value="t(data.is_admin ? 'user_admin.administrator' : 'user_admin.regular_user')" :severity="data.is_admin ? 'info' : 'secondary'" /></template></Column>
                    <Column :header="t('user_admin.status')">
                        <template #body="{ data }"><Tag :value="t(data.deleted_at ? 'user_admin.deleted' : 'user_admin.active')" :severity="data.deleted_at ? 'secondary' : 'success'" /><time v-if="data.deleted_at" :datetime="data.deleted_at" class="deleted-time muted">{{ deletionDate(data.deleted_at) }}</time></template>
                    </Column>
                    <Column field="node_count" :header="t('user_admin.nodes')" />
                    <Column :header="t('user_admin.instances')"><template #body="{ data }">{{ data.disabled_network_instance_count }} / {{ data.network_instance_count }}</template></Column>
                    <Column :header="t('user_admin.actions')" frozen alignFrozen="right" style="min-width: 11rem">
                        <template #body="{ data }"><span :title="protectedReason(data)"><Button :label="actionLabel(data)" :icon="data.deleted_at ? 'pi pi-refresh' : 'pi pi-trash'" text size="small"
                            :severity="data.deleted_at ? 'secondary' : 'danger'" :aria-label="`${actionLabel(data)} ${data.username}`"
                            :disabled="loading || deleting || data.is_current_user || (!data.deleted_at && !data.can_delete)" @click="openDelete(data)" /></span></template>
                    </Column>
                </DataTable>
                <div class="mobile-list">
                    <article v-for="user in users" :key="user.id" class="mobile-list-item">
                        <div class="user-mobile-heading"><strong class="user-name">{{ user.username }}</strong><Tag :value="t(user.deleted_at ? 'user_admin.deleted' : 'user_admin.active')" :severity="user.deleted_at ? 'secondary' : 'success'" /></div>
                        <p class="muted user-details">{{ t('user_admin.id') }}: {{ user.id }} · {{ t(user.is_admin ? 'user_admin.administrator' : 'user_admin.regular_user') }}<span v-if="user.is_current_user"> · {{ t('user_admin.current_user') }}</span></p>
                        <dl class="user-counts"><div><dt>{{ t('user_admin.nodes') }}</dt><dd>{{ user.node_count }}</dd></div><div><dt>{{ t('user_admin.instances') }}</dt><dd>{{ user.disabled_network_instance_count }} / {{ user.network_instance_count }}</dd></div></dl>
                        <time v-if="user.deleted_at" :datetime="user.deleted_at" class="deleted-time muted">{{ deletionDate(user.deleted_at) }}</time>
                        <p v-if="protectedReason(user)" class="muted user-details">{{ protectedReason(user) }}</p>
                        <Button :label="actionLabel(user)" :icon="user.deleted_at ? 'pi pi-refresh' : 'pi pi-trash'" text size="small"
                            :severity="user.deleted_at ? 'secondary' : 'danger'" :aria-label="`${actionLabel(user)} ${user.username}`"
                            :disabled="loading || deleting || user.is_current_user || (!user.deleted_at && !user.can_delete)" @click="openDelete(user)" />
                    </article>
                </div>
            </template>
        </section>
        <div v-else-if="loading && !accessProblem" class="console-panel loading-rows"><Skeleton v-for="i in 3" :key="i" height="2rem" /></div>

        <Dialog v-model:visible="dialogVisible" modal class="console-dialog" :header="t(deleteTarget?.deleted_at ? 'user_admin.retry_title' : 'user_admin.delete_title')"
            :style="{ width: '32rem', maxWidth: 'calc(100vw - 2rem)' }" :closable="!deleting" :closeOnEscape="!deleting" :dismissableMask="false">
            <div v-if="deleteTarget" class="delete-content">
                <p class="user-name"><strong>{{ deleteTarget.username }}</strong><span class="muted">{{ t('user_admin.id') }}: {{ deleteTarget.id }}</span></p>
                <p>{{ t(deleteTarget.deleted_at ? 'user_admin.retry_description' : 'user_admin.delete_description') }}</p>
                <ul><li>{{ t('user_admin.effect_login') }}</li><li>{{ t('user_admin.effect_networks') }}</li><li>{{ t('user_admin.data_retained') }}</li></ul>
                <Message severity="warn" :closable="false">{{ t('user_admin.offline_note') }}</Message>
                <Message v-if="deleteError" severity="error" :closable="false">{{ deleteError }}</Message>
            </div>
            <template #footer>
                <Button :label="t('user_admin.cancel')" text severity="secondary" :disabled="deleting" @click="dialogVisible = false" />
                <Button :label="t(deleteTarget?.deleted_at ? 'user_admin.retry_disable' : 'user_admin.confirm_delete')"
                    :icon="deleteTarget?.deleted_at ? 'pi pi-refresh' : 'pi pi-trash'" :severity="deleteTarget?.deleted_at ? 'secondary' : 'danger'"
                    :loading="deleting" :disabled="!canManage || !deleteTarget" @click="confirmDelete" />
            </template>
        </Dialog>
    </div>
</template>

<style scoped>
.user-subtitle { margin: 0.5rem 0 0; line-height: 1.6; }
.user-name { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; overflow-wrap: anywhere; }
.user-mobile-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 1rem; }
.user-details { margin: 0.5rem 0; font-size: 0.8125rem; line-height: 1.6; }
.deleted-time { display: block; margin-top: 0.4rem; font-size: 0.75rem; }
.result-lines, .delete-content { display: flex; flex-direction: column; gap: 0.65rem; overflow-wrap: anywhere; }
.delete-content p { margin: 0; }
.delete-content ul { margin: 0; padding-left: 1.3rem; line-height: 1.8; }
.user-counts { display: flex; flex-wrap: wrap; gap: 0.75rem 1.5rem; margin: 0.75rem 0; }
.user-counts dt { color: var(--console-muted); font-size: 0.75rem; }
.user-counts dd { margin: 0.2rem 0 0; }
@media (max-width: 640px) {
    .user-management .page-heading { flex-wrap: wrap; }
}
</style>
