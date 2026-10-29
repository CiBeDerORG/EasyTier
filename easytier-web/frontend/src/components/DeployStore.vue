<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { Button, Checkbox, InputNumber, InputText, Message, Select, Tag } from 'primevue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import ApiClient from '../modules/api';
import { getApiBase } from '../modules/api-host';
import type { DeployCatalogApp, DeployCatalogConfigField, DeployCatalogIndex, DeployCatalogPackage } from '../types/deploy-catalog';

const router = useRouter();
const { locale } = useI18n();
const cn = computed(() => locale.value === 'cn' || locale.value.startsWith('zh'));
const title = computed(() => cn.value ? 'Deploy 应用商店' : 'Deploy application store');
const api = new ApiClient(getApiBase(), () => router.push({ name: 'login' }));
const catalog = ref<DeployCatalogIndex | null>(null);
const selected = ref<DeployCatalogApp | null>(null);
const query = ref('');
const category = ref('all');
const actionNames = ['install', 'update', 'uninstall'] as const;
const working = ref(false);
const error = ref('');
const notice = ref('');
const formValues = ref<Record<string, any>>({});
const formErrors = ref<Record<string, string>>({});
const packageResult = ref<DeployCatalogPackage | null>(null);
const preparing = ref(false);
let requestId = 0;
let disposed = false;

const categories = computed(() => [
    { label: cn.value ? '全部分类' : 'All categories', value: 'all' },
    { label: cn.value ? '基础设施' : 'Infrastructure', value: 'infrastructure' },
    { label: cn.value ? '网络' : 'Network', value: 'network' },
    { label: cn.value ? '开发工具' : 'Developer', value: 'developer' },
    { label: cn.value ? '媒体' : 'Media', value: 'media' },
    { label: cn.value ? '效率工具' : 'Productivity', value: 'productivity' },
    { label: cn.value ? '其他' : 'Other', value: 'other' },
]);
const categoryLabel = (id: string) => categories.value.find(x => x.value === id)?.label ?? id;
const results = computed(() => (catalog.value?.items ?? []).filter(app =>
    (category.value === 'all' || category.value === app.category)
    && [app.name, app.id, app.summary, app.compose_project].join(' ').toLowerCase().includes(query.value.trim().toLowerCase())
));
const displayTime = (date?: string) => date ? new Date(date).toLocaleString(cn.value ? 'zh-CN' : 'en-US') : '—';
const describe = (cause: unknown) => {
    const e = cause as { response?: { data?: { message?: string } }; message?: string };
    return e.response?.data?.message || e.message || (cn.value ? '商店请求失败' : 'Catalog request failed');
};

function initializeForm(app: DeployCatalogApp | null) {
    const values: Record<string, any> = {};
    for (const field of app?.detail.config ?? []) {
        // Secret defaults are never returned by the API and are never prefilled.
        if (field.type === 'secret') values[field.key] = '';
        else if (field.default !== undefined) values[field.key] = field.default;
        else if (field.type === 'boolean') values[field.key] = false;
        else if (field.type === 'number' || field.type === 'port') values[field.key] = null;
        else values[field.key] = '';
    }
    formValues.value = values;
    formErrors.value = {};
    packageResult.value = null;
}

function isIpv4(value: string): boolean {
    const parts = value.split('.');
    return parts.length === 4 && parts.every(part =>
        /^\d{1,3}$/.test(part) && Number(part) >= 0 && Number(part) <= 255
        && (part === '0' || !part.startsWith('0'))
    );
}

function validateForm(): boolean {
    const errors: Record<string, string> = {};
    for (const field of selected.value?.detail.config ?? []) {
        const value = formValues.value[field.key];
        const blank = value === null || value === undefined
            || (typeof value === 'string' && value.trim() === '');
        const generatedSecret = field.type === 'secret' && field.generated_by_script;
        if (blank) {
            if (field.required && !generatedSecret) {
                errors[field.key] = cn.value ? '此项为必填项' : 'This field is required';
            }
            continue;
        }
        if (field.type === 'port' || field.type === 'number') {
            const number = Number(value);
            const min = field.minimum ?? (field.type === 'port' ? 1 : Number.NEGATIVE_INFINITY);
            const max = field.maximum ?? (field.type === 'port' ? 65535 : Number.POSITIVE_INFINITY);
            if (!Number.isFinite(number) || (field.type === 'port' && !Number.isInteger(number))
                || number < min || number > max) {
                errors[field.key] = cn.value ? `请输入 ${min} 至 ${max} 范围内的有效数值` : `Enter a valid value between ${min} and ${max}`;
            }
        } else if (field.type === 'ipv4' && !isIpv4(String(value))) {
            errors[field.key] = cn.value ? '请输入有效的 IPv4 地址' : 'Enter a valid IPv4 address';
        } else if ((field.type === 'string' || field.type === 'secret')
            && typeof value === 'string'
            && ((field.min_length !== undefined && value.length < field.min_length)
                || (field.max_length !== undefined && value.length > field.max_length))) {
            errors[field.key] = cn.value ? '输入长度不符合要求' : 'Input length is outside the allowed range';
        } else if (field.type === 'boolean' && typeof value !== 'boolean') {
            errors[field.key] = cn.value ? '请选择布尔值' : 'Choose a boolean value';
        }
    }
    formErrors.value = errors;
    return Object.keys(errors).length === 0;
}

async function preparePackage() {
    if (!selected.value || working.value || !validateForm()) return;
    const current = ++requestId;
    working.value = true;
    preparing.value = true;
    error.value = '';
    notice.value = '';
    try {
        // Deliberately send only the application ID. Config and secret values
        // remain in page memory and are not posted, logged, or persisted.
        const result = await api.deployCatalogPreparePackage(selected.value.id);
        if (disposed || current !== requestId) return;
        packageResult.value = result;
        notice.value = cn.value ? '表单格式检查通过，固定版本包已准备完成（未安装）' : 'Form checks passed; the pinned package is prepared (not installed)';
    } catch (cause) {
        if (!disposed && current === requestId) error.value = describe(cause);
    } finally {
        if (!disposed && current === requestId) {
            working.value = false;
            preparing.value = false;
        }
    }
}

async function load() {
    if (working.value) return;
    const current = ++requestId;
    working.value = true; error.value = '';
    try {
        const data = await api.deployCatalogIndex();
        if (disposed || current !== requestId) return;
        catalog.value = data;
        if (selected.value) {
            const previous = selected.value;
            const next = data.items.find(app => app.id === previous.id) ?? null;
            selected.value = next;
            if (!next) initializeForm(null);
            else if (next.git_revision !== previous.git_revision) initializeForm(next);
        }
    } catch (cause) {
        if (!disposed && current === requestId) error.value = describe(cause);
    } finally {
        if (!disposed && current === requestId) working.value = false;
    }
}
async function refresh() {
    if (working.value) return;
    const current = ++requestId;
    working.value = true; error.value = ''; notice.value = '';
    try {
        const result = await api.deployCatalogRefresh();
        if (disposed || current !== requestId) return;
        const data = await api.deployCatalogIndex();
        if (disposed || current !== requestId) return;
        catalog.value = data;
        if (selected.value) {
            const previous = selected.value;
            const next = data.items.find(app => app.id === previous.id) ?? null;
            selected.value = next;
            if (!next || next.git_revision !== previous.git_revision) initializeForm(next);
        }
        notice.value = (cn.value ? '已手动更新商店到提交 ' : 'Store refreshed at commit ') + result.git_revision.slice(0, 12);
    } catch (cause) {
        // Preserve the preceding catalog and selection; no destructive clear.
        if (!disposed && current === requestId) error.value = describe(cause);
    } finally {
        if (!disposed && current === requestId) working.value = false;
    }
}
async function showDetail(id: string) {
    if (working.value) return;
    const current = ++requestId;
    working.value = true; error.value = '';
    try {
        const detail = await api.deployCatalogDetail(id);
        if (!disposed && current === requestId) { selected.value = detail; initializeForm(detail); }
    } catch (cause) {
        if (!disposed && current === requestId) error.value = describe(cause);
    } finally {
        if (!disposed && current === requestId) working.value = false;
    }
}
onMounted(() => void load()); // Reads SQLite only; NEVER triggers git clone/pull.
onBeforeUnmount(() => { disposed = true; ++requestId; });
</script>

<template>
    <section class="console-page store-page" aria-labelledby="deploy-store-title">
        <header class="page-heading">
            <div class="heading-identity">
                <span class="entity-icon page-icon"><i class="pi pi-shopping-bag" aria-hidden="true"></i></span>
                <div>
                    <h1 id="deploy-store-title">{{ title }}</h1>
                    <p class="page-description">{{ cn ? '独立于设备的中央目录 · 仅浏览，安装任务尚未开放' : 'A device-independent catalog · browsing only; installs are not available yet' }}</p>
                </div>
            </div>
            <div class="actions">
                <Button icon="pi pi-refresh" severity="secondary" :disabled="working" :label="cn ? '读取缓存' : 'Read cache'" @click="load" />
                <Button icon="pi pi-cloud-download" :loading="working" :label="cn ? '手动刷新 Git' : 'Refresh from Git'" @click="refresh" />
            </div>
        </header>

        <Message v-if="error" severity="error" :closable="false" role="alert">{{ error }}{{ catalog?.initialized ? (cn ? ' · 已保留上次目录' : ' · Previous catalog retained') : '' }}</Message>
        <Message v-if="notice" severity="success" :closable="false" role="status">{{ notice }}</Message>
        <Message severity="secondary" :closable="false">
            {{ cn ? '只有管理员可以刷新。浏览、搜索及查看详情都读取上次成功的 SQLite 快照，不会自动拉取 Git。商店条目不授予安装或接管权限。' : 'Only administrators can refresh. Browsing, searching and viewing details use the last successful SQLite snapshot without Git polling. Catalog entries grant no installation or adoption privileges.' }}
        </Message>

        <div class="store-version console-panel" aria-live="polite">
            <div><strong>{{ cn ? '本地目录版本' : 'Local catalog revision' }}</strong>
                <p class="mono-value">{{ catalog?.git_revision?.slice(0, 16) || (cn ? '尚未初始化' : 'Not initialized') }}</p>
            </div>
            <div><strong>{{ cn ? '最后手动刷新' : 'Last manual refresh' }}</strong><p>{{ displayTime(catalog?.fetched_at) }}</p></div>
            <div><strong>{{ cn ? '已收录应用' : 'Indexed apps' }}</strong><p>{{ catalog?.count ?? 0 }}</p></div>
            <Tag :severity="catalog?.initialized ? 'info' : 'warn'" :value="catalog?.initialized ? (cn ? '只读快照' : 'Read-only snapshot') : (cn ? '待刷新' : 'Refresh required')" />
        </div>

        <div class="store-controls">
            <InputText v-model="query" :placeholder="cn ? '搜索名称、介绍或项目 ID' : 'Search name, summary or project ID'" :aria-label="cn ? '搜索应用' : 'Search apps'" />
            <Select v-model="category" :options="categories" optionLabel="label" optionValue="value" :aria-label="cn ? '应用分类' : 'App category'" />
        </div>
        <div v-if="catalog?.initialized && !results.length" class="console-panel store-empty">
            {{ cn ? '没有符合条件的应用。' : 'No matching applications.' }}
        </div>
        <div v-else-if="!catalog?.initialized" class="console-panel store-empty">
            {{ cn ? '尚无已验证的本地商店快照。管理员可点击“手动刷新 Git”建立目录。' : 'No locally indexed catalog yet. An administrator can manually refresh from Git.' }}
        </div>
        <div v-else class="store-grid">
            <button v-for="app in results" :key="app.id" type="button" class="store-app console-panel" :class="{ chosen: selected?.id === app.id }" :disabled="working"
                    :aria-pressed="selected?.id === app.id" @click="showDetail(app.id)">
                <span class="store-card-top"><strong>{{ app.name }}</strong><Tag severity="secondary" :value="categoryLabel(app.category)" /></span>
                <span class="store-summary">{{ app.summary }}</span>
                <span class="mono-value">{{ app.id }} · {{ app.compose_project }}</span>
                <small>{{ cn ? '查看详细信息' : 'View details' }}</small>
            </button>
        </div>

        <section v-if="selected" class="console-panel store-details" aria-label="Application details">
            <div class="store-card-top"><h2>{{ selected.name }}</h2><Button icon="pi pi-times" text severity="secondary" :aria-label="cn ? '关闭详情' : 'Close details'" @click="selected = null" /></div>
            <p>{{ selected.summary }}</p>
            <div class="store-facts">
                <div><strong>{{ cn ? '分类' : 'Category' }}</strong><span>{{ categoryLabel(selected.category) }}</span></div>
                <div><strong>Compose</strong><span class="mono-value">{{ selected.compose_project }}</span></div>
                <div><strong>{{ cn ? '运行方式' : 'Runtime' }}</strong><span>{{ selected.detail.runtime_kind }}</span></div>
                <div><strong>{{ cn ? '支持平台' : 'Platforms' }}</strong><span>{{ selected.detail.platforms.join(', ') }}</span></div>
                <div><strong>{{ cn ? '支持架构' : 'Architectures' }}</strong><span>{{ selected.detail.architectures.join(', ') }}</span></div>
                <div><strong>{{ cn ? '清单路径' : 'Manifest' }}</strong><span class="mono-value">{{ selected.manifest_path }}</span></div>
                <div><strong>{{ cn ? '模板校验摘要' : 'Manifest SHA-256' }}</strong><span class="mono-value store-hash">{{ selected.manifest_sha256 }}</span></div>
                <div><strong>{{ cn ? '源代码版本' : 'Source revision' }}</strong><span class="mono-value store-hash">{{ selected.git_revision }}</span></div>
            </div>
            <div class="store-definition" v-if="selected.detail.actions">
                <h3>{{ cn ? '部署方式（只读）' : 'Deployment strategies (read-only)' }}</h3>
                <div v-for="action in actionNames" :key="action" class="store-definition-row">
                    <span>{{ action }}</span><span class="mono-value">{{ selected.detail.actions[action] || '—' }}</span>
                </div>
            </div>
            <div v-if="selected.detail.ports?.length" class="store-definition">
                <h3>{{ cn ? '服务端口' : 'Service ports' }}</h3>
                <div v-for="port in selected.detail.ports" :key="port.id" class="store-definition-row">
                    <span><strong>{{ port.label || port.id }}</strong> · {{ port.service }}</span>
                    <span class="mono-value">{{ port.host_port_key }} → {{ port.container_port }}/{{ port.protocol }}</span>
                </div>
            </div>
            <div v-if="selected.detail.config?.length" class="store-definition">
                <h3>{{ cn ? '动态安装配置表单' : 'Dynamic installation configuration' }}</h3>
                <Message severity="info" :closable="false">
                    {{ cn ? '字段来自固定版本 app.yaml。值仅暂存在当前页面内存；生成部署包不会提交或保存任何配置值，密码不会预填。' : 'Fields are generated from the pinned app.yaml. Values stay in this page memory only; package preparation does not submit or save them. Secrets are never prefilled.' }}
                </Message>
                <div class="install-form">
                    <div v-for="field in selected.detail.config" :key="field.key" class="install-field">
                        <label :for="'deploy-config-' + field.key">
                            {{ field.label }} <span v-if="field.required" class="required-mark">*</span>
                        </label>
                        <small class="mono-value">{{ field.key }} · {{ field.target }}</small>
                        <p v-if="field.description" class="field-description">{{ field.description }}</p>
                        <Checkbox v-if="field.type === 'boolean'"
                            :inputId="'deploy-config-' + field.key"
                            v-model="formValues[field.key]" binary
                            :disabled="working || field.editable === false" />
                        <InputNumber v-else-if="field.type === 'number' || field.type === 'port'"
                            :inputId="'deploy-config-' + field.key"
                            v-model="formValues[field.key]"
                            :min="field.minimum ?? (field.type === 'port' ? 1 : undefined)"
                            :max="field.maximum ?? (field.type === 'port' ? 65535 : undefined)"
                            :useGrouping="false" fluid
                            :disabled="working || field.editable === false"
                            :aria-invalid="Boolean(formErrors[field.key])" />
                        <InputText v-else
                            :id="'deploy-config-' + field.key"
                            v-model="formValues[field.key]"
                            :type="field.type === 'secret' ? 'password' : 'text'"
                            :autocomplete="field.type === 'secret' ? 'new-password' : 'off'"
                            :maxlength="field.max_length ?? undefined"
                            :placeholder="field.type === 'secret'
                                ? (field.generated_by_script ? (cn ? '留空由安装脚本生成' : 'Leave blank to generate securely') : (cn ? '请输入，不会预填' : 'Enter value; never prefilled'))
                                : (field.default !== undefined ? String(field.default) : '')"
                            :disabled="working || field.editable === false"
                            :aria-invalid="Boolean(formErrors[field.key])" />
                        <small v-if="field.minimum !== undefined || field.maximum !== undefined" class="field-hint">
                            {{ cn ? '允许范围' : 'Allowed range' }}: {{ field.minimum ?? '—' }} – {{ field.maximum ?? '—' }}
                        </small>
                        <small v-if="field.min_length !== undefined || field.max_length !== undefined" class="field-hint">
                            {{ cn ? '字符长度' : 'Length' }}: {{ field.min_length ?? 0 }} – {{ field.max_length ?? '∞' }}
                        </small>
                        <small v-if="field.pattern" class="field-hint">
                            {{ cn ? '此格式约束会在后端实际安装阶段再次校验。' : 'This format constraint will be revalidated by the backend before installation.' }}
                        </small>
                        <small v-if="formErrors[field.key]" class="form-error" role="alert">{{ formErrors[field.key] }}</small>
                    </div>
                </div>
                <Button icon="pi pi-shield" :loading="preparing" :disabled="working"
                    :label="cn ? '校验表单并准备固定版本包' : 'Validate form and prepare pinned package'"
                    @click="preparePackage" />
            </div>
            <div v-if="packageResult" class="store-definition package-result" role="status">
                <h3>{{ cn ? '固定版本部署包已准备' : 'Pinned package prepared' }}</h3>
                <div class="store-definition-row"><strong>{{ cn ? '源版本' : 'Source revision' }}</strong><span class="mono-value">{{ packageResult.git_revision }}</span></div>
                <div class="store-definition-row"><strong>SHA-256</strong><span class="mono-value store-hash">{{ packageResult.package_sha256 }}</span></div>
                <div class="store-definition-row"><strong>{{ cn ? '包大小' : 'Package size' }}</strong><span>{{ (packageResult.package_size / 1024 / 1024).toFixed(2) }} MiB</span></div>
                <div class="store-definition-row"><strong>{{ cn ? '文件数' : 'Files' }}</strong><span>{{ packageResult.file_count }}</span></div>
                <div class="store-definition-row"><strong>{{ cn ? '构建时间' : 'Built at' }}</strong><span>{{ displayTime(packageResult.built_at) }}</span></div>
                <Message severity="warn" :closable="false">
                    {{ cn ? '这是只读、不可变的包预览，不是安装授权。表单值尚未上传；不会执行脚本、启动容器或修改设备。' : 'This is an immutable package preview, not an install authorization. Form values were not uploaded; no script, container or device was changed.' }}
                </Message>
            </div>
            <Message severity="warn" :closable="false">{{ cn ? '实际安装仍未开放：后续任务将把表单值绑定到受控 Agent 执行链路，并再次验证包签名/摘要与设备授权。' : 'Actual installation remains disabled. A later task will bind form values to the controlled Agent flow and revalidate package integrity and device authorization.' }}</Message>
        </section>
    </section>
</template>

<style scoped>
.store-page { display: grid; gap: 1rem; }
.store-version { display: flex; flex-wrap: wrap; align-items: center; gap: 1.5rem; padding: 1rem; }
.store-version p { margin: .35rem 0 0; }
.store-controls { display: flex; gap: .75rem; flex-wrap: wrap; }
.store-controls input { flex: 1; min-width: 14rem; }
.store-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: .75rem; }
.store-app { display: grid; gap: .8rem; padding: 1rem; text-align: left; cursor: pointer; color: inherit; }
.store-app:hover, .store-app.chosen { outline: 1px solid var(--p-primary-color); }
.store-app:disabled { cursor: wait; }
.store-card-top { display: flex; align-items: center; justify-content: space-between; gap: .75rem; }
.store-summary { color: var(--p-text-muted-color); }
.store-empty { padding: 1.5rem; }
.store-details { display: grid; gap: .75rem; padding: 1rem; }
.store-details h2 { margin: 0; }
.store-facts { display: grid; gap: .7rem; }
.store-facts > div { display: flex; gap: 1rem; justify-content: space-between; border-bottom: 1px solid var(--p-content-border-color); padding-bottom: .35rem; }
.store-facts strong { flex: 0 0 auto; }
.store-hash { overflow-wrap: anywhere; text-align: right; }
.install-form { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 1rem; }
.install-field { display: flex; flex-direction: column; align-items: stretch; gap: .4rem; min-width: 0; }
.install-field label { font-weight: 600; }
.install-field input { width: 100%; }
.field-description, .field-hint { color: var(--p-text-muted-color); margin: 0; font-size: .85rem; }
.required-mark, .form-error { color: var(--p-red-500); }
.package-result { border: 1px solid var(--p-content-border-color); border-radius: .5rem; padding: 1rem; }
.store-definition { display: grid; gap: .45rem; }
.store-definition h3 { margin: .5rem 0; }
.store-definition-row { display: flex; flex-wrap: wrap; justify-content: space-between; gap: .5rem; border-bottom: 1px solid var(--p-content-border-color); padding: .45rem 0; }
.mono-value { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .85rem; overflow-wrap: anywhere; }
</style>
