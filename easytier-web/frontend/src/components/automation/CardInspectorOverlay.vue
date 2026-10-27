<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { InspectorCardDragGuide, InspectorCardHighlight } from '../../modules/automation-inspector-overlay';

/** Paints on the EXISTING phone card, not a second player. */
const props=defineProps<{
    videoWidth:number;videoHeight:number;
    highlight?:InspectorCardHighlight|null;
    dragMode?:boolean;dragGuide?:InspectorCardDragGuide|null;
}>();
const emit=defineEmits<{pick:[event:MouseEvent]}>();

// A point is accepted only after one primary press/release on the same
// surface. Touch drags, additional fingers and cancelled pointers must not
// silently fill both A/B slots; no event is ever forwarded to Android.
let activePointer: {id:number;x:number;y:number}|null=null;
let suppressSyntheticClickUntil=0;
const keyboardCursorVisible=ref(false);
const keyboardX=ref(0.5);
const keyboardY=ref(0.5);
function beginPointerPick(event:PointerEvent) {
    if(event.button!==0 || !event.isPrimary || activePointer)return;
    activePointer={id:event.pointerId,x:event.clientX,y:event.clientY};
    try {(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);}
    catch {/* Pointer capture is optional; matching ID remains mandatory. */}
}
function clearPointerPick(){activePointer=null;}
function endPointerPick(event:PointerEvent){
    if(!activePointer || activePointer.id!==event.pointerId)return;
    const first=activePointer;
    activePointer=null;
    suppressSyntheticClickUntil=Date.now()+700;
    // A gesture that traveled is NOT a tap/point selection.
    if(Math.hypot(event.clientX-first.x,event.clientY-first.y)>12)return;
    emit('pick',event);
}
function moveKeyboardCursor(event:KeyboardEvent){
    const step=event.shiftKey?0.01:0.05;
    if(event.key==='ArrowLeft')keyboardX.value=Math.max(0.01,keyboardX.value-step);
    else if(event.key==='ArrowRight')keyboardX.value=Math.min(0.99,keyboardX.value+step);
    else if(event.key==='ArrowUp')keyboardY.value=Math.max(0.01,keyboardY.value-step);
    else if(event.key==='ArrowDown')keyboardY.value=Math.min(0.99,keyboardY.value+step);
    else if(event.key==='Home'){keyboardX.value=0.5;keyboardY.value=0.5;}
    else return;
    event.preventDefault();event.stopPropagation();
}
// Keyboard-generated clicks have no usable clientX/Y. Convert the keyboard
// cursor to the *contained primary video* viewport, including letterboxing.
function keyboardPick(event:MouseEvent){
    if(event.detail!==0 || Date.now()<suppressSyntheticClickUntil || !props.videoWidth || !props.videoHeight)return;
    const bounds=(event.currentTarget as HTMLElement).getBoundingClientRect();
    const scale=Math.min(bounds.width/props.videoWidth,bounds.height/props.videoHeight);
    if(!(scale>0 && Number.isFinite(scale)))return;
    const left=bounds.left+(bounds.width-props.videoWidth*scale)/2;
    const top=bounds.top+(bounds.height-props.videoHeight*scale)/2;
    emit('pick',new MouseEvent('click',{
        clientX:left+props.videoWidth*scale*keyboardX.value,
        clientY:top+props.videoHeight*scale*keyboardY.value,
    }));
}
watch(()=>props.dragMode,clearPointerPick);
const viewBox=computed(()=>`0 0 ${Math.max(1,props.videoWidth)} ${Math.max(1,props.videoHeight)}`);
const rectangle=computed(()=>{
    const rect=props.highlight;
    if(!rect || !props.videoWidth || !props.videoHeight || !rect.sourceWidth || !rect.sourceHeight ||
        Math.abs((rect.sourceWidth/rect.sourceHeight)/(props.videoWidth/props.videoHeight)-1)>=.035 ||
        rect.left<0 || rect.top<0 || rect.right>rect.sourceWidth || rect.bottom>rect.sourceHeight) return null;
    // Render native UIA pixels on the encoded primary-video SVG viewBox.
    return {
        x:rect.left/rect.sourceWidth*props.videoWidth,
        y:rect.top/rect.sourceHeight*props.videoHeight,
        width:(rect.right-rect.left)/rect.sourceWidth*props.videoWidth,
        height:(rect.bottom-rect.top)/rect.sourceHeight*props.videoHeight,
    };
});
const marker=(p:{x:number;y:number}|null|undefined)=>{
    return p ? {x:p.x*props.videoWidth,y:p.y*props.videoHeight}:null;
};
const a=computed(()=>marker(props.dragGuide?.start));
const b=computed(()=>marker(props.dragGuide?.end));
const keyboardCursor=computed(()=>({x:keyboardX.value*props.videoWidth,y:keyboardY.value*props.videoHeight}));
</script>

<template>
    <div class="card-inspector-overlay" aria-label="原画面控件定位只读叠加">
        <svg v-if="videoWidth>0 && videoHeight>0" :viewBox="viewBox" preserveAspectRatio="xMidYMid meet"
            aria-hidden="true">
            <rect v-if="rectangle" class="inspect-rect" :x="rectangle.x" :y="rectangle.y"
                :width="rectangle.width" :height="rectangle.height" vector-effect="non-scaling-stroke" />
            <line v-if="dragMode && a && b" :x1="a.x" :y1="a.y" :x2="b.x" :y2="b.y"
                class="drag-guide" vector-effect="non-scaling-stroke" />
            <circle v-if="dragMode && a" :cx="a.x" :cy="a.y" r="8" class="drag-a"
                vector-effect="non-scaling-stroke" />
            <circle v-if="dragMode && b" :cx="b.x" :cy="b.y" r="8" class="drag-b"
                vector-effect="non-scaling-stroke" />
            <circle v-if="dragMode && keyboardCursorVisible" :cx="keyboardCursor.x" :cy="keyboardCursor.y" r="6"
                class="keyboard-cursor" vector-effect="non-scaling-stroke" />
        </svg>
        <!-- The only interactive overlay is read-only drag-point selection.
             This intercepts touches so no Android touch can be emitted. -->
        <button v-if="dragMode" type="button" class="drag-pick-surface"
            title="在原手机画面选择拖拽起点或终点（只读）"
            aria-label="原画面拖拽点位拾取，点击 A 起点再点击 B 终点；键盘方向键移动光标，Enter 记录，Shift 加方向键微调"
            @pointerdown.stop.prevent="beginPointerPick" @pointermove.stop
            @pointerup.stop.prevent="endPointerPick" @pointercancel.stop.prevent="clearPointerPick"
            @lostpointercapture="clearPointerPick" @contextmenu.stop.prevent
            @keydown="moveKeyboardCursor" @focus="keyboardCursorVisible=true" @blur="keyboardCursorVisible=false"
            @click.stop.prevent="keyboardPick" />
    </div>
</template>

<style scoped>
.card-inspector-overlay{position:absolute;inset:0;z-index:4;pointer-events:none;overflow:hidden}
.card-inspector-overlay svg{position:absolute;inset:0;width:100%;height:100%;pointer-events:none}
.inspect-rect{fill:#10b98127;stroke:#34d399;stroke-width:2.5;stroke-dasharray:5 3}
.drag-guide{stroke:#f59e0b;stroke-width:2;stroke-dasharray:6 3}
.drag-a{fill:#2563eb;stroke:white;stroke-width:2}
.drag-b{fill:#c2410c;stroke:white;stroke-width:2}
.keyboard-cursor{fill:none;stroke:#fef08a;stroke-width:2;stroke-dasharray:3 2}
.drag-pick-surface{position:absolute;inset:0;width:100%;height:100%;padding:0;border:0;
    background:transparent;pointer-events:auto;cursor:crosshair;touch-action:none}
.drag-pick-surface:focus-visible{outline:2px solid #34d399;outline-offset:-3px}
</style>
