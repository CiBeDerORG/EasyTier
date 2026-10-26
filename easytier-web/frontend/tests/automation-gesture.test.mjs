import assert from 'node:assert/strict';
import test from 'node:test';
import {
  beginCapturedGesture, observeCapturedGesture, finishCapturedGesture,
} from '../src/modules/automation-gesture.ts';

const pos = (x, y, width = 1000, height = 2000) => ({ x, y, width, height });
const gesture = (point = pos(500, 1000)) => beginCapturedGesture('android-a', point, 1000);

test('quick stationary down/up is one tap with no swipe or drag', () => {
  const value = finishCapturedGesture(gesture(), pos(501, 1002), 1090, 'tap-1');
  assert.equal(value.kind, 'step');
  assert.equal(value.step.type, 'tap');
  assert.equal(value.step.id, 'tap-1');
  assert.equal(value.step.x, 0.5);
  assert.equal(value.step.y, 0.5);
});

test('stationary 500ms and longer holds become long presses, not swipes', () => {
  for (const ms of [500, 800, 1500, 3000]) {
    const capture = gesture();
    observeCapturedGesture(capture, pos(504, 1003));
    const value = finishCapturedGesture(capture, pos(502, 1001), 1000 + ms, 'hold');
    assert.equal(value.kind, 'step');
    assert.equal(value.step.type, 'long_press');
    assert.equal(value.step.duration_ms, ms);
  }
});

test('one mobile movement requires an explicit swipe-versus-drag decision', () => {
  const capture = gesture();
  observeCapturedGesture(capture, pos(500, 1350));
  const value = finishCapturedGesture(capture, pos(700, 1800), 1600, 'move-1');
  assert.equal(value.kind, 'confirm');
  assert.equal(value.swipe.id, value.drag.id);
  assert.equal(value.swipe.type, 'swipe');
  assert.equal(value.drag.type, 'drag');
  assert.equal(value.swipe.duration_ms, 600);
  assert.equal(value.drag.duration_ms, 600);
  assert.equal(value.swipe.to_x, 0.7);
  assert.equal(value.drag.to_y, 0.9);
});

test('quick movement offers drag with a legal minimum duration', () => {
  const value = finishCapturedGesture(gesture(), pos(700, 1300), 1060, 'fast');
  assert.equal(value.kind, 'confirm');
  assert.equal(value.swipe.duration_ms, 100);
  assert.equal(value.drag.duration_ms, 300);
});

test('gesture with mismatched phone geometry and partial releases never creates steps', () => {
  const capture = gesture();
  assert.equal(observeCapturedGesture(capture, pos(700, 1300, 2000, 2000)), false);
  const value = finishCapturedGesture(capture, pos(800, 1600), 1600, 'bad');
  assert.deepEqual(value, { kind: 'invalid', reason: 'incomplete' });
  assert.equal(beginCapturedGesture('x', pos(-1, 30), 1000), null);
  assert.equal(beginCapturedGesture('x', pos(1, 1, 0, 1), 1000), null);
});

test('moving away and returning cannot be faithfully replayed as straight gesture', () => {
  const capture = gesture();
  observeCapturedGesture(capture, pos(850, 1450));
  observeCapturedGesture(capture, pos(500, 1000));
  assert.equal(finishCapturedGesture(capture, pos(500, 1000), 1600, 'loop').reason, 'returned_to_origin');
});

test('out of range and excessive duration fail closed', () => {
  assert.equal(finishCapturedGesture(gesture(), pos(501,1001), 4100, 'late').reason, 'too_long');
  assert.equal(finishCapturedGesture(gesture(), pos(501,1001), 500, 'clock').reason, 'incomplete');
  assert.equal(finishCapturedGesture(gesture(), pos(1000,1001), 1200, 'bounds').reason, 'incomplete');
});

test('path excursion, not only endpoints, decides stationary-versus-moving gesture', () => {
  const capture = gesture();
  observeCapturedGesture(capture, pos(500, 1050));
  const result = finishCapturedGesture(capture, pos(500, 1000), 1800, 'loop');
  assert.equal(result.reason, 'returned_to_origin');
});
