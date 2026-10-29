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
test('LOC-09 fuzzy mode defaults OFF: original text is exact native selector',()=>{
  const shot=hierarchy();
  const original='升级中 长按可加速升级 松手开始加速升级 升级赛车拿钱 剩余 4450 能量  长按可加速升级';
  shot.nodes[3].text=original;
  shot.nodes[5].text='升级中   长按可加速升级 松手开始加速升级 升级赛车拿钱 剩余 9981 能量';
  shot.nodes[7].text='升级中 长按可加速升级 升级赛车拿钱 剩余 9981 能量';
  const options=buildFilterOptions(shot,4);
  assert.equal(options.find(x=>x.id==='self:text')?.fuzzy,undefined);
  assert.deepEqual(filterHierarchy(shot,options,['self:text']).map(n=>n.id),[4]);
  assert.deepEqual(buildCrossFilterSelector(options,['self:text']).selector,{text:original});
  const fuzzy=options.map(x=>x.id==='self:text'?{...x,fuzzy:true,fuzzyText:original}:x);
  assert.deepEqual(filterHierarchy(shot,fuzzy,['self:text']).map(n=>n.id),[4,6]);
  const built=buildCrossFilterSelector(fuzzy,['self:text']);
  assert.ok(built.selector?.xpath);
  assert.match(built.selector.xpath,/contains\(@text,'升级中'\)/);
  assert.match(built.selector.xpath,/contains\(@text,'升级赛车拿钱'\)/);
  assert.doesNotMatch(built.selector.xpath,/@text=/);
  assert.doesNotMatch(built.selector.xpath,/4450|9981/);
  assert.equal((built.selector.xpath.match(/contains\(@text,'长按可加速升级'\)/g)||[]).length,1);
  assert.equal(built.requiresVerification,true);
  const turnedOff=fuzzy.map(x=>({...x,fuzzy:false}));
  assert.deepEqual(filterHierarchy(shot,turnedOff,['self:text']).map(n=>n.id),[4]);
  assert.deepEqual(buildCrossFilterSelector(turnedOff,['self:text']).selector,{text:original});
});
test('LOC-09 target and ancestor text/description have independent fuzzy switches',()=>{
  const shot=hierarchy();
  shot.nodes[3].text='目标 123 充值';
  shot.nodes[5].text='目标 456 充值';
  shot.nodes[3].description='开始 123 升级';shot.nodes[5].description='开始 456 升级';
  shot.nodes[2].text='活动 123 次数';shot.nodes[4].text='活动 456 次数';
  shot.nodes[2].description='活动说明 首单 123';shot.nodes[4].description='活动说明 首单 456';
  const original=buildFilterOptions(shot,4);
  const ids=['self:text','self:description','ancestor:1:text','ancestor:1:description'];
  assert.deepEqual(filterHierarchy(shot,original,ids).map(x=>x.id),[4]);
  const fuzz=original.map(x=>ids.includes(x.id)?{...x,fuzzy:true}:x);
  assert.deepEqual(filterHierarchy(shot,fuzz,ids).map(x=>x.id),[4,6]);
  const xpath=buildCrossFilterSelector(fuzz,ids).selector?.xpath||'';
  assert.match(xpath,/contains\(@text,'目标'\)/);
  assert.match(xpath,/contains\(@content-desc,'开始'\)/);
  assert.match(xpath,/ancestor::\*\[1\]\[contains\(@text,'活动'\)/);
  assert.match(xpath,/ancestor::\*\[1\]\[contains\(@content-desc,'活动说明'\)/);
  assert.doesNotMatch(xpath,/123|456/);
  const onlyParentDescription=original.map(x=>x.id==='ancestor:1:description'
    ?{...x,fuzzy:true,fuzzyText:'首单'}:x);
  assert.deepEqual(filterHierarchy(shot,onlyParentDescription,['ancestor:1:description']).map(x=>x.id),[4,6]);
  const exact=buildCrossFilterSelector(original,['self:description']).selector;
  assert.deepEqual(exact,{description:'开始 123 升级'});
});
test('LOC-09 edited fuzzy fragments apply to one node, empty strings fail closed, quotes escape',()=>{
  const shot=hierarchy();
  shot.nodes[3].description='今日活动 限时加速 4450';
  shot.nodes[5].description='今日活动 限时加速 7780';
  const options=buildFilterOptions(shot,4);
  const fuzzy=options.map(x=>x.id==='self:description'
    ?{...x,fuzzy:true,fuzzyText:'活动 加速'}:x);
  assert.deepEqual(filterHierarchy(shot,fuzzy,['self:description']).map(x=>x.id),[4,6]);
  const built=buildCrossFilterSelector(fuzzy,['self:description']);
  assert.match(built.selector.xpath,/contains\(@content-desc,'活动'\)/);
  assert.match(built.selector.xpath,/contains\(@content-desc,'加速'\)/);
  const exact=options.map(x=>x.id==='self:description'
    ?{...x,fuzzy:false,fuzzyText:'不存在'}:x);
  assert.deepEqual(buildCrossFilterSelector(exact,['self:description']).selector,
    {description:'今日活动 限时加速 4450'});
  const empty=fuzzy.map(x=>x.id==='self:description'?{...x,fuzzyText:'   '}:x);
  assert.equal(buildCrossFilterSelector(empty,['self:description']).selector,null);
  assert.deepEqual(filterHierarchy(shot,empty,['self:description']),[]);
  const quote=fuzzy.map(x=>x.id==='self:description'?{...x,fuzzyText:`活动 "it's"`}:x);
  assert.match(buildCrossFilterSelector(quote,['self:description']).selector.xpath,/concat\(/);
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
