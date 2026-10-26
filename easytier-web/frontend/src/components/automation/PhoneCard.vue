<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { Button, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import { v4 as uuid } from 'uuid';
import AutomationApi from '../../modules/automation-api';
import type { AutomationControl, AutomationDevice, AutomationMediaSettings, AutomationComponentStatus, AutomationPhoneConnectionState } from '../../types/automation';
import { automationMessages } from './messages';
import type { EndpointEvent } from '../../modules/discovery-api';
import { usePhoneSession } from './usePhoneSession';
import PhoneAutomationPanel from './PhoneAutomationPanel.vue';

const props = defineProps<{
    device: AutomationDevice; ownerId: number; selected: boolean; runnerAvailable: boolean;
    phoneConnection?: AutomationPhoneConnectionState; phoneConnectionError?: string;
    componentStatus?: AutomationComponentStatus;
    syncRole?: 'master' | 'follower'; syncActive: boolean; recording: boolean; unlocking?: boolean;
}>();
const emit = defineEmits<{
    selection: [id: string, selected: boolean]; configure: [device: AutomationDevice];
    input: [id: string, message: AutomationControl]; action: [id: string, message: AutomationControl]; unlock: [id: string];
    text: [id: string]; record: [id: string]; connect: [id: string]; 'toggle-screen': [id: string]; inspect: [id: string];
    state: [id: string, phase: string]; geometry: [id: string]; stopSync: []; refresh: [];
}>();
const { t } = useI18n({ useScope: 'local', messages: automationMessages });
const api = new AutomationApi();
const session = usePhoneSession(api);
const video = ref<HTMLVideoElement | null>(null);
const ambientVideo = ref<HTMLVideoElement | null>(null);
const card = ref<HTMLElement | null>(null);
const screen = ref<HTMLElement | null>(null);
const isFullscreen = ref(false);
function syncFullscreen() {
    isFullscreen.value = document.fullscreenElement === screen.value;
}
onMounted(() => document.addEventListener('fullscreenchange', syncFullscreen));
const dock = ref<'left' | 'right'>('right');
const anchorFraction = ref(0.5);
const menuOpen = ref(false);
const floatingStyle = computed(() => ({
    left: dock.value === 'right'
        ? menuOpen.value ? 'calc(100% - 48px)' : 'calc(100% - 20px)'
        : menuOpen.value ? '4px' : '-24px',
    top: `${anchorFraction.value * 100}%`,
}));
let drag: { id: number; firstX: number; firstY: number; firstFraction: number; moved: boolean } | null = null;
let suppressOrbClick = false;
let frameCallback: number | null = null;
function dragStart(event: PointerEvent) {
    if (event.button !== 0 || !screen.value) return;
    drag = { id: event.pointerId, firstX: event.clientX, firstY: event.clientY,
        firstFraction: anchorFraction.value, moved: false };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
}
function dragMove(event: PointerEvent) {
    if (drag?.id !== event.pointerId || !screen.value) return;
    const bounds = screen.value.getBoundingClientRect();
    const x = event.clientX - drag.firstX, y = event.clientY - drag.firstY;
    if (!drag.moved && Math.hypot(x, y) < 7) return;
    event.preventDefault();
    drag.moved = true;
    anchorFraction.value = Math.max(menuOpen.value ? 0.23 : 0.07, Math.min(menuOpen.value ? 0.69 : 0.93, drag.firstFraction + y / bounds.height));
    dock.value = event.clientX - bounds.left < bounds.width / 2 ? 'left' : 'right';
}
function dragEnd(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    if (screen.value && drag.moved) {
        const bounds = screen.value.getBoundingClientRect();
        dock.value = event.clientX - bounds.left < bounds.width / 2 ? 'left' : 'right';
        suppressOrbClick = true;
    }
    drag = null;
}
const menuIdleMs = 6000;
let menuTimer: ReturnType<typeof setTimeout> | undefined;
function clearMenuTimer() {
    if (menuTimer !== undefined) clearTimeout(menuTimer);
    menuTimer = undefined;
}
function markMenuInteraction() {
    if (!menuOpen.value) return;
    clearMenuTimer();
    menuTimer = setTimeout(() => { menuOpen.value = false; }, menuIdleMs);
}
watch(menuOpen, open => { if (open) markMenuInteraction(); else clearMenuTimer(); });
function toggleOrb() {
    if (suppressOrbClick) { suppressOrbClick = false; return; }
    menuOpen.value = !menuOpen.value;
    if (menuOpen.value) anchorFraction.value = Math.max(0.23, Math.min(0.69, anchorFraction.value));
}
function stopFrameChase() {
    if (frameCallback !== null && video.value?.cancelVideoFrameCallback) video.value.cancelVideoFrameCallback(frameCallback);
    frameCallback = null;
    if (video.value) {
        try { video.value.playbackRate = 1; } catch { /* live stream may not allow changing playback rate */ }
    }
}
function startFrameChase() {
    stopFrameChase();
    const player = video.value;
    if (!player?.requestVideoFrameCallback || !watchMedia.low_latency) return;
    const inspect: VideoFrameRequestCallback = (now, metadata) => {
        if (disposed || session.state.phase !== 'live' || video.value !== player) { stopFrameChase(); return; }
        // Best-effort catch-up on browsers that support live playback rate
        // adjustments. The bounded RTP track independently discards stale
        // interframes until a decodable keyframe is received.
        try { player.playbackRate = now - metadata.expectedDisplayTime > 120 ? 1.12 : 1; }
        catch { /* Unsupported on this browser's MediaStream playback */ }
        frameCallback = player.requestVideoFrameCallback(inspect);
    };
    frameCallback = player.requestVideoFrameCallback(inspect);
}
function videoReady() {
    session.videoReady();
    startFrameChase();
}
const cleaningSaved = ref(false);
const pointer = ref<{ id: number; width: number; height: number } | null>(null);
const currentGeometry = ref({ width: 0, height: 0 });
// Match EasyADB: the card tracks encoded video width, not an expanding grid
// column. Keep the phone's natural width unless the viewport is narrower.
const cardWidth = computed(() => `${(currentGeometry.value.width > 0 ? currentGeometry.value.width : 360) + 26}px`);
const ready = computed(() => session.state.phase === 'live' && session.state.controlReady);
const manualAllowed = computed(() => ready.value && !props.syncRole);
const touchAllowed = computed(() => ready.value && props.syncRole !== 'follower');
const pending = computed(() => ['opening', 'connecting'].includes(session.state.phase));
const adbConnectionHint = computed(() => {
    if (props.phoneConnectionError) return props.phoneConnectionError;
    const hints: Record<string,string> = {
        checking: '正在查询 ADB 连接状态', connected: 'ADB 已连接',
        offline: 'ADB 离线', unauthorized: 'ADB 未授权',
        disconnected: 'ADB 未连接', unavailable: 'ADB 状态查询超时',
        query_failed: 'ADB 查询失败', connecting: '正在连接 ADB',
        failed: 'ADB 连接失败',
    };
    return hints[props.phoneConnection || ''] || t('phoneNotConnected');
});
const canOpen = computed(() => props.runnerAvailable && props.device.connection.enabled && !props.device.availability_reason && !props.device.active_session && !pending.value && session.state.phase !== 'live' && session.state.phase !== 'cleanup_pending');
const inspectorHint = computed(() => {
    if (!props.runnerAvailable) return '控件拾取 · 执行器不可用';
    if (props.syncRole) return '控件拾取 · 请先结束群控同步';
    if (props.recording) return '控件拾取 · 请先结束脚本录制';
    return ready.value ? '控件拾取 · 点击查看实时画面中的 UiAutomator2 控件' : '控件拾取 · 请先打开手机实时画面并等待控制就绪';
});
const screenActionHint = computed(() => {
    const port = props.device.connection.adb_port || '未设置';
    const prefix = `ADB 端口：${port} · ${adbConnectionHint.value}`;
    if (!props.runnerAvailable) return prefix + ' · 执行器不可用';
    if (!props.device.connection.enabled) return prefix + ' · 连接未启用';
    if (pending.value) return prefix + ' · 正在打开画面';
    if (session.state.phase === 'live') return prefix + ' · 正在投屏，点击关闭画面';
    if (session.state.phase === 'cleanup_pending') return prefix + ' · 资源清理待确认';
    if (session.state.phase === 'failed') return prefix + ' · 上次打开失败，点击重试';
    return prefix + ' · 点击打开画面';
});
watch(() => session.state.phase, phase => {
    emit('state', props.device.id, phase);
    // The browser must not remain fullscreen once this card loses its stream.
    if (phase !== 'live' && document.fullscreenElement === screen.value) {
        void document.exitFullscreen().catch(() => undefined);
    }
});
watch(session.stream, value => {
    if (video.value) video.value.srcObject = value;
    if (!value) {
        stopFrameChase();
        currentGeometry.value = { width: 0, height: 0 };
    }
}, { flush: 'post' });
// An ambient copy only exists in fullscreen, behind the real un-cropped frame.
// It fills unused monitor space without distorting the interactive picture.
watch([isFullscreen, session.stream], ([active, stream]) => {
    if (ambientVideo.value) ambientVideo.value.srcObject = active ? stream : null;
}, { flush: 'post' });

function geometry() { return { width: video.value?.videoWidth ?? 0, height: video.value?.videoHeight ?? 0 }; }
function onGeometry() {
    const next = geometry();
    if (currentGeometry.value.width && (next.width !== currentGeometry.value.width || next.height !== currentGeometry.value.height)) {
        cancelPointer(); emit('geometry', props.device.id);
    }
    currentGeometry.value = next;
}
function point(event: PointerEvent, clamp: boolean) {
    if (!video.value) return null;
    const { width, height } = geometry();
    if (!width || !height) return null;
    const rect = video.value.getBoundingClientRect();
    // Match EasyADB: map only the undistorted picture (object-fit:contain).
    // Ignore the surrounding fullscreen ambient backdrop when tapping.
    const scale = Math.min(rect.width / width, rect.height / height);
    const left = rect.left + (rect.width - width * scale) / 2;
    const top = rect.top + (rect.height - height * scale) / 2;
    const x = (event.clientX - left) / scale;
    const y = (event.clientY - top) / scale;
    if (!clamp && (x < 0 || y < 0 || x >= width || y >= height)) return null;
    return { x: Math.max(0, Math.min(width - 1, Math.round(x))), y: Math.max(0, Math.min(height - 1, Math.round(y))), width, height };
}
function down(event: PointerEvent) {
    if (!touchAllowed.value || event.button !== 0 || pointer.value || menuOpen.value) return;
    const position = point(event, false); if (!position) return;
    event.preventDefault();
    pointer.value = { id: event.pointerId, width: position.width, height: position.height };
    video.value?.setPointerCapture(event.pointerId);
    emit('input', props.device.id, { type: 'touch', action: 'down', ...position });
}
function move(event: PointerEvent) {
    if (pointer.value?.id !== event.pointerId) return;
    const position = point(event, true); if (!position) return;
    if (position.width !== pointer.value.width || position.height !== pointer.value.height) { cancelPointer(); emit('geometry', props.device.id); return; }
    event.preventDefault(); emit('input', props.device.id, { type: 'touch', action: 'move', ...position });
}
function up(event: PointerEvent) {
    if (pointer.value?.id !== event.pointerId) return;
    const position = point(event, true); const previous = pointer.value; pointer.value = null;
    if (video.value?.hasPointerCapture(event.pointerId)) video.value.releasePointerCapture(event.pointerId);
    if (position && position.width === previous.width && position.height === previous.height) emit('input', props.device.id, { type: 'touch', action: 'up', ...position });
    else emit('input', props.device.id, { type: 'touch.release' });
}
function cancelPointer() {
    if (!pointer.value) return;
    const id = pointer.value.id; pointer.value = null;
    if (video.value?.hasPointerCapture(id)) video.value.releasePointerCapture(id);
    emit('input', props.device.id, { type: 'touch.release' });
}
function resetPointer() {
    if (!pointer.value) return;
    const id = pointer.value.id; pointer.value = null;
    if (video.value?.hasPointerCapture(id)) video.value.releasePointerCapture(id);
}
async function fullscreen() {
    menuOpen.value = false;
    // Fullscreen only the video surface, not its card/header/footer.
    // The screen owns the floating controls and the input dialog portal.
    if (document.fullscreenElement === screen.value) await document.exitFullscreen();
    else await screen.value?.requestFullscreen();
}
function localAction(message: AutomationControl) { if (manualAllowed.value) emit('action', props.device.id, message); }
let watching = false, disposed = false, intentVersion = 0, latestEndpoint = -1, restoredEndpoint = -1;
let watchQuality: 'wall' | 'focus' = 'wall';
let watchMedia: AutomationMediaSettings = { max_size: 640, video_bit_rate: 1000000, max_fps: 15, low_latency: true, screen_off: false };
async function open(quality: 'wall' | 'focus', options: AutomationMediaSettings = watchMedia) {
    watching = true; ++intentVersion; watchQuality = quality; watchMedia = { ...options };
    await session.open(props.ownerId, props.device.id, quality, watchMedia);
}
function cancelRestoreIntent() { watching = false; ++intentVersion; }
async function close() {
    cancelRestoreIntent(); cancelPointer(); menuOpen.value = false; clearMenuTimer(); stopFrameChase();
    // Close the fullscreen container *before* tearing down its video.
    if (document.fullscreenElement === screen.value) {
        try { await document.exitFullscreen(); } catch { /* session cleanup still takes priority */ }
    }
    await session.close();
}
async function endpointChanged(event: EndpointEvent) {
    if (event.device_id !== props.device.id || event.endpoint_revision < latestEndpoint || event.endpoint_revision <= restoredEndpoint) return;
    const changed = event.endpoint_revision > latestEndpoint;
    latestEndpoint = event.endpoint_revision;
    const intent = intentVersion;
    resetPointer(); emit('geometry', props.device.id);
    if (changed) await session.close(); // clears queued input and the previous peer before any replacement
    if (disposed || !watching || intent !== intentVersion || event.endpoint_revision !== latestEndpoint) return;
    if (event.cleanup.state !== 'confirmed' || event.connection.state !== 'connected') return;
    await session.close();
    if (disposed || !watching || intent !== intentVersion || event.endpoint_revision !== latestEndpoint || session.state.phase === 'cleanup_pending') return;
    restoredEndpoint = event.endpoint_revision;
    try { await session.open(props.ownerId, props.device.id, watchQuality, watchMedia); }
    catch { /* bounded attempt: session exposes its failure, user can retry */ }
}
async function cleanupSaved() {
    if (!props.device.active_session || cleaningSaved.value) return;
    cleaningSaved.value = true;
    try {
        const result = await api.closeSession(props.ownerId, props.device.active_session.id);
        if (result.cleanup_pending) { session.state.phase = 'failed'; session.state.error = result.message || t('cleanupFirst'); }
        else { session.state.phase = 'closed'; session.state.error = ''; }
        emit('refresh');
    } catch (cause) { session.state.error = cause instanceof Error ? cause.message : t('actionFailed'); }
    finally { cleaningSaved.value = false; }
}
async function retryEndpoint(stage: 'cleanup'|'connection'|'publication') {
    const event = props.device.endpoint_event; if (!event || cleaningSaved.value) return;
    cleaningSaved.value = true;
    try { await api.retryEndpoint(props.ownerId, props.device.id, event.event_id, {request_id:uuid(),stage}); emit('refresh'); }
    catch(cause) { session.state.error = cause instanceof Error ? cause.message : '重试未完成'; }
    finally { cleaningSaved.value = false; }
}
function getState() { return { phase: session.state.phase, ready: ready.value, ...geometry() }; }
function getStream() { return session.stream.value; }
function getSessionId() { return session.getSessionId(); }
defineExpose({ open, close, cancelRestoreIntent, endpointChanged, send: session.send, unlock: session.unlock, getState, getStream, getSessionId, cancelPointer, resetPointer });
onBeforeUnmount(() => {
    document.removeEventListener('fullscreenchange', syncFullscreen);
    if (document.fullscreenElement === screen.value) void document.exitFullscreen().catch(() => undefined);
    if (ambientVideo.value) ambientVideo.value.srcObject = null;
    disposed = true; watching = false; ++intentVersion; clearMenuTimer(); stopFrameChase(); cancelPointer(); void session.close();
});
</script>

<template>
    <article ref="card" class="phone-card" :class="{ selected, master: syncRole === 'master' }" :style="{ width: cardWidth }">
        <header>
            <div class="device-heading">
                <label class="device-selection" :title="device.name"><input type="checkbox" :checked="selected" :aria-label="device.name" @change="emit('selection', device.id, ($event.target as HTMLInputElement).checked)"><span class="device-name">{{ device.name }}</span></label>
                <button type="button" class="adb-status" :class="phoneConnection || 'unknown'"
                    :disabled="!runnerAvailable || !device.connection.enabled || phoneConnection === 'connecting'"
                    :aria-label="'ADB：' + adbConnectionHint" :title="adbConnectionHint"
                    @click="emit('connect', device.id)">
                    <span class="adb-status-dot" aria-hidden="true" /><span class="sr-only">{{ t(phoneConnection === 'connected' ? 'phoneConnected' : phoneConnection === 'failed' ? 'phoneConnectionFailed' : phoneConnection === 'connecting' ? 'phoneConnecting' : 'phoneNotConnected') }}</span>
                </button>
            </div>
            <div class="card-header-actions">
                <PhoneAutomationPanel :device="device" :owner-id="ownerId" :runner-available="runnerAvailable" :observed-status="componentStatus" />
                <span class="card-action-tip" :title="inspectorHint">
                    <button type="button" class="card-icon inspector-icon"
                        :class="{ active: ready && !syncRole && !recording, pending, blocked: !!syncRole || recording }"
                        :aria-label="inspectorHint" :disabled="!ready || !!syncRole || recording"
                        @click="emit('inspect', device.id)">
                        <i class="pi pi-search" aria-hidden="true" />
                    </button>
                </span>
                <span class="card-action-tip" :title="screenActionHint">
                    <button type="button" class="card-icon adb-port"
                        :class="[session.state.phase, { unavailable: !runnerAvailable || !device.connection.enabled }]"
                        :aria-label="screenActionHint" :disabled="!runnerAvailable || !device.connection.enabled || pending"
                        @click="emit('toggle-screen', device.id)">
                        <i class="pi pi-desktop" aria-hidden="true" />
                    </button>
                </span>
                <Button class="card-settings" icon="pi pi-cog" text rounded
                    :aria-label="t('configure')" :title="t('configure')"
                    :disabled="pending || session.state.phase === 'live'" @click="emit('configure', device)" />
            </div>
        </header>
        <div class="device-meta">
            <span class="node-name" :title="device.gateway_name || t('ungrouped')">{{ device.gateway_name || t('ungrouped') }}</span>
        </div>
        <div ref="screen" class="phone-screen" :style="currentGeometry.width && currentGeometry.height ? { aspectRatio: `${currentGeometry.width} / ${currentGeometry.height}` } : undefined" @keydown.esc="menuOpen = false">
            <video v-if="isFullscreen && session.state.phase === 'live'" ref="ambientVideo" class="ambient-video" autoplay muted playsinline aria-hidden="true" tabindex="-1" />
            <video ref="video" class="primary-video" autoplay muted playsinline :class="{ interactive: touchAllowed }" :aria-label="device.name" @loadedmetadata="onGeometry" @loadeddata="videoReady" @resize="onGeometry" @pointerdown="down" @pointermove="move" @pointerup="up" @pointercancel="cancelPointer" @lostpointercapture="cancelPointer" @contextmenu.prevent />
            <div v-if="session.state.phase !== 'live'" class="screen-placeholder">
                <i :class="pending ? 'pi pi-spin pi-spinner' : 'pi pi-mobile'" aria-hidden="true" />
                <span>{{ t(session.state.phase) }}</span>
                <Button v-if="canOpen" :label="t('open')" icon="pi pi-play" size="small" @click="emit('toggle-screen', device.id)" />
                <Button v-if="session.state.phase === 'cleanup_pending'" :label="t('clearResources')" icon="pi pi-refresh" size="small" severity="warn" @click="close().then(() => emit('refresh'))" />
                <Button v-if="device.active_session && !pending && session.state.phase !== 'cleanup_pending'" :label="t('cleanupSaved')" icon="pi pi-stop" size="small" severity="warn" :loading="cleaningSaved" @click="cleanupSaved" />
                <small v-if="!device.connection.enabled">{{ t('disabled') }}</small>
            </div>
            <Tag v-if="syncRole" class="role-badge" :value="t(syncRole)" :severity="syncRole === 'master' ? 'info' : 'secondary'" />
            <span v-if="recording" class="record-badge"><span /> REC</span>
            <Button v-if="syncActive" class="fullscreen-stop" severity="danger" :label="t('stopSync')" size="small" @click="emit('stopSync')" />
            <button v-if="menuOpen" class="control-dismiss" :aria-label="t('collapse')" @pointerdown.stop @click.stop="menuOpen = false" />
            <div class="phone-controls" :class="`dock-${dock}`" :style="floatingStyle" @pointerdown.capture="markMenuInteraction" @click.capture="markMenuInteraction" @pointerdown.stop @click.stop>
                <div v-if="menuOpen" class="floating-actions" role="group" :aria-label="t('localControl')">
                    <div class="input-actions">
                        <Button icon="pi pi-pencil" rounded :aria-label="t('text')" :title="t('text')" :disabled="!manualAllowed" @click="emit('text', device.id)" />
                        <Button icon="pi pi-unlock" rounded severity="primary" :aria-label="t('unlockPhone')" :title="t('unlockPhone')" :loading="unlocking" :disabled="!manualAllowed || unlocking || recording" @click="emit('unlock', device.id)" />
                        <Button icon="pi pi-ban" rounded severity="secondary" :aria-label="t('releaseManual')" :title="t('releaseManual')" :disabled="!manualAllowed || unlocking" @click="localAction({ type: 'stop' })" />
                    </div>
                    <div class="side-actions">
                        <Button icon="pi pi-stop" rounded severity="danger" :aria-label="t('close')" :title="t('close')" @click="close().then(() => emit('refresh'))" />
                        <Button icon="pi pi-expand" rounded :aria-label="t('fullscreen')" :title="t('fullscreen')" :disabled="!ready" @click="fullscreen().catch(() => {})" />
                        <Button icon="pi pi-power-off" rounded :aria-label="t('wake')" :title="t('wake')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'WAKE' })" />
                        <Button icon="pi pi-circle-fill" rounded :severity="recording ? 'danger' : 'secondary'" :aria-label="t(recording ? 'stopRecord' : 'record')" :title="t(recording ? 'stopRecord' : 'record')" :disabled="!manualAllowed" @click="emit('record', device.id)" />
                    </div>
                    <div class="nav-actions">
                        <Button icon="pi pi-arrow-left" rounded :aria-label="t('back')" :title="t('back')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'BACK' })" />
                        <Button icon="pi pi-clone" rounded :aria-label="t('recent')" :title="t('recent')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'RECENT' })" />
                        <Button icon="pi pi-home" rounded :aria-label="t('home')" :title="t('home')" :disabled="!manualAllowed" @click="localAction({ type: 'key', key: 'HOME' })" />
                    </div>
                </div>
                <Button class="main-orb" :icon="menuOpen ? 'pi pi-times' : 'pi pi-sliders-h'" rounded :aria-expanded="menuOpen" :aria-label="t('localControl')" :title="syncRole ? t('controlsLocked') : t('localControl')" @pointerdown.stop="dragStart" @pointermove.stop="dragMove" @pointerup.stop="dragEnd" @pointercancel.stop="dragEnd" @click.stop="toggleOrb" />
            </div>
        </div>
        <footer><span class="phase" :class="{ live: session.state.phase === 'live' }">{{ t(session.state.phase) }}</span><span class="mono-value">{{ device.overlay_ip }}</span></footer>
        <small v-if="session.state.unlockState === 'wake_sent'" class="unlock-notice">手机已发送亮屏指令；该 ADB 服务没有保存开屏密码，请在设备管理中配置后重试。</small>
        <small v-else-if="session.state.unlockState === 'already_unlocked'" class="unlock-notice">已检测到手机处于解锁状态，因此没有输入密码。</small>
        <small v-else-if="session.state.unlockState === 'lock_state_unknown'" class="unlock-notice">手机已唤醒，但 Android 未返回可靠的锁屏状态；为避免把密码输入其他应用，已跳过自动解锁。</small>
        <small v-else-if="session.state.unlockState === 'lock_state_changed'" class="unlock-notice">等待后锁屏状态不明确，已停止自动输入密码；请检查手机锁屏界面。</small>
        <small v-else-if="session.state.unlockState === 'unlock_size_unknown'" class="unlock-notice">读取手机屏幕尺寸失败，无法执行解锁上滑；请检查 ADB 权限。</small>
        <small v-else-if="session.state.unlockState === 'unlock_swipe_failed'" class="unlock-notice">手机拒绝或未完成解锁上滑，已跳过密码输入；请检查 ADB 权限。</small>
        <small v-else-if="session.state.unlockState === 'unlock_skipped'" class="unlock-notice">本次解锁中途停止，未确认最终锁屏状态。</small>
        <small v-else-if="session.state.unlockState === 'unlock_sent'" class="unlock-notice">已发送开屏密码输入和确认指令，但手机尚未确认解锁成功。</small>
        <small v-else-if="session.state.unlockState === 'unlock_confirmed'" class="unlock-notice">已确认手机解锁。</small>
        <div v-if="device.endpoint_event" class="endpoint-status"><small>目标切换：{{ device.endpoint_event.cleanup.state }} / {{ device.endpoint_event.connection.state }}</small><Button v-if="device.endpoint_event.cleanup.state !== 'confirmed'" label="重试清理" text size="small" :loading="cleaningSaved" @click="retryEndpoint('cleanup')" /><Button v-else-if="device.endpoint_event.connection.state === 'pending'" label="重试连接" text size="small" :loading="cleaningSaved" @click="retryEndpoint('connection')" /><Button v-if="device.endpoint_event.publication && !['applied','not_applicable'].includes(device.endpoint_event.publication.state)" label="重试公开应用" text size="small" :loading="cleaningSaved" @click="retryEndpoint('publication')" /></div>
        <p v-if="session.state.error || device.availability_reason" class="phone-error" role="status">{{ session.state.error || device.availability_reason }}</p>
    </article>
</template>

<style scoped>
.endpoint-status{padding:8px 14px;display:flex;gap:6px;flex-wrap:wrap;color:var(--console-muted)}
.phone-card{min-width:0;max-width:100%;flex:0 1 auto;border:1px solid var(--console-border);border-radius:14px;background:var(--console-panel);overflow:hidden;box-shadow:var(--console-shadow)}
.phone-card.selected{border-color:var(--p-primary-color);box-shadow:0 0 0 1px color-mix(in srgb,var(--p-primary-color) 20%,transparent)}
.phone-card.master{box-shadow:0 0 0 2px var(--p-primary-color)}
header{display:flex;align-items:center;justify-content:space-between;padding:10px 10px 0 14px;gap:6px}
.device-heading{display:flex;align-items:center;gap:4px;min-width:0;flex:1 1 auto}
.device-selection{display:flex;align-items:center;gap:8px;min-width:0;flex:0 1 auto;font-weight:600}
.device-selection .device-name{display:block;min-width:0;overflow:hidden;white-space:nowrap;text-overflow:ellipsis}
.device-selection input{accent-color:var(--p-primary-color);width:16px;height:16px;flex-shrink:0}
.card-header-actions{display:flex;align-items:center;justify-content:flex-end;flex:0 0 auto;gap:3px}
.card-action-tip{display:inline-flex;align-items:center;justify-content:center;flex:0 0 28px}
.card-icon{display:inline-flex;align-items:center;justify-content:center;width:28px;height:28px;padding:0;border:0;border-radius:8px;background:transparent;color:var(--console-muted);font-size:15px;cursor:pointer}
.card-icon:hover:not(:disabled){background:var(--console-accent-soft)}
.card-icon:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:2px}
.card-icon:disabled{cursor:default;opacity:.7}
.inspector-icon.active,.adb-port.live{color:var(--p-green-500)}
.inspector-icon.pending,.adb-port.opening,.adb-port.connecting{color:var(--p-primary-color)}
.inspector-icon.blocked,.adb-port{color:var(--p-orange-500)}
.inspector-icon.blocked{color:var(--p-orange-500)}
.adb-port.failed,.adb-port.cleanup_pending{color:var(--p-red-500)}
.adb-port.unavailable{color:var(--console-muted)}
.card-header-actions :deep(.card-settings){width:28px;height:28px;min-width:28px;padding:0;flex-shrink:0}
.device-meta{display:flex;align-items:center;font-size:11px;color:var(--console-muted);padding:1px 14px 10px}
.device-meta .node-name{min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.adb-status{border:0;background:transparent;display:inline-flex;align-items:center;justify-content:center;width:23px;height:23px;padding:0;cursor:pointer;flex-shrink:0;border-radius:50%}
.adb-status:hover:not(:disabled){background:var(--console-accent-soft)}.adb-status:disabled{cursor:default;opacity:.6}
.adb-status-dot{width:9px;height:9px;border-radius:50%;background:var(--console-muted);box-shadow:0 0 0 2px color-mix(in srgb,var(--console-muted) 20%,transparent)}
.adb-status.connected .adb-status-dot{background:var(--p-green-500);box-shadow:0 0 0 2px color-mix(in srgb,var(--p-green-500) 22%,transparent)}
.adb-status.failed .adb-status-dot,.adb-status.query_failed .adb-status-dot,.adb-status.disconnected .adb-status-dot,.adb-status.offline .adb-status-dot{background:var(--p-red-500)}.adb-status.unauthorized .adb-status-dot,.adb-status.unavailable .adb-status-dot{background:var(--p-orange-500)}.adb-status.connecting .adb-status-dot,.adb-status.checking .adb-status-dot{background:var(--p-blue-500);animation:adb-pulse 1s ease-in-out infinite}
@keyframes adb-pulse{50%{opacity:.35}}
.phone-screen{width:100%;height:auto;aspect-ratio:9/16;position:relative;background:#07111d;overflow:hidden}.phone-screen video{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;display:block;touch-action:none;user-select:none}.phone-screen video.interactive{cursor:crosshair}
.screen-placeholder{position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:14px;color:#91a6bd;background:radial-gradient(ellipse at center,#11273a,#07111d 75%);padding:18px;text-align:center}.screen-placeholder>.pi{font-size:34px;color:#54738f}.screen-placeholder small{font-size:11px}
.role-badge{position:absolute;top:10px;left:10px;pointer-events:none}.record-badge{position:absolute;top:12px;right:12px;color:#fda4af;font-size:11px;display:flex;align-items:center;gap:5px}.record-badge>span{width:7px;height:7px;border-radius:50%;background:#fb7185}
.phone-controls{position:absolute;z-index:6;transform:translateY(-50%);display:flex;align-items:center;gap:6px;transition:left .16s ease;touch-action:none}
.phone-controls :deep(.p-button){width:42px;height:42px;min-width:42px;box-shadow:0 3px 14px #0006;border:1px solid #ffffff38;backdrop-filter:blur(10px)}
.phone-controls :deep(.p-button-secondary){background:#142c42e8;color:#e1edf7}
.main-orb{background:var(--p-primary-color);cursor:grab;touch-action:none;position:relative;z-index:2}.main-orb:active{cursor:grabbing}
.floating-actions{position:absolute;inset:0;pointer-events:none;overflow:visible;z-index:1}
.floating-actions>div{position:absolute;pointer-events:auto;display:flex;gap:4px;align-items:center;border-radius:12px;padding:3px;background:color-mix(in srgb,#081b30 82%,transparent);box-shadow:0 4px 16px #0006;backdrop-filter:blur(9px)}
.input-actions{bottom:calc(100% + 10px);left:0;flex-direction:column}
.nav-actions{top:calc(100% + 10px);left:0;flex-direction:column}
.side-actions{top:0;flex-direction:row}.side-actions :deep(.p-button){width:37px;height:37px;min-width:37px}
.dock-right .side-actions{right:calc(100% + 7px)}
.dock-left .side-actions{left:calc(100% + 7px)}
.control-dismiss{position:absolute;inset:0;z-index:5;background:transparent;border:0;cursor:default}
footer{display:flex;align-items:center;justify-content:space-between;padding:10px 12px;gap:8px;font-size:11px}.phase{color:var(--console-muted)}.phase.live{color:var(--p-green-500)}.phase.live:before{content:'';display:inline-block;width:6px;height:6px;background:currentColor;border-radius:50%;margin-right:6px}.unlock-notice{display:block;padding:0 12px 10px;color:var(--console-muted);font-size:11px}.phone-error{font-size:11px;padding:0 12px 10px;color:var(--p-red-500);overflow-wrap:anywhere;max-height:80px;overflow:auto}.fullscreen-stop{display:none}
/* Match EasyADB: only the phone SCREEN enters the browser top layer.
   The decoded image is never stretched or cropped, on desktop or mobile. */
.phone-screen:fullscreen{width:100vw;max-width:none;height:100dvh;max-height:none;aspect-ratio:auto!important;border-radius:0;background:#142033}
.phone-screen:fullscreen .primary-video{object-fit:contain;z-index:1}
.phone-screen:fullscreen .ambient-video{object-fit:cover;z-index:0;pointer-events:none;filter:blur(28px) brightness(.4);transform:scale(1.04);touch-action:none}
.phone-screen:fullscreen .phone-controls.dock-right{left:auto!important;right:14px;top:50%}
.phone-screen:fullscreen .phone-controls.dock-left{left:14px!important;right:auto;top:50%}
.phone-screen:fullscreen .fullscreen-stop{display:inline-flex;position:absolute;top:12px;left:50%;transform:translateX(-50%);z-index:7}
@media(max-width:600px){.floating-actions :deep(.p-button){width:36px;height:36px;min-width:36px}.side-actions{gap:2px!important}}
</style>
