<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { Button, Checkbox, InputText, Message, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AgentAddressChoice, AssetAgentProbe, AssetAgentStatus, AssetScope, ServerAgentEntry } from '../../types/node-assets';
import { serverAgentPairingMessages } from '../../modules/server-agent-pairing-messages';
import ServerAgentHostPanel from './ServerAgentHostPanel.vue';

const props = defineProps<{ api: ApiClient; scope: AssetScope; entry: ServerAgentEntry }>();
const { t } = useI18n({ useScope: 'local', messages: serverAgentPairingMessages });
const status = ref<AssetAgentStatus | null>(null);
const address = ref<AgentAddressChoice>('overlay');
const pin = ref('');
const pairingCode = ref('');
const trustedLocally = ref(false);
const verified = ref<AssetAgentProbe | null>(null);
const busy = ref(false);
const error = ref('');
const message = ref('');
let alive = true;
let requestNumber = 0;
const normalized = computed(() => pin.value.trim().toLowerCase());
const selectedIp = computed(() => address.value === 'overlay' ? props.entry.overlay_ip : props.entry.lan_ip);
const canProbe = computed(() => !busy.value && !!selectedIp.value && /^[a-f0-9]{64}$/.test(normalized.value)
    && (status.value?.bound === true || trustedLocally.value));
const canPair = computed(() => canProbe.value && verified.value?.tls_verified === true
    && /^[a-fA-F0-9]{32}$/.test(pairingCode.value.trim()));

function describe(cause: unknown): string {
    const e = cause as { response?: { data?: { message?: string } }; message?: string };
    return e.response?.data?.message || e.message || t('failed');
}
async function localStatus() {
    const sequence = ++requestNumber;
    busy.value = true;
    try {
        const saved = await props.api.assetsAgentStatus(props.scope, props.entry.asset_device_id);
        if (!alive || sequence !== requestNumber) return;
        status.value = saved;
        if (saved.bound && saved.tls_spki_sha256) pin.value = saved.tls_spki_sha256;
    } catch (cause) {
        if (alive && sequence === requestNumber) error.value = describe(cause);
    } finally {
        if (alive && sequence === requestNumber) busy.value = false;
    }
}
watch([address, pin], () => { verified.value = null; message.value = ''; trustedLocally.value = false; });
async function detect() {
    if (!canProbe.value) return;
    const sequence = ++requestNumber;
    const fingerprint = normalized.value;
    busy.value = true; error.value = ''; message.value = ''; verified.value = null;
    try {
        const checked = await props.api.assetsAgentProbe(props.scope, props.entry.asset_device_id, {
            address: address.value, trusted_spki_sha256: fingerprint,
        });
        if (!alive || sequence !== requestNumber) return;
        if (checked.identity.tls_spki_sha256 !== fingerprint || !checked.tls_verified) {
            throw new Error(t('mismatch'));
        }
        verified.value = checked;
        message.value = t('verified');
        status.value = await props.api.assetsAgentStatus(props.scope, props.entry.asset_device_id);
    } catch (cause) {
        if (alive && sequence === requestNumber) error.value = describe(cause);
    } finally {
        if (alive && sequence === requestNumber) busy.value = false;
    }
}
async function pair() {
    if (!canPair.value || !verified.value) return;
    const expectedAgent = verified.value.identity.agent_id;
    const sequence = ++requestNumber;
    const oneTimeCode = pairingCode.value.trim().toLowerCase();
    pairingCode.value = ''; // Never retain a used/rejected code in the page.
    busy.value = true; error.value = ''; message.value = '';
    try {
        const result = await props.api.assetsAgentPair(props.scope, props.entry.asset_device_id, {
            address: address.value, trusted_spki_sha256: normalized.value,
            pairing_code: oneTimeCode,
        });
        if (!alive || sequence !== requestNumber) return;
        if (!result.bound || result.identity.agent_id !== expectedAgent) {
            throw new Error(t('mismatch'));
        }
        pairingCode.value = '';
        verified.value = null;
        message.value = t('paired');
        status.value = await props.api.assetsAgentStatus(props.scope, props.entry.asset_device_id);
    } catch (cause) {
        if (alive && sequence === requestNumber) error.value = describe(cause);
    } finally {
        if (alive && sequence === requestNumber) busy.value = false;
    }
}
onMounted(localStatus);
onBeforeUnmount(() => { alive = false; ++requestNumber; pairingCode.value = ''; });
</script>

<template>
    <section class="server-agent-pairing">
        <h3>{{ t('title') }}</h3>
        <p v-if="status?.bound"><Tag severity="success" :value="t('bound')" />
            <span class="muted">{{ t('cachedOnly') }}</span></p>
        <p v-if="status?.agent_id">{{ t('agentId') }}：<code>{{ status.agent_id }}</code></p>
        <p v-if="status?.verified_at" class="muted">{{ t('verifiedAt') }}：{{ status.verified_at }}</p>
        <label for="agent-ip-choice">{{ t('address') }}</label>
        <select id="agent-ip-choice" v-model="address" :disabled="busy">
            <option value="overlay">{{ t('overlay') }} · {{ entry.overlay_ip || '—' }}</option>
            <option value="lan" :disabled="!entry.lan_ip">{{ t('lan') }} · {{ entry.lan_ip || '—' }}</option>
        </select>
        <label for="agent-pin">{{ t('fingerprint') }}</label>
        <InputText id="agent-pin" v-model="pin" autocomplete="off" spellcheck="false"
            :placeholder="t('pinPlaceholder')" :disabled="busy" />
        <p class="muted">{{ t('pinHint') }}</p>
        <div v-if="!status?.bound" class="trusted-checkbox">
            <Checkbox inputId="agent-trusted-fingerprint" v-model="trustedLocally" binary :disabled="busy" />
            <label for="agent-trusted-fingerprint">{{ t('trustedConfirm') }}</label>
        </div>
        <Button :label="t('detect')" icon="pi pi-link" :loading="busy" :disabled="!canProbe" @click="detect" />
        <Message v-if="verified" severity="success" :closable="false">{{ t('verified') }}
            · {{ verified.identity.hostname }} · {{ verified.identity.agent_id }} · {{ verified.endpoint }}
        </Message>
        <label for="agent-code">{{ t('pairingCode') }}</label>
        <InputText id="agent-code" v-model="pairingCode" type="password" autocomplete="off" spellcheck="false"
            :placeholder="t('codePlaceholder')" :disabled="busy" />
        <Button :label="t('pair')" icon="pi pi-lock" :loading="busy" :disabled="!canPair" @click="pair" />
        <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
        <Message v-if="message" severity="info" :closable="false">{{ message }}</Message>
        <p class="muted">{{ t('manual') }}</p>
        <ServerAgentHostPanel :api="api" :scope="scope" :entry="entry" :bound="status?.bound === true" />
    </section>
</template>

<style scoped>
.server-agent-pairing { display: grid; gap: .55rem; margin-top: .7rem; padding-top: .8rem; border-top: 1px solid var(--console-border); }
.server-agent-pairing h3 { font-weight: 600; }
.server-agent-pairing label { font-weight: 600; font-size: .85rem; }
.server-agent-pairing p { margin: .15rem 0; line-height: 1.5; }
.server-agent-pairing select { background: var(--console-panel-raised); color: inherit; border: 1px solid var(--console-border); border-radius: .5rem; padding: .55rem; }
.server-agent-pairing :deep(.p-inputtext) { width: 100%; font-family: ui-monospace, monospace; }
.server-agent-pairing :deep(.p-button) { justify-self: start; }
.server-agent-pairing code { overflow-wrap: anywhere; }
.trusted-checkbox { display: flex; gap: .5rem; align-items: center; }
.trusted-checkbox label { font-weight: 400; }
</style>
