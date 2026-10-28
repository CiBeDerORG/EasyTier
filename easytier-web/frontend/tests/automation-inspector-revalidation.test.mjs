import test from 'node:test';
import assert from 'node:assert/strict';
import {revalidatePickedNativeTarget, samePickedBounds}
  from '../src/modules/automation-inspector-revalidation.ts';

const node=(id,description='领淘金币',bounds={left:100,top:200,right:250,bottom:280},extras={})=>({
    id,parent_id:null,depth:3,
    class_name:'android.widget.FrameLayout',package:'com.taobao.taobao',
    resource_id:'',text:'',description,
    clickable:true,enabled:true,scrollable:false,selected:false,checked:false,password:false,
    bounds,...extras,
});
const fresh=(nodes,{width=400,height=800,truncated=false}={})=>({
    device_id:'phone',target_fingerprint:'fingerprint',captured_at:new Date().toISOString(),
    snapshot:{nodes,width,height,truncated},
});
const old=node(127);
test('T58 stale picker is still recordable after a FRESH unique native validation',()=>{
    // The capture may be minutes old; recording is gated by the NEW capture,
    // not by the old captured_at timestamp or ephemeral node ID.
    const next=fresh([node(999)]);
    assert.equal(revalidatePickedNativeTarget(old,400,800,next,
        {description:'领淘金币',package:'com.taobao.taobao',clickable:true}),true);
});
test('T58 refuses changed position, wrong business target, or ambiguous selector',()=>{
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([
        node(999,'领淘金币',{left:120,top:200,right:270,bottom:280}),
    ]),{description:'领淘金币'}),false);
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([node(999,'另一活动')]),
        {clickable:true}),false);
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([
        node(999),node(1000,'领淘金币',{left:110,top:300,right:260,bottom:380}),
    ]),{description:'领淘金币'}),false);
});
test('T58 refuses stale/foreign geometry, password targets and XPath client matching',()=>{
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([node(999)],{width:800}),{clickable:true}),false);
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([node(999)],{truncated:true}),{clickable:true}),false);
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([node(999)]),{xpath:"//*[@text='领淘金币']"}),false);
    assert.equal(revalidatePickedNativeTarget(node(127,'领淘金币',old.bounds,{password:true}),
        400,800,fresh([node(999)]),null),false);
});
test('T58 coordinate fallback also validates the exact picked node before permitting a stale submission',()=>{
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([node(999)]),null),true);
    assert.equal(revalidatePickedNativeTarget(old,400,800,fresh([
        node(999),node(1000,'领淘金币',old.bounds),
    ]),null),false);
    assert.equal(samePickedBounds(old.bounds,{left:102,top:200,right:250,bottom:280}),true);
    assert.equal(samePickedBounds(old.bounds,{left:104,top:200,right:250,bottom:280}),false);
});
