<script setup lang="ts">
import { computed } from 'vue';
import type { InspectorCardDragGuide, InspectorCardHighlight } from '../../modules/automation-inspector-overlay';

/** Paints on the EXISTING phone card, not a second player. */
const props=defineProps<{
    videoWidth:number;videoHeight:number;
    highlight?:InspectorCardHighlight|null;
    dragMode?:boolean;dragGuide?:InspectorCardDragGuide|null;
}>();
const emit=defineEmits<{pick:[event:MouseEvent]}>();
function keyboardPick(event:MouseEvent){
    if(event.detail===0)emit('pick',event);
}
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
        </svg>
        <!-- The only interactive overlay is read-only drag-point selection.
             This intercepts touches so no Android touch can be emitted. -->
        <button v-if="dragMode" type="button" class="drag-pick-surface"
            title="在原手机画面选择拖拽起点或终点（只读）"
            aria-label="原画面拖拽点位拾取，点击 A 起点再点击 B 终点"
            @pointerdown.stop.prevent @pointermove.stop
            @pointerup.stop.prevent="emit('pick',$event)"
            @contextmenu.stop.prevent
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
.drag-pick-surface{position:absolute;inset:0;width:100%;height:100%;padding:0;border:0;
    background:transparent;pointer-events:auto;cursor:crosshair;touch-action:none}
.drag-pick-surface:focus-visible{outline:2px solid #34d399;outline-offset:-3px}
</style>
