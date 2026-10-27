/**
 * T21: Position-only floating layer for a PhoneCard's existing video surface.
 *
 * No media session, ADB action, script store or browser persistence lives here.
 * T23 will reuse this clamp when switching in/out of the fullscreen top layer.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch, type CSSProperties } from 'vue';
import type { AutomationFloatingPoint } from './automation-floating-contract';

type ActiveDrag = {
    pointerId: number;
    startClientX: number;
    startClientY: number;
    startPosition: AutomationFloatingPoint;
};

const EDGE = 8;
const INITIAL_X = 12;
const INITIAL_Y = 44;

export function useAutomationFloatingLayer() {
    const host = ref<HTMLElement | null>(null);
    const panel = ref<HTMLElement | null>(null);
    const position = ref<AutomationFloatingPoint>({ x: INITIAL_X, y: INITIAL_Y });
    let active: ActiveDrag | null = null;
    let resizeObserver: ResizeObserver | null = null;

    function constrain(): void {
        const container = host.value, windowElement = panel.value;
        if (!container || !windowElement || !container.clientWidth || !container.clientHeight) return;
        const maximumX = Math.max(0, container.clientWidth - windowElement.offsetWidth - EDGE);
        const maximumY = Math.max(0, container.clientHeight - windowElement.offsetHeight - EDGE);
        position.value = {
            x: Math.max(0, Math.min(position.value.x, maximumX)),
            y: Math.max(0, Math.min(position.value.y, maximumY)),
        };
    }

    function pointerDown(event: PointerEvent): void {
        if (event.button !== 0 || !event.isPrimary || !host.value || !panel.value) return;
        active = {
            pointerId: event.pointerId,
            startClientX: event.clientX,
            startClientY: event.clientY,
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
        position.value = {
            x: Math.max(0, Math.min(active.startPosition.x + event.clientX - active.startClientX, maximumX)),
            y: Math.max(0, Math.min(active.startPosition.y + event.clientY - active.startClientY, maximumY)),
        };
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

    watch([host, panel], observe, { flush: 'post' });
    onMounted(() => {
        window.addEventListener('resize', constrain);
        document.addEventListener('fullscreenchange', constrain);
        observe();
    });
    onBeforeUnmount(() => {
        resizeObserver?.disconnect();
        window.removeEventListener('resize', constrain);
        document.removeEventListener('fullscreenchange', constrain);
    });
    const style = computed<CSSProperties>(() => ({
        left: `${position.value.x}px`,
        top: `${position.value.y}px`,
    }));
    return { host, panel, position, style, constrain, pointerDown, pointerMove, pointerEnd };
}
