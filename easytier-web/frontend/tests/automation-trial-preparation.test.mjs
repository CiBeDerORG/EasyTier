import assert from "node:assert/strict";
import test from "node:test";
import { useCardTrialPreparation } from "../src/modules/useCardTrialPreparation.ts";

const ownerId = 42;
const deviceId = "trial-phone";
const fingerprint = "trial-fingerprint";
const sessionId = "trial-session";

function fixture(componentStateSequence) {
    let componentCalls = 0;
    let starts = 0;
    const device = {
        id: deviceId,
        name: "Trial phone",
        owner_user_id: ownerId,
        target_fingerprint: fingerprint,
        connection: { enabled: true, adb_port: 37123, connection_mode: "wireless" },
        availability_reason: null,
        active_session: {
            id: sessionId, state: "active", actor_user_id: ownerId,
            expires_at: "2099-01-01T00:00:00.000Z",
        },
        run_occupied: false,
    };
    const job = {
        id: "prep-job",
        request_id: "prep-request",
        device_id: deviceId,
        target_fingerprint: fingerprint,
        state: "queued",
        stage: "queued",
        message: "queued",
    };
    const api = {
        devices: async () => ({ items: [device] }),
        adbStatus: async () => ({ device_id: deviceId, state: "connected" }),
        componentStatus: async () => ({
            device_id: deviceId,
            state: componentStateSequence[Math.min(componentCalls++, componentStateSequence.length - 1)],
        }),
        prepare: async () => { throw new Error("unexpected direct prepare call"); },
        preparationHistory: async () => ({ items: [] }),
        startPreparation: async () => { starts++; return { ...job }; },
        preparationDetail: async () => ({ ...job, state: "succeeded", stage: "ready", message: "ready" }),
        cancelPreparation: async () => ({ ...job, state: "cancelled" }),
    };
    const manager = useCardTrialPreparation(api, {
        requestTimeoutMs: 1500,
        preparationTimeoutMs: 10000,
        pollIntervalMs: 500,
    });
    const target = {
        ownerId, deviceId, targetFingerprint: fingerprint,
        ownedSessionId: sessionId,
        steps: [{ id: "step-1", type: "tap", selector: { text: "Continue" } }],
    };
    return { manager, target, getStarts: () => starts, getComponentCalls: () => componentCalls };
}

test("single-step preparation rebuilds an unavailable UiAutomator2 helper before proceeding", async () => {
    const f = fixture(["unavailable", "ready"]);
    try {
        const result = await f.manager.run(f.target);
        assert.equal(result.phase, "ready", result.message);
        assert.equal(f.getStarts(), 1, "must start one fresh preparation job");
        assert.equal(f.getComponentCalls(), 2, "must verify the replacement helper is ready");
    } finally {
        f.manager.dispose();
    }
});

test("read-only reconciliation never starts a replacement preparation", async () => {
    const f = fixture(["unavailable"]);
    try {
        const result = await f.manager.reconcile(f.target);
        assert.equal(result.phase, "failed");
        assert.match(result.message, /只读复核不会自动重启组件/);
        assert.equal(f.getStarts(), 0);
    } finally {
        f.manager.dispose();
    }
});
