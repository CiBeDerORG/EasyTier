import test from 'node:test';
import assert from 'node:assert/strict';
import { rankRelationalCandidates, xpathString } from '../src/modules/automation-relational-locator.ts';

const node=(id,parent_id,depth,values={})=>({
    id,parent_id,depth,class_name:'android.widget.FrameLayout',package:'demo.app',
    resource_id:'',description:'',text:'',clickable:true,enabled:true,
    scrollable:false,selected:false,checked:false,password:false,
    bounds:{left:0,top:0,right:100,bottom:100},...values,
});
function tasks() {
    return {truncated:false,nodes:[
        node(0,null,0),
        node(1,0,1),
        node(2,1,2),
        node(3,2,3),
        node(4,3,4,{text:'每日签到',class_name:'android.widget.TextView'}),
        node(5,3,4,{description:'领淘金币'}),
        node(6,2,3),
        node(7,6,4,{text:'浏览商品',class_name:'android.widget.TextView'}),
        node(8,6,4,{description:'领淘金币'}),
    ]};
}
test('LOC-08 same-container anchor disambiguates repeated targets without position',()=>{
    const shot=tasks();
    const candidates=rankRelationalCandidates(shot,8);
    assert.ok(candidates.length>0);
    const c=candidates.find(x=>x.id.startsWith('rel:anchor:7:'));
    assert.ok(c);
    assert.ok(c.selector.xpath.includes('@text='));
    assert.ok(c.selector.xpath.includes('浏览商品'));
    assert.ok(c.selector.xpath.includes('@content-desc='));
    assert.ok(!/\[\d+\]\s*\/\*/.test(c.selector.xpath.split('ancestor::')[0]));
    assert.equal(rankRelationalCandidates(shot,8,7).some(x=>x.id.startsWith('rel:anchor:7:')),true);
    assert.equal(rankRelationalCandidates(shot,8,4).length,0,'an anchor in the other task must not identify the selected target');
});
test('LOC-08 unique ancestor resource id scopes a repeated description',()=>{
    const shot=tasks();
    shot.nodes[6].resource_id='demo:id/task-b';
    const c=rankRelationalCandidates(shot,8).find(x=>x.id.startsWith('rel:parent:6:'));
    assert.ok(c);
    assert.match(c.selector.xpath,/resource-id/);
    assert.match(c.selector.xpath,/content-desc/);
});
test('LOC-08 refuses page-wide anchor, identical cards, sensitive and truncated trees',()=>{
    const shot=tasks();
    shot.nodes[7].text='每日签到';
    assert.deepEqual(rankRelationalCandidates(shot,8),[]);
    assert.deepEqual(rankRelationalCandidates({...shot,truncated:true},8),[]);
    shot.nodes[7].password=true;
    assert.deepEqual(rankRelationalCandidates(shot,8,7),[]);
});
test('LOC-08 XPath 1.0 quoting is safe for apostrophes and double quotes',()=>{
    assert.equal(xpathString('Hello'),"'Hello'");
    assert.equal(xpathString("it's"),'"it\'s"');
    const mixed=xpathString('He said "it\'s"');
    assert.ok(mixed.startsWith('concat('));
    assert.ok(mixed.includes('","')===false);
    assert.ok(mixed.includes('"\'"'));
});
