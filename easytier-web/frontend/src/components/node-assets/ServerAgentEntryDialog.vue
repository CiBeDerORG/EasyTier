<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { Button, Dialog, Message, Skeleton, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import { serverAgentMessages } from '../../modules/server-agent-messages';
import { matchesServerAgentEntry } from '../../modules/server-agent-entry';
import ServerAgentPairingPanel from './ServerAgentPairingPanel.vue';
import type { AssetDevice, AssetScope, ServerAgentEntry } from '../../types/node-assets';

const props = defineProps<{ api: ApiClient; scope: AssetScope; device: AssetDevice }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n({ useScope: 'local', messages: serverAgentMessages });
const entry = ref<ServerAgentEntry | null>(null);
const loading = ref(true);
const error = ref('');
let requestVersion = 0;
let alive = true;

const sourceCommands = 'git clone --depth 1 https://github.com/CiBeDerORG/EasyHostAgent.git\ncd EasyHostAgent';
const installCommand = 'sudo bash scripts/install.sh --binary /absolute/path/easyhostagent-linux-ARCH';
const identityCommands = 'sudo /usr/local/bin/easyhostagent -identity\nsudo /usr/local/bin/easyhostagent -pairing-code';

async function load() {
    const current = ++requestVersion;
    loading.value = true; entry.value = null; error.value = '';
    try {
        const result = await props.api.assetsServerAgentEntry(props.scope, props.device.id);
        if (!alive || current !== requestVersion) return;
        if (!matchesServerAgentEntry(props.scope, props.device.id, result)) {
            error.value = t('changed');
            return;
        }
        entry.value = result;
    } catch (cause) {
        if (!alive || current !== requestVersion) return;
        const detail = cause as { response?: { data?: { message?: string } }; message?: string };
        error.value = detail.response?.data?.message || detail.message || t('loadError');
    } finally {
        if (alive && current === requestVersion) loading.value = false;
    }
}
onMounted(load);
onBeforeUnmount(() => { alive = false; ++requestVersion; });
</script>

<template>
    <Dialog :visible="true" modal :header="t('title')" :style="{ width: '40rem', maxWidth: 'calc(100vw - 2rem)' }"
        @update:visible="value => { if (!value) emit('close'); }">
        <div class="agent-onboarding">
            <p class="muted">{{ t('intro') }}</p>
            <div v-if="loading" aria-live="polite"><Skeleton height="4.5rem" /><p class="muted">{{ t('loading') }}</p></div>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <section v-if="entry" class="agent-entry-device" aria-label="Agent device context">
                <div class="agent-entry-heading"><strong>{{ t('currentDevice') }}：{{ entry.device_name }}</strong>
                    <Tag severity="secondary" :value="t('unchecked')" /></div>
                <dl>
                    <div><dt>{{ t('node') }}</dt><dd><code>{{ entry.machine_id }}</code></dd></div>
                    <div><dt>{{ t('deviceId') }}</dt><dd><code>{{ entry.asset_device_id }}</code></dd></div>
                    <div><dt>{{ t('overlay') }}</dt><dd><code>{{ entry.overlay_ip || '—' }}</code></dd></div>
                    <div><dt>{{ t('lan') }}</dt><dd><code>{{ entry.lan_ip || '—' }}</code></dd></div>
                </dl>
                <Message severity="info" :closable="false">{{ t('uncheckedHint') }}</Message>
                <ServerAgentPairingPanel :api="api" :scope="scope" :entry="entry" />
                <h3>{{ t('prepare') }}</h3>
                <p>{{ t('stepOne') }}</p>
                <p><a href="https://github.com/CiBeDerORG/EasyHostAgent/actions/workflows/go-build.yml" target="_blank" rel="noopener noreferrer">{{ t('download') }}</a></p>
                <p>{{ t('stepTwo') }}</p>
                <label>{{ t('repoCommand') }}</label>
                <pre><code>{{ sourceCommands }}</code></pre>
                <label>{{ t('installCommand') }}</label>
                <pre><code>{{ installCommand }}</code></pre>
                <p>{{ t('stepThree') }}</p>
                <label>{{ t('identityCommand') }}</label>
                <pre><code>{{ identityCommands }}</code></pre>
                <p><a href="https://github.com/CiBeDerORG/EasyHostAgent#fnos--linux-systemd-%E5%8E%9F%E7%94%9F%E5%AE%89%E8%A3%85%E5%99%A8t01-02"
                    target="_blank" rel="noopener noreferrer">{{ t('source') }}</a></p>
                <Message severity="warn" :closable="false">{{ t('trustHint') }}</Message>
                <p class="muted">{{ t('upcoming') }}</p>
            </section>
        </div>
        <template #footer>
            <Button :label="t('retry')" icon="pi pi-refresh" text severity="secondary" :loading="loading" @click="load" />
            <Button :label="t('close')" @click="emit('close')" />
        </template>
    </Dialog>
</template>

<style scoped>
.agent-onboarding { display: grid; gap: .65rem; font-size: .875rem; }
.agent-onboarding p { margin: .25rem 0; line-height: 1.55; }
.agent-entry-device { display: grid; gap: .55rem; min-width: 0; }
.agent-entry-heading { display: flex; gap: .75rem; align-items: center; flex-wrap: wrap; }
.agent-entry-device h3 { font-weight: 600; font-size: .95rem; margin: .35rem 0 0; }
.agent-entry-device dl { display: grid; gap: .45rem; margin: .2rem 0; }
.agent-entry-device dl > div { display: grid; grid-template-columns: 7.5rem minmax(0, 1fr); gap: .6rem; }
.agent-entry-device dt { color: var(--console-muted); }
.agent-entry-device dd { margin: 0; overflow-wrap: anywhere; }
.agent-entry-device code { font-size: .8rem; overflow-wrap: anywhere; }
.agent-entry-device pre { white-space: pre-wrap; overflow-wrap: anywhere; padding: .65rem; background: var(--console-panel-raised, var(--p-content-hover-background)); border-radius: .5rem; margin: 0; }
.agent-entry-device label { font-weight: 600; font-size: .8rem; }
@media (max-width: 550px) { .agent-entry-device dl > div { grid-template-columns: 1fr; gap: .1rem; } }
</style>
