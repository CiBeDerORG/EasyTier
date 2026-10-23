<script setup lang="ts">
import { computed, watch } from 'vue';
import { Button, Checkbox, InputText, Select } from 'primevue';
import { useI18n } from 'vue-i18n';
import type ApiClient from '../../modules/api';
import type { AddressSpec, AssetScope, GatewayGroup, InstancePreview, Subject } from '../../types/node-assets';
import { useInstanceAddressPreview } from '../../composables/useInstanceAddressPreview';
import { formatAssetError, getAssetError } from '../../composables/useAssetMutation';

const props = defineProps<{
    api: ApiClient; scope: AssetScope; subject?: Subject; gateway?: GatewayGroup | null;
    addressSpec: AddressSpec | null; disabled?: boolean;
}>();
const emit = defineEmits<{
    'update:addressSpec': [value: AddressSpec | null];
    'preview-change': [value: InstancePreview | null]; 'context-error': [cause: unknown];
}>();
const { t } = useI18n();
const rules = computed(() => props.gateway?.binding.snapshot?.rules ?? []);
const ruleOptions = computed(() => rules.value.map(rule => ({ ...rule,
    label: rule.mapped_cidr ? `${rule.real_cidr} → ${rule.mapped_cidr}` : rule.real_cidr,
})));
const selectedRule = computed(() => rules.value.find(rule => rule.rule_key ===
    (props.addressSpec?.mode === 'direct_instance' ? null : props.addressSpec?.rule_key)));
const isSelf = computed(() => props.addressSpec?.mode === 'gateway_self');
const selfLan = computed(() => isSelf.value && props.addressSpec?.mode === 'gateway_self' && props.addressSpec.rule_key !== null);
const child = computed(() => !!props.addressSpec && props.addressSpec.mode !== 'direct_instance');
const parentUsable = computed(() => !child.value || props.gateway?.binding.state === 'current');
const maxOffset = computed(() => selectedRule.value ? 2 ** (32 - selectedRule.value.prefix_len) - 1 : 0);
const previewInput = computed(() => props.addressSpec ? {
    subject: props.subject ?? { resource_kind: 'device' as const, id: null }, address_spec: props.addressSpec,
} : null);
const previewer = useInstanceAddressPreview({
    api: () => props.api, scope: () => props.scope, input: () => previewInput.value,
    enabled: () => !props.disabled && parentUsable.value,
    onContextError: cause => emit('context-error', cause),
});
const fieldErrors = computed(() => previewer.preview.value?.field_errors
    ?? (previewer.error.value ? getAssetError(previewer.error.value).field_errors : {}));
const oldGeneration = computed(() => {
    const before = previewer.preview.value?.before?.snapshot?.config_ref;
    const after = previewer.preview.value?.after?.snapshot?.config_ref;
    return !!before && !!after && (before.id !== after.id || before.create_time !== after.create_time);
});
watch(previewer.preview, value => emit('preview-change', value), { flush: 'sync' });
// The parent snapshot may change without changing a child draft's rule key.
watch(() => JSON.stringify(props.gateway?.binding), () => previewer.invalidate(), { flush: 'sync' });
function changeRule(ruleKey: string | null) {
    const value = props.addressSpec;
    if (!value || value.mode === 'direct_instance') return;
    emit('update:addressSpec', { ...value, rule_key: ruleKey ?? '', host_offset: 0 });
}
function changeHost(raw: string | undefined) {
    const value = props.addressSpec;
    if (!value || value.mode === 'direct_instance') return;
    const host = raw?.trim() ? Number(raw) : -1;
    emit('update:addressSpec', { ...value, host_offset: Number.isFinite(host) ? host : -1 });
}
function toggleSelfLan(enabled: boolean) {
    const value = props.addressSpec;
    if (value?.mode !== 'gateway_self') return;
    emit('update:addressSpec', { ...value, rule_key: enabled ? rules.value[0]?.rule_key ?? '' : null,
        host_offset: enabled ? 0 : null });
}
function changeLan(value: string | undefined) {
    if (props.addressSpec?.mode === 'direct_instance') {
        emit('update:addressSpec', { ...props.addressSpec, lan_ip: value?.trim() || null });
    }
}
</script>

<template>
    <section class="address-fields" :aria-label="t('assets.binding.address')">
        <p class="muted">{{ t('assets.binding.newIpv4Only') }}</p>
        <template v-if="child">
            <p class="inherited-source">
                <span>{{ t('assets.binding.inherited') }}:</span>
                <strong>{{ gateway?.name }}</strong>
                <span v-if="gateway?.binding.snapshot?.display_name">· {{ gateway.binding.snapshot.display_name }}</span>
                <span v-if="gateway?.binding.instance_id">· <code :title="gateway.binding.instance_id">{{ gateway.binding.instance_id.slice(0, 8) }}</code></span>
            </p>
            <p v-if="!parentUsable" class="warning" role="status">{{ t('assets.binding.parentRequired') }}</p>
            <div v-if="isSelf" class="self-lan">
                <Checkbox inputId="asset-self-lan" :modelValue="selfLan" binary :disabled="disabled || !parentUsable"
                    @update:modelValue="toggleSelfLan" />
                <label for="asset-self-lan">{{ t('assets.binding.selfLan') }}</label>
            </div>
            <p v-if="isSelf" class="muted">{{ t('assets.binding.selfHint') }}</p>
            <template v-if="!isSelf || selfLan">
                <label for="asset-address-rule">{{ t('assets.binding.rule') }} *</label>
                <Select inputId="asset-address-rule" :modelValue="addressSpec && addressSpec.mode !== 'direct_instance' ? addressSpec.rule_key : null"
                    :options="ruleOptions" optionLabel="label" optionValue="rule_key" :placeholder="t('assets.binding.chooseRule')"
                    :disabled="disabled || !parentUsable" @update:modelValue="changeRule" />
                <label for="asset-address-host">{{ t(selectedRule?.prefix_len === 24 ? 'assets.binding.lastOctet' : 'assets.binding.host') }} *</label>
                <InputText id="asset-address-host" inputmode="numeric" autocomplete="off" spellcheck="false"
                    :modelValue="addressSpec && addressSpec.mode !== 'direct_instance' && addressSpec.host_offset !== null && addressSpec.host_offset >= 0 ? String(addressSpec.host_offset) : ''"
                    :disabled="disabled || !parentUsable || !selectedRule" @update:modelValue="changeHost" />
                <small v-if="selectedRule" class="muted">{{ t('assets.binding.hostRange', { max: maxOffset }) }}</small>
                <small class="muted">{{ t('assets.binding.hostHint') }}</small>
            </template>
        </template>
        <template v-else-if="addressSpec?.mode === 'direct_instance'">
            <label for="asset-direct-lan">{{ t('assets.deviceLanIp') }}</label>
            <InputText id="asset-direct-lan" :modelValue="addressSpec.lan_ip || ''" :disabled="disabled" autocomplete="off"
                spellcheck="false" placeholder="192.168.1.20" @update:modelValue="changeLan" />
        </template>
        <Button :label="t('assets.binding.preview')" icon="pi pi-search" severity="secondary"
            :loading="previewer.pending.value" :disabled="disabled || !addressSpec || !parentUsable || previewer.pending.value"
            @click="previewer.run" />
        <p v-for="(message, field) in fieldErrors" :key="field" class="error" role="alert">{{ message }}</p>
        <p v-if="previewer.error.value" class="error" role="alert">{{ formatAssetError(previewer.error.value, t('assets.binding.previewFailed'), key => t(key)) }}</p>
        <template v-if="previewer.preview.value">
            <p v-if="oldGeneration" class="warning" role="status">{{ t('assets.binding.generationChanged') }}</p>
            <div class="diff">
                <div v-if="previewer.preview.value.before" class="addresses">
                    <strong>{{ t('assets.binding.previous') }}</strong>
                    <small v-if="previewer.preview.value.before.snapshot" class="muted">{{ t('assets.binding.configIdentity') }}: #{{ previewer.preview.value.before.snapshot.config_ref.id }} · {{ previewer.preview.value.before.snapshot.config_ref.create_time }}</small>
                    <span>{{ t('assets.deviceOverlayIp') }}: <code>{{ previewer.preview.value.before.overlay_ip || t('assets.binding.noValue') }}</code></span>
                    <span>{{ t('assets.deviceLanIp') }}: <code>{{ previewer.preview.value.before.lan_ip || '—' }}</code></span>
                </div>
                <div v-if="previewer.preview.value.addresses" class="addresses">
                    <strong>{{ t('assets.binding.proposed') }}</strong>
                    <small v-if="previewer.preview.value.snapshot" class="muted">{{ t('assets.binding.configIdentity') }}: #{{ previewer.preview.value.snapshot.config_ref.id }} · {{ previewer.preview.value.snapshot.config_ref.create_time }}</small>
                    <span>{{ t('assets.deviceOverlayIp') }}: <code>{{ previewer.preview.value.addresses.overlay_ip }}</code></span>
                    <span>{{ t('assets.deviceLanIp') }}: <code>{{ previewer.preview.value.addresses.lan_ip || '—' }}</code></span>
                </div>
            </div>
            <p v-for="(warning, index) in previewer.preview.value.warnings" :key="index" class="warning" role="status">{{ warning.message }}</p>
        </template>
    </section>
</template>

<style scoped>
.address-fields,.addresses { display:grid; gap:.5rem; min-width:0; }
.self-lan { display:flex; gap:.5rem; align-items:center; }
.inherited-source { display:flex; flex-wrap:wrap; align-items:baseline; gap:.35rem; }
.address-fields p { margin:0; overflow-wrap:anywhere; }
.address-fields label { font-weight:500; }
.address-fields > button { justify-self:start; }
.diff { display:grid; grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); gap:.75rem; }
.addresses { padding:.75rem; border:1px solid var(--p-content-border-color,#e2e8f0); border-radius:.5rem; overflow-wrap:anywhere; }
.muted { color:var(--p-text-muted-color,#64748b); font-size:.8125rem; }
.warning { color:var(--p-orange-700,#9a3412); font-size:.8125rem; }
.error { color:var(--p-red-600,#dc2626); font-size:.8125rem; }
</style>
