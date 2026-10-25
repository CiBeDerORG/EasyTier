import { reactive, shallowRef } from 'vue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationControl, AutomationControlReply, AutomationSession, AutomationMediaSettings } from '../../types/automation';

export type PhonePhase = 'idle' | 'opening' | 'connecting' | 'live' | 'closed' | 'failed' | 'cleanup_pending';
interface PendingControl {
    message: AutomationControl;
    resolve: (value: AutomationControlReply) => void;
    reject: (error: Error) => void;
}

/** One explicitly opened media session. No discovery, retries, polling or browser heartbeat. */
export function usePhoneSession(api: AutomationApi) {
    const state = reactive({ phase: 'idle' as PhonePhase, error: '', controlReady: false, width: 0, height: 0 });
    const stream = shallowRef<MediaStream | null>(null);
    let session: AutomationSession | null = null;
    let sessionOwner = 0;
    let peer: RTCPeerConnection | null = null;
    let socket: WebSocket | null = null;
    let generation = 0;
    let sequence = 0;
    let queue: PendingControl[] = [];
    let inflight: { item: PendingControl; id: string; timer: ReturnType<typeof setTimeout> } | null = null;
    let closing = false;
    let closePromise: Promise<void> | null = null;
    let firstFrameTimer: ReturnType<typeof setTimeout> | null = null;
    let pendingReady: { reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> } | null = null;

    function clearCommands(reason: string) {
        const error = new Error(reason);
        if (inflight) { clearTimeout(inflight.timer); inflight.item.reject(error); inflight = null; }
        for (const item of queue.splice(0)) item.reject(error);
    }
    function stopLocal(reason: string) {
        if (firstFrameTimer) { clearTimeout(firstFrameTimer); firstFrameTimer = null; }
        if (pendingReady) { clearTimeout(pendingReady.timer); pendingReady.reject(new Error(reason)); pendingReady = null; }
        state.controlReady = false;
        clearCommands(reason);
        const currentSocket = socket; socket = null;
        if (currentSocket) { currentSocket.onclose = null; currentSocket.onmessage = null; currentSocket.onerror = null; currentSocket.close(); }
        const currentPeer = peer; peer = null;
        if (currentPeer) { currentPeer.ontrack = null; currentPeer.onconnectionstatechange = null; currentPeer.close(); }
        for (const track of stream.value?.getTracks() ?? []) track.stop();
        stream.value = null;
    }
    function fail(message: string) {
        state.error = message;
        state.phase = 'failed';
        stopLocal(message);
        // A failed media session is explicitly closed once; a lost reply remains visible.
        if (session) void close(true);
    }
    function close(keepFailure = false): Promise<void> {
        if (closePromise) return closePromise;
        closePromise = closeOnce(keepFailure).finally(() => { closePromise = null; });
        return closePromise;
    }
    async function closeOnce(keepFailure: boolean) {
        closing = true; ++generation;
        stopLocal('Session closed');
        const current = session;
        try {
            if (current) {
                const result = await api.closeSession(sessionOwner, current.id);
                if (result.cleanup_pending || result.state === 'cleanup_pending') {
                    state.phase = 'cleanup_pending';
                    return;
                }
                session = null;
            }
            if (!keepFailure) { state.phase = 'closed'; state.error = ''; }
        } catch (cause) {
            state.phase = 'cleanup_pending'; state.error = automationErrorMessage(cause);
        } finally { closing = false; }
    }
    function pump() {
        if (inflight || !state.controlReady || socket?.readyState !== WebSocket.OPEN) return;
        const item = queue.shift();
        if (!item) return;
        const id = uuid();
        const timer = setTimeout(() => {
            if (inflight?.id !== id) return;
            fail('Control result not confirmed; reconnect manually.');
        }, 15000);
        inflight = { item, id, timer };
        try { socket.send(JSON.stringify({ ...item.message, command_id: id, seq: ++sequence })); }
        catch { fail('Control connection closed before delivery was confirmed.'); }
    }
    function send(message: AutomationControl): Promise<AutomationControlReply> {
        if (!state.controlReady || socket?.readyState !== WebSocket.OPEN) return Promise.reject(new Error('Control channel is unavailable'));
        return new Promise((resolve, reject) => {
            if (message.type === 'touch' && message.action === 'move') {
                const tail = queue[queue.length - 1];
                if (tail?.message.type === 'touch' && tail.message.action === 'move') {
                    queue.pop(); tail.resolve({ type: 'ack', state: 'coalesced' });
                }
            }
            if (message.type === 'touch.release' || message.type === 'stop') {
                for (const stale of queue.splice(0)) stale.reject(new Error('Control cancelled'));
            }
            if (queue.length >= 32) { reject(new Error('Control queue full')); fail('Control queue full; input stopped.'); return; }
            queue.push({ message, resolve, reject }); pump();
        });
    }
    function waitForIce(pc: RTCPeerConnection): Promise<void> {
        if (pc.iceGatheringState === 'complete') return Promise.resolve();
        return new Promise((resolve, reject) => {
            const finish = () => { clearTimeout(timer); pc.removeEventListener('icegatheringstatechange', change); };
            const change = () => { if (pc.iceGatheringState === 'complete') { finish(); resolve(); } };
            const timer = setTimeout(() => { finish(); reject(new Error('ICE gathering timed out')); }, 15000);
            pc.addEventListener('icegatheringstatechange', change);
        });
    }
    async function open(ownerId: number, deviceId: string, quality: 'wall' | 'focus', media?: AutomationMediaSettings) {
        if (session || closing || state.phase === 'opening' || state.phase === 'connecting') throw new Error('Close the existing session first');
        if (typeof RTCPeerConnection === 'undefined') throw new Error('This browser does not support WebRTC');
        const mine = ++generation;
        sessionOwner = ownerId; state.phase = 'opening'; state.error = ''; sequence = 0;
        try {
            const created = await api.createSession(ownerId, { device_id: deviceId, quality, ...(media ? { media } : {}) });
            if (mine !== generation) { await api.closeSession(ownerId, created.id); return; }
            session = created;
            const ws = new WebSocket(api.controlUrl(ownerId, created.id)); socket = ws;
            await new Promise<void>((resolve, reject) => {
                let ready = false;
                const timeout = setTimeout(() => { pendingReady = null; reject(new Error('Control connection timed out')); }, 15000);
                pendingReady = { reject, timer: timeout };
                ws.onerror = () => { clearTimeout(timeout); pendingReady = null; if (!ready) reject(new Error('Control connection failed')); else fail('Control connection failed'); };
                ws.onclose = () => { clearTimeout(timeout); pendingReady = null; if (!ready) reject(new Error('Control connection closed')); else if (mine === generation) fail('Control connection closed'); };
                ws.onmessage = event => {
                    let reply: AutomationControlReply;
                    try { reply = JSON.parse(event.data); } catch { return; }
                    if (reply.type === 'ready') { ready = true; clearTimeout(timeout); pendingReady = null; state.controlReady = true; resolve(); return; }
                    if (reply.command_id && reply.command_id === inflight?.id) {
                        const pending = inflight; inflight = null; clearTimeout(pending.timer);
                        if (reply.type === 'error') pending.item.reject(new Error(reply.message ?? reply.code ?? 'Control failed'));
                        else pending.item.resolve(reply);
                        pump();
                    } else if (reply.type === 'error') {
                        const message = reply.message ?? reply.code ?? 'Session authorization expired';
                        if (!ready) { clearTimeout(timeout); pendingReady = null; reject(new Error(message)); }
                        else fail(message);
                    }
                };
            });
            if (mine !== generation) return;
            state.phase = 'connecting';
            const pc = new RTCPeerConnection({ iceServers: created.ice_servers ?? [] }); peer = pc;
            const transceiver = pc.addTransceiver('video', { direction: 'recvonly' });
            if (media?.low_latency) {
                // Browser-specific receive-buffer hints; the bounded H.264
                // sender queue remains the authoritative stale-frame fence.
                const receiver = transceiver.receiver as RTCRtpReceiver & { playoutDelayHint?: number; jitterBufferTarget?: number };
                if ('playoutDelayHint' in receiver) receiver.playoutDelayHint = 0;
                if ('jitterBufferTarget' in receiver) receiver.jitterBufferTarget = 0;
            }
            const h264 = RTCRtpReceiver.getCapabilities?.('video')?.codecs.filter(codec => codec.mimeType.toLowerCase() === 'video/h264') ?? [];
            if (!h264.length || !transceiver.setCodecPreferences) throw new Error('This browser does not support the required H.264 WebRTC receiver');
            transceiver.setCodecPreferences(h264);
            pc.ontrack = event => {
                if (mine !== generation) return;
                stream.value = event.streams[0] ?? new MediaStream([event.track]);
                event.track.onended = () => { if (mine === generation && !closing) fail('Video track ended'); };
            };
            pc.onconnectionstatechange = () => {
                if (mine !== generation || closing) return;
                if (pc.connectionState === 'failed' || pc.connectionState === 'disconnected' || pc.connectionState === 'closed') fail('Media connection ended; reconnect manually.');
            };
            await pc.setLocalDescription(await pc.createOffer());
            await waitForIce(pc);
            if (mine !== generation || !pc.localDescription) return;
            const answer = await api.offer(ownerId, created.id, { type: 'offer', sdp: pc.localDescription.sdp });
            if (mine !== generation) return;
            state.width = answer.width; state.height = answer.height;
            await pc.setRemoteDescription({ type: 'answer', sdp: answer.sdp });
            if (mine === generation && (state.phase as PhonePhase) !== 'live') firstFrameTimer = setTimeout(() => {
                if (mine === generation && state.phase !== 'live') fail('No video frame received. Check the media route or TURN configuration.');
            }, 30000);
        } catch (cause) {
            if (mine !== generation) return;
            fail(automationErrorMessage(cause));
            throw cause;
        }
    }
    function videoReady() {
        if (!stream.value || !state.controlReady || closing) return;
        if (firstFrameTimer) { clearTimeout(firstFrameTimer); firstFrameTimer = null; }
        state.phase = 'live';
    }
    return { state, stream, open, close, send, videoReady };
}
