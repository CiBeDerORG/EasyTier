import { computed, inject, onMounted, onScopeDispose, provide, readonly, ref, watch, type InjectionKey, type Ref } from 'vue';

const storageKey = 'webConsole.autoRefresh';
const autoRefreshKey: InjectionKey<Readonly<Ref<boolean>>> = Symbol('web-console-auto-refresh');

function storedAutoRefresh(): boolean {
    try {
        return typeof window === 'undefined' || window.localStorage.getItem(storageKey) !== 'false';
    } catch {
        return true;
    }
}

/** The console owns the preference; native hosts keep their existing refresh behavior. */
export function provideAutoRefresh() {
    const enabled = ref(storedAutoRefresh());
    provide(autoRefreshKey, readonly(enabled));
    watch(enabled, value => {
        try {
            window.localStorage.setItem(storageKey, String(value));
        } catch {
            // The switch still works in memory if browser storage is unavailable.
        }
    }, { flush: 'sync' });
    const syncStorage = (event: StorageEvent) => {
        if (event.key === storageKey || event.key === null) enabled.value = storedAutoRefresh();
    };
    onMounted(() => window.addEventListener('storage', syncStorage));
    onScopeDispose(() => window.removeEventListener('storage', syncStorage));
    return enabled;
}

export function useAutoRefresh(): Readonly<Ref<boolean>> {
    return inject(autoRefreshKey, ref(true));
}

/** Load once on entry, then poll only while the shared switch and local view are active. */
export function useAutoRefreshTask(
    task: (canContinue: () => boolean) => Promise<void>,
    interval: number,
    options: { active?: () => boolean; key?: () => unknown; enabled?: Readonly<Ref<boolean>> } = {},
) {
    const enabled = options.enabled ?? useAutoRefresh();
    const active = computed(() => options.active?.() ?? true);
    let mounted = false;
    let disposed = false;
    let inFlight = false;
    let generation = 0;
    let queuedInitial = false;
    let queuedRefresh = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const clearTimer = () => {
        clearTimeout(timer);
        timer = undefined;
    };
    const schedule = (delay = interval) => {
        clearTimer();
        if (!mounted || disposed || !active.value || !enabled.value) return;
        timer = setTimeout(() => { void run(false); }, delay);
    };
    const run = async (initial: boolean) => {
        if (!mounted || disposed || !active.value || (!initial && !enabled.value)) return;
        if (inFlight) {
            queuedInitial ||= initial;
            queuedRefresh ||= !initial;
            return;
        }
        inFlight = true;
        const version = generation;
        try {
            await task(() => !disposed && mounted && active.value && version === generation
                && (initial || enabled.value));
        } catch (error) {
            // One failed request must not silently disable all later refreshes.
            console.error('Automatic refresh failed', error);
        } finally {
            inFlight = false;
            const initialQueued = queuedInitial;
            const refreshQueued = queuedRefresh;
            queuedInitial = false;
            queuedRefresh = false;
            if (initialQueued || refreshQueued) void run(initialQueued);
            else schedule();
        }
    };

    watch(enabled, value => {
        clearTimer();
        if (value) schedule(0);
        else queuedRefresh = false;
    }, { flush: 'sync' });
    watch([active, () => options.key?.()], ([value]) => {
        ++generation;
        clearTimer();
        queuedInitial = false;
        queuedRefresh = false;
        if (!mounted || !value) return;
        void run(true);
    });
    onMounted(() => {
        mounted = true;
        void run(true);
    });
    onScopeDispose(() => {
        disposed = true;
        mounted = false;
        ++generation;
        queuedInitial = false;
        queuedRefresh = false;
        clearTimer();
    });
}
