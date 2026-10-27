<script setup lang="ts">
import { computed } from 'vue';
import type { LocatorDemoResult } from '../../modules/automation-locator-demo';
const props=defineProps<{ demo: LocatorDemoResult | null; videoWidth: number; videoHeight: number }>();
const rectangle=computed(()=>{
    const d=props.demo, b=d?.bounds;
    if(!d||d.status!=='one'||!b||!d.sourceWidth||!d.sourceHeight||
        props.videoWidth<=0||props.videoHeight<=0) return null;
    if(Math.abs((d.sourceWidth/d.sourceHeight)/(props.videoWidth/props.videoHeight)-1)>=0.035) return null;
    return {
        x:b.left/d.sourceWidth*props.videoWidth,y:b.top/d.sourceHeight*props.videoHeight,
        width:(b.right-b.left)/d.sourceWidth*props.videoWidth,
        height:(b.bottom-b.top)/d.sourceHeight*props.videoHeight,
    };
});
const viewBox=computed(()=>`0 0 ${props.videoWidth} ${props.videoHeight}`);
</script>

<template>
    <div v-if="rectangle" class="locator-overlay" aria-hidden="true">
        <svg :viewBox="viewBox" preserveAspectRatio="xMidYMid meet">
            <rect :x="rectangle.x" :y="rectangle.y" :width="rectangle.width"
                :height="rectangle.height" class="locator-frame" vector-effect="non-scaling-stroke"/>
        </svg>
        <span class="readonly-label">定位演示 · 只读</span>
    </div>
</template>

<style scoped>
.locator-overlay{position:absolute;inset:0;z-index:3;pointer-events:none;overflow:hidden}
.locator-overlay svg{position:absolute;inset:0;width:100%;height:100%;overflow:visible}
.locator-frame{fill:#22c55e22;stroke:#34d399;stroke-width:2.5;stroke-dasharray:6 3}
.readonly-label{position:absolute;left:8px;bottom:8px;border-radius:5px;background:#073326e8;color:#b2f5d6;padding:4px 7px;font-size:11px}
</style>
