import { computed, onBeforeUnmount, ref, shallowRef, watch } from 'vue';
import type ApiClient from '../modules/api';
import type { AddressPreviewInput, AssetScope, InstancePreview } from '../types/node-assets';
import { getAssetError } from './useAssetMutation';

/** A preview is valid only for its exact scope, subject and draft selection. */
export function useInstanceAddressPreview(options: {
    api: () => ApiClient;
    scope: () => AssetScope;
    input: () => AddressPreviewInput | null;
    enabled: () => boolean;
    onContextError?: (cause: unknown) => void;
}) {
    const preview = shallowRef<InstancePreview | null>(null);
    const pending = ref(false);
    const error = shallowRef<unknown>(null);
    let generation = 0;
    let disposed = false;
    const signature = computed(() => JSON.stringify([options.scope(), options.input(), options.enabled()]));
    function invalidate() {
        generation += 1;
        preview.value = null;
        pending.value = false;
        error.value = null;
    }
    watch(signature, invalidate, { flush: 'sync' });
    async function run() {
        const input = options.input();
        if (!input || !options.enabled() || pending.value) return;
        const version = ++generation;
        const original = signature.value;
        const current = () => !disposed && version === generation && original === signature.value;
        pending.value = true;
        preview.value = null;
        error.value = null;
        try {
            const result = await options.api().previewAssetAddress({ ...options.scope() }, input);
            if (current()) preview.value = result;
        } catch (cause) {
            if (!current()) return;
            error.value = cause;
            if ([401, 403, 404].includes(getAssetError(cause).status ?? 0)) options.onContextError?.(cause);
        } finally {
            if (current()) pending.value = false;
        }
    }
    onBeforeUnmount(() => { disposed = true; invalidate(); });
    return { preview, pending, error, run, invalidate };
}
