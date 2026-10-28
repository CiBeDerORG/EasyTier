import test from 'node:test';
import assert from 'node:assert/strict';
import {diagnosticXPath,normalizeTrialDiagnosis,summarizeTrialDiagnosis}
    from '../src/modules/automation-trial-diagnostics.ts';

test('native selector diagnostics compile AND constraints, including explicit false',()=>{
    const xp=diagnosticXPath({
        description:'领淘金币',package:'com.taobao.taobao',clickable:false,
    });
    assert.ok(xp.includes("@content-desc='领淘金币'"));
    assert.ok(xp.includes("@package='com.taobao.taobao'"));
    assert.ok(xp.includes("@clickable='false'"));
    assert.equal(diagnosticXPath({xpath:"//*[@text='看见']"}),"//*[@text='看见']");
    assert.equal(diagnosticXPath({xpath:"//*[@text='保存']"}),"//*[@text='保存']");
});
test('invalid and unbounded native expressions cannot be used to reveal arbitrary data',()=>{
    assert.equal(diagnosticXPath({}),null);
    assert.equal(diagnosticXPath({resource_id:''}),null);
    assert.equal(diagnosticXPath({description:'x'.repeat(510)}),null);
    assert.equal(diagnosticXPath({unknown_field:'Y'}),null);
});
test('0,1,many snapshots preserve all source and parent attributes',()=>{
    const selector={description:'领金币'};
    const row={attributes:{'content-desc':'领金币','class':'android.widget.FrameLayout',
        'resource-id':'a:id/b'},parent:{text:'每日签到','resource-id':'a:id/card'},
        depth:5};
    const none=normalizeTrialDiagnosis(selector,{count:0,matches:[]},'now');
    assert.equal(none.status,'none');
    assert.match(summarizeTrialDiagnosis(none),/0 个/);
    const one=normalizeTrialDiagnosis(selector,{count:1,matches:[row]},'now');
    assert.equal(one.status,'one');
    assert.deepEqual(one.matches[0].parent,{text:'每日签到','resource-id':'a:id/card'});
    const many=normalizeTrialDiagnosis(selector,{count:90,matches:[row],limited:true},'now');
    assert.equal(many.status,'many');
    assert.equal(many.limited,true);
    assert.match(summarizeTrialDiagnosis(many),/90/);
});
test('malformed or overlarge backend matching lists are not accepted',()=>{
    assert.equal(normalizeTrialDiagnosis({text:'x'},{count:-1,matches:[]},'now'),null);
    assert.equal(normalizeTrialDiagnosis({text:'x'},{count:2},'now'),null);
    assert.equal(normalizeTrialDiagnosis({text:'x'},{count:120,matches:Array.from({length:81},()=>({}))},'now'),null);
});
