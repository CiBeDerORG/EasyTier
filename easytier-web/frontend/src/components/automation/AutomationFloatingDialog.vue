<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, toRef, watch } from 'vue';
import { useAutomationFloatingLayer } from '../../modules/useAutomationFloatingLayer';
import { useAutomationFloatingDialogOrder } from '../../modules/automation-floating-dialog-stack';

/**
 * T24: one reusable non-modal child floating window in the ORIGINAL phone video.
 * Callers supply only UI content and retain ownership of edits/dirty state.
 * Teleport targets PhoneCard's scoped host; normal mode uses the viewport,
 * fullscreen mode moves that same host into the phone-screen top layer.
 */
const props = withDefaults(defineProps<{
    visible: boolean;
    portalTarget: HTMLElement | null;
    fullscreen: boolean;
    title: string;
    dirty?: boolean;
    closable?: boolean;
    /** Wider read-only comparison tables, still bounded inside the phone screen. */
    wide?: boolean;
}>(), { dirty: false, closable: true });

const emit = defineEmits<{
    /** Parent must close explicitly; on discard it must also clear unsaved fields. */
    close: [reason: 'normal' | 'discard'];
}>();
const cancelDiscardButton = ref<HTMLButtonElement | null>(null);
const discardRequested = ref(false);
let previouslyFocused: HTMLElement | null = null;
let registered = false;
const { host, panel, position, style, constrain, moveBy, pointerDown, pointerMove, pointerEnd } =
    useAutomationFloatingLayer(toRef(props, 'fullscreen'), {
        normalStart: { x: 24, y: 72 },
        fullscreenTop: 72,
    });
const { zIndex, raise, unregister, isTopmost } = useAutomationFloatingDialogOrder();

function restoreFocus() {
    const prior = previouslyFocused;
    previouslyFocused = null;
    if (prior?.isConnected) prior.focus({ preventScroll: true });
}
function requestClose() {
    if (!props.closable) return;
    if (props.dirty) {
        discardRequested.value = true;
        void nextTick(() => cancelDiscardButton.value?.focus({ preventScroll: true }));
        return;
    }
    emit('close', 'normal');
}
defineExpose({ requestClose });
function keepEditing() {
    discardRequested.value = false;
    void nextTick(() => panel.value?.focus({ preventScroll: true }));
}
function discardAndClose() {
    if (!props.closable || !props.dirty) return;
    discardRequested.value = false;
    emit('close', 'discard');
}
/**
 * Child dialogs are non-modal, so their default fullscreen placement must
 * leave the main script panel's insertion menu clickable. An enlarged editor
 * (custom title, ADB options, etc.) can otherwise blanket that menu.
 * Move only when the natural/default position actually intersects its button;
 * preserve user-draggable placement at all other times.
 */
function avoidCoveringMainAction() {
    if (!props.visible || !props.fullscreen || !host.value || !panel.value) return;
    const main = host.value.closest('.phone-screen')
        ?.querySelector<HTMLElement>('.automation-floating-panel');
    const target = main?.querySelector<HTMLElement>('.flow-tools .choice-trigger');
    if (!target) return;
    const bounds = host.value.getBoundingClientRect();
    const targetBox = target.getBoundingClientRect();
    const mainBox = main!.getBoundingClientRect();
    const width = panel.value.offsetWidth, height = panel.value.offsetHeight;
    if (!width || !height || !bounds.width || !bounds.height) return;
    const maxX = Math.max(0, bounds.width - width - 8);
    const maxY = Math.max(0, bounds.height - height - 8);
    const targetRect = {x:targetBox.left-bounds.left,y:targetBox.top-bounds.top,
        width:targetBox.width,height:targetBox.height};
    const mainRect = {x:mainBox.left-bounds.left,y:mainBox.top-bounds.top,
        width:mainBox.width,height:mainBox.height};
    const overlap = (point:{x:number;y:number}, rect:typeof targetRect) =>
        Math.max(0,Math.min(point.x+width,rect.x+rect.width)-Math.max(point.x,rect.x)) *
        Math.max(0,Math.min(point.y+height,rect.y+rect.height)-Math.max(point.y,rect.y));
    const current = position.value;
    if (!overlap(current,targetRect)) return;
    const clamp = (x:number,y:number) => ({
        x:Math.max(0,Math.min(maxX,x)), y:Math.max(0,Math.min(maxY,y)),
    });
    const xs = [8,maxX,targetRect.x-width-12,targetRect.x+targetRect.width+12];
    const ys = [8,maxY,targetRect.y-height-12,targetRect.y+targetRect.height+12];
    const choices = xs.flatMap(x=>ys.map(y=>clamp(x,y)));
    const score = (point:typeof current) =>
        overlap(point,targetRect)*10000 + overlap(point,mainRect) +
        Math.hypot(point.x-current.x,point.y-current.y)*0.01;
    const best = choices.reduce((a,b)=>score(b)<score(a)?b:a,current);
    if (!overlap(best,targetRect) && (best.x!==current.x || best.y!==current.y))
        position.value = best;
}

function beginDrag(event: PointerEvent) {
    raise();
    pointerDown(event);
}
function moveByArrow(event: KeyboardEvent) {
    // The drag header itself must have focus; arrow keys in editors stay usable.
    if (event.target !== event.currentTarget) return;
    const delta = event.shiftKey ? 30 : 10;
    const moves: Record<string, [number, number]> = {
        ArrowLeft: [-delta, 0], ArrowRight: [delta, 0],
        ArrowUp: [0, -delta], ArrowDown: [0, delta],
    };
    const step = moves[event.key];
    if (!step) return;
    event.preventDefault();
    event.stopPropagation();
    moveBy(step[0], step[1]);
}
function onDocumentKeydown(event: KeyboardEvent) {
    // Only the topmost focused child handles ESC; never stop script execution.
    if (event.key !== 'Escape' || event.isComposing || !props.visible || !isTopmost() ||
        !(event.target instanceof Node) || !panel.value?.contains(event.target)) return;
    // An expanded in-window choice list owns the first Escape; close it
    // without closing or discarding the containing step editor.
    if(panel.value.querySelector('.automation-inline-choice [aria-expanded="true"]'))return;
    event.preventDefault();
    event.stopPropagation();
    if (discardRequested.value) keepEditing();
    else requestClose();
}
watch([() => props.visible, () => props.portalTarget], async ([visible, target]) => {
    host.value = target;
    if (visible && target) {
        if (!registered) {
            previouslyFocused = document.activeElement instanceof HTMLElement
                ? document.activeElement : null;
            raise();
            registered = true;
        }
        await nextTick();
        if(!props.fullscreen && target){
            const css=getComputedStyle(target);
            const x=parseFloat(css.getPropertyValue('--automation-dialog-x'));
            const y=parseFloat(css.getPropertyValue('--automation-dialog-y'));
            if(Number.isFinite(x) && Number.isFinite(y))
                position.value={x,y};
        }
        constrain();
        // Host and panel are mounted after teleport, so geometry is available.
        await nextTick();
        avoidCoveringMainAction();
        if (props.visible && panel.value && !panel.value.contains(document.activeElement))
            panel.value.focus({ preventScroll: true });
    } else if (registered) {
        registered = false;
        discardRequested.value = false;
        unregister();
        await nextTick();
        restoreFocus();
    }
}, { immediate: true, flush: 'post' });
watch(() => props.fullscreen, async fullscreen => {
    if (!fullscreen || !props.visible) return;
    // The shared layer computes its fullscreen coordinates on nextTick.
    await nextTick();
    await nextTick();
    avoidCoveringMainAction();
}, { flush:'post' });
watch(() => props.dirty, value => { if (!value) discardRequested.value = false; });
onMounted(() => document.addEventListener('keydown', onDocumentKeydown, true));
onBeforeUnmount(() => {
    document.removeEventListener('keydown', onDocumentKeydown, true);
    unregister();
    restoreFocus();
});
</script>

<template>
    <!-- In normal mode the PhoneCard portal host uses viewport bounds; in
         fullscreen the same host is moved into the top-layer phone screen. -->
    <Teleport v-if="portalTarget" :to="portalTarget">
        <section v-show="visible" ref="panel" class="automation-child-dialog"
            :class="{ 'automation-child-dialog-wide': wide }"
            :style="[style, { zIndex }]" role="dialog" aria-modal="false"
            :aria-label="title" tabindex="-1"
            @pointerdown.stop="raise" @pointermove.stop @pointerup.stop
            @click.stop @dblclick.stop @contextmenu.stop @wheel.stop
            @focusin="raise">
            <header class="automation-child-titlebar" tabindex="0"
                :aria-label="`拖动${title}；方向键移动浮窗`"
                @keydown="moveByArrow" @pointerdown="beginDrag"
                @pointermove="pointerMove" @pointerup="pointerEnd"
                @pointercancel="pointerEnd">
                <span class="automation-child-title">{{ title }}</span>
                <button type="button" class="automation-child-close"
                    :disabled="!closable" :title="`关闭${title}`"
                    :aria-label="`关闭${title}`" @pointerdown.stop @click.stop="requestClose">
                    <i class="pi pi-times" aria-hidden="true" />
                </button>
            </header>
            <div v-if="discardRequested" class="automation-child-unsaved" role="alert">
                <strong>当前修改尚未保存</strong>
                <p>关闭将放弃本次未确认的编辑，已保存的脚本和服务器草稿不受影响。</p>
                <div class="automation-child-unsaved-actions">
                    <button ref="cancelDiscardButton" type="button" @click="keepEditing">继续编辑</button>
                    <button type="button" class="automation-child-discard"
                        @click="discardAndClose">放弃修改并关闭</button>
                </div>
            </div>
            <!-- Hide, don't unmount the editor while a discard decision is pending. -->
            <div v-show="!discardRequested" class="automation-child-body">
                <slot />
            </div>
            <footer v-if="$slots.footer" v-show="!discardRequested" class="automation-child-footer">
                <slot name="footer" />
            </footer>
        </section>
    </Teleport>
</template>

<style scoped>
.automation-child-dialog{
    position:absolute;box-sizing:border-box;display:flex;flex-direction:column;
    width:min(400px,calc(100% - 16px));max-height:calc(100% - 16px);
    min-width:0;min-height:0;overflow:hidden;pointer-events:auto;
    border:1px solid var(--console-border);border-radius:11px;
    background:var(--console-panel);color:var(--p-text-color);
    box-shadow:0 12px 38px #000a;
}
.automation-child-dialog-wide{width:min(960px,calc(100% - 16px))}
.automation-child-dialog:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:1px}
.automation-child-titlebar{
    display:flex;align-items:center;justify-content:space-between;gap:8px;
    min-height:42px;flex:0 0 auto;padding:6px 8px 6px 12px;
    background:var(--console-panel);border-bottom:1px solid var(--console-border);
    cursor:grab;touch-action:none;user-select:none;
}
.automation-child-titlebar:active{cursor:grabbing}
.automation-child-titlebar:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:-2px}
.automation-child-title{font-size:13px;font-weight:600;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.automation-child-close{
    display:inline-flex;justify-content:center;align-items:center;flex:0 0 28px;
    width:28px;height:28px;border:0;border-radius:7px;
    background:transparent;color:inherit;cursor:pointer
}
.automation-child-close:hover:not(:disabled){background:var(--console-accent-soft)}
.automation-child-close:disabled{opacity:.5;cursor:default}
.automation-child-close:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:1px}
.automation-child-body{padding:12px;overflow-y:auto;overscroll-behavior:contain;min-height:0;font-size:12px;line-height:1.5}
.automation-child-footer{flex:0 0 auto;padding:9px 12px;border-top:1px solid var(--console-border);display:flex;gap:8px;justify-content:flex-end}
.automation-child-unsaved{padding:12px;overflow-y:auto;overscroll-behavior:contain;font-size:12px}
.automation-child-unsaved strong{display:block;margin-bottom:6px}
.automation-child-unsaved p{margin:0 0 12px;line-height:1.5}
.automation-child-unsaved-actions{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap}
.automation-child-unsaved-actions button{padding:6px 10px;border-radius:7px;border:1px solid var(--console-border);background:var(--console-panel);color:inherit;cursor:pointer}
.automation-child-unsaved-actions .automation-child-discard{border-color:var(--p-red-500);color:var(--p-red-500)}
.automation-child-unsaved-actions button:focus-visible{outline:2px solid var(--p-primary-color)}
</style>
