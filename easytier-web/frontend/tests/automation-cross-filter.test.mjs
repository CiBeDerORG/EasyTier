import test from 'node:test';
import assert from 'node:assert/strict';
import {buildFilterOptions,filterHierarchy,buildCrossFilterSelector}
  from '../src/modules/automation-cross-filter.ts';
const node=(id,parent_id,depth,values={})=>({
  id,parent_id,depth,class_name:'android.widget.FrameLayout',package:'demo.app',
  resource_id:'',description:'',text:'',clickable:true,enabled:true,
  long_clickable:false,focusable:false,checkable:false,scrollable:false,
  selected:false,checked:false,password:false,
  bounds:{left:10,top:id*30,right:150,bottom:id*30+25},...values,
});
function hierarchy(){
  return {truncated:false,nodes:[
    node(1,null,0,{class_name:'android.widget.LinearLayout'}),
    node(2,1,1,{resource_id:'demo:id/list'}),
    node(3,2,2,{resource_id:'demo:id/card-a'}),
    node(4,3,3,{description:'领淘金币',clickable:true,enabled:true}),
    node(5,2,2,{resource_id:'demo:id/card-b'}),
    node(6,5,3,{description:'领淘金币',clickable:true,enabled:true}),
    node(7,2,2,{resource_id:'demo:id/card-c'}),
    node(8,7,3,{description:'领淘金币',clickable:false,enabled:false}),
  ]};
}
test('LOC-08 cross filter allows unlimited AND fields, including true and false',()=>{
  const shot=hierarchy(),options=buildFilterOptions(shot,4);
  const ids=['self:description','self:clickable','self:enabled'];
  assert.equal(filterHierarchy(shot,options,[]).length,shot.nodes.length);
  assert.deepEqual(filterHierarchy(shot,options,ids).map(n=>n.id),[4,6]);
  assert.equal(buildCrossFilterSelector(options,ids).requiresVerification,false);
  assert.deepEqual(buildCrossFilterSelector(options,ids).selector,{
    description:'领淘金币',clickable:true,enabled:true,
  });
  const other=buildFilterOptions(shot,8);
  assert.equal(other.find(o=>o.id==='self:clickable')?.value,false);
  assert.deepEqual(filterHierarchy(shot,other,['self:description','self:clickable','self:enabled']).map(n=>n.id),[8]);
  assert.equal(buildCrossFilterSelector(other,['self:description','self:clickable','self:enabled']).selector.clickable,false);
});
test('LOC-08 parent filters refine the repeated target and compile stable relations',()=>{
  const shot=hierarchy(),options=buildFilterOptions(shot,4);
  assert.ok(options.some(o=>o.id==='ancestor:1:resource_id'&&o.value==='demo:id/card-a'));
  const ids=['self:description','self:clickable','ancestor:1:resource_id'];
  const matches=filterHierarchy(shot,options,ids);
  assert.deepEqual(matches.map(n=>n.id),[4]);
  const built=buildCrossFilterSelector(options,ids);
  assert.equal(built.requiresVerification,true);
  assert.match(built.selector.xpath,/ancestor::\*\[1\]\[@resource-id='demo:id\/card-a'\]/);
  assert.ok(!built.selector.xpath.includes('#4'));
  assert.ok(!built.selector.xpath.includes('instance'));
});
test('LOC-08 selected/checked, bounds and deep ancestry use XPath rather than fake native fields',()=>{
  const shot=hierarchy(),options=buildFilterOptions(shot,4);
  for(const field of ['selected','checked','bounds']){
    const id='self:'+field,sel=buildCrossFilterSelector(options,[id]);
    assert.equal(sel.requiresVerification,true);
    assert.ok(sel.selector?.xpath);
    assert.equal(filterHierarchy(shot,options,[id]).some(n=>n.id===4),true);
  }
  assert.equal(buildCrossFilterSelector(options,['ancestor:2:resource_id','self:description']).requiresVerification,true);
});
test('LOC-08 excludes hidden password text and fails closed for stale or unknown choices',()=>{
  const shot=hierarchy();shot.nodes[3].password=true;
  assert.deepEqual(buildFilterOptions(shot,4),[]);
  const options=buildFilterOptions(shot,6);
  assert.equal(filterHierarchy(shot,options,['self:unavailable']).length,0);
  assert.equal(buildCrossFilterSelector(options,['self:unavailable']).selector,null);
  assert.deepEqual(buildFilterOptions({...shot,truncated:true},6),[]);
  assert.deepEqual(filterHierarchy({...shot,truncated:true},options,[]),[]);
});
test('LOC-08 XPath text escapes both quote kinds and refuses oversize expressions',()=>{
  const shot=hierarchy();
  shot.nodes[3].description='他说 "it\'s"';
  const options=buildFilterOptions(shot,4);
  const out=buildCrossFilterSelector(options,['self:description','self:bounds']);
  assert.match(out.selector.xpath,/concat\(/);
  const big=hierarchy(); big.nodes[3].description='x'.repeat(240);
  big.nodes[2].resource_id='a'.repeat(240);
  const many=buildFilterOptions(big,4);
  const result=buildCrossFilterSelector(many,['self:description','ancestor:1:resource_id','self:bounds']);
  assert.equal(result.selector,null);
  assert.match(result.error,/512/);
});
