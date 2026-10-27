/**
 * T17 · AC-09/11 device portability negative / race contracts. A synthetic
 * UiAutomator hierarchy is not an Android device or real pixel screenshot.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  reviewAutomationDeviceCompatibility, statelessCompatibility,
  compatibilityStillCurrent, requiresReuseHierarchy, reuseStepFingerprint,
} from '../src/modules/automation-device-compatibility.ts';

const now=Date.parse('2026-10-27T08:00:00Z');
const phone=(id='a', owner=7, overrides={})=>({
  id,name:id,owner_user_id:owner,target_fingerprint:'fp-'+id,
  connection:{enabled:true,adb_port:38000,connection_mode:'wireless'},
  availability_reason:null,active_session:null,run_occupied:false,
  ...overrides,
});
const node=(id, resource='demo:id/save')=>({
  id,parent_id:null,depth:1,class_name:'android.widget.Button',package:'demo.app',
  resource_id:resource,text:'Save',description:'',clickable:true,enabled:true,
  scrollable:false,selected:false,checked:false,password:false,
  bounds:{left:40,top:100,right:400,bottom:240},
});
const hierarchy=(p,width=1080,height=2400,extras={})=>({
  device_id:p.id,target_fingerprint:p.target_fingerprint,captured_at:new Date(now).toISOString(),
  snapshot:{width,height,truncated:false,nodes:[node(1)]},...extras,
});
const tap=(id='tap-1',width=1080,height=2400)=>({
  id,type:'tap',x:0.4,y:0.3,width,height,
});
const expectBlock=(owner,p,shot,steps)=>assert.equal(
  reviewAutomationDeviceCompatibility(owner,p,shot,steps,now).status,'blocked');

test('AC11 reference coordinates never silently rescale across screen sizes or rotation',()=>{
  const p=phone('b');
  const reference=tap();
  expectBlock(7,p,hierarchy(p,720,1600),[reference]);
  expectBlock(7,p,hierarchy(p,2400,1080),[reference]);
  expectBlock(7,p,hierarchy(p),[{...reference,width:undefined,height:undefined}]);
  const same=reviewAutomationDeviceCompatibility(7,p,hierarchy(p),[reference],now);
  assert.equal(same.status,'confirm','matching resolution is not proof of matching UI layout');
  assert.ok(same.issues.some(x=>x.severity==='confirm'));
  assert.equal(same.viewport.width,1080);
  assert.equal(same.issues[0].stepId,'tap-1');
});

test('AC11 device owner, target fingerprint and snapshot freshness fail closed',()=>{
  const p=phone('b');
  const source=hierarchy(p);
  assert.throws(()=>reviewAutomationDeviceCompatibility(8,p,source,[tap()],now),/授权|绑定/);
  assert.throws(()=>reviewAutomationDeviceCompatibility(7,{...p,target_fingerprint:'new'},source,[tap()],now),/绑定/);
  assert.throws(()=>reviewAutomationDeviceCompatibility(7,{...p,availability_reason:'offline'},source,[tap()],now),/绑定/);
  assert.throws(()=>reviewAutomationDeviceCompatibility(7,p,{
    ...source,captured_at:new Date(now-21000).toISOString(),
  },[tap()],now),/过期/);
  const safe=reviewAutomationDeviceCompatibility(7,p,source,[tap()],now);
  assert.equal(compatibilityStillCurrent(safe,7,p,[tap()],now+19000),true);
  assert.equal(compatibilityStillCurrent(safe,7,p,[tap()],now+21000),false);
  assert.equal(compatibilityStillCurrent(safe,7,{...p,target_fingerprint:'other'},[tap()],now),false);
  assert.equal(compatibilityStillCurrent(safe,7,p,[{...tap(),x:.8}],now),false);
  assert.equal(compatibilityStillCurrent(null,7,p,[tap()],now),false);
});

test('AC11 unique selector is read-only; ambiguous, missing, truncated and XPath require review',()=>{
  const p=phone('b');
  const only={id:'wait-1',type:'wait_for',selector:{resource_id:'demo:id/save'}};
  const shot=hierarchy(p);
  const good=reviewAutomationDeviceCompatibility(7,p,shot,[only],now);
  assert.equal(good.status,'compatible');
  assert.deepEqual(good.issues,[]);
  const duplicate={...shot,snapshot:{...shot.snapshot,nodes:[node(1),node(2)]}};
  const many=reviewAutomationDeviceCompatibility(7,p,duplicate,[only],now);
  assert.equal(many.status,'confirm');
  assert.match(many.issues[0].message,/匹配 2 个|不唯一/);
  const missing={...shot,snapshot:{...shot.snapshot,nodes:[]}};
  assert.equal(reviewAutomationDeviceCompatibility(7,p,missing,[only],now).status,'confirm');
  assert.equal(reviewAutomationDeviceCompatibility(7,p,missing,[{
    id:'gone-1',type:'wait_gone',selector:{resource_id:'demo:id/save'},
  }],now).status,'compatible');
  assert.equal(reviewAutomationDeviceCompatibility(7,p,{
    ...shot,snapshot:{...shot.snapshot,truncated:true},
  },[only],now).status,'confirm');
  assert.equal(reviewAutomationDeviceCompatibility(7,p,shot,[{
    id:'xpath',type:'assert',selector:{xpath:'//button'},
  }],now).status,'confirm');
});

test('AC09/11 native-only actions use no hierarchy; unknown target cannot be reused',()=>{
  const p=phone('a');
  const steps=[{id:'home',type:'key',key:'HOME'},
    {id:'app',type:'launch_app',package:'demo.app'}];
  assert.equal(requiresReuseHierarchy(steps),false);
  const info=statelessCompatibility(7,p,steps);
  assert.equal(info.status,'compatible');
  assert.equal(info.viewport.width,0);
  assert.equal(compatibilityStillCurrent(info,7,p,steps),true);
  assert.equal(compatibilityStillCurrent(info,9,p,steps),false);
  assert.equal(requiresReuseHierarchy([tap()]),true);
  assert.equal(requiresReuseHierarchy([{id:'selector',type:'assert',selector:{text:'ready'}}]),true);
  assert.throws(()=>statelessCompatibility(7,p,[tap()]),/控件快照/);
  assert.throws(()=>statelessCompatibility(7,phone('b',9),steps),/授权/);
});

test('AC11 fingerprint of a script is sensitive to ordering but does not mutate source',()=>{
  const steps=[tap('first'),{id:'second',type:'key',key:'BACK'}];
  const original=structuredClone(steps);
  assert.notEqual(reuseStepFingerprint(steps),reuseStepFingerprint([...steps].reverse()));
  assert.deepEqual(steps,original);
});
