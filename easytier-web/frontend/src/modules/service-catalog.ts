import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import type ApiClient from './api';
import type { AssetAccount, AssetContextResponse, AssetScope, AssetServiceCatalogEntry } from '../types/node-assets';
import { serviceAccessLinks, serviceError } from './service-utils';

/** Service management and navigation read the same device-owned service records. */
export function useServiceCatalog(api: ApiClient, fallback: () => string) {
    const route = useRoute();
    const router = useRouter();
    const context = ref<AssetContextResponse>();
    const accounts = ref<AssetAccount[]>([]);
    const ownerId = ref<number | null>(null);
    const services = ref<AssetServiceCatalogEntry[]>();
    const loading = ref(false);
    const error = ref('');
    let generation = 0;
    let disposed = false;
    const available = computed(() => context.value?.available === true);
    const ownerQuery = computed(() => ownerId.value === null ? {} : { owner: String(ownerId.value) });

    async function load(reset = false) {
        const current = ++generation;
        const active = () => !disposed && current === generation;
        if (reset) { services.value = undefined; ownerId.value = null; }
        loading.value = true;
        error.value = '';
        try {
            const nextContext = await api.assetsContext();
            if (!active()) return;
            if (context.value?.actor.id !== nextContext.actor.id) services.value = undefined;
            context.value = nextContext;
            if (!nextContext.available) {
                services.value = undefined;
                ownerId.value = null;
                accounts.value = [];
                return;
            }
            const requested = typeof route.query.owner === 'string' && /^\d+$/.test(route.query.owner)
                ? Number(route.query.owner) : nextContext.actor.id;
            // Revoked administrator rights must never leave another owner's rows visible.
            if (!nextContext.can_manage_others && requested !== nextContext.actor.id) {
                services.value = undefined;
                ownerId.value = null;
                accounts.value = [nextContext.actor];
                await router.replace({ query: { owner: String(nextContext.actor.id) } });
                return;
            }
            if (ownerId.value !== requested) services.value = undefined;
            ownerId.value = requested;
            const [catalog, owners] = await Promise.all([
                api.assetsServiceCatalog(requested),
                nextContext.can_manage_others ? api.assetsAccounts() : Promise.resolve({ items: [nextContext.actor] }),
            ]);
            if (!active()) return;
            accounts.value = owners.items;
            services.value = catalog.items;
        } catch (cause) {
            if (!active()) return;
            const status = (cause as { response?: { status?: number } })?.response?.status;
            if (status === 401 || status === 403 || status === 404) { services.value = undefined; ownerId.value = null; }
            error.value = serviceError(cause, fallback());
        } finally {
            if (active()) loading.value = false;
        }
    }

    function selectOwner(value: number) {
        return router.replace({ query: { owner: String(value) } });
    }
    const isCurrentOwner = (owner: number) => !disposed && available.value && ownerId.value === owner;
    watch(() => route.query.owner, () => { void load(true); }, { immediate: true });
    onBeforeUnmount(() => { disposed = true; generation++; });
    return { context, accounts, ownerId, ownerQuery, services, loading, error, available, load, selectOwner, isCurrentOwner };
}

export function sortAssetServices(entries: AssetServiceCatalogEntry[]): AssetServiceCatalogEntry[] {
    return [...entries].sort((a, b) => a.node_name.localeCompare(b.node_name)
        || a.device_name.localeCompare(b.device_name) || a.name.localeCompare(b.name) || a.id.localeCompare(b.id));
}

export function matchesAssetService(service: AssetServiceCatalogEntry, query: string): boolean {
    return `${service.name} ${service.remarks || ''} ${serviceAccessLinks(service).map(link => link.url).join(' ')} ${service.node_name} ${service.device_name}`
        .toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
}

export function assetServiceScope(service: AssetScope): AssetScope {
    return { owner_user_id: service.owner_user_id, machine_id: service.machine_id };
}
