<script setup lang="ts">
import { computed } from 'vue';
import type { FilterOption } from '../../modules/automation-cross-filter';

const props = defineProps<{
  option: FilterOption;
  checked: boolean;
  truncated: boolean;
}>();
const emit = defineEmits<{
  toggle: [id: string, checked: boolean];
  'toggle-fuzzy': [id: string, enabled: boolean];
  'edit-fuzzy': [id: string, text: string];
}>();
const textual = computed(() => props.option.field === 'text' || props.option.field === 'description');
const fuzzyText = computed(() => props.option.fuzzyText ?? String(props.option.value));
</script>

<template>
  <div class="filter-option" :class="{ selected: checked }">
    <div class="filter-option-row">
      <label class="filter-value" :title="option.caution || ''">
        <input type="checkbox" :checked="checked" :disabled="truncated"
          @change="emit('toggle', option.id, ($event.target as HTMLInputElement).checked)" />
        <span class="filter-name">{{ option.label }}</span>
        <code :title="option.displayValue">{{ option.displayValue }}</code>
        <small v-if="option.caution" class="caution" aria-label="注意事项">!</small>
      </label>
      <button v-if="textual" type="button" class="fuzzy-switch"
        role="switch" :aria-checked="!!option.fuzzy"
        :aria-label="option.label + '模糊匹配'"
        :title="option.fuzzy ? '关闭后按原文精确匹配' : '开启后按多段关键词匹配'"
        :disabled="truncated"
        @click.stop="emit('toggle-fuzzy', option.id, !option.fuzzy)">
        <span class="track"><span class="thumb" /></span>
        <span>模糊</span>
      </button>
    </div>
    <div v-if="textual && option.fuzzy" class="fuzzy-editor">
      <input type="text" :value="fuzzyText" maxlength="256"
        :aria-label="option.label + '模糊匹配关键词'"
        placeholder="输入关键词，空格分段"
        :disabled="truncated"
        @input="emit('edit-fuzzy', option.id, ($event.target as HTMLInputElement).value)" />
      <small>空格分段 · 全部包含 (AND) · 忽略独立数字</small>
    </div>
  </div>
</template>

<style scoped>
.filter-option{border:1px solid var(--console-border);border-radius:7px;padding:7px;background:var(--console-panel-raised);font-size:11px;min-width:0}
.filter-option.selected{border-color:var(--p-primary-color);background:var(--console-accent-soft)}
.filter-option-row{display:flex;align-items:center;gap:6px;min-width:0}
.filter-value{display:flex;align-items:center;gap:7px;flex:1;min-width:0;cursor:pointer}
.filter-value input{flex:none;accent-color:var(--p-primary-color)}
.filter-name{flex:none}
.filter-value code{font-size:10px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}
.caution{color:var(--p-orange-500);font-weight:bold}
.fuzzy-switch{display:flex;flex:none;align-items:center;gap:4px;border:0;background:transparent;color:var(--console-muted);padding:3px 1px;min-height:30px;font-size:10px;cursor:pointer}
.fuzzy-switch[aria-checked="true"]{color:var(--p-primary-color)}
.fuzzy-switch:focus-visible{outline:2px solid var(--p-primary-color);border-radius:4px}
.fuzzy-switch:disabled{opacity:.5;cursor:not-allowed}
.track{position:relative;width:27px;height:15px;flex:none;border-radius:20px;background:var(--console-border);transition:background .15s}
.thumb{position:absolute;left:2px;top:2px;width:11px;height:11px;background:white;border-radius:100%;box-shadow:0 1px 2px #0003;transition:transform .15s}
.fuzzy-switch[aria-checked="true"] .track{background:var(--p-primary-color)}
.fuzzy-switch[aria-checked="true"] .thumb{transform:translateX(12px)}
.fuzzy-editor{display:grid;gap:4px;margin-top:7px}
.fuzzy-editor input{box-sizing:border-box;width:100%;min-width:0;border:1px solid var(--console-border);border-radius:6px;padding:7px 8px;background:var(--console-panel);color:var(--p-text-color);font-size:11px}
.fuzzy-editor input:focus{outline:2px solid var(--p-primary-color);outline-offset:-1px}
.fuzzy-editor small{font-size:10px;color:var(--console-muted);line-height:1.4}
@media(pointer:coarse){.fuzzy-switch{min-height:38px;padding:3px 4px}}
</style>
