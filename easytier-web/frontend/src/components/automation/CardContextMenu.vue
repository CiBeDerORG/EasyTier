<script setup lang="ts">
import { nextTick, onMounted, ref, watch } from 'vue';
import type { PickerMenuAction } from '../../modules/automation-picker-state';

const props = defineProps<{ left: number; top: number }>();
const emit = defineEmits<{ select: [action: PickerMenuAction]; close: [] }>();
const element = ref<HTMLElement | null>(null);
const position = ref({ left: props.left, top: props.top });
const groups: { title: string; options: { action: PickerMenuAction; label: string; icon: string }[] }[] = [
    { title: '控件', options: [
        { action:'inspect', label:'查看控件树与属性', icon:'pi pi-sitemap' },
        { action:'refresh', label:'刷新控件树', icon:'pi pi-refresh' },
    ] },
    { title: '步骤（需确认定位）', options: [
        { action:'tap', label:'配置点击', icon:'pi pi-check-circle' },
        { action:'long_press', label:'配置长按', icon:'pi pi-stop-circle' },
        { action:'input_text', label:'配置输入文字', icon:'pi pi-pencil' },
        { action:'wait_for', label:'配置等待出现', icon:'pi pi-clock' },
        { action:'wait_gone', label:'配置等待消失', icon:'pi pi-eye-slash' },
        { action:'assert', label:'配置检查控件', icon:'pi pi-verified' },
        { action:'assert_property', label:'配置属性断言', icon:'pi pi-list-check' },
    ] },
    { title:'拾取', options:[{action:'exit',label:'退出拾取模式',icon:'pi pi-times'}] },
];
function fitMenu() {
    void nextTick(() => {
        const node = element.value, host = node?.parentElement;
        if (!node || !host) return;
        position.value = {
            left: Math.max(8, Math.min(props.left, Math.max(8, host.clientWidth - node.offsetWidth - 8))),
            top: Math.max(8, Math.min(props.top, Math.max(8, host.clientHeight - Math.min(node.offsetHeight, host.clientHeight - 16) - 8))),
        };
        node.querySelector<HTMLButtonElement>('button[role="menuitem"]')?.focus();
    });
}
function keydown(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); emit('close'); return; }
    if (!['ArrowDown','ArrowUp','Home','End'].includes(e.key)) return;
    const items = [...(element.value?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]') ?? [])];
    if (!items.length) return;
    e.preventDefault();
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const index = e.key === 'Home' ? 0 : e.key === 'End' ? items.length - 1
        : (current + (e.key === 'ArrowUp' ? items.length - 1 : 1)) % items.length;
    items[index]?.focus();
}
watch(() => [props.left,props.top], fitMenu);
onMounted(fitMenu);
</script>

<template>
    <div ref="element" class="pick-context-menu" :style="{ left: position.left + 'px', top: position.top + 'px' }"
        role="menu" aria-label="手机控件右键快捷菜单"
        @keydown.stop="keydown" @contextmenu.prevent.stop @pointerdown.stop @click.stop>
        <div v-for="group in groups" :key="group.title" class="menu-group">
            <small>{{ group.title }}</small>
            <button v-for="item in group.options" :key="item.action" type="button" role="menuitem"
                @click="emit('select', item.action)"><i :class="item.icon" aria-hidden="true" /><span>{{ item.label }}</span></button>
        </div>
    </div>
</template>

<style scoped>
.pick-context-menu{position:absolute;z-index:9;box-sizing:border-box;width:min(232px,calc(100% - 16px));max-height:calc(100% - 16px);overflow:auto;overscroll-behavior:contain;padding:5px;border:1px solid #456780;border-radius:11px;background:#14263bf5;box-shadow:0 12px 30px #000b;color:#edf5ff;font-size:12px;touch-action:pan-y}
.menu-group+.menu-group{border-top:1px solid #ffffff24;margin-top:4px;padding-top:4px}
small{display:block;padding:6px 9px 4px;color:#aec4d7;font-size:10px}
button{display:flex;align-items:center;gap:9px;width:100%;padding:8px 9px;border:0;border-radius:6px;background:transparent;color:inherit;font:inherit;text-align:left;cursor:pointer}
button i{width:15px;color:#a8d8f8}
button:hover,button:focus-visible{background:#345879;outline:none}
</style>
