/** T48: the original PhoneCard scope is a pure identity snapshot, never
 * an action authorization or a second media session. No UI/ADB is started.
 */
import assert from 'node:assert/strict';
import test from 'node:test';
import {
    canAccessAutomationCard, captureAutomationFloatingScope,
    automationFloatingScopeIsCurrent,
} from '../src/modules/automation-floating-contract.ts';

const owner=42, deviceId='phone-a', fingerprint='binding-a';
const device=()=>({
    id:deviceId,owner_user_id:owner,target_fingerprint:fingerprint,
});
const workspace=()=>({
    ownerId:owner,editingId:'script-a',revision:3,
    name:'demo',description:'',steps:[],selectedStepId:undefined,
    baseline:'{}',
});
const context=()=>({
    ownerId:owner,device:device(),workspace:workspace(),
    draftSync:{id:'draft-a',revision:8},
    sessionId:'live-session-a',generation:1,
});

test('T48 one authorized original card scope is read-only and owner constrained',()=>{
    const source=context();
    assert.equal(canAccessAutomationCard(owner,deviceId,source.device,source.workspace),true);
    assert.equal(canAccessAutomationCard(owner+1,deviceId,source.device,source.workspace),false);
    assert.equal(canAccessAutomationCard(owner,'other-phone',source.device,source.workspace),false);
    assert.equal(canAccessAutomationCard(owner,deviceId,null,source.workspace),false);
    const expected=captureAutomationFloatingScope(source);
    assert.deepEqual(expected,{
        ownerId:owner,deviceId,targetFingerprint:fingerprint,
        sessionId:'live-session-a',scriptId:'script-a',scriptRevision:3,
        draftId:'draft-a',draftRevision:8,generation:1,
    });
    assert.equal(automationFloatingScopeIsCurrent(expected,source,{requireLiveMedia:true}),true);
});

test('T48 edits are blocked on stale fingerprint, owner, revision or operation generation',()=>{
    const source=context(),snapshot=captureAutomationFloatingScope(source);
    const variations=[
        {ownerId:owner+1},
        {device:{...device(),owner_user_id:owner+1}},
        {device:{...device(),target_fingerprint:'rebound'}},
        {workspace:{...workspace(),editingId:'script-b'}},
        {workspace:{...workspace(),revision:4}},
        {draftSync:{id:'draft-b',revision:8}},
        {draftSync:{id:'draft-a',revision:9}},
        {generation:2},
    ];
    for(const change of variations){
        assert.equal(automationFloatingScopeIsCurrent(snapshot,{...source,...change}),false,
            'stale '+Object.keys(change).join(',')+' must invalidate the operation');
    }
});

test('T48 offline editing is allowed by identity, but video/pixel actions require same live session',()=>{
    const source=context(),snapshot=captureAutomationFloatingScope(source);
    const disconnected={...source,sessionId:null};
    assert.equal(automationFloatingScopeIsCurrent(snapshot,disconnected),true,
        'detaching media must not delete an already confirmed draft');
    assert.equal(automationFloatingScopeIsCurrent(snapshot,disconnected,
        {requireLiveMedia:true}),false);
    assert.equal(automationFloatingScopeIsCurrent(snapshot,{
        ...source,sessionId:'new-session',
    },{requireLiveMedia:true}),false);
    const offline={...source,sessionId:null};
    const offlineSnapshot=captureAutomationFloatingScope(offline);
    assert.ok(offlineSnapshot);
    assert.equal(automationFloatingScopeIsCurrent(offlineSnapshot,offline),true);
    assert.equal(automationFloatingScopeIsCurrent(offlineSnapshot,offline,
        {requireLiveMedia:true}),false);
});

test('T48 device change and account change invalidate fresh window identity',()=>{
    const source=context();
    const snapshot=captureAutomationFloatingScope(source);
    assert.equal(captureAutomationFloatingScope({
        ...source,ownerId:owner+1,
    }),null);
    assert.equal(captureAutomationFloatingScope({
        ...source,device:null,
    }),null);
    assert.equal(automationFloatingScopeIsCurrent(snapshot,{
        ...source,device:{...device(),id:'another-device'},
    }),false);
});
