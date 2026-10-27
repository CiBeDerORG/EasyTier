<script setup lang="ts">
import type { PickerPhase } from '../../modules/automation-picker-state';

defineProps<{ phase: PickerPhase; error?: string }>();
defineEmits<{ exit: [] }>();
</script>

<template>
    <!-- Non-blocking indicator, so left-clicks still reach the scrcpy video. -->
    <div v-if="phase !== 'off'" class="card-picking-overlay" :class="phase" role="status" aria-live="polite">
        <span class="picker-status">
            <i :class="phase === 'preparing' ? 'pi pi-spin pi-spinner' : phase === 'error' ? 'pi pi-exclamation-circle' : 'pi pi-search'" aria-hidden="true" />
            {{ phase === 'preparing' ? '拾取准备中' : phase === 'error' ? (error || '拾取异常') : '拾取中 · 右键选控件' }}
        </span>
        <button type="button" class="exit-picking" aria-label="退出拾取模式" @click.stop="$emit('exit')" @pointerdown.stop>退出</button>
    </div>
</template>

<style scoped>
.card-picking-overlay{position:absolute;top:10px;right:10px;z-index:4;display:flex;align-items:center;max-width:calc(100% - 20px);gap:7px;pointer-events:none}
.picker-status{min-width:0;display:inline-flex;align-items:center;gap:5px;padding:6px 8px;border-radius:8px;background:#071c29d9;color:#c3f6d7;font-size:11px;box-shadow:0 2px 9px #0006;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.card-picking-overlay.preparing .picker-status{color:#a9d4ff}
.card-picking-overlay.error .picker-status{color:#ffbcb8}
.exit-picking{pointer-events:auto;padding:5px 8px;border-radius:7px;border:1px solid #ffffff50;background:#10263ddd;color:#fff;cursor:pointer;font-size:11px}
.exit-picking:focus-visible{outline:2px solid #fff;outline-offset:2px}
</style>
