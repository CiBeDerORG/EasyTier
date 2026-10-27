/**
 * T21/T23: position-only floating layer on the EXISTING PhoneCard video surface.
 *
 * Fullscreen and card modes have independent coordinates. The same DOM subtree,
 * script state and MediaStream stay mounted across browser Fullscreen changes.
 * This module never creates a second player, session, action lease or store.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type CSSProperties, type Ref } from 'vue';
import type { AutomationFloatingPoint } from './automation-floating-contract';

type ActiveDrag = {
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startPosition: AutomationFloatingPoint;
};

const EDGE = 8;
const NORMAL_POSITION = { x: 12, y: 44 } as const;
const FULLSCREEN_TOP = 44;

export interface AutomationFloatingLayerOptions {
    /** A child dialog starts offset from its parent rather than obscuring its title. */
    normalStart?: AutomationFloatingPoint;
    fullscreenTop?: number;
}
export function useAutomationFloatingLayer(fullscreen: Readonly<Ref<boolean>>, options: AutomationFloatingLayerOptions = {}) {
    const host = ref<HTMLElement | null>(null);
    const panel = ref<HTMLElement | null>(null);
    const normalPosition = ref<AutomationFloatingPoint>({ ...(options.normalStart ?? NORMAL_POSITION) });
    const fullscreenPosition = ref<AutomationFloatingPoint>({ x: EDGE, y: options.fullscreenTop ?? FULLSCREEN_TOP });
    const position = computed<AutomationFloatingPoint>({
        get: () => fullscreen.value ? fullscreenPosition.value : normalPosition.value,
        set: value => {
            if (fullscreen.value) fullscreenPosition.value = value;
            else normalPosition.value = value;
        },
    });
    let fullscreenPlaced = false;
    let active: ActiveDrag | null = null;
    let resizeObserver: ResizeObserver | null = null;

    function constrain(): void {
        const container = host.value, windowElement = panel.value;
        if (!container || !windowElement || !container.clientWidth || !container.clientHeight ||
            !windowElement.offsetWidth || !windowElement.offsetHeight) return;
        const maximumX = Math.max(0, container.clientWidth - windowElement.offsetWidth - EDGE);
        const maximumY = Math.max(0, container.clientHeight - windowElement.offsetHeight - EDGE);
        const x = Math.max(0, Math.min(position.value.x, maximumX));
        const y = Math.max(0, Math.min(position.value.y, maximumY));
        if (x !== position.value.x || y !== position.value.y) position.value = { x, y };
    }

    function pointerDown(event: PointerEvent): void {
        if (event.button !== 0 || !event.isPrimary || !host.value || !panel.value) return;
        active = {
            pointerId: event.pointerId, startClientX: event.clientX, startClientY: event.clientY,
            startPosition: { ...position.value },
        };
        (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
        event.stopPropagation();
        event.preventDefault();
    }

    function pointerMove(event: PointerEvent): void {
        if (!active || active.pointerId !== event.pointerId) return;
        const container = host.value, windowElement = panel.value;
        if (!container || !windowElement) return;
        event.stopPropagation();
        event.preventDefault();
        const maximumX = Math.max(0, container.clientWidth - windowElement.offsetWidth - EDGE);
        const maximumY = Math.max(0, container.clientHeight - windowElement.offsetHeight - EDGE);
        const x = Math.max(0, Math.min(active.startPosition.x + event.clientX - active.startClientX, maximumX));
        const y = Math.max(0, Math.min(active.startPosition.y + event.clientY - active.startClientY, maximumY));
        position.value = { x, y };
    }

    function pointerEnd(event: PointerEvent): void {
        if (active?.pointerId !== event.pointerId) return;
        active = null;
        const target = event.currentTarget as HTMLElement;
        if (target.hasPointerCapture(event.pointerId)) target.releasePointerCapture(event.pointerId);
        event.stopPropagation();
        constrain();
    }

    function observe(): void {
        resizeObserver?.disconnect();
        resizeObserver = null;
        if (typeof ResizeObserver !== 'undefined') {
            resizeObserver = new ResizeObserver(constrain);
            if (host.value) resizeObserver.observe(host.value);
            if (panel.value) resizeObserver.observe(panel.value);
        }
        constrain();
    }

    // Browser Fullscreen places the existing .phone-screen in the top layer.
    // Restore the previous location for EACH mode, instead of clamping the
    // fullscreen pixel coordinate into the original small card on exit.
    watch(fullscreen, async enabled => {
        active = null;
        await nextTick();
        if (enabled && !fullscreenPlaced && host.value && panel.value) {
            fullscreenPosition.value = {
                x: Math.max(EDGE, host.value.clientWidth - panel.value.offsetWidth - 12),
                y: options.fullscreenTop ?? FULLSCREEN_TOP,
            };
            fullscreenPlaced = true;
        }
        constrain();
    }, { flush: 'post' });
    watch([host, panel], observe, { flush: 'post' });
    onMounted(() => {
        window.addEventListener('resize', constrain);
        window.addEventListener('orientationchange', constrain);
        document.addEventListener('fullscreenchange', constrain);
        observe();
    });
    onBeforeUnmount(() => {
        active = null;
        resizeObserver?.disconnect();
        window.removeEventListener('resize', constrain);
        window.removeEventListener('orientationchange', constrain);
        document.removeEventListener('fullscreenchange', constrain);
    });
    /** Keyboard movement uses the same boundary constraints as pointer dragging. */
    function moveBy(dx: number, dy: number): void {
        position.value = { x: position.value.x + dx, y: position.value.y + dy };
        constrain();
    }
    const style = computed<CSSProperties>(() => ({
        left: `${position.value.x}px`,
        top: `${position.value.y}px`,
    }));
    return { host, panel, position, style, constrain, moveBy, pointerDown, pointerMove, pointerEnd };
}
