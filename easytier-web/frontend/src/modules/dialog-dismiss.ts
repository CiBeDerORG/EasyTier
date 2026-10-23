import type { DialogPassThroughMethodOptions, DialogPassThroughOptions } from 'primevue/dialog';

// One shared Web-console rule, including dialogs opened by DialogService and
// dialogs from frontend-lib. Emit the same event as the header close button;
// the owning form still handles draft disposal and in-flight request fencing.
const outsidePointers = new WeakMap<EventTarget, number>();

export const consoleDialogPassThrough: DialogPassThroughOptions = {
    mask: ({ instance, props }: DialogPassThroughMethodOptions<unknown>) => {
        // Existing dismissable dialogs keep PrimeVue's native handler. Respect
        // closable=false while saving and on a parent with a nested editor open.
        const canDismiss = () => props.visible === true && props.modal === true
            && props.closable !== false && !props.dismissableMask;
        return {
            onPointerdown: (event: PointerEvent) => {
                const mask = event.currentTarget;
                if (!mask) return;
                outsidePointers.delete(mask);
                if (event.button === 0 && event.isPrimary !== false
                    && event.target === mask && canDismiss()) {
                    outsidePointers.set(mask, event.pointerId);
                }
            },
            onPointerup: (event: PointerEvent) => {
                const mask = event.currentTarget;
                if (!mask) return;
                const startedOutside = outsidePointers.get(mask) === event.pointerId;
                outsidePointers.delete(mask);
                // Starting inside a form/map and releasing on its mask is a
                // drag, not an outside click. Child/popover events do not match.
                if (startedOutside && event.button === 0 && event.isPrimary !== false
                    && event.target === mask && canDismiss()) {
                    instance.$emit('update:visible', false);
                }
            },
            onPointercancel: (event: PointerEvent) => {
                if (event.currentTarget) outsidePointers.delete(event.currentTarget);
            },
        };
    },
};
