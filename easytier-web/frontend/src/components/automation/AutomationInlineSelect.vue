<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';

/**
 * Window-native selector. Options are rendered in the same DOM/scroll area as
 * the trigger: no Teleport, PrimeVue auto-z-index, fullscreen top-layer, or
 * pointer-events inheritance dependency.
 */
type ChoiceValue = string | boolean | number;
type Choice = { value:ChoiceValue; label?:string; name?:string };
const props = withDefaults(defineProps<{
    modelValue?:ChoiceValue|null;
    options:Choice[];
    disabled?:boolean;
    inputId?:string;
    ariaLabel?:string;
    optionLabel?:string;
    optionValue?:string;
}>(), {disabled:false,optionLabel:'label',optionValue:'value'});
const emit=defineEmits<{ 'update:modelValue':[value:ChoiceValue] }>();
const open=ref(false);
const focusIndex=ref(0);
const menu=ref<HTMLElement|null>(null);
const root=ref<HTMLElement|null>(null);
const trigger=ref<HTMLButtonElement|null>(null);
const displayed=computed(()=>{
    const found=props.options.find(x=>x.value===props.modelValue);
    return found ? (found.label??found.name??String(found.value)) : '请选择';
});
function close(){
    open.value=false;
}
function onOutside(event:PointerEvent){
    if(root.value && !root.value.contains(event.target as Node))close();
}
function onEscape(event:KeyboardEvent){
    if(event.key==='Escape' && open.value){
        event.stopPropagation();
        event.preventDefault();
        close();
        trigger.value?.focus();
    }
}
function toggle(){
    if(props.disabled)return;
    open.value=!open.value;
    if(open.value){
        focusIndex.value=Math.max(0,props.options.findIndex(o=>o.value===props.modelValue));
        void nextTick(()=>{
            menu.value?.scrollIntoView({block:'nearest',inline:'nearest'});
        });
    }
}
function choose(value:ChoiceValue){
    if(props.disabled)return;
    emit('update:modelValue',value);
    close();
    void nextTick(()=>trigger.value?.focus({preventScroll:true}));
}
function key(event:KeyboardEvent){
    if(props.disabled)return;
    const count=props.options.length;
    if(event.key==='Escape'){onEscape(event);return;}
    if(event.key==='Enter'||event.key===' '){
        event.preventDefault();
        if(open.value && count)choose(props.options[focusIndex.value]?.value);
        else toggle();
        return;
    }
    if(event.key==='ArrowDown'||event.key==='ArrowUp'){
        event.preventDefault();
        if(!open.value)toggle();
        else if(count)focusIndex.value=(focusIndex.value+(event.key==='ArrowDown'?1:-1)+count)%count;
        void nextTick(()=>menu.value?.querySelector('[data-focused="true"]')?.scrollIntoView({block:'nearest'}));
    }
}
onMounted(()=>document.addEventListener('pointerdown',onOutside,true));
onBeforeUnmount(()=>document.removeEventListener('pointerdown',onOutside,true));
</script>
<template>
    <div ref="root" class="automation-inline-choice" :class="{ 'is-disabled':disabled }" @keydown="key" @pointerdown.stop @click.stop>
        <button :id="inputId" ref="trigger" type="button" class="choice-trigger"
            :disabled="disabled" :aria-label="ariaLabel" aria-haspopup="listbox" :aria-expanded="open"
            @click.stop="toggle">
            <span class="choice-current">{{displayed}}</span>
            <i :class="open?'pi pi-chevron-up':'pi pi-chevron-down'" aria-hidden="true" />
        </button>
        <div v-if="open" ref="menu" class="choice-list" role="listbox" :aria-label="ariaLabel">
            <button v-for="(option,index) in options" :key="String(option.value)"
                type="button" role="option" class="choice-option"
                :class="{ 'is-focused':index===focusIndex, 'is-selected':modelValue===option.value }"
                :data-focused="index===focusIndex" :aria-selected="modelValue===option.value"
                @pointerenter="focusIndex=index" @click.stop="choose(option.value)">
                <span>{{option.label??option.name??String(option.value)}}</span>
                <i v-if="modelValue===option.value" class="pi pi-check" aria-hidden="true"/>
            </button>
        </div>
    </div>
</template>
<style scoped>
.automation-inline-choice{display:flex;flex-direction:column;min-width:0;width:100%;gap:4px;pointer-events:auto}
.choice-trigger{display:flex;align-items:center;justify-content:space-between;gap:10px;
    width:100%;min-width:0;min-height:42px;padding:8px 12px;box-sizing:border-box;
    border:1px solid var(--console-border,#cad7e6);border-radius:7px;
    background:var(--console-panel-raised,#fff);color:var(--p-text-color,#334155);
    text-align:left;cursor:pointer;font:inherit;touch-action:manipulation}
.choice-trigger:disabled{opacity:.65;cursor:not-allowed}
.choice-trigger:focus-visible,.choice-option:focus-visible{outline:2px solid var(--p-primary-color,#0d718c);outline-offset:1px}
.choice-current{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}
.choice-trigger .pi{font-size:12px;opacity:.75;flex:0 0 auto}
.choice-list{position:relative;z-index:1;display:flex;flex-direction:column;gap:2px;box-sizing:border-box;
    max-height:min(32vh,230px);overflow-y:auto;overscroll-behavior:contain;pointer-events:auto;
    padding:5px;border-radius:8px;border:1px solid var(--console-border,#cad7e6);
    background:var(--console-panel-raised,#fff);box-shadow:0 5px 15px #0002}
.choice-option{display:flex;align-items:center;justify-content:space-between;gap:8px;
    width:100%;flex:0 0 auto;min-height:42px;padding:8px 10px;border:0;
    border-radius:6px;background:transparent;color:inherit;text-align:left;
    cursor:pointer;font:inherit;overflow-wrap:anywhere;touch-action:manipulation}
.choice-option:hover,.choice-option.is-focused{background:var(--console-accent-soft,#e6f3f8)}
.choice-option.is-selected{color:var(--p-primary-color,#0d718c);font-weight:600}
.choice-option .pi{flex:0 0 auto}
@media (pointer:coarse){.choice-trigger,.choice-option{min-height:46px}}
</style>
