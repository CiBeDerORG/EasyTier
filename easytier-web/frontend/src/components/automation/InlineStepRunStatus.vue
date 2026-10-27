<script setup lang="ts">
import type { InlineStepRunProgress } from '../../modules/useInlineStepRun';

defineProps<{ progress: InlineStepRunProgress }>();
</script>

<template>
    <!-- T41: read-only step status. It never POSTs, alters the step or creates
         a second video/screen surface. Reopen ▶ for the detailed run record. -->
    <small class="inline-run" :class="'state-' + progress.phase" role="status"
        :title="'请求 '+progress.requestId+(progress.runId?'；运行 '+progress.runId:'')">
        <i v-if="progress.phase==='preparing' || progress.phase==='submitting' || progress.phase==='running'"
            class="pi pi-spin pi-spinner" aria-hidden="true" />
        <i v-else-if="progress.phase==='succeeded' || progress.phase==='action_sent'"
            class="pi pi-check-circle" aria-hidden="true" />
        <i v-else-if="progress.phase==='failed' || progress.phase==='unknown'"
            class="pi pi-exclamation-triangle" aria-hidden="true" />
        <span>{{ progress.label }}</span>
        <span v-if="progress.elapsedMs !== undefined" class="id">{{ (progress.elapsedMs / 1000).toFixed(1) }}s</span>
        <span v-if="progress.runId" class="id">{{ progress.runId.slice(0,8) }}</span>
    </small>
</template>

<style scoped>
.inline-run{display:flex;align-items:center;gap:4px;font-size:10px;max-width:100%;line-height:1.4;white-space:nowrap;overflow:hidden}
.inline-run span{overflow:hidden;text-overflow:ellipsis}
.inline-run .id{font-variant-numeric:tabular-nums;opacity:.7;flex:0 0 auto}
.state-running,.state-submitting,.state-preparing{color:var(--p-primary-color)}
.state-succeeded,.state-action_sent{color:var(--p-green-500)}
.state-failed,.state-unknown{color:var(--p-orange-500)}
</style>
