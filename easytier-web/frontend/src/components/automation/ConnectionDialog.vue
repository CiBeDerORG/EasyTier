<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { v4 as uuid } from 'uuid';
import { Button, Dialog, InputText, Message } from 'primevue';
import { useI18n } from 'vue-i18n';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationConnection, AutomationDevice, AutomationPrepareResult } from '../../types/automation';
import { automationMessages } from './messages';

const props = defineProps<{ device: AutomationDevice | null; ownerId: number; runnerAvailable: boolean }>();
const emit = defineEmits<{ close: []; saved: [device: AutomationDevice] }>();
const { t } = useI18n({ useScope: 'local', messages: automationMessages });
const api = new AutomationApi();
const form = ref<AutomationConnection>({ enabled: true, adb_port: 0, connection_mode: 'tcp' });
const pairingPort = ref<number | undefined>();
const pairingCode = ref('');
const busy = ref(false); const error = ref(''); const notice = ref('');
const result = ref<AutomationPrepareResult | null>(null);
let generation = 0;
const visible = computed(() => props.device !== null);
watch(() => props.device?.id, () => {
    ++generation; error.value = ''; notice.value = ''; result.value = null; pairingCode.value = ''; pairingPort.value = undefined;
    if (props.device) form.value = { ...props.device.connection };
});
function dismiss() {
    if (busy.value) return;
    pairingCode.value = ''; result.value = null; ++generation; emit('close');
}
function validPort(port: unknown): port is number { return typeof port === 'number' && Number.isInteger(port) && port > 0 && port < 65536; }
async function perform(action?: 'pair' | 'connect' | 'inspect') {
    if (!props.device || busy.value) return;
    error.value = ''; notice.value = ''; result.value = null;
    if (!validPort(form.value.adb_port)) { error.value = t('portInvalid'); return; }
    if (action && !form.value.enabled) { error.value = t('enabledRequired'); return; }
    if (action === 'pair' && (!validPort(pairingPort.value) || !/^\d{6}$/.test(pairingCode.value))) { error.value = t('pairingInvalid'); return; }
    const target = props.device; const owner = props.ownerId; const mine = generation;
    busy.value = true;
    try {
        if (!action) return;
        const body = action === 'pair' ? { action, pairing_port: pairingPort.value, code: pairingCode.value } : { action };
        pairingCode.value = '';
        // Pairing must not bypass the saved-endpoint retirement fence. Retry its
        // durable cleanup event first and only pair after the server confirms it.
        if (action === 'pair' && target.endpoint_event && target.endpoint_event.cleanup?.state !== 'confirmed') {
            const event = target.endpoint_event;
            if (!event?.event_id) throw new Error(t('endpointCleanupPending'));
            const cleanup = await api.retryEndpoint(owner, target.id, event.event_id, {
                request_id: uuid(), stage: 'cleanup',
            }) as { cleanup?: { state?: string; code?: string } };
            if (cleanup?.cleanup?.state !== 'confirmed') {
                const code = typeof cleanup?.cleanup?.code === 'string' ? cleanup.cleanup.code : 'cleanup_unconfirmed';
                throw new Error(`${t('endpointCleanupPending')} (${code})`);
            }
        }
        const observed = await api.prepare(owner, target.id, body);
        if (mine !== generation) return;
        result.value = observed;
    } catch (cause) { if (mine === generation) error.value = automationErrorMessage(cause); }
    finally { busy.value = false; pairingCode.value = ''; }
}
</script>

<template>
    <Dialog :visible="visible" modal :header="t('connectionTitle')" :closable="!busy" :close-on-escape="!busy" :style="{ width: '580px', maxWidth: 'calc(100vw - 24px)' }" @update:visible="value => { if (!value) dismiss(); }">
        <div v-if="device" class="connection-form">
            <div class="target-context"><strong>{{ device.name }}</strong><span>{{ device.node_name }}<template v-if="device.gateway_name"> / {{ device.gateway_name }}</template></span><span>{{ t('address') }}：<code>{{ device.overlay_ip }}</code></span></div>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <Message v-if="notice" severity="success" :closable="false">{{ notice }}</Message>
            <Message v-if="result" :severity="['failed', 'unauthorized', 'offline'].includes(result.state) ? 'warn' : 'info'" :closable="false">{{ result.message }} <span v-if="result.state">({{ result.state }})</span></Message>
            <p class="muted">连接端口和模式来自该手机的 ADB 服务；请到节点设备的服务编辑器修改。</p>
            <label>{{ t('connectionMode') }}<select v-model="form.connection_mode" disabled><option value="tcp">{{ t('tcp') }}</option><option value="wireless">{{ t('wireless') }}</option></select></label>
            <label>{{ t('adbPort') }}<input v-model.number="form.adb_port" type="number" min="1" max="65535" step="1" disabled></label>
            <p class="muted">{{ t('phoneAuthHint') }}</p>
            <fieldset v-if="form.connection_mode === 'wireless'" :disabled="busy">
                <legend>{{ t('pair') }}</legend><p>{{ t('pairingHint') }}</p>
                <div class="pair-fields"><label>{{ t('pairingPort') }}<input v-model.number="pairingPort" type="number" min="1" max="65535" step="1"></label><label>{{ t('pairingCode') }}<InputText v-model="pairingCode" type="password" inputmode="numeric" maxlength="6" autocomplete="off" /></label></div>
                <Button :label="t('pair')" icon="pi pi-link" :loading="busy" :disabled="!runnerAvailable" @click="perform('pair')" />
            </fieldset>
            <div class="prepare-actions"><Button :label="t('connect')" icon="pi pi-link" :loading="busy" :disabled="!runnerAvailable" @click="perform('connect')" /><Button :label="t('inspect')" icon="pi pi-info-circle" outlined :disabled="busy || !runnerAvailable" @click="perform('inspect')" /></div>
            <dl v-if="result?.metadata" class="metadata"><template v-for="(value, key) in result.metadata" :key="key"><dt>{{ key }}</dt><dd>{{ value ?? '—' }}</dd></template></dl>
        </div>
        <template #footer><Button :label="t('closeDialog')" text :disabled="busy" @click="dismiss" /></template>
    </Dialog>
</template>

<style scoped>
.connection-form{display:flex;flex-direction:column;gap:16px}.target-context{display:flex;flex-direction:column;gap:4px;padding:12px 14px;border:1px solid var(--console-border);border-radius:10px;background:var(--console-panel-raised)}.target-context>span{font-size:12px;color:var(--console-muted)}label{display:flex;flex-direction:column;gap:7px;font-size:13px}input[type=number],select{border:1px solid var(--console-border-strong);background:var(--console-panel);color:var(--p-text-color);border-radius:7px;padding:9px 11px;width:100%;min-width:0}.checkbox-field{flex-direction:row;align-items:center}.checkbox-field input{accent-color:var(--p-primary-color)}p{font-size:12px;line-height:1.6;color:var(--console-muted)}fieldset{border:1px solid var(--console-border);padding:14px;border-radius:10px;display:flex;flex-direction:column;gap:12px}legend{padding:0 6px;font-size:13px}.pair-fields{display:grid;grid-template-columns:1fr 1fr;gap:12px;min-width:0}.pair-fields :deep(input){width:100%;min-width:0}.prepare-actions{display:flex;gap:10px;flex-wrap:wrap}.script-prepare{padding-top:12px;border-top:1px solid var(--console-border);display:flex;flex-direction:column;gap:10px}.script-prepare>.p-button{align-self:flex-start}.metadata{display:grid;grid-template-columns:minmax(100px,1fr) 2fr;gap:7px;font-size:12px;overflow-wrap:anywhere}.metadata dt{color:var(--console-muted)}
</style>
