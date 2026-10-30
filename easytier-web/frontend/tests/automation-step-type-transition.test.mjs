import assert from 'node:assert/strict';
import test from 'node:test';
import { switchTapLongPressStep } from '../src/modules/automation-step-type-transition.ts';

const id = '11111111-1111-4111-8111-111111111111';

test('tap to long press keeps every selected element condition intact', () => {
  const selector = {
    resource_id: 'com.example:id/confirm', description: '确认', text: '提交',
    class_name: 'android.widget.Button', package: 'com.example',
    clickable: false, enabled: true, long_clickable: true, focusable: false,
    checkable: false, scrollable: false,
  };
  const tap = { id, title: '确认操作', type: 'tap', selector };
  const press = switchTapLongPressStep(tap, 'long_press');
  assert.equal(press.type, 'long_press');
  assert.equal(press.duration_ms, 800);
  assert.equal(press.title, tap.title);
  assert.deepEqual(press.selector, selector);
  assert.notStrictEqual(press.selector, selector);
  const back = switchTapLongPressStep(press, 'tap');
  assert.deepEqual(back.selector, selector);
  assert.equal('duration_ms' in back, false, 'tap must not include unsupported duration_ms');
  assert.deepEqual(tap.selector, selector, 'original editor data remains untouched');
});

test('XPath and normalized coordinates persist across both operation types', () => {
  const xpath = { id, type: 'tap', selector: { xpath: '//android.widget.Button[@text="确认"]' } };
  const updated = switchTapLongPressStep(xpath, 'long_press', 1600);
  assert.deepEqual(updated.selector, xpath.selector);
  assert.equal(updated.duration_ms, 1600);
  const coord = { id, type: 'long_press', x: 0.25, y: 0.75, width: 1080, height: 2400, duration_ms: 1900 };
  const tap = switchTapLongPressStep(coord, 'tap');
  assert.equal('selector' in tap, false);
  assert.deepEqual([tap.x, tap.y, tap.width, tap.height], [0.25, 0.75, 1080, 2400]);
  assert.equal('duration_ms' in tap, false);
  const restored = switchTapLongPressStep(tap, 'long_press', coord.duration_ms);
  assert.deepEqual([restored.x, restored.y, restored.width, restored.height], [0.25, 0.75, 1080, 2400]);
  assert.equal(restored.duration_ms, 1900);
});
