/** T48 formal script CAS write receipts: real storage functions, no network.
 * These tests were authored in debug mode; they must not be reported passed
 * until test:unit is explicitly authorized and executed.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import {
    readFormalReceipt,reserveFormalReceipt,clearFormalReceipt,formalReceiptKey,
} from '../src/modules/automation-formal-script-receipt.ts';

const owner=42;
const uid='123e4567-e89b-42d3-a456-426614174000';
const timestamp='2026-10-28T01:00:00Z';
const update=()=>({
    owner,kind:'publish',scriptId:uid,revision:3,
    expectedName:'Demo',expectedStepIds:['step-1','step-2'],createdAt:timestamp,
});
const initial=()=>({
    owner,kind:'create',scriptId:'new:'+uid,revision:0,
    expectedName:'First formal',expectedStepIds:[],createdAt:timestamp,
});
const remove=()=>({
    owner,kind:'delete',scriptId:uid,revision:3,createdAt:timestamp,
});
function store(){
    const values=new Map();
    return {
        values,
        getItem:key=>values.get(key)??null,
        setItem:(key,value)=>values.set(key,String(value)),
        removeItem:key=>values.delete(key),
    };
}

test('T48 formal PUT receipt survives remount and cannot be replaced by a new revision',()=>{
    const storage=store(), first=update();
    assert.equal(readFormalReceipt(storage,owner),null);
    reserveFormalReceipt(storage,first);
    assert.deepEqual(readFormalReceipt(storage,owner),first);
    assert.equal(readFormalReceipt(storage,owner+1),null);
    assert.throws(()=>reserveFormalReceipt(storage,{...first,revision:4}),/未核对/);
    assert.throws(()=>clearFormalReceipt(storage,{...first,revision:4}),/身份已变化/);
    assert.deepEqual(readFormalReceipt(storage,owner),first);
    clearFormalReceipt(storage,first);
    assert.equal(readFormalReceipt(storage,owner),null);
});

test('T48 new formal script has an explicit ephemeral new:UUID and revision zero',()=>{
    const storage=store();
    reserveFormalReceipt(storage,initial());
    assert.deepEqual(readFormalReceipt(storage,owner),initial());
    assert.throws(()=>clearFormalReceipt(storage,{...initial(),scriptId:'new:another'}),/身份已变化/);
    clearFormalReceipt(storage,initial());
    reserveFormalReceipt(storage,remove());
    assert.deepEqual(readFormalReceipt(storage,owner),remove());
    clearFormalReceipt(storage,remove());
});

test('T48 a damaged stored receipt fails CLOSED and is not auto-removed',()=>{
    const storage=store(), key=formalReceiptKey(owner);
    for(const corrupted of [
        '{', 'null', '[]', '{}',
        JSON.stringify({...update(),owner:777}),
        JSON.stringify({...update(),revision:-1}),
        JSON.stringify({...update(),expectedStepIds:['same','same']}),
        JSON.stringify({...update(),createdAt:'invalid date'}),
    ]){
        storage.setItem(key,corrupted);
        assert.throws(()=>readFormalReceipt(storage,owner));
        assert.throws(()=>reserveFormalReceipt(storage,update()));
        assert.equal(storage.getItem(key),corrupted,'bad evidence must remain for operator inspection');
    }
});

test('T48 rejected proofs never touch storage',()=>{
    const storage=store();
    for(const invalid of [
        {...initial(),scriptId:'new:invalid'},
        {...initial(),revision:1},
        {...update(),expectedName:''},
        {...update(),expectedStepIds:['same','same']},
        {...update(),expectedStepIds:Array.from({length:101},(_,i)=>'step-'+i)},
        {...update(),createdAt:'no date'},
        {...update(),owner:0},
        {...remove(),revision:0},
    ]){
        assert.throws(()=>reserveFormalReceipt(storage,invalid),/无效/);
        assert.equal(storage.values.size,0);
    }
});

test('T48 persistence explicitly allowlists metadata and excludes script body',()=>{
    const storage=store();
    reserveFormalReceipt(storage,{
        ...update(),description:'private description',
        steps:[{id:'step-1',type:'input_text',text:'sensitive-value'}],
        selector:{resource_id:'secret-selector'},
        parameters:{password:'secret-password'},
    });
    const raw=storage.getItem(formalReceiptKey(owner));
    assert.ok(raw.includes(uid));
    for(const secret of ['private description','sensitive-value','secret-selector',
        'secret-password','parameters','selector','description','steps'])
        assert.equal(raw.includes(secret),false,secret+' must never appear in storage');
});

test('T48 failed storage write refuses new network write identity',()=>{
    const broken={getItem:()=>null,setItem:()=>{throw new Error('quota denied')},removeItem:()=>{}};
    assert.throws(()=>reserveFormalReceipt(broken,update()),/quota denied/);
});
