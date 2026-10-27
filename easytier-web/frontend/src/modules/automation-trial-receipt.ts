/**
 * P0: page-close resistant single-step POST handoff fence.
 * Persist ONLY request metadata, never the immutable POST body, passwords,
 * selectors, authentication tokens or user text. In particular, reload must
 * never reconstruct a new POST from a possibly different step.
 */
export interface TrialSubmissionReceipt {
    ownerId: number;
    deviceId: string;
    stepId: string;
    requestId: string;
    createdAt: string;
}
export interface ReceiptStore {
    getItem(key:string):string|null;
    setItem(key:string,value:string):void;
    removeItem(key:string):void;
}
const key=(ownerId:number,deviceId:string)=>
    'easytier.automation.pending-trial.v1.'+ownerId+'.'+deviceId;
const uuid=/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
function valid(receipt:TrialSubmissionReceipt,ownerId:number,deviceId:string):boolean {
    return receipt.ownerId===ownerId && receipt.deviceId===deviceId &&
        !!receipt.stepId && uuid.test(receipt.requestId) &&
        Number.isFinite(Date.parse(receipt.createdAt));
}
export function readTrialReceipt(store:ReceiptStore,ownerId:number,deviceId:string):TrialSubmissionReceipt|null {
    const raw=store.getItem(key(ownerId,deviceId));
    if(raw===null)return null;
    try {
        const parsed=JSON.parse(raw) as TrialSubmissionReceipt;
        if(valid(parsed,ownerId,deviceId))return parsed;
    }catch { /* Invalid bytes must fail CLOSED, never vanish silently. */ }
    throw new Error('该手机的历史试跑请求凭据已损坏；为避免重复动作，已禁止创建新请求。');
}
/** Write BEFORE sending POST; never overwrite a different pending UUID. */
export function reserveTrialReceipt(store:ReceiptStore,receipt:TrialSubmissionReceipt):void {
    if(!valid(receipt,receipt.ownerId,receipt.deviceId))
        throw new Error('试跑请求凭据无效，不能发送手机动作。');
    const existing=readTrialReceipt(store,receipt.ownerId,receipt.deviceId);
    if(existing && existing.requestId!==receipt.requestId)
        throw new Error('该手机还有接收结果未确认的试跑请求：'+existing.requestId+
            '；必须先只读核对历史，不允许另建执行请求。');
    store.setItem(key(receipt.ownerId,receipt.deviceId),JSON.stringify(receipt));
    if(readTrialReceipt(store,receipt.ownerId,receipt.deviceId)?.requestId!==receipt.requestId)
        throw new Error('无法确认试跑请求凭据已保存；为避免重复动作，停止执行。');
}
/** Delete only the exact recognized receipt. Never clear another trial. */
export function clearTrialReceipt(store:ReceiptStore,receipt:TrialSubmissionReceipt):void {
    const current=readTrialReceipt(store,receipt.ownerId,receipt.deviceId);
    if(current?.requestId===receipt.requestId)store.removeItem(key(receipt.ownerId,receipt.deviceId));
}
/** Request identity is from persisted server results, never from a new POST. */
export function findTrialRun<T extends {request_id?:string;devices:{device_id:string}[]}>(
    runs:T[],receipt:TrialSubmissionReceipt,
):T|null {
    return runs.find(run=>run.request_id===receipt.requestId &&
        run.devices.some(device=>device.device_id===receipt.deviceId))??null;
}
