import type { PickerMenuAction } from './automation-picker-state';

/** Actions supported by UiInspector without bypassing locator confirmation. */
export type InspectorAction = 'tap' | 'long_press' | 'input_text'
    | 'wait_for' | 'wait_gone' | 'assert' | 'assert_property' | 'scroll_find';

export const inspectorActionLabels: Record<InspectorAction,string> = {
    tap:'点击', long_press:'长按', input_text:'输入文字', wait_for:'等待出现',
    wait_gone:'等待消失', assert:'断言存在', assert_property:'属性断言',
    scroll_find:'滚动查找',
};

export function menuToInspectorAction(menu: PickerMenuAction): InspectorAction | null {
    switch(menu) {
        case 'tap': case 'long_press': case 'input_text': case 'wait_for':
        case 'wait_gone': case 'assert': case 'assert_property': return menu;
        default: return null;
    }
}
