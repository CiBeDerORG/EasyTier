import axios from 'axios';
import { ref, shallowRef } from 'vue';
import { v4 as uuidV4 } from 'uuid';
import type { AssetError, CreateAssetRequest } from '../types/node-assets';

type AssetErrorTranslator = (key: string) => string;
type SendCreate<Payload extends object, Result> = (request: CreateAssetRequest<Payload>) => Promise<Result>;

const isRecord = (value: unknown): value is Record<string, unknown> =>
    typeof value === 'object' && value !== null && !Array.isArray(value);

function stringFields(value: unknown): Record<string, string> {
    if (!isRecord(value)) return {};
    return Object.fromEntries(Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === 'string'));
}

class AssetMutationError extends Error {
    readonly detail: AssetError;

    constructor(code: string, message: string) {
        super(message);
        this.name = 'AssetMutationError';
        this.detail = { code, message, field_errors: {} };
    }
}

/** Normalize only public response fields; callers may render field_errors near inputs. */
export function getAssetError(error: unknown): AssetError {
    if (error instanceof AssetMutationError) return error.detail;
    if (axios.isAxiosError<unknown>(error)) {
        const data = isRecord(error.response?.data) ? error.response.data : {};
        return {
            code: typeof data.code === 'string' ? data.code : error.response ? 'request_failed' : 'network_error',
            message: typeof data.message === 'string' ? data.message : '',
            field_errors: stringFields(data.field_errors),
            status: error.response?.status,
        };
    }
    if (isRecord(error)) {
        return {
            code: typeof error.code === 'string' ? error.code : 'request_failed',
            message: typeof error.message === 'string' ? error.message : '',
            field_errors: stringFields(error.field_errors),
            status: typeof error.status === 'number' ? error.status : undefined,
        };
    }
    return { code: 'request_failed', message: '', field_errors: {} };
}

const localErrorKeys: Record<string, string> = {
    asset_mutation_pending: 'assets.errors.mutationPending',
    asset_mutation_unknown: 'assets.errors.unknownOutcome',
    asset_mutation_context_changed: 'assets.errors.contextChanged',
    asset_mutation_no_attempt: 'assets.errors.noPendingAttempt',
};

/** Pass the current locale's fallback and optionally its t() function. */
export function formatAssetError(
    error: unknown,
    fallback = 'Unable to complete the operation.',
    translate?: AssetErrorTranslator,
): string {
    const detail = getAssetError(error);
    const key = localErrorKeys[detail.code];
    if (key && translate) {
        const translated = translate(key);
        if (translated !== key) return translated;
    }
    return detail.message || fallback;
}

/** A missing/gateway response is not proof that the server failed to commit. */
export function isUncertainMutationError(error: unknown): boolean {
    if (error instanceof AssetMutationError) return false;
    const status = getAssetError(error).status;
    return status === undefined || status === 408 || status >= 500;
}

function serializePayload(payload: object): string {
    return JSON.stringify(payload, (_key, value: unknown) => {
        if (!isRecord(value)) return value;
        return Object.fromEntries(Object.keys(value).sort().map(key => [key, value[key]]));
    });
}

interface Attempt {
    requestId: string;
    serialized: string;
    contextKey: string;
}

/**
 * One create form owns one attempt. Keep fields disabled while uncertain and
 * call run again with the same payload, or retry with the same typed sender.
 * reset() is for closing/starting a form or changing its account/node context;
 * it prevents late responses from updating this helper's new state.
 */
export function useAssetMutation<DefaultPayload extends object = object, DefaultResult = unknown>() {
    const pending = ref(false);
    const uncertain = ref(false);
    const error = shallowRef<unknown>(null);
    let generation = 0;
    let attempt: Attempt | null = null;

    const reset = () => {
        generation += 1;
        attempt = null;
        pending.value = false;
        uncertain.value = false;
        error.value = null;
    };

    const run = async <Payload extends DefaultPayload, Result = DefaultResult>(
        payload: Payload,
        send: SendCreate<Payload, Result>,
        contextKey = '',
    ): Promise<Result> => {
        if (pending.value) {
            throw new AssetMutationError('asset_mutation_pending', 'A save is already in progress.');
        }
        const serialized = serializePayload(payload);
        if (attempt && uncertain.value) {
            if (attempt.contextKey !== contextKey) {
                throw new AssetMutationError('asset_mutation_context_changed', 'The account or node changed. Reopen the form before continuing.');
            }
            if (attempt.serialized !== serialized) {
                throw new AssetMutationError('asset_mutation_unknown', 'The previous save may have succeeded. Retry its original values before changing them.');
            }
        } else {
            attempt = { requestId: uuidV4(), serialized, contextKey };
        }

        const activeAttempt = attempt;
        const activeGeneration = generation;
        pending.value = true;
        error.value = null;
        const isCurrent = () => generation === activeGeneration && attempt === activeAttempt;
        try {
            // A separate copy prevents mutation of the submitted snapshot by forms or senders.
            const snapshot = JSON.parse(activeAttempt.serialized) as Payload;
            const result = await send({ ...snapshot, request_id: activeAttempt.requestId });
            if (isCurrent()) {
                attempt = null;
                uncertain.value = false;
            }
            return result;
        } catch (cause: unknown) {
            if (isCurrent()) {
                error.value = cause;
                uncertain.value = isUncertainMutationError(cause);
                if (!uncertain.value) attempt = null;
            }
            throw cause;
        } finally {
            // Success clears attempt, so use generation rather than isCurrent() here.
            if (generation === activeGeneration) pending.value = false;
        }
    };

    const retry = async <Payload extends DefaultPayload = DefaultPayload, Result = DefaultResult>(
        send: SendCreate<Payload, Result>,
        contextKey = '',
    ): Promise<Result> => {
        if (!attempt || !uncertain.value) {
            throw new AssetMutationError('asset_mutation_no_attempt', 'There is no unresolved save to retry.');
        }
        return run(JSON.parse(attempt.serialized) as Payload, send, contextKey);
    };

    return { pending, uncertain, error, run, execute: run, retry, reset };
}
