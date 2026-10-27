<script setup lang="ts">
import { nextTick, toRef, watch } from 'vue';
import { useAutomationFloatingLayer } from '../../modules/useAutomationFloatingLayer';

const props = defineProps<{
    visible: boolean;
    fullscreen: boolean;
    title?: string;
    deviceName: string;
}>();
const emit = defineEmits<{ close: [] }>();
const { host, panel, style, constrain, pointerDown, pointerMove, pointerEnd } = useAutomationFloatingLayer(toRef(props, 'fullscreen'));

watch(() => props.visible, open => {
    if (open) void nextTick(constrain);
}, { flush: 'post' });
</script>

<template>
    <!-- Host is the SAME PhoneCard video surface; it never owns a video or stream. -->
    <div ref="host" class="automation-floating-host" :class="{ hidden: !visible }"
        :aria-hidden="!visible" @pointerdown.stop @pointermove.stop @pointerup.stop
        @click.stop @dblclick.stop @contextmenu.stop @wheel.stop>
        <section v-show="visible" ref="panel" class="automation-floating-panel"
            :style="style" role="region" :aria-label="title || '自动化脚本'">
            <header class="automation-floating-titlebar" aria-label="拖动自动化脚本窗口"
                @pointerdown="pointerDown" @pointermove="pointerMove"
                @pointerup="pointerEnd" @pointercancel="pointerEnd">
                <span class="automation-floating-heading">
                    <i class="pi pi-bolt" aria-hidden="true" />
                    {{ title || '自动化脚本' }}
                </span>
                <button type="button" class="automation-floating-close"
                    aria-label="关闭自动化脚本窗口" title="关闭自动化脚本窗口"
                    @pointerdown.stop @click.stop="emit('close')">
                    <i class="pi pi-times" aria-hidden="true" />
                </button>
            </header>
            <div class="automation-floating-body">
                <slot>
                    <p class="automation-floating-caption">{{ deviceName }}</p>
                    <p>自动化脚本工作区</p>
                    <small>脚本选择、步骤列表和编辑功能将在后续任务迁入此窗口。原卡片操作保持不变。</small>
                </slot>
            </div>
        </section>
    </div>
</template>

<style scoped>
.automation-floating-host{position:absolute;inset:0;z-index:9;pointer-events:none;contain:layout style}
.automation-floating-host.hidden{visibility:hidden}
.automation-floating-panel{
    position:absolute;display:flex;flex-direction:column;
    width:min(328px, calc(100% - 16px));max-height:calc(100% - 16px);
    min-width:0;min-height:0;overflow:hidden;
    border:1px solid var(--console-border);border-radius:11px;
    background:var(--console-panel);color:var(--p-text-color);
    box-shadow:0 10px 32px #0008;pointer-events:auto;
}
.automation-floating-titlebar{
    display:flex;align-items:center;justify-content:space-between;gap:8px;
    min-height:42px;padding:7px 9px 7px 12px;
    background:var(--console-panel-raised);border-bottom:1px solid var(--console-border);
    touch-action:none;cursor:grab;user-select:none;flex:0 0 auto;
}
.automation-floating-titlebar:active{cursor:grabbing}
.automation-floating-heading{display:flex;align-items:center;gap:7px;min-width:0;font-size:13px;font-weight:600;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.automation-floating-close{display:inline-flex;align-items:center;justify-content:center;flex:0 0 28px;width:28px;height:28px;padding:0;border:0;border-radius:7px;cursor:pointer;background:transparent;color:inherit}
.automation-floating-close:hover{background:var(--console-accent-soft)}
.automation-floating-close:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:2px}
.automation-floating-body{overflow-y:auto;overscroll-behavior:contain;min-height:0;padding:12px;font-size:12px;line-height:1.6}
.automation-floating-body p{margin:0 0 8px}
.automation-floating-body small{color:var(--console-muted)}
.automation-floating-caption{font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
/* Only the containing phone-screen enters the fullscreen top layer. */
:global(.phone-screen:fullscreen) .automation-floating-host{position:absolute;inset:0;z-index:9}
:global(.phone-screen:fullscreen) .automation-floating-panel{
    max-height:calc(100% - 16px);
    background:var(--console-panel);
}
@media (max-width:500px){.automation-floating-panel{border-radius:9px}}
</style>
