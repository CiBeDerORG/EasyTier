<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Checkbox, InputNumber, InputText, Textarea } from 'primevue';
import AutomationInlineSelect from './AutomationInlineSelect.vue';
import { useI18n } from 'vue-i18n';
import type { AutomationAdbFavorite, AutomationKey, AutomationSelector, AutomationStep, AutomationStepType } from '../../types/automation';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import { builtinAdbCommands } from '../../modules/automation-adb-presets';

const props = defineProps<{ modelValue: AutomationStep; disabled?: boolean; lockActionType?: boolean; excludeConditionalTypes?: boolean; ownerId?: number }>();
const emit = defineEmits<{ 'update:modelValue': [step: AutomationStep] }>();
const { locale } = useI18n();
const cn = computed(() => locale.value === 'cn' || locale.value.startsWith('zh'));
const text = (zh: string, en: string) => cn.value ? zh : en;
const fieldId = (name: string) => `automation-step-${props.modelValue.id}-${name}`;
const actionOptions = computed(() => {
    const options = [
        { value: 'key', label: text('导航按键', 'Navigation key') },
        { value: 'tap', label: text('点击', 'Tap') },
        { value: 'long_press', label: text('长按', 'Long press') },
        { value: 'swipe', label: text('滑动', 'Swipe') },
        { value: 'drag', label: text('拖拽', 'Drag') },
        { value: 'input_text', label: text('输入文字', 'Enter text') },
        { value: 'launch_app', label: text('打开应用', 'Launch app') },
        { value: 'wait_for', label: text('等待控件出现', 'Wait for element') },
        { value: 'wait_gone', label: text('等待控件消失', 'Wait until element disappears') },
        { value: 'if_gone', label: text('如果控件消失，则执行后续步骤', 'If gone, execute following steps') },
        { value: 'if_else', label: text('条件分支 If / Else', 'Conditional If / Else') },
        { value: 'assert', label: text('断言控件存在', 'Assert element exists') },
        { value: 'assert_property', label: text('断言控件属性', 'Assert element property') },
        { value: 'scroll_find', label: text('滚动查找控件', 'Scroll until element appears') },
        { value: 'screenshot', label: text('截图检查点', 'Screenshot checkpoint') },
        { value: 'adb_command', label: text('ADB 命令', 'ADB shell command') },
    ];
    // Conditional blocks own the following steps; the card editor cannot
    // safely create or reshape them as a single-step edit.
    return props.excludeConditionalTypes
        ? options.filter(option => option.value !== 'if_gone' && option.value !== 'if_else')
        : options;
});
const keys = computed(() => [
    { value: 'HOME', label: text('主页', 'Home') }, { value: 'BACK', label: text('返回', 'Back') },
    { value: 'RECENT', label: text('最近任务', 'Recent apps') }, { value: 'WAKE', label: text('唤醒', 'Wake') },
]);
const positionModes = computed(() => [
    { value: 'element', label: text('控件定位', 'Element selector') },
    { value: 'coordinates', label: text('画面坐标', 'Screen coordinates') },
]);
const selectorModes = computed(() => [
    { value: 'native', label: text('组合控件条件', 'Combined element fields') },
    { value: 'xpath', label: 'XPath 1.0' },
]);
const propertyOptions = computed(() => [
    { value: 'enabled', label: text('已启用', 'Enabled') },
    { value: 'checked', label: text('已勾选', 'Checked') },
    { value: 'selected', label: text('已选中', 'Selected') },
    { value: 'clickable', label: text('可点击', 'Clickable') },
    { value: 'scrollable', label: text('可滚动', 'Scrollable') },
]);
const booleanOptions = computed(() => [
    { value: true, label: text('是 / True', 'True') },
    { value: false, label: text('否 / False', 'False') },
]);
const directionOptions = computed(() => [
    { value: 'down', label: text('向下查找（上滑列表）', 'Down (swipe up)') },
    { value: 'up', label: text('向上查找（下滑列表）', 'Up (swipe down)') },
]);
const inputModes = computed(() => [
    { value: 'literal', label: text('固定文字', 'Fixed text') },
    { value: 'parameter', label: text('运行时参数', 'Run parameter') },
]);
const api = new AutomationApi();
const favorites = ref<AutomationAdbFavorite[]>([]);
const hiddenBuiltinIds = ref<string[]>([]);
const presetBusy = ref(false);
const favoriteTitle = ref('');
const favoriteBusy = ref(false);
const favoriteMessage = ref('');
const favoriteError = ref('');
let favoriteLoadToken = 0;
const builtinOptions = computed(() => [
    { value: '', label: text('自定义命令', 'Custom command') },
    ...builtinAdbCommands.filter(item => !hiddenBuiltinIds.value.includes(item.id)).map(item => ({
        value: item.command, label: cn.value ? item.title : item.englishTitle,
    })),
]);
const selectedBuiltinPreset = computed(() => builtinAdbCommands.find(item =>
    item.command === props.modelValue.command && !hiddenBuiltinIds.value.includes(item.id)) ?? null);
const selectedBuiltin = computed(() => selectedBuiltinPreset.value?.command ?? '');
const favoriteOptions = computed(() => [
    { value: '', label: text('选择已收藏命令', 'Select saved command') },
    ...favorites.value.map(item => ({ value: item.id, label: item.title })),
]);
const selectedFavorite = computed(() =>
    favorites.value.find(item => item.command === props.modelValue.command) ?? null);
function chooseBuiltin(value: string) {
    const chosen = builtinAdbCommands.find(item => item.command === value);
    if (!chosen) return;
    update({ command: chosen.command, timeout_ms: props.modelValue.timeout_ms ?? 8000,
        title: props.modelValue.title?.trim() ? props.modelValue.title : cn.value ? chosen.title : chosen.englishTitle });
}
function chooseFavorite(value: string) {
    const chosen = favorites.value.find(item => item.id === value);
    if (!chosen) return;
    update({ command: chosen.command, timeout_ms: props.modelValue.timeout_ms ?? 8000,
        title: props.modelValue.title?.trim() ? props.modelValue.title : chosen.title });
    favoriteMessage.value = ''; favoriteError.value = '';
}
async function loadFavorites(owner: number) {
    const token = ++favoriteLoadToken;
    try {
        const [result, hidden] = await Promise.all([
            api.adbFavorites(owner), api.hiddenAdbPresets(owner),
        ]);
        if (token === favoriteLoadToken && props.ownerId === owner) {
            favorites.value = result.items;
            hiddenBuiltinIds.value = hidden.items;
            favoriteError.value = '';
        }
    } catch (cause) {
        if (token === favoriteLoadToken && props.ownerId === owner)
            favoriteError.value = '加载常用命令失败：' + automationErrorMessage(cause);
    }
}
watch(() => [props.ownerId, props.modelValue.type] as const, ([owner, kind]) => {
    ++favoriteLoadToken; favorites.value = []; hiddenBuiltinIds.value = [];
    favoriteMessage.value = ''; favoriteError.value = '';
    if (kind === 'adb_command' && owner && owner > 0) void loadFavorites(owner);
}, { immediate: true });
async function saveFavorite() {
    const owner = props.ownerId, command = props.modelValue.command?.trim() ?? '';
    if (!owner || favoriteBusy.value || props.disabled) return;
    if (!command.startsWith('adb shell ') || !command.slice(10).trim()) {
        favoriteError.value = '请输入完整的 adb shell 命令后再收藏。'; return;
    }
    const name = (favoriteTitle.value.trim() || props.modelValue.title?.trim() || '我的 ADB 命令');
    if (Array.from(name).length > 80) { favoriteError.value = '收藏名称最多 80 字。'; return; }
    favoriteBusy.value = true; favoriteError.value = '';
    try {
        const saved = await api.saveAdbFavorite(owner, name, command);
        if (props.ownerId !== owner) return;
        favorites.value = [saved, ...favorites.value.filter(item => item.id !== saved.id)];
        favoriteMessage.value = '已保存到此账号的常用 ADB 命令';
        favoriteTitle.value = '';
    } catch (cause) { favoriteError.value = automationErrorMessage(cause); }
    finally { favoriteBusy.value = false; }
}
async function deleteBuiltinPreset() {
    const owner = props.ownerId, chosen = selectedBuiltinPreset.value;
    if (!owner || !chosen || presetBusy.value || favoriteBusy.value || props.disabled) return;
    presetBusy.value = true; favoriteError.value = '';
    try {
        await api.hideAdbPreset(owner, chosen.id);
        if (props.ownerId !== owner) return;
        hiddenBuiltinIds.value = [...new Set([...hiddenBuiltinIds.value, chosen.id])];
        favoriteMessage.value = `已从当前账号的内置常用列表删除“${chosen.title}”；当前步骤命令仍保留`;
    } catch (cause) {
        favoriteError.value = '删除内置命令失败：' + automationErrorMessage(cause);
    } finally { presetBusy.value = false; }
}
async function restoreHiddenBuiltinPresets() {
    const owner = props.ownerId, ids = [...hiddenBuiltinIds.value];
    if (!owner || !ids.length || presetBusy.value || favoriteBusy.value || props.disabled) return;
    presetBusy.value = true; favoriteError.value = '';
    try {
        for (const id of ids) await api.restoreAdbPreset(owner, id);
        if (props.ownerId !== owner) return;
        hiddenBuiltinIds.value = [];
        favoriteMessage.value = '已恢复全部内置常用 ADB 命令';
    } catch (cause) {
        const message = '恢复内置命令失败：' + automationErrorMessage(cause);
        await loadFavorites(owner);
        favoriteError.value = message;
    } finally { presetBusy.value = false; }
}
async function deleteFavorite() {
    const owner = props.ownerId, chosen = selectedFavorite.value;
    if (!owner || !chosen || favoriteBusy.value || props.disabled) return;
    favoriteBusy.value = true; favoriteError.value = '';
    try {
        await api.deleteAdbFavorite(owner, chosen.id);
        if (props.ownerId !== owner) return;
        favorites.value = favorites.value.filter(item => item.id !== chosen.id);
        favoriteMessage.value = '已从常用命令中移除；当前步骤命令仍保留';
    } catch (cause) { favoriteError.value = automationErrorMessage(cause); }
    finally { favoriteBusy.value = false; }
}
const coordinateFields = computed(() => ['swipe', 'drag'].includes(props.modelValue.type)
    ? [{ key: 'x', label: text('起点 X', 'Start X') }, { key: 'y', label: text('起点 Y', 'Start Y') },
        { key: 'to_x', label: text('终点 X', 'End X') }, { key: 'to_y', label: text('终点 Y', 'End Y') }]
    : [{ key: 'x', label: 'X' }, { key: 'y', label: 'Y' }]);

function update(patch: Partial<AutomationStep>) {
    if (props.disabled) return;
    const next: AutomationStep = { ...props.modelValue, ...patch };
    // The input displays 8000 as a fallback; ensure the fallback is also
    // materialized in the step model whenever an existing ADB step is edited.
    if (next.type === 'adb_command' && next.timeout_ms == null) next.timeout_ms = 8000;
    emit('update:modelValue', next);
}
function changeType(type: AutomationStepType) {
    if (props.disabled || props.lockActionType || type === props.modelValue.type) return;
    const step: AutomationStep = { id: props.modelValue.id, type,
        ...(props.modelValue.title ? { title: props.modelValue.title } : {}) };
    if (type === 'key') step.key = 'HOME';
    if (['tap', 'long_press', 'wait_for', 'wait_gone', 'if_gone', 'if_else', 'assert', 'assert_property', 'scroll_find'].includes(type)) step.selector = { resource_id: '' };
    if (type === 'swipe' || type === 'drag') Object.assign(step, {
        x: 0.5, y: 0.8, to_x: 0.5, to_y: 0.2,
        duration_ms: type === 'drag' ? 700 : 400,
    });
    if (type === 'long_press') step.duration_ms = 800;
    if (type === 'if_gone') step.skip_count = 1;
    if (type === 'if_else') { step.then_count = 1; step.else_count = 1; }
    if (type === 'input_text') step.text = '';
    if (type === 'launch_app') step.package = '';
    if (type === 'adb_command') { step.command = ''; step.timeout_ms = 8000; }
    if (['wait_for', 'wait_gone', 'assert', 'scroll_find'].includes(type)) step.timeout_ms = 10000;
    if (type === 'assert_property') { step.property = 'enabled'; step.expected = true; }
    if (type === 'scroll_find') {
        step.scroll_container = { resource_id: '' };
        step.direction = 'down'; step.max_swipes = 3; step.duration_ms = 350;
    }
    emit('update:modelValue', step);
}
function positionMode(mode: string) {
    if (props.disabled) return;
    const step = { ...props.modelValue };
    if (mode === 'element') {
        step.selector = { resource_id: '' };
        delete step.x; delete step.y; delete step.width; delete step.height;
    } else {
        delete step.selector;
        step.x = 0.5; step.y = 0.5;
    }
    emit('update:modelValue', step);
}
function inputMode(mode: string) {
    if (props.disabled) return;
    const step = { ...props.modelValue };
    if (mode === 'parameter') { delete step.text; step.parameter = ''; }
    else { delete step.parameter; step.text = ''; }
    emit('update:modelValue', step);
}
function changeSelectorMode(value: string) {
    if (props.disabled || !props.modelValue.selector) return;
    update({ selector: value === 'xpath' ? { xpath: '//*[@text=""]' } : { resource_id: '' } });
}
function selectorValue(field: keyof AutomationSelector, value: string | undefined) {
    update({ selector: { ...props.modelValue.selector, [field]: value ?? '' } });
}
function clickableValue(value: string) {
    if (props.disabled || !props.modelValue.selector) return;
    const selector = { ...props.modelValue.selector };
    if (value === 'unset') delete selector.clickable;
    else if (value === 'true' || value === 'false') selector.clickable = value === 'true';
    else return;
    update({ selector });
}
const clickableOptions = [
    { label: '不使用', value: 'unset' },
    { label: '可点击', value: 'true' },
    { label: '不可点击', value: 'false' },
];
const booleanFields: { key: 'enabled' | 'long_clickable' | 'focusable' | 'checkable' | 'scrollable'; label: string }[] = [
    { key: 'enabled', label: '是否启用' },
    { key: 'long_clickable', label: '是否支持长按' },
    { key: 'focusable', label: '是否可聚焦' },
    { key: 'checkable', label: '是否可勾选' },
    { key: 'scrollable', label: '是否可滚动' },
];
const selectorBooleanOptions = [
    { label: '不使用', value: 'unset' },
    { label: '是', value: 'true' },
    { label: '否', value: 'false' },
];
function booleanValue(field: typeof booleanFields[number]['key'], value: string) {
    if (props.disabled || !props.modelValue.selector) return;
    const selector = { ...props.modelValue.selector };
    if (value === 'unset') delete selector[field];
    else if (value === 'true' || value === 'false') selector[field] = value === 'true';
    else return;
    update({ selector });
}
function scrollContainerValue(field: keyof AutomationSelector, value: string | undefined) {
    if (props.disabled) return;
    const scroll_container: AutomationSelector = { ...props.modelValue.scroll_container, [field]: value ?? '' };
    update({ scroll_container });
}
function numberValue(key: string): number | undefined {
    return (props.modelValue as unknown as Record<string, number | undefined>)[key];
}
function updateNumber(key: string, value: number | null | undefined) {
    update({ [key]: value ?? undefined });
}
</script>

<template>
    <div class="step-editor">
        <label class="field" :for="fieldId('title')">
            <span>{{ text('步骤标题', 'Step title') }}</span>
            <InputText :id="fieldId('title')" :model-value="modelValue.title ?? ''"
                :maxlength="80" :disabled="disabled" autocomplete="off"
                :placeholder="text('不填写则显示动作类型', 'Defaults to action name')"
                @update:model-value="update({title: String($event ?? '').trim() ? String($event) : undefined})"/>
            <small>{{ text('步骤列表会优先显示标题，不改变动作的实际类型。', 'The timeline displays this title instead of the action label.') }}</small>
        </label>
        <div class="field" :for="fieldId('type')">
            <span>{{ text('操作类型', 'Action') }}</span>
            <AutomationInlineSelect :input-id="fieldId('type')" :model-value="modelValue.type" :options="actionOptions" option-label="label" option-value="value" :disabled="disabled || lockActionType || ['if_gone','if_else'].includes(modelValue.type)" @update:model-value="changeType($event as AutomationStepType)" />
        </div>

        <small v-if="['if_gone','if_else'].includes(modelValue.type)" class="hint">{{ text('条件节点的类型已锁定。需要修改类型时请删除整个条件块后重建。', 'Conditional types are locked. Remove the whole condition block to change the type.') }}</small>
        <template v-if="modelValue.type === 'adb_command'">
            <div class="field" :for="fieldId('adb-preset')">
                <span>{{ text('内置常用 ADB 命令', 'Built-in ADB commands') }}</span>
                <AutomationInlineSelect :input-id="fieldId('adb-preset')"
                    :model-value="selectedBuiltin" :options="builtinOptions"
                    option-label="label" option-value="value" :disabled="disabled"
                    @update:model-value="chooseBuiltin(String($event))" />
            </div>
            <div class="adb-favorite-actions">
                <button v-if="selectedBuiltinPreset" type="button"
                    :disabled="disabled || presetBusy || favoriteBusy" @click="deleteBuiltinPreset">
                    {{ text('删除选中的内置命令', 'Delete selected built-in command') }}
                </button>
                <button v-if="hiddenBuiltinIds.length" type="button"
                    :disabled="disabled || presetBusy || favoriteBusy" @click="restoreHiddenBuiltinPresets">
                    {{ text('恢复已删除内置命令', 'Restore deleted built-ins') }} ({{ hiddenBuiltinIds.length }})
                </button>
            </div>
            <small v-if="selectedBuiltinPreset" class="hint">
                {{ text('删除只影响当前账号的内置命令列表，不会修改已经填入步骤的命令。', 'Deletion only hides this preset for your account; existing step commands are unchanged.') }}
            </small>
            <div v-if="ownerId" class="field" :for="fieldId('adb-favorite')">
                <span>{{ text('我收藏的常用命令', 'My saved ADB commands') }}</span>
                <AutomationInlineSelect :input-id="fieldId('adb-favorite')"
                    :model-value="selectedFavorite?.id ?? ''" :options="favoriteOptions"
                    option-label="label" option-value="value" :disabled="disabled || favoriteBusy"
                    @update:model-value="chooseFavorite(String($event))" />
            </div>
            <label class="field" :for="fieldId('adb-command')">
                <span>{{ text('手动输入 ADB 命令', 'ADB shell command') }}</span>
                <Textarea :id="fieldId('adb-command')" :model-value="modelValue.command ?? ''"
                    :rows="3" :maxlength="1024" :disabled="disabled"
                    autocomplete="off" spellcheck="false"
                    placeholder="adb shell input keyevent 3"
                    @update:model-value="update({command: String($event ?? '')})"/>
                <small>{{ text('仅支持当前手机的 adb shell 单条命令；禁止宿主机操作、管道和重定向。执行结果写入脚本记录，请勿填入密码。', 'One adb shell command against this phone only. No host operations, pipes or redirects. Do not include secrets.') }}</small>
            </label>
            <label class="field" :for="fieldId('adb-timeout')">
                <span>{{ text('ADB 命令超时（毫秒）', 'ADB timeout (ms)') }}</span>
                <InputNumber :input-id="fieldId('adb-timeout')"
                    :model-value="modelValue.timeout_ms ?? 8000"
                    :min="1000" :max="20000" :step="1000" :use-grouping="false"
                    :disabled="disabled" @update:model-value="updateNumber('timeout_ms', $event)" />
            </label>
            <div v-if="ownerId" class="adb-favorite-actions">
                <InputText v-model="favoriteTitle" :maxlength="80" :disabled="disabled || favoriteBusy"
                    :placeholder="text('收藏名称（默认使用步骤标题）', 'Favorite name (defaults to title)')" />
                <button type="button" :disabled="disabled || favoriteBusy || !modelValue.command?.trim()"
                    @click="saveFavorite">{{ text('保存为常用', 'Save as favorite') }}</button>
                <button v-if="selectedFavorite" type="button"
                    :disabled="disabled || favoriteBusy" @click="deleteFavorite">
                    {{ text('移除常用', 'Remove favorite') }}
                </button>
            </div>
            <small v-if="favoriteMessage" class="hint" role="status">{{ favoriteMessage }}</small>
            <small v-if="favoriteError" class="adb-favorite-error" role="alert">{{ favoriteError }}</small>
        </template>

        <div v-if="modelValue.type === 'key'" class="field" :for="fieldId('key')">
            <span>{{ text('导航按键', 'Navigation key') }}</span>
            <AutomationInlineSelect :input-id="fieldId('key')" :model-value="modelValue.key" :options="keys" option-label="label" option-value="value" :disabled="disabled" @update:model-value="update({ key: String($event) as AutomationKey })" />
        </div>

        <div v-if="modelValue.type === 'tap' || modelValue.type === 'long_press'" class="field" :for="fieldId('position')">
            <span>{{ text('定位方式', 'Targeting') }}</span>
            <AutomationInlineSelect :input-id="fieldId('position')" :model-value="modelValue.selector ? 'element' : 'coordinates'" :options="positionModes" option-label="label" option-value="value" :disabled="disabled" @update:model-value="positionMode(String($event))" />
        </div>

        <template v-if="modelValue.type === 'input_text'">
            <div class="field" :for="fieldId('input-mode')">
                <span>{{ text('文字来源', 'Text source') }}</span>
                <AutomationInlineSelect :input-id="fieldId('input-mode')" :model-value="modelValue.parameter !== undefined ? 'parameter' : 'literal'" :options="inputModes" option-label="label" option-value="value" :disabled="disabled" @update:model-value="inputMode(String($event))" />
            </div>
            <label v-if="modelValue.parameter !== undefined" class="field" :for="fieldId('parameter')">
                <span>{{ text('参数名称', 'Parameter name') }}</span>
                <InputText :id="fieldId('parameter')" :model-value="modelValue.parameter" :maxlength="64" :disabled="disabled" autocomplete="off" @update:model-value="update({ parameter: $event ?? '' })" />
                <small>{{ text('执行前填写参数值；密码等内容请使用参数。', 'Enter its value before running. Use parameters for sensitive text.') }}</small>
            </label>
            <label v-else class="field" :for="fieldId('text')">
                <span>{{ text('输入内容', 'Text') }}</span>
                <Textarea :id="fieldId('text')" :model-value="modelValue.text" :rows="3" :maxlength="2000" :disabled="disabled" autocomplete="off" @update:model-value="update({ text: $event ?? '' })" />
                <small>{{ text('固定文字会随脚本保存，请勿填入密码。', 'Fixed text is saved with the script. Do not put passwords here.') }}</small>
            </label>
            <label class="checkbox-field" :for="fieldId('target-input')">
                <Checkbox :input-id="fieldId('target-input')" :model-value="!!modelValue.selector" binary :disabled="disabled" @update:model-value="update({ selector: $event ? { resource_id: '' } : undefined })" />
                <span>{{ text('指定输入控件', 'Choose an input element') }}</span>
            </label>
            <small v-if="!modelValue.selector" class="hint">{{ text('将文字输入当前已聚焦的输入框。', 'Text goes to the currently focused input field.') }}</small>
        </template>

        <fieldset v-if="modelValue.selector && ['tap', 'long_press', 'input_text', 'wait_for', 'wait_gone', 'if_gone', 'if_else', 'assert', 'assert_property', 'scroll_find'].includes(modelValue.type)" class="selector-fields">
            <legend>{{ text('控件条件（至少填写一项）', 'Element conditions (at least one)') }}</legend>
            <div class="field" :for="fieldId('selector-mode')">
                <span>{{ text('定位策略', 'Locator mode') }}</span>
                <AutomationInlineSelect :input-id="fieldId('selector-mode')" :model-value="modelValue.selector.xpath !== undefined ? 'xpath' : 'native'"
                    :options="selectorModes" option-label="label" option-value="value"
                    :disabled="disabled || ['input_text', 'assert_property'].includes(modelValue.type)" @update:model-value="changeSelectorMode(String($event))" />
            </div>
            <label v-if="modelValue.selector.xpath !== undefined" class="field" :for="fieldId('xpath')">
                <span>XPath 1.0</span>
                <Textarea :id="fieldId('xpath')" :model-value="modelValue.selector.xpath" :rows="2" :maxlength="512"
                    :disabled="disabled" @update:model-value="selectorValue('xpath', $event)" />
                <small>{{ text('例如 //*[@text="设置"] 或 //android.widget.Button[@text="确定"]；匹配多个控件会停止。', 'Example: //*[@text="Settings"]. Ambiguous matches stop execution.') }}</small>
            </label>
            <label v-if="modelValue.selector.xpath === undefined" class="field" :for="fieldId('resource-id')">
                <span>Resource ID</span>
                <InputText :id="fieldId('resource-id')" :model-value="modelValue.selector.resource_id" :maxlength="256" :disabled="disabled" @update:model-value="selectorValue('resource_id', $event)" />
            </label>
            <label v-if="modelValue.selector.xpath === undefined" class="field" :for="fieldId('description')">
                <span>{{ text('无障碍描述', 'Accessibility description') }}</span>
                <InputText :id="fieldId('description')" :model-value="modelValue.selector.description" :maxlength="256" :disabled="disabled" @update:model-value="selectorValue('description', $event)" />
            </label>
            <label v-if="modelValue.selector.xpath === undefined" class="field" :for="fieldId('selector-text')">
                <span>{{ text('控件文字', 'Element text') }}</span>
                <InputText :id="fieldId('selector-text')" :model-value="modelValue.selector.text" :maxlength="256" :disabled="disabled" @update:model-value="selectorValue('text', $event)" />
            </label>
            <template v-if="modelValue.selector.xpath === undefined">
                <label class="field" :for="fieldId('selector-class')">
                    <span>{{ text('控件类名', 'Class name') }}</span>
                    <InputText :id="fieldId('selector-class')" :model-value="modelValue.selector.class_name" :maxlength="256" :disabled="disabled" @update:model-value="selectorValue('class_name', $event)" />
                </label>
                <label class="field" :for="fieldId('selector-package')">
                    <span>{{ text('应用包名', 'Package') }}</span>
                    <InputText :id="fieldId('selector-package')" :model-value="modelValue.selector.package" :maxlength="256" :disabled="disabled" @update:model-value="selectorValue('package', $event)" />
                </label>
                <div class="field" :for="fieldId('selector-clickable')">
                    <span>{{ text('可点击状态', 'Clickable state') }}</span>
                    <AutomationInlineSelect :input-id="fieldId('selector-clickable')"
                        :model-value="modelValue.selector.clickable === undefined ? 'unset' : modelValue.selector.clickable ? 'true' : 'false'"
                        :options="clickableOptions" option-label="label" option-value="value"
                        :disabled="disabled" @update:model-value="clickableValue(String($event))" />
                </div>
                <div v-for="field in booleanFields" :key="field.key" class="field" :for="fieldId('selector-' + field.key)">
                    <span>{{ field.label }}</span>
                    <AutomationInlineSelect :input-id="fieldId('selector-' + field.key)"
                        :model-value="modelValue.selector[field.key] === undefined ? 'unset' : modelValue.selector[field.key] ? 'true' : 'false'"
                        :options="selectorBooleanOptions" option-label="label" option-value="value"
                        :disabled="disabled" @update:model-value="booleanValue(field.key, String($event))" />
                </div>
                <small>{{ text('布尔条件支持“是 / 否”并可与其他属性组合；执行时仍必须匹配唯一控件。', 'Boolean conditions support true / false in combinations; runtime match must still be unique.') }}</small>
            </template>
        </fieldset>

        <template v-if="['swipe', 'drag'].includes(modelValue.type) || (['tap', 'long_press'].includes(modelValue.type) && !modelValue.selector)">
            <p class="hint">{{ text('坐标范围为 0–1，以有效手机画面为基准；不同应用布局需重新校准。', 'Coordinates range from 0 to 1 within the phone image. Different layouts need calibration.') }}</p>
            <div class="number-grid">
                <label v-for="field in coordinateFields" :key="field.key" class="field" :for="fieldId(field.key)">
                    <span>{{ field.label }}</span>
                    <InputNumber :input-id="fieldId(field.key)" :model-value="numberValue(field.key)" :min="0" :max="1" :step="0.01" :max-fraction-digits="4" :disabled="disabled" @update:model-value="updateNumber(field.key, $event)" />
                </label>
                <label class="field" :for="fieldId('width')">
                    <span>{{ text('参考画面宽度（像素）', 'Reference width (px)') }}</span>
                    <InputNumber :input-id="fieldId('width')" :model-value="modelValue.width" :min="1" :max="16384" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('width', $event)" />
                </label>
                <label class="field" :for="fieldId('height')">
                    <span>{{ text('参考画面高度（像素）', 'Reference height (px)') }}</span>
                    <InputNumber :input-id="fieldId('height')" :model-value="modelValue.height" :min="1" :max="16384" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('height', $event)" />
                </label>
            </div>
        </template>

        <template v-if="modelValue.type === 'assert_property'">
            <div class="field" :for="fieldId('property')">
                <span>{{ text('断言属性', 'Property to assert') }}</span>
                <AutomationInlineSelect :input-id="fieldId('property')" :model-value="modelValue.property"
                    :options="propertyOptions" option-label="label" option-value="value"
                    :disabled="disabled" @update:model-value="update({ property: String($event) as AutomationStep['property'] })" />
            </div>
            <div class="field" :for="fieldId('expected')">
                <span>{{ text('预期状态', 'Expected state') }}</span>
                <AutomationInlineSelect :input-id="fieldId('expected')" :model-value="modelValue.expected"
                    :options="booleanOptions" option-label="label" option-value="value"
                    :disabled="disabled" @update:model-value="update({ expected: $event as boolean })" />
            </div>
        </template>

        <fieldset v-if="modelValue.type === 'scroll_find'" class="selector-fields">
            <legend>{{ text('滚动容器（必须唯一且可滚动）', 'Unique scrollable container') }}</legend>
            <label class="field" :for="fieldId('scroll-resource-id')">
                <span>Resource ID</span>
                <InputText :id="fieldId('scroll-resource-id')" :model-value="modelValue.scroll_container?.resource_id"
                    :disabled="disabled" :maxlength="256" @update:model-value="scrollContainerValue('resource_id', $event)" />
            </label>
            <label class="field" :for="fieldId('scroll-class')">
                <span>{{ text('控件类名', 'Class name') }}</span>
                <InputText :id="fieldId('scroll-class')" :model-value="modelValue.scroll_container?.class_name"
                    :disabled="disabled" :maxlength="256" @update:model-value="scrollContainerValue('class_name', $event)" />
            </label>
            <label class="field" :for="fieldId('scroll-package')">
                <span>{{ text('应用包名', 'Package name') }}</span>
                <InputText :id="fieldId('scroll-package')" :model-value="modelValue.scroll_container?.package"
                    :disabled="disabled" :maxlength="256" @update:model-value="scrollContainerValue('package', $event)" />
            </label>
            <div class="field" :for="fieldId('direction')">
                <span>{{ text('查找方向', 'Search direction') }}</span>
                <AutomationInlineSelect :input-id="fieldId('direction')" :model-value="modelValue.direction"
                    :options="directionOptions" option-label="label" option-value="value"
                    :disabled="disabled" @update:model-value="update({ direction: String($event) as AutomationStep['direction'] })" />
            </div>
            <label class="field" :for="fieldId('max-swipes')">
                <span>{{ text('最多滚动次数', 'Maximum swipes') }}</span>
                <InputNumber :input-id="fieldId('max-swipes')" :model-value="modelValue.max_swipes"
                    :min="1" :max="5" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('max_swipes', $event)" />
            </label>
            <small>{{ text('先检查目标是否存在，再滚动指定容器。每次仅发送一个手势；结果不确定立即停止。', 'Check before scrolling; only one gesture per pass. Uncertain actions stop immediately.') }}</small>
        </fieldset>

        <label v-if="['swipe', 'drag', 'long_press', 'scroll_find'].includes(modelValue.type)" class="field" :for="fieldId('duration')">
            <span>{{ modelValue.type === 'long_press' ? text('按住时长（毫秒）', 'Press duration (ms)')
                : modelValue.type === 'drag' ? text('拖拽时长（毫秒）', 'Drag duration (ms)') : text('滑动时长（毫秒）', 'Swipe duration (ms)') }}</span>
            <InputNumber :input-id="fieldId('duration')" :model-value="modelValue.duration_ms"
                :min="modelValue.type === 'long_press' ? 400 : modelValue.type === 'drag' ? 300 : modelValue.type === 'scroll_find' ? 200 : 100"
                :max="modelValue.type === 'scroll_find' ? 1000 : 3000" :step="100" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('duration_ms', $event)" />
        </label>

        <label v-if="modelValue.type === 'launch_app'" class="field" :for="fieldId('package')">
            <span>{{ text('Android 应用包名', 'Android package name') }}</span>
            <InputText :id="fieldId('package')" :model-value="modelValue.package" :maxlength="255" :disabled="disabled" autocomplete="off" placeholder="com.android.settings" @update:model-value="update({ package: $event ?? '' })" />
        </label>

        <fieldset v-if="modelValue.type === 'if_else'" class="selector-fields">
            <legend>{{ text('Then / Else 条件区域', 'Then / Else branches') }}</legend>
            <p class="hint">{{ text('只读检查：控件已消失时执行 Then；控件仍存在时执行 Else。两段步骤必须依次排在本步骤之后；未选分支会显示为已跳过。', 'Read-only check: missing element executes Then, present element executes Else. Arrange both blocks immediately after this step; the other branch is skipped.') }}</p>
            <label class="field" :for="fieldId('then-count')">
                <span>{{ text('Then 分支步数（控件消失）', 'Then steps (element gone)') }}</span>
                <InputNumber :input-id="fieldId('then-count')" :model-value="modelValue.then_count"
                    :min="1" :max="20" :use-grouping="false" disabled @update:model-value="updateNumber('then_count', $event)" />
            </label>
            <label class="field" :for="fieldId('else-count')">
                <span>{{ text('Else 分支步数（控件存在）', 'Else steps (element present)') }}</span>
                <InputNumber :input-id="fieldId('else-count')" :model-value="modelValue.else_count"
                    :min="1" :max="20" :use-grouping="false" disabled @update:model-value="updateNumber('else_count', $event)" />
            </label>
            <small>{{ text('各分支步数由流程编辑器根据增删步骤自动维护，最多嵌套 3 层，每段 1–20 步。', 'Branch sizes are computed from the flow editor. Up to three levels, 1–20 steps per lane.') }}</small>
        </fieldset>
        <label v-if="modelValue.type === 'if_gone'" class="field" :for="fieldId('skip-count')">
            <span>{{ text('条件控制范围（后续步骤数）', 'Conditional range (following steps)') }}</span>
            <InputNumber :input-id="fieldId('skip-count')" :model-value="modelValue.skip_count" :min="1" :max="20"
                :use-grouping="false" disabled @update:model-value="updateNumber('skip_count', $event)" />
            <small>{{ text('控件消失时进入 Then，仍存在时跳过。区域步数由流程编辑器自动维护。', 'Run the Then lane when gone; otherwise skip it. The flow editor maintains its size.') }}</small>
        </label>

        <label v-if="['wait_for', 'wait_gone', 'assert', 'scroll_find'].includes(modelValue.type)" class="field" :for="fieldId('timeout')">
            <span>{{ text('最长等待（毫秒）', 'Maximum wait (ms)') }}</span>
            <InputNumber :input-id="fieldId('timeout')" :model-value="modelValue.timeout_ms" :min="100" :max="30000" :step="100" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('timeout_ms', $event)" />
            <small>{{ text('仅在本次步骤内检查条件；超时后停止该设备后续步骤。', 'Checks only during this step. A timeout stops subsequent steps on this device.') }}</small>
        </label>

        <p v-if="modelValue.type === 'screenshot'" class="hint">{{ text('保存该步骤的画面证据；截图成功不代表应用业务成功。', 'Save screen evidence for this step. A screenshot does not prove the app operation succeeded.') }}</p>
    </div>
</template>

<style scoped>
.step-editor { display: grid; gap: 16px; min-width: 0; }
.field { display: flex; flex-direction: column; gap: 7px; min-width: 0; font-size: 13px; }
.field > span { font-weight: 550; }
.field small, .hint, .selector-fields > small { color: var(--console-muted); font-size: 12px; line-height: 1.6; }
.field :deep(.p-inputnumber-input) { width: 100%; }
.field :deep(.p-inputtext), .field :deep(.p-select), .field :deep(.p-inputnumber) { width: 100%; min-width: 0; }
.selector-fields { border: 1px solid var(--console-border); border-radius: 10px; padding: 12px; display: grid; gap: 12px; }
.selector-fields legend { padding: 0 5px; font-size: 12px; color: var(--console-muted); }
.number-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.checkbox-field { display: flex; align-items: center; gap: 9px; font-size: 13px; }
.adb-favorite-actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
.adb-favorite-actions :deep(.p-inputtext) { flex: 1 1 180px; min-width: 0; }
.adb-favorite-actions button { padding: 7px 10px; border: 1px solid var(--console-border); border-radius: 6px; color: var(--p-primary-color); background: transparent; cursor: pointer; font-size: 12px; }
.adb-favorite-actions button:disabled { opacity: .5; cursor: not-allowed; }
.adb-favorite-error { color: var(--p-red-500); white-space: normal; overflow-wrap: anywhere; }
@media (max-width: 480px) { .number-grid { grid-template-columns: 1fr; } }
</style>
