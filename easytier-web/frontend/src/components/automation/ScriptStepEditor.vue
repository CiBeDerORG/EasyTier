<script setup lang="ts">
import { computed } from 'vue';
import { Checkbox, InputNumber, InputText, Select, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import type { AutomationKey, AutomationSelector, AutomationStep, AutomationStepType } from '../../types/automation';

const props = defineProps<{ modelValue: AutomationStep; disabled?: boolean }>();
const emit = defineEmits<{ 'update:modelValue': [step: AutomationStep] }>();
const { locale } = useI18n();
const cn = computed(() => locale.value === 'cn' || locale.value.startsWith('zh'));
const text = (zh: string, en: string) => cn.value ? zh : en;
const fieldId = (name: string) => `automation-step-${props.modelValue.id}-${name}`;
const actionOptions = computed(() => [
    { value: 'key', label: text('导航按键', 'Navigation key') },
    { value: 'tap', label: text('点击', 'Tap') },
    { value: 'swipe', label: text('滑动', 'Swipe') },
    { value: 'input_text', label: text('输入文字', 'Enter text') },
    { value: 'launch_app', label: text('打开应用', 'Launch app') },
    { value: 'wait_for', label: text('等待控件出现', 'Wait for element') },
    { value: 'wait_gone', label: text('等待控件消失', 'Wait until element disappears') },
    { value: 'assert', label: text('断言控件存在', 'Assert element exists') },
    { value: 'assert_property', label: text('断言控件属性', 'Assert element property') },
    { value: 'scroll_find', label: text('滚动查找控件', 'Scroll until element appears') },
    { value: 'screenshot', label: text('截图检查点', 'Screenshot checkpoint') },
]);
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
const coordinateFields = computed(() => props.modelValue.type === 'swipe'
    ? [{ key: 'x', label: text('起点 X', 'Start X') }, { key: 'y', label: text('起点 Y', 'Start Y') },
        { key: 'to_x', label: text('终点 X', 'End X') }, { key: 'to_y', label: text('终点 Y', 'End Y') }]
    : [{ key: 'x', label: 'X' }, { key: 'y', label: 'Y' }]);

function update(patch: Partial<AutomationStep>) {
    if (!props.disabled) emit('update:modelValue', { ...props.modelValue, ...patch });
}
function changeType(type: AutomationStepType) {
    if (props.disabled || type === props.modelValue.type) return;
    const step: AutomationStep = { id: props.modelValue.id, type };
    if (type === 'key') step.key = 'HOME';
    if (['tap', 'wait_for', 'wait_gone', 'assert', 'assert_property', 'scroll_find'].includes(type)) step.selector = { resource_id: '' };
    if (type === 'swipe') Object.assign(step, { x: 0.5, y: 0.8, to_x: 0.5, to_y: 0.2, duration_ms: 400 });
    if (type === 'input_text') step.text = '';
    if (type === 'launch_app') step.package = '';
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
        <label class="field" :for="fieldId('type')">
            <span>{{ text('操作类型', 'Action') }}</span>
            <Select :input-id="fieldId('type')" :model-value="modelValue.type" :options="actionOptions" option-label="label" option-value="value" :disabled="disabled" @update:model-value="changeType" />
        </label>

        <label v-if="modelValue.type === 'key'" class="field" :for="fieldId('key')">
            <span>{{ text('导航按键', 'Navigation key') }}</span>
            <Select :input-id="fieldId('key')" :model-value="modelValue.key" :options="keys" option-label="label" option-value="value" :disabled="disabled" @update:model-value="update({ key: $event as AutomationKey })" />
        </label>

        <label v-if="modelValue.type === 'tap'" class="field" :for="fieldId('position')">
            <span>{{ text('定位方式', 'Targeting') }}</span>
            <Select :input-id="fieldId('position')" :model-value="modelValue.selector ? 'element' : 'coordinates'" :options="positionModes" option-label="label" option-value="value" :disabled="disabled" @update:model-value="positionMode" />
        </label>

        <template v-if="modelValue.type === 'input_text'">
            <label class="field" :for="fieldId('input-mode')">
                <span>{{ text('文字来源', 'Text source') }}</span>
                <Select :input-id="fieldId('input-mode')" :model-value="modelValue.parameter !== undefined ? 'parameter' : 'literal'" :options="inputModes" option-label="label" option-value="value" :disabled="disabled" @update:model-value="inputMode" />
            </label>
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

        <fieldset v-if="modelValue.selector && ['tap', 'input_text', 'wait_for', 'wait_gone', 'assert', 'assert_property', 'scroll_find'].includes(modelValue.type)" class="selector-fields">
            <legend>{{ text('控件条件（至少填写一项）', 'Element conditions (at least one)') }}</legend>
            <label class="field" :for="fieldId('selector-mode')">
                <span>{{ text('定位策略', 'Locator mode') }}</span>
                <Select :input-id="fieldId('selector-mode')" :model-value="modelValue.selector.xpath !== undefined ? 'xpath' : 'native'"
                    :options="selectorModes" option-label="label" option-value="value"
                    :disabled="disabled || ['input_text', 'assert_property'].includes(modelValue.type)" @update:model-value="changeSelectorMode" />
            </label>
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
                <small>{{ text('多个非空条件需同时匹配且只命中一个控件；文字输入仅支持组合控件条件。', 'All nonempty fields must uniquely match. Text input supports native fields only.') }}</small>
            </template>
        </fieldset>

        <template v-if="modelValue.type === 'swipe' || (modelValue.type === 'tap' && !modelValue.selector)">
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
            <label class="field" :for="fieldId('property')">
                <span>{{ text('断言属性', 'Property to assert') }}</span>
                <Select :input-id="fieldId('property')" :model-value="modelValue.property"
                    :options="propertyOptions" option-label="label" option-value="value"
                    :disabled="disabled" @update:model-value="update({ property: $event })" />
            </label>
            <label class="field" :for="fieldId('expected')">
                <span>{{ text('预期状态', 'Expected state') }}</span>
                <Select :input-id="fieldId('expected')" :model-value="modelValue.expected"
                    :options="booleanOptions" option-label="label" option-value="value"
                    :disabled="disabled" @update:model-value="update({ expected: $event })" />
            </label>
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
            <label class="field" :for="fieldId('direction')">
                <span>{{ text('查找方向', 'Search direction') }}</span>
                <Select :input-id="fieldId('direction')" :model-value="modelValue.direction"
                    :options="directionOptions" option-label="label" option-value="value"
                    :disabled="disabled" @update:model-value="update({ direction: $event })" />
            </label>
            <label class="field" :for="fieldId('max-swipes')">
                <span>{{ text('最多滚动次数', 'Maximum swipes') }}</span>
                <InputNumber :input-id="fieldId('max-swipes')" :model-value="modelValue.max_swipes"
                    :min="1" :max="5" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('max_swipes', $event)" />
            </label>
            <small>{{ text('先检查目标是否存在，再滚动指定容器。每次仅发送一个手势；结果不确定立即停止。', 'Check before scrolling; only one gesture per pass. Uncertain actions stop immediately.') }}</small>
        </fieldset>

        <label v-if="modelValue.type === 'swipe' || modelValue.type === 'scroll_find'" class="field" :for="fieldId('duration')">
            <span>{{ text('滑动时长（毫秒）', 'Swipe duration (ms)') }}</span>
            <InputNumber :input-id="fieldId('duration')" :model-value="modelValue.duration_ms" :min="modelValue.type === 'scroll_find' ? 200 : 100" :max="modelValue.type === 'scroll_find' ? 1000 : 3000" :step="100" :use-grouping="false" :disabled="disabled" @update:model-value="updateNumber('duration_ms', $event)" />
        </label>

        <label v-if="modelValue.type === 'launch_app'" class="field" :for="fieldId('package')">
            <span>{{ text('Android 应用包名', 'Android package name') }}</span>
            <InputText :id="fieldId('package')" :model-value="modelValue.package" :maxlength="255" :disabled="disabled" autocomplete="off" placeholder="com.android.settings" @update:model-value="update({ package: $event ?? '' })" />
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
@media (max-width: 480px) { .number-grid { grid-template-columns: 1fr; } }
</style>
