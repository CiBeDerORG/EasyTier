import assert from 'node:assert/strict';
import test from 'node:test';
import { replaceAutomationStepTarget, stepCanPickTarget } from '../src/modules/automation-step-target.ts';

const element = { resource_id: 'com.example:id/old' };
const picked = { selector: { resource_id: 'com.example:id/new' } };
const make = (type, extras={}) => ({ id: 'existing-1', type, selector: element, ...extras });

test('re-picking an action does not append, rename or change timing', () => {
  const original = make('long_press', { duration_ms: 1100, x: 0.3, y: 0.5, width: 1080, height: 2400 });
  const script = [original, {id:'second',type:'key',key:'BACK'}];
  const result = replaceAutomationStepTarget(script, 'existing-1', 'long_press', picked);
  assert.ok(result);
  assert.equal(result.length, 2);
  assert.equal(result[0].id, original.id);
  assert.equal(result[0].type, 'long_press');
  assert.equal(result[0].duration_ms, 1100);
  assert.deepEqual(result[0].selector, picked.selector);
  assert.equal(result[0].x, undefined);
  assert.equal(result[0].width, undefined);
  assert.deepEqual(script[0], original, 'original draft must not be mutated');
});

test('coordinates may replace a tap locator and preserve following steps', () => {
  const script = [make('tap'), {id:'next',type:'key',key:'BACK'}];
  const target = { coordinates: {x: 0.5, y: 0.75, width: 1080, height: 2400}};
  const result = replaceAutomationStepTarget(script, 'existing-1', 'tap', target);
  assert.equal(result[0].selector, undefined);
  assert.deepEqual([result[0].x, result[0].y, result[0].width, result[0].height],
    [0.5,0.75,1080,2400]);
  assert.strictEqual(result[1], script[1]);
});

test('input_text retains its runtime parameter and refuses XPath', () => {
  const script = [make('input_text', {parameter:'password_parameter'})];
  assert.deepEqual(replaceAutomationStepTarget(script,'existing-1','input_text',picked)[0],
    {...script[0],selector:picked.selector});
  assert.equal(replaceAutomationStepTarget(script,'existing-1','input_text',
    {selector:{xpath:'//*[@text="secret"]'}}),null);
  assert.equal(replaceAutomationStepTarget(script,'existing-1','input_text',
    {coordinates:{x:.5,y:.5,width:320,height:640}}),null);
});

test('branch selector replacement keeps Then/Else range and nested steps unchanged', () => {
  const script = [
    make('if_else', {then_count:1,else_count:1}),{id:'then',type:'key',key:'BACK'},
    {id:'else',type:'key',key:'HOME'},
  ];
  const result = replaceAutomationStepTarget(script,'existing-1','if_else',picked);
  assert.ok(result);
  assert.deepEqual(result[0], {...script[0],selector:picked.selector});
  assert.deepEqual(result.slice(1),script.slice(1));
});

test('scroll_find retains scroll container and its direction/timing', () => {
  const script=[make('scroll_find',{scroll_container:{resource_id:'com.example:id/list'},
    direction:'down',max_swipes:3,timeout_ms:20000,duration_ms:350})];
  const result=replaceAutomationStepTarget(script,'existing-1','scroll_find',picked);
  assert.equal(result[0].scroll_container,script[0].scroll_container);
  assert.equal(result[0].max_swipes,3);
});

test('invalid and stale targets cannot mutate a different step or widen privilege', () => {
  const script=[make('tap')];
  assert.equal(replaceAutomationStepTarget(script,'missing','tap',picked),null);
  assert.equal(replaceAutomationStepTarget(script,'existing-1','input_text',picked),null);
  assert.equal(replaceAutomationStepTarget([{id:'existing-1',type:'key',key:'HOME'}],
    'existing-1','key',picked),null);
  assert.equal(replaceAutomationStepTarget([...script,...script],'existing-1','tap',picked),null);
  assert.equal(replaceAutomationStepTarget(script,'existing-1','tap',{selector:{}}),null);
  assert.equal(replaceAutomationStepTarget(script,'existing-1','tap',
    {selector:{resource_id:'a',xpath:'//*'}}),null);
  assert.equal(replaceAutomationStepTarget(script,'existing-1','tap',
    {coordinates:{x:1.1,y:.2,width:100,height:200}}),null);
  assert.equal(stepCanPickTarget(make('swipe')),false);
});


test('re-picked selector retains explicit false clickable and rejects strings', () => {
  const script=[make('tap')];
  const result=replaceAutomationStepTarget(script,'existing-1','tap',
    {selector:{description:'领淘金币',clickable:false}});
  assert.deepEqual(result?.[0].selector,{description:'领淘金币',clickable:false});
  assert.equal(replaceAutomationStepTarget(script,'existing-1','tap',
    {selector:{clickable:'false'}}),null);
  assert.deepEqual(replaceAutomationStepTarget(script,'existing-1','tap',
    {selector:{clickable:true}})?.[0].selector,{clickable:true});
});

test('LOC-01..05 target replacement preserves boolean combinations without coercion', () => {
  const script=[make('tap')];
  for (const field of ['enabled','long_clickable','focusable','checkable','scrollable']) {
    const target={selector:{description:'领淘金币',[field]:false}};
    assert.deepEqual(replaceAutomationStepTarget(script,'existing-1','tap',target)?.[0].selector,target.selector);
    assert.equal(replaceAutomationStepTarget(script,'existing-1','tap',
      {selector:{[field]:'false'}}),null);
  }
});
