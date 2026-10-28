/**
 * T48 – Formal script POST/PUT/DELETE recovery fence.
 *
 * Storage contains only metadata necessary to identify a possible write.
 * Never save text input values, the script body, selectors or credentials.
 * A corrupt receipt must fail closed; never silently replace or remove it.
 */
export interface FormalScriptWriteReceipt {
    owner: number;
    kind: 'publish' | 'create' | 'delete';
    scriptId: string;
    revision: number;
    expectedName?: string;
    expectedStepIds?: string[];
    createdAt: string;
}
export interface FormalReceiptStorage {
    getItem(key:string):string|null;
    setItem(key:string,value:string):void;
    removeItem(key:string):void;
}
export const formalReceiptKey=(owner:number):string=>
    'easytier.automation.formal-write.v1.'+owner;

function valid(proof:Partial<FormalScriptWriteReceipt>,owner:number):boolean {
    if(!Number.isSafeInteger(owner)||owner<=0||proof.owner!==owner ||
        !['publish','create','delete'].includes(proof.kind??'') ||
        typeof proof.scriptId!=='string'||!proof.scriptId.trim() ||
        !Number.isSafeInteger(proof.revision) ||
        typeof proof.createdAt!=='string' ||
        !Number.isFinite(Date.parse(proof.createdAt)))return false;
    if(proof.kind==='create') {
        if(proof.revision!==0 || !/^new:[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(proof.scriptId))return false;
    }else if((proof.revision??0)<1)return false;
    if(proof.kind!=='delete'){
        const ids=proof.expectedStepIds;
        if(typeof proof.expectedName!=='string'||!proof.expectedName.trim() ||
            !Array.isArray(ids)||ids.length>100||
            ids.some(id=>typeof id!=='string'||!id.trim()) ||
            new Set(ids).size!==ids.length)return false;
    }
    return true;
}
export function readFormalReceipt(
    storage:FormalReceiptStorage,owner:number,
):FormalScriptWriteReceipt|null {
    const raw=storage.getItem(formalReceiptKey(owner));
    if(raw===null)return null;
    let parsed:unknown;
    try{parsed=JSON.parse(raw);}
    catch {throw new Error('正式脚本写操作凭据已损坏；禁止新发请求，请人工核对。');}
    if(!parsed||typeof parsed!=='object'||Array.isArray(parsed) ||
        !valid(parsed as Partial<FormalScriptWriteReceipt>,owner))
        throw new Error('正式脚本写操作身份不完整，禁止新发请求。');
    return parsed as FormalScriptWriteReceipt;
}
export function reserveFormalReceipt(
    storage:FormalReceiptStorage,proof:FormalScriptWriteReceipt,
):void {
    if(!valid(proof,proof.owner))
        throw new Error('正式脚本写操作凭据无效，未发送写请求。');
    if(readFormalReceipt(storage,proof.owner))
        throw new Error('存在尚未核对的正式脚本写入，禁止重复提交。');
    // Build an explicit allowlist rather than serializing the caller's
    // object. Runtime extras must not leak credentials, script bodies or text.
    const metadata:FormalScriptWriteReceipt={
        owner:proof.owner,kind:proof.kind,scriptId:proof.scriptId,
        revision:proof.revision,createdAt:proof.createdAt,
        ...(proof.kind==='delete'?{}:{
            expectedName:proof.expectedName,expectedStepIds:[...proof.expectedStepIds!],
        }),
    };
    storage.setItem(formalReceiptKey(proof.owner),JSON.stringify(metadata));
    const saved=readFormalReceipt(storage,proof.owner);
    if(!saved||saved.kind!==proof.kind||saved.scriptId!==proof.scriptId||
        saved.revision!==proof.revision ||saved.createdAt!==proof.createdAt)
        throw new Error('无法确认已保存本次正式脚本写入身份，禁止发送。');
}
export function clearFormalReceipt(
    storage:FormalReceiptStorage,proof:FormalScriptWriteReceipt,
):void {
    const saved=readFormalReceipt(storage,proof.owner);
    if(!saved||saved.kind!==proof.kind||saved.scriptId!==proof.scriptId||
        saved.revision!==proof.revision||saved.createdAt!==proof.createdAt)
        throw new Error('正式脚本回执身份已变化，不允许清除原写入保护。');
    storage.removeItem(formalReceiptKey(proof.owner));
}
