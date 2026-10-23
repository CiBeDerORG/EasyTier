import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import ApiClient from '../modules/api';
import { getAssetError } from './useAssetMutation';
import type { AssetAccount, AssetContextResponse, AssetDevice, AssetDeviceFilters, AssetNode, AssetScope, DeviceType, GatewayGroup } from '../types/node-assets';

const blankFilters = (): AssetDeviceFilters => ({ group_id: null, type_id: null, keyword: '' });
const message = (error: unknown) => (error as { response?: { data?: { message?: string } }; message?: string })?.response?.data?.message
    || (error as { message?: string })?.message || 'Unable to load assets';
type RetryTarget = { kind: 'initialize' } | { kind: 'route'; useMemory: boolean }
    | { kind: 'devices'; page: number; locateId?: string } | { kind: 'catalogs' } | { kind: 'refresh' };

/** Owns only manual-asset state. No timers or instance/session subscriptions. */
export function useNodeAssetsPage(api: ApiClient) {
    const route = useRoute();
    const router = useRouter();
    const context = ref<AssetContextResponse | null>(null);
    const accounts = ref<AssetAccount[]>([]);
    const ownerId = ref<number | null>(null);
    const nodeId = ref<string | null>(null);
    const nodes = ref<AssetNode[]>([]);
    const groups = ref<GatewayGroup[]>([]);
    const types = ref<DeviceType[]>([]);
    const rows = ref<AssetDevice[]>([]);
    const total = ref(0);
    const page = ref(1);
    const pageSize = ref(20);
    const expandedIds = ref<string[]>([]);
    const draft = reactive(blankFilters());
    const applied = ref(blankFilters());
    const contextVersion = ref(0);
    const booting = ref(true);
    const loading = ref(false);
    const error = ref('');
    const listError = ref(false);
    const hasLoadedDevices = ref(false);
    const invalidNode = ref(false);
    const scope = computed<AssetScope | null>(() => ownerId.value !== null && nodeId.value
        ? { owner_user_id: ownerId.value, machine_id: nodeId.value } : null);
    const selectedNode = computed(() => nodes.value.find(node => node.machine_id === nodeId.value) ?? null);
    const ownerName = computed(() => accounts.value.find(account => account.id === ownerId.value)?.username ?? '');
    let disposed = false;
    let listRequest = 0;
    let catalogRequest = 0;
    let nodesRequest = 0;
    let refreshRequest = 0;
    let retryTarget: RetryTarget | null = null;
    let processedRoute = '';
    let contextRecovery: Promise<void> | null = null;
    const routeKey = () => JSON.stringify([route.query.owner ?? null, route.query.node ?? null]);
    const storageKey = () => `easytier.assets.last-node.${context.value?.actor.id}`;

    const remember = (value: AssetScope) => {
        try { localStorage.setItem(storageKey(), JSON.stringify(value)); } catch { /* storage is optional */ }
    };
    const remembered = (): Partial<Record<keyof AssetScope, unknown>> | null => {
        try {
            const value: unknown = JSON.parse(localStorage.getItem(storageKey()) || 'null');
            return value !== null && typeof value === 'object' && !Array.isArray(value)
                ? value as Partial<Record<keyof AssetScope, unknown>> : null;
        } catch { return null; }
    };
    const clearRemembered = () => { try { localStorage.removeItem(storageKey()); } catch { /* optional */ } };
    const clearError = () => { error.value = ''; retryTarget = null; };
    const reportError = (cause: unknown, target: RetryTarget) => {
        error.value = message(cause); retryTarget = target;
    };
    function resetPageState() {
        ++listRequest; ++catalogRequest; ++nodesRequest; ++refreshRequest;
        rows.value = []; groups.value = []; types.value = []; nodes.value = [];
        expandedIds.value = []; total.value = 0; page.value = 1;
        Object.assign(draft, blankFilters()); applied.value = blankFilters();
        nodeId.value = null; ownerId.value = null; invalidNode.value = false;
        loading.value = false; listError.value = false; hasLoadedDevices.value = false;
        clearError();
    }

    async function replaceContextRoute(owner: number | null) {
        const query = owner === null ? {} : { owner: String(owner) };
        processedRoute = JSON.stringify([owner === null ? null : String(owner), null]);
        if (routeKey() !== processedRoute) await router.replace({ name: 'nodeAssets', query });
    }

    /** Only confirmed loss of access clears data. A resource 404 may concern a
     * device/type/instance rather than the node, so verify the node first. */
    async function handleContextFailure(cause: unknown): Promise<boolean> {
        const detail = getAssetError(cause);
        if (![401, 403, 404].includes(detail.status ?? 0)) return false;
        if (contextRecovery) { await contextRecovery; return true; }
        const generation = contextVersion.value;
        const owner = ownerId.value;
        const node = nodeId.value;
        const isScopeMissing = (code: string) => code === 'asset_owner_not_found' || code === 'asset_node_not_found';
        if (detail.status === 404 && !isScopeMissing(detail.code)) {
            if (owner === null || !node) return false;
            try {
                const result = await api.assetsNodes(owner);
                if (disposed || generation !== contextVersion.value) return true;
                if (result.items.some(item => item.machine_id === node)) return false;
            } catch (probeCause) {
                if (disposed || generation !== contextVersion.value) return true;
                const probe = getAssetError(probeCause);
                if (probe.status !== 401 && probe.status !== 403 && !isScopeMissing(probe.code)) return false;
            }
        }
        if (disposed || generation !== contextVersion.value) return true;
        clearRemembered();
        const recoveryGeneration = ++contextVersion.value;
        resetPageState();
        context.value = null; accounts.value = []; invalidNode.value = true; booting.value = true;
        contextRecovery = (async () => {
            try {
                // Clear the invalid deep link before retrying authentication.
                await replaceContextRoute(null);
                const fresh = await api.assetsContext();
                if (disposed || recoveryGeneration !== contextVersion.value) return;
                const allowed = fresh.available
                    ? fresh.can_manage_others ? (await api.assetsAccounts()).items : [fresh.actor]
                    : [];
                if (disposed || recoveryGeneration !== contextVersion.value) return;
                context.value = fresh; accounts.value = allowed;
                if (!fresh.available) return;
                const allowedOwner = allowed.some(account => account.id === owner) ? owner
                    : fresh.can_manage_others ? null : fresh.actor.id;
                ownerId.value = allowedOwner;
                await replaceContextRoute(allowedOwner);
                if (disposed || recoveryGeneration !== contextVersion.value || allowedOwner === null) return;
                const [nodeResult, typeResult] = await Promise.all([
                    api.assetsNodes(allowedOwner), api.assetsTypes(allowedOwner),
                ]);
                if (disposed || recoveryGeneration !== contextVersion.value) return;
                nodes.value = nodeResult.items; types.value = typeResult.items;
            } catch (recoveryCause) {
                if (!disposed && recoveryGeneration === contextVersion.value) {
                    reportError(recoveryCause, { kind: 'initialize' });
                }
            } finally {
                if (!disposed && recoveryGeneration === contextVersion.value) booting.value = false;
            }
        })();
        try { await contextRecovery; } finally { contextRecovery = null; }
        return true;
    }

    async function loadDevices(targetPage = page.value, locateId?: string) {
        const current = scope.value;
        if (!current) return;
        const generation = contextVersion.value;
        const request = ++listRequest;
        loading.value = true;
        listError.value = false;
        clearError();
        try {
            const result = await api.assetsDevices(current, {
                page: targetPage, page_size: pageSize.value,
                group_id: applied.value.group_id ?? undefined,
                type_id: applied.value.type_id ?? undefined,
                keyword: applied.value.keyword.trim() || undefined,
                locate_id: locateId,
            });
            if (disposed || generation !== contextVersion.value || request !== listRequest) return;
            rows.value = result.items;
            total.value = result.total;
            page.value = result.page;
            pageSize.value = result.page_size;
            hasLoadedDevices.value = true;
            expandedIds.value = expandedIds.value.filter(id => result.items.some(device => device.id === id));
            if (locateId && result.items.some(device => device.id === locateId)) {
                if (!expandedIds.value.includes(locateId)) expandedIds.value.push(locateId);
                await nextTick();
                if (!disposed && generation === contextVersion.value && request === listRequest) {
                    document.querySelector(`[data-asset-device-id="${locateId}"]`)?.scrollIntoView({ block: 'nearest' });
                }
            }
        } catch (cause) {
            if (!disposed && generation === contextVersion.value && request === listRequest) {
                if (await handleContextFailure(cause)) return;
                if (disposed || generation !== contextVersion.value || request !== listRequest) return;
                // Keep the last successful snapshot; a failed query is not an empty result.
                listError.value = true;
                reportError(cause, { kind: 'devices', page: targetPage, locateId });
            }
        } finally {
            if (!disposed && generation === contextVersion.value && request === listRequest) loading.value = false;
        }
    }

    async function applyRoute(useMemory = false) {
        const activeContext = context.value;
        if (!activeContext?.available) return;
        processedRoute = routeKey();
        const generation = ++contextVersion.value;
        resetPageState();
        booting.value = true;
        const explicit = route.query.owner !== undefined || route.query.node !== undefined;
        const memory = useMemory && !explicit ? remembered() : null;
        const rawOwner = route.query.owner ?? memory?.owner_user_id;
        let targetOwner = rawOwner === undefined || rawOwner === null
            ? (activeContext.can_manage_others ? null : activeContext.actor.id) : Number(rawOwner);
        let targetNode = typeof route.query.node === 'string' ? route.query.node
            : typeof memory?.machine_id === 'string' ? memory.machine_id : null;
        try {
            if (memory && (typeof memory.owner_user_id !== 'number' || !Number.isSafeInteger(memory.owner_user_id)
                || !accounts.value.some(account => account.id === memory.owner_user_id))) {
                clearRemembered();
                invalidNode.value = true;
                targetOwner = activeContext.can_manage_others ? null : activeContext.actor.id;
                targetNode = null;
            }
            if (targetOwner === null) return;
            if (!Number.isSafeInteger(targetOwner) || !accounts.value.some(account => account.id === targetOwner)) {
                invalidNode.value = true;
                clearRemembered();
                if (activeContext.can_manage_others) { await replaceContextRoute(null); return; }
                // A stale or unauthorized deep link must still allow selecting
                // one of the signed-in user's own nodes.
                targetOwner = activeContext.actor.id;
                targetNode = null;
                await replaceContextRoute(targetOwner);
                if (disposed || generation !== contextVersion.value) return;
            }
            ownerId.value = targetOwner;
            const [nodeResult, typeResult] = await Promise.all([
                api.assetsNodes(targetOwner), api.assetsInitializeTypes(targetOwner),
            ]);
            if (disposed || generation !== contextVersion.value) return;
            nodes.value = nodeResult.items;
            types.value = typeResult.items;
            if (!targetNode) return;
            if (!nodes.value.some(node => node.machine_id === targetNode)) {
                invalidNode.value = true;
                clearRemembered();
                await replaceContextRoute(targetOwner);
                return;
            }
            nodeId.value = targetNode;
            const current = scope.value!;
            const groupResult = await api.assetsGroups(current);
            if (disposed || generation !== contextVersion.value) return;
            groups.value = groupResult.items;
            remember(current);
            // Keep only the node's necessary owner/identity in the deep link.
            const query = { owner: String(targetOwner), node: targetNode };
            processedRoute = JSON.stringify([query.owner, query.node]);
            if (routeKey() !== processedRoute) await router.replace({ name: 'nodeAssets', query });
            if (!disposed && generation === contextVersion.value) await loadDevices(1);
        } catch (cause) {
            if (!disposed && generation === contextVersion.value && !await handleContextFailure(cause)
                && !disposed && generation === contextVersion.value) reportError(cause, { kind: 'route', useMemory });
        } finally {
            if (!disposed && generation === contextVersion.value) booting.value = false;
        }
    }

    async function initialize() {
        const generation = ++contextVersion.value;
        resetPageState();
        context.value = null; accounts.value = []; booting.value = true;
        try {
            const result = await api.assetsContext();
            if (disposed || generation !== contextVersion.value) return;
            if (!result.available) { context.value = result; booting.value = false; return; }
            const accountItems = result.can_manage_others ? (await api.assetsAccounts()).items : [result.actor];
            if (disposed || generation !== contextVersion.value) return;
            accounts.value = accountItems;
            context.value = result;
            await applyRoute(true);
        } catch (cause) {
            if (!disposed && generation === contextVersion.value) {
                reportError(cause, { kind: 'initialize' }); booting.value = false;
            }
        }
    }

    const selectOwner = (owner: number | null) => {
        // Clearing the account is intentional; a bare sidebar link may restore memory.
        if (owner === null) clearRemembered();
        return router.replace({ name: 'nodeAssets', query: owner === null ? {} : { owner: String(owner) } });
    };
    const selectNode = (node: string | null) => router.replace({ name: 'nodeAssets', query: {
        ...(ownerId.value !== null ? { owner: String(ownerId.value) } : {}), ...(node ? { node } : {}),
    } });
    const query = async () => {
        applied.value = { ...draft }; expandedIds.value = []; await loadDevices(1);
    };
    const matches = (device: AssetDevice) => {
        const filter = applied.value;
        const word = filter.keyword.trim().toLowerCase();
        return (!filter.group_id || device.gateway_group_id === filter.group_id)
            && (!filter.type_id || device.device_type_id === filter.type_id)
            && (!word || [device.name, device.model, device.overlay_ip, device.lan_ip].some(value => value?.toLowerCase().includes(word)));
    };
    async function refreshNodes() {
        const owner = ownerId.value, generation = contextVersion.value;
        if (owner === null) return;
        const request = ++nodesRequest;
        try {
            const result = await api.assetsNodes(owner);
            if (!disposed && generation === contextVersion.value && request === nodesRequest) {
                if (nodeId.value && !result.items.some(node => node.machine_id === nodeId.value)) {
                    await handleContextFailure({ status: 404, code: 'asset_node_not_found' });
                    return;
                }
                nodes.value = result.items;
            }
        } catch (cause) {
            if (!disposed && generation === contextVersion.value && request === nodesRequest
                && !await handleContextFailure(cause) && generation === contextVersion.value) throw cause;
        }
    }
    async function refreshCatalogs() {
        const current = scope.value, generation = contextVersion.value;
        if (!current) return;
        const request = ++catalogRequest;
        try {
            const [groupResult, typeResult] = await Promise.all([api.assetsGroups(current), api.assetsTypes(current.owner_user_id)]);
            if (disposed || generation !== contextVersion.value || request !== catalogRequest) return;
            groups.value = groupResult.items; types.value = typeResult.items;
            for (const filter of [draft, applied.value]) {
                if (filter.group_id && !groups.value.some(group => group.id === filter.group_id)) filter.group_id = null;
                if (filter.type_id && !types.value.some(type => type.id === filter.type_id)) filter.type_id = null;
            }
            await loadDevices();
        } catch (cause) {
            if (!disposed && generation === contextVersion.value && request === catalogRequest) {
                if (await handleContextFailure(cause)) return;
                if (disposed || generation !== contextVersion.value || request !== catalogRequest) return;
                reportError(cause, { kind: 'catalogs' });
            }
        }
    }
    async function refreshPage() {
        if (!scope.value) return;
        const generation = contextVersion.value, request = ++refreshRequest;
        clearError();
        const [nodeResult] = await Promise.allSettled([refreshNodes(), refreshCatalogs()]);
        if (disposed || generation !== contextVersion.value || request !== refreshRequest) return;
        if (nodeResult.status === 'rejected') reportError(nodeResult.reason, { kind: 'refresh' });
        else if (error.value) retryTarget = { kind: 'refresh' };
    }
    async function retry() {
        const target = retryTarget;
        if (!target) return;
        switch (target.kind) {
            case 'initialize': await initialize(); break;
            case 'route': await applyRoute(target.useMemory); break;
            case 'devices': await loadDevices(target.page, target.locateId); break;
            case 'catalogs': await refreshCatalogs(); break;
            case 'refresh': await refreshPage(); break;
        }
    }
    async function afterDeviceSaved(device: AssetDevice, created: boolean) {
        if (!scope.value || device.owner_user_id !== scope.value.owner_user_id || device.machine_id !== scope.value.machine_id) return;
        if (created && !matches(device)) { Object.assign(draft, blankFilters()); applied.value = blankFilters(); }
        await loadDevices(created ? 1 : page.value, created ? device.id : undefined);
        try { await refreshNodes(); } catch { /* List operation remains successful; retry counters on refresh. */ }
    }
    function expand(event: { id: string; expanded: boolean }) {
        expandedIds.value = event.expanded ? Array.from(new Set([...expandedIds.value, event.id])) : expandedIds.value.filter(id => id !== event.id);
    }
    async function afterDeviceDeleted() {
        const target = rows.value.length === 1 && page.value > 1 ? page.value - 1 : page.value;
        await loadDevices(target);
        try { await refreshNodes(); } catch { /* no fabricated count */ }
    }
    watch(() => route.fullPath, () => {
        if (context.value?.available && routeKey() !== processedRoute) {
            const useMemory = route.query.owner === undefined && route.query.node === undefined;
            void applyRoute(useMemory);
        }
    });
    onMounted(initialize);
    onBeforeUnmount(() => { disposed = true; ++contextVersion.value; ++listRequest; ++catalogRequest; ++nodesRequest; ++refreshRequest; });
    return { context, accounts, ownerId, nodeId, ownerName, nodes, selectedNode, groups, types, rows,
        total, page, pageSize, expandedIds, draft, applied, contextVersion, booting, loading, error, listError, hasLoadedDevices,
        invalidNode, scope, initialize, selectOwner, selectNode, query, loadDevices, refreshCatalogs,
        refreshNodes, refreshPage, retry, afterDeviceSaved, afterDeviceDeleted, expand, handleContextFailure };
}
