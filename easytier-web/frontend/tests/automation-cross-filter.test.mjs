import test from 'node:test';
import assert from 'node:assert/strict';
import {buildFilterOptions,filterHierarchy,buildCrossFilterSelector}
  from '../src/modules/automation-cross-filter.ts';
const node=(id,parent_id,depth,values={})=>({
  id,parent_id,depth,class_name:'android.widget.FrameLayout',package:'demo.app',
  resource_id:'',description:'',text:'',clickable:true,enabled:true,
  long_clickable:false,focusable:false,checkable:false,scrollable:false,
  selected:false,checked:false,password:false,
  bounds:{left:10,top:id*30,right:150,bottom:id*30+25},
  raw_bounds:`[10,${id*30}][150,${id*30+25}]`,...values,
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
test('LOC-09 text filter uses same-node multi-segment fuzzy matching and ignores standalone dynamic numbers',()=>{
  const shot=hierarchy();
  const original='升级中 长按可加速升级 松手开始加速升级 升级赛车拿钱 剩余 4450 能量  长按可加速升级';
  shot.nodes[3].text=original;
  shot.nodes[5].text='升级中   长按可加速升级 松手开始加速升级 升级赛车拿钱 剩余 9981 能量';
  shot.nodes[7].text='升级中 长按可加速升级 升级赛车拿钱 剩余 9981 能量';
  const options=buildFilterOptions(shot,4);
  const matches=filterHierarchy(shot,options,['self:text']);
  assert.deepEqual(matches.map(n=>n.id),[4,6]);
  const built=buildCrossFilterSelector(options,['self:text']);
  assert.ok(built.selector?.xpath);
  assert.match(built.selector.xpath,/contains\(@text,'升级中'\)/);
  assert.match(built.selector.xpath,/contains\(@text,'升级赛车拿钱'\)/);
  assert.match(built.selector.xpath,/contains\(@text,'能量'\)/);
  assert.doesNotMatch(built.selector.xpath,/@text=/);
  assert.doesNotMatch(built.selector.xpath,/4450|9981/);
  assert.equal((built.selector.xpath.match(/contains\(@text,'长按可加速升级'\)/g)||[]).length,1);
  assert.equal(built.requiresVerification,true);
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

test('LOC-08 raw XML bounds match the selector even when visible bounds were clamped',()=>{
  const shot=hierarchy(),original=shot.nodes[3];
  original.bounds={left:0,top:90,right:150,bottom:125};
  original.raw_bounds='[-25,90][150,125]';
  const opts=buildFilterOptions(shot,4);
  const bounds=opts.find(o=>o.id==='self:bounds');
  assert.equal(bounds?.value,'[-25,90][150,125]');
  const built=buildCrossFilterSelector(opts,['self:description','self:bounds']);
  assert.match(built.selector.xpath,/\[-25,90\]\[150,125\]/);
});
test('LOC-08 parent filtering added from a matching row can reduce two targets to one',()=>{
  const shot=hierarchy(),opts=buildFilterOptions(shot,4);
  const parent=opts.find(x=>x.id==='ancestor:1:resource_id');
  assert.equal(parent?.value,'demo:id/card-a');
  assert.deepEqual(filterHierarchy(shot,opts,['self:description','ancestor:1:resource_id']).map(x=>x.id),[4]);
  const other={
    id:'parent:1:resource_id:demo:id/card-b',scope:'ancestor',distance:1,nodeId:5,
    field:'resource_id',value:'demo:id/card-b',label:'父级Resource ID',displayValue:'demo:id/card-b'
  };
  assert.deepEqual(filterHierarchy(shot,[...opts,other],['self:description',other.id]).map(x=>x.id),[6]);
  // A foreign row's parent must never silently retarget the originally selected action.
  assert.notEqual(filterHierarchy(shot,[...opts,other],['self:description',other.id])[0].id,4);
});
test('LOC-08 selectors reject missing raw bounds rather than claiming a false unique result',()=>{
  const shot=hierarchy();delete shot.nodes[3].raw_bounds;
  const opts=buildFilterOptions(shot,4);
  assert.equal(opts.some(x=>x.id==='self:bounds'),false);
  assert.equal(buildCrossFilterSelector(opts,['self:bounds']).selector,null);
});
