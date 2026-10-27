/**
 * T24: Ordering for visible automation child dialogs, across PhoneCards.
 * Only window ordering is kept; script data, stream and action permissions
 * always belong to their existing authoritative owners.
 */
import { computed, ref } from 'vue';

const sequence = ref(0);
const visibleDialogs = new Map<symbol, number>();

export function useAutomationFloatingDialogOrder() {
    const key = Symbol('automation-child-dialog');
    const zIndex = computed(() => {
        // Invalidate computed z-index when another dialog is raised/closed.
        void sequence.value;
        return 100 + (visibleDialogs.get(key) ?? 0);
    });
    function raise() {
        visibleDialogs.set(key, ++sequence.value);
    }
    function unregister() {
        if (visibleDialogs.delete(key)) sequence.value += 1;
    }
    function isTopmost() {
        const own = visibleDialogs.get(key);
        if (own === undefined) return false;
        for (const order of visibleDialogs.values()) if (order > own) return false;
        return true;
    }
    return { zIndex, raise, unregister, isTopmost };
}
