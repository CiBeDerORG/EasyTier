/**
 * A batch POST may have reached Rust even if HTTP timed out. Persist only its
 * request ID and target IDs; never store credentials, parameters or steps.
 * A missing entry in a paged list is NOT proof the command was not dispatched.
 */
export interface BatchSubmissionReceipt {
    ownerId: number;
    requestId: string;
    deviceIds: string[];
    createdAt: string;
}

function key(owner: number): string { return 'easytier.automation.batch.pending.v1.' + owner; }
export function readBatchReceipt(storage: Storage, owner: number): BatchSubmissionReceipt | null {
    const raw=storage.getItem(key(owner));
    if(!raw)return null;
    const value:unknown=JSON.parse(raw);
    if(!value || typeof value!=='object')throw new Error('批量请求保护凭据损坏，请人工核对历史记录。');
    const receipt=value as Partial<BatchSubmissionReceipt>;
    if(receipt.ownerId!==owner || typeof receipt.requestId!=='string' ||
        !/^[a-f\d-]{36}$/i.test(receipt.requestId) || !Array.isArray(receipt.deviceIds) ||
        !receipt.deviceIds.length || receipt.deviceIds.some(id=>typeof id!=='string'))
        throw new Error('批量请求保护凭据不完整，禁止新建运行。');
    return receipt as BatchSubmissionReceipt;
}
export function reserveBatchReceipt(storage: Storage, receipt: BatchSubmissionReceipt): void {
    if(readBatchReceipt(storage,receipt.ownerId))
        throw new Error('存在尚未人工核对的批量运行请求，禁止产生第二次操作。');
    storage.setItem(key(receipt.ownerId),JSON.stringify(receipt));
    if(readBatchReceipt(storage,receipt.ownerId)?.requestId!==receipt.requestId)
        throw new Error('不能确认已保存批量请求身份，禁止提交。');
}
export function clearBatchReceipt(storage: Storage, receipt: BatchSubmissionReceipt): void {
    if(readBatchReceipt(storage,receipt.ownerId)?.requestId!==receipt.requestId)
        throw new Error('请求身份已变化，禁止移除批量保护。');
    storage.removeItem(key(receipt.ownerId));
}
