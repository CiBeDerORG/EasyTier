<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { Button, InputText, Message } from 'primevue';
import { useI18n } from 'vue-i18n';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import ExecutorCapacitySettings from './ExecutorCapacitySettings.vue';
import type { AutomationRunnerCheck, AutomationRunnerSettings } from '../../types/automation';

const props = defineProps<{ liveCount: number }>();
const emit = defineEmits<{ saved: []; closeScreens: [] }>();
const { t } = useI18n({ useScope: 'local', messages: {
    cn: {
        title: '执行器服务器', description: '配置供所有账号使用的手机执行器，保存后立即生效。',
        address: '服务器地址', addressHint: '填写 EasyTier 服务端可访问的 HTTP / HTTPS 基础地址，例如 http://192.168.1.20:8766。',
        credential: '访问凭证', preserve: '保留已保存的 Token', replace: '填写新的 Token', token: '执行器 Token',
        tokenHint: '与执行器的 AUTOMATION_TOKEN 保持一致。按执行器要求填写 32～4096 字节、无控制字符的值；字符和空格按原值保存。',
        save: '保存设置', check: '检查已保存连接', reload: '重新读取设置', discard: '放弃修改并重新读取',
        saved: '执行器设置已保存并生效。', loadFailed: '无法读取执行器设置。',
        changed: '有未保存的修改，请先保存后检查连接。', configured: '已配置', unconfigured: '未配置',
        source: '配置来源', database: '自动化中心设置', environment: '服务端环境变量', none: '尚未配置',
        envHint: '当前使用环境变量。保存后使用此页面的配置，服务重启后仍保留。',
        encrypted: 'Token 加密保存，读取设置不会返回原值。', updated: '最近保存',
        connection: '连接检查', unchecked: '尚未检查', checking: '正在检查', passed: '接口连接成功',
        checkHint: '仅点击时请求执行器。结果只确认认证和协议接口，手机连接与视频画面需另行验证。',
        executor: '执行器 ID', checkedAt: '检查时间', setup: '执行器侧配置',
        setupHint: '执行器需要独立部署，并配置回调地址为它能够访问的 EasyTier 后端地址。',
        bindHint: '执行器默认只监听本机；跨主机接入时，请配置可达的私网监听地址。',
        busyHint: '修改前请先关闭活动画面，完成或取消任务并确认清理。切换服务器地址时会先释放旧执行器的空闲连接，手机需要重新连接。',
        closeScreens: '关闭当前页面画面', checkFailed: '连接检查失败',
    },
    en: {
        title: 'Executor server', description: 'Configure the shared phone executor. Saved settings take effect immediately.',
        address: 'Server URL', addressHint: 'Use an HTTP / HTTPS base URL reachable from EasyTier, such as http://192.168.1.20:8766.',
        credential: 'Credential', preserve: 'Keep the saved token', replace: 'Enter a new token', token: 'Executor token',
        tokenHint: 'Match AUTOMATION_TOKEN on the executor: 32–4096 bytes with no control characters. Characters and spaces are preserved.',
        save: 'Save settings', check: 'Check saved connection', reload: 'Reload settings', discard: 'Discard changes and reload',
        saved: 'Executor settings saved and applied.', loadFailed: 'Could not load executor settings.',
        changed: 'Save your changes before checking the connection.', configured: 'Configured', unconfigured: 'Not configured',
        source: 'Configuration source', database: 'Automation settings', environment: 'Server environment', none: 'Not configured',
        envHint: 'Environment settings are currently in use. Saving here overrides them and survives server restarts.',
        encrypted: 'The token is encrypted at rest and is never returned when reading settings.', updated: 'Last saved',
        connection: 'Connection check', unchecked: 'Not checked', checking: 'Checking', passed: 'API connection successful',
        checkHint: 'Only checked on click. This verifies authentication and the protocol; phone access and video require separate validation.',
        executor: 'Executor ID', checkedAt: 'Checked at', setup: 'Executor configuration',
        setupHint: 'Deploy the executor separately and set its callback URL to an EasyTier backend address it can reach.',
        bindHint: 'The executor listens on localhost by default. Use a reachable private interface for remote access.',
        busyHint: 'Close active screens and finish or cancel tasks with confirmed cleanup before editing. Changing the server URL releases old idle connections; reconnect phones afterwards.',
        closeScreens: 'Close screens on this page', checkFailed: 'Connection check failed',
    },
} });
const api = new AutomationApi();
const saved = ref<AutomationRunnerSettings | null>(null);
const endpoint = ref('');
const tokenAction = ref<'preserve' | 'replace'>('replace');
const token = ref('');
const busy = ref<'load' | 'save' | 'check' | null>(null);
const error = ref('');
const notice = ref('');
const checked = ref<AutomationRunnerCheck | null>(null);
const checkError = ref('');
const dirty = computed(() => !!saved.value && (endpoint.value !== saved.value.endpoint || tokenAction.value === 'replace' && !!token.value));
const canSave = computed(() => !!saved.value && !!endpoint.value.trim() && (tokenAction.value === 'preserve' ? saved.value.token_configured : !!token.value));
function accept(value: AutomationRunnerSettings) {
    saved.value = value; endpoint.value = value.endpoint;
    token.value = ''; tokenAction.value = value.token_configured ? 'preserve' : 'replace';
    checked.value = null; checkError.value = '';
}
async function load() {
    busy.value = 'load'; error.value = ''; notice.value = '';
    try { accept(await api.runnerSettings()); }
    catch (cause) { error.value = automationErrorMessage(cause); }
    finally { busy.value = null; }
}
async function save() {
    if (!saved.value || !canSave.value || busy.value) return;
    busy.value = 'save'; error.value = ''; notice.value = ''; checked.value = null; checkError.value = '';
    try {
        const value = await api.saveRunnerSettings({
            endpoint: endpoint.value, token_action: tokenAction.value, revision: saved.value.revision,
            ...(tokenAction.value === 'replace' ? { token: token.value } : {}),
        });
        accept(value); notice.value = t('saved'); emit('saved');
    } catch (cause) { error.value = automationErrorMessage(cause); }
    finally { busy.value = null; }
}
async function check() {
    if (!saved.value?.configured || dirty.value || busy.value) return;
    busy.value = 'check'; checked.value = null; checkError.value = ''; error.value = ''; notice.value = '';
    try { checked.value = await api.checkRunner(saved.value.revision); }
    catch (cause) { checkError.value = automationErrorMessage(cause); }
    finally { busy.value = null; }
}
function date(value: string) { return new Date(value).toLocaleString(); }
onMounted(load);
</script>

<template>
    <div class="executor-settings">
        <form class="console-panel settings-card" @submit.prevent="save">
            <div class="settings-heading"><div><h2>{{ t('title') }}</h2><p>{{ t('description') }}</p></div><span class="config-badge" :class="{ configured: saved?.configured }">{{ t(saved?.configured ? 'configured' : 'unconfigured') }}</span></div>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <Message v-if="notice" severity="success" :closable="false">{{ notice }}</Message>
            <Message v-if="saved?.source === 'environment'" severity="info" :closable="false">{{ t('envHint') }}</Message>
            <fieldset :disabled="!!busy || !saved">
                <label for="executor-url">{{ t('address') }}</label>
                <InputText id="executor-url" v-model="endpoint" type="url" autocomplete="off" placeholder="http://192.168.1.20:8766" aria-describedby="executor-url-hint" required />
                <p id="executor-url-hint" class="hint">{{ t('addressHint') }}</p>
                <label for="executor-token-mode">{{ t('credential') }}</label>
                <select id="executor-token-mode" v-model="tokenAction" @change="token = ''">
                    <option v-if="saved?.token_configured" value="preserve">{{ t('preserve') }}</option>
                    <option value="replace">{{ t('replace') }}</option>
                </select>
                <template v-if="tokenAction === 'replace'">
                    <label for="executor-token">{{ t('token') }}</label>
                    <InputText id="executor-token" v-model="token" type="password" autocomplete="new-password" aria-describedby="executor-token-hint" required />
                    <p id="executor-token-hint" class="hint">{{ t('tokenHint') }}</p>
                </template>
                <p class="hint"><i class="pi pi-lock" aria-hidden="true" /> {{ t('encrypted') }}</p>
            </fieldset>
            <p class="hint">{{ t('busyHint') }}</p>
            <Button v-if="props.liveCount" :label="t('closeScreens')" icon="pi pi-stop" outlined :disabled="!!busy" @click="emit('closeScreens')" />
            <div class="settings-actions"><Button type="submit" :label="t('save')" icon="pi pi-save" :loading="busy === 'save'" :disabled="!!busy || !canSave" /><Button :label="t(dirty ? 'discard' : 'reload')" icon="pi pi-refresh" text :loading="busy === 'load'" :disabled="!!busy" @click="load" /></div>
            <dl v-if="saved" class="settings-meta"><div><dt>{{ t('source') }}</dt><dd>{{ t(saved.source) }}</dd></div><div v-if="saved.updated_at"><dt>{{ t('updated') }}</dt><dd>{{ date(saved.updated_at) }}</dd></div></dl>
        </form>
        <div class="settings-side">
            <ExecutorCapacitySettings :enabled="!!saved?.configured" :connection-revision="saved?.revision ?? 0" />
            <section class="console-panel settings-card" aria-live="polite">
                <h2>{{ t('connection') }}</h2><p class="hint">{{ t('checkHint') }}</p>
                <Message v-if="checkError" severity="error" :closable="false">{{ t('checkFailed') }}：{{ checkError }}</Message>
                <Message v-else-if="checked && !dirty" severity="success" :closable="false">{{ t('passed') }}</Message>
                <p v-else class="hint">{{ t(dirty ? 'changed' : busy === 'check' ? 'checking' : 'unchecked') }}</p>
                <dl v-if="checked && !dirty" class="settings-meta"><div><dt>{{ t('executor') }}</dt><dd>{{ checked.executor_id }}</dd></div><div><dt>{{ t('checkedAt') }}</dt><dd>{{ date(checked.checked_at) }}</dd></div></dl>
                <Button :label="t('check')" icon="pi pi-link" outlined :loading="busy === 'check'" :disabled="!!busy || !saved?.configured || dirty" @click="check" />
            </section>
            <section class="console-panel settings-card"><h2>{{ t('setup') }}</h2><p class="hint">{{ t('setupHint') }}</p><code>AUTOMATION_TOKEN<br>AUTOMATION_CALLBACK_URL<br>AUTOMATION_BIND_IP</code><p class="hint">{{ t('bindHint') }}</p></section>
        </div>
    </div>
</template>

<style scoped>
.executor-settings{display:grid;grid-template-columns:minmax(0,1.4fr) minmax(0,1fr);gap:20px;align-items:start}.settings-card{padding:22px;display:flex;flex-direction:column;gap:16px;min-width:0}.settings-heading{display:flex;justify-content:space-between;align-items:flex-start;gap:16px}.settings-heading p,.hint{font-size:12px;line-height:1.7;color:var(--console-muted);overflow-wrap:anywhere}.settings-heading p{margin-top:7px}h2{font-size:16px;font-weight:600}fieldset{border:0;padding:0;min-width:0;display:flex;flex-direction:column;gap:10px}label{font-size:13px;font-weight:550}label:not(:first-child){margin-top:10px}select{padding:10px;border:1px solid var(--console-border-strong);border-radius:7px;background:var(--console-panel);color:var(--p-text-color);max-width:100%;font-size:13px}.config-badge{padding:5px 9px;border-radius:6px;font-size:11px;white-space:nowrap;background:var(--console-panel-raised);color:var(--console-muted)}.config-badge.configured{background:var(--console-accent-soft);color:var(--p-primary-color)}.settings-actions{display:flex;gap:10px;flex-wrap:wrap}.settings-meta{display:flex;flex-direction:column;gap:10px;font-size:12px}.settings-meta div{display:flex;gap:16px;justify-content:space-between}.settings-meta dt{color:var(--console-muted);flex-shrink:0}.settings-meta dd{text-align:right;overflow-wrap:anywhere;min-width:0}.settings-side{display:flex;flex-direction:column;gap:20px}code{display:block;padding:14px;background:var(--console-panel-raised);border:1px solid var(--console-border);border-radius:8px;font-size:12px;line-height:1.9;overflow-wrap:anywhere}
@media(max-width:850px){.executor-settings{grid-template-columns:1fr}.settings-card{padding:17px}.settings-heading{flex-wrap:wrap}}
</style>
