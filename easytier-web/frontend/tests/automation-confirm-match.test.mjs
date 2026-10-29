import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {buildFilterOptions,buildCrossFilterSelector} from '../src/modules/automation-cross-filter.ts';
import {confirmationMatches,comparisonFilterOption,replaceComparisonCondition}
    from '../src/modules/automation-confirm-match.ts';

const node=(id,parent_id,depth,values={})=>({
    id,parent_id,depth,resource_id:'',description:'',text:'',class_name:'android.widget.TextView',
    package:'sample.app',clickable:true,enabled:true,long_clickable:false,
    focusable:false,checkable:false,scrollable:false,selected:false,checked:false,password:false,
    bounds:{left:5,top:id*30,right:200,bottom:id*30+20},
    raw_bounds:'[5,'+(id*30)+'][200,'+(id*30+20)+']',...values,
});
const tree=()=>({truncated:false,nodes:[
    node(1,null,0,{class_name:'android.widget.LinearLayout'}),
    node(2,1,1,{resource_id:'sample:id/list'}),
    node(3,2,2,{resource_id:'sample:id/card-a'}),
    node(4,3,3,{description:'同名按钮',text:'领金币'}),
    node(5,2,2,{resource_id:'sample:id/card-b'}),
    node(6,5,3,{description:'同名按钮',text:'领金币'}),
    node(7,2,2,{resource_id:'sample:id/card-c'}),
    node(8,7,3,{description:'同名按钮',text:'其他按钮',clickable:false}),
]});
test('确认录入：两个原生属性匹配时不能直接唯一录入',()=>{
    const shot=tree(),options=buildFilterOptions(shot,4);
    const ids=['self:description','self:text'];
    const selector=buildCrossFilterSelector(options,ids).selector;
    assert.deepEqual(confirmationMatches(shot,selector,options,ids,4).map(n=>n.id),[4,6]);
});
test('对比父级属性：从第6行右键 ID 收敛为一条，保留已有条件',()=>{
    const shot=tree(),options=buildFilterOptions(shot,4),ids=['self:description','self:text'];
    const next=comparisonFilterOption(shot,6,'resource_id',true);
    assert.equal(next?.value,'sample:id/card-b');
    const all=[...options,next];
    const chosen=replaceComparisonCondition(all,ids,next);
    assert.deepEqual(chosen.slice(0,2),ids);
    const selector=buildCrossFilterSelector(all,chosen).selector;
    assert.match(selector.xpath,/card-b/);
    assert.deepEqual(confirmationMatches(shot,selector,all,chosen,4).map(n=>n.id),[6]);
});
test('同属性不同值自动替换而非产生不可能的 AND，允许不是原位置的目标',()=>{
    const shot=tree(),options=buildFilterOptions(shot,4);
    const replacement=comparisonFilterOption(shot,8,'clickable',false);
    assert.equal(replacement.value,false);
    const all=[...options,replacement];
    const ids=replaceComparisonCondition(all,['self:clickable','self:description'],replacement);
    assert.ok(!ids.includes('self:clickable'));
    assert.ok(ids.includes('self:description'));
    assert.equal(ids.at(-1),replacement.id);
    const selector=buildCrossFilterSelector(all,ids).selector;
    assert.deepEqual(confirmationMatches(shot,selector,all,ids,4).map(n=>n.id),[8]);
});
test('敏感文本不能作为过滤条件，空结果也不能确认',()=>{
    const shot=tree(); shot.nodes[3].password=true;
    assert.equal(comparisonFilterOption(shot,4,'text',false),null);
    assert.equal(comparisonFilterOption(shot,4,'description',false),null);
    assert.deepEqual(confirmationMatches(shot,{description:'不存在'},[],[],4),[]);
});
test('不可在快照可靠计算的多命中 XPath 不伪造目标列表',()=>{
    const shot=tree();
    assert.equal(confirmationMatches(shot,{xpath:'//*'},[],[],4,5),null);
    assert.deepEqual(confirmationMatches(shot,{xpath:'//*'},[],[],4,1)?.map(n=>n.id),[4]);
    assert.equal(confirmationMatches({...shot,truncated:true},{description:'同名按钮'},[],[],4),null);
});
test('对比浮窗存在实际属性差异及桌面右键/手机点击交互；提交门禁唯一',()=>{
    const root=new URL('../src/components/automation/',import.meta.url);
    const review=readFileSync(new URL('InspectorMatchReview.vue',root),'utf8');
    const editor=readFileSync(new URL('UiInspectorDialog.vue',root),'utf8');
    assert.match(review,/@contextmenu\.prevent\.stop="add\(node,col\)"/);
    assert.match(review,/@click\.stop="add\(node,col\)"/);
    assert.match(review,/:disabled="!matches \|\| matches\.length!==1 \|\| truncated"/);
    assert.match(editor,/<InspectorMatchReview/);
    assert.match(editor,/if \(selector && confirmationRows\.value\?\.length !== 1\)/);
    assert.ok(!editor.includes('重新读取控件树后无法确认原目标仍然唯一且位置一致'));
});
