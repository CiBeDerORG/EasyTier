<script setup lang="ts">
import { NetworkTypes, Utils, Api, RemoteManagement } from 'easytier-frontend-lib';
import { computed } from 'vue';
import { useRoute, useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';
import ApiClient from '../modules/api';


const props = defineProps<{
    api: ApiClient;
    deviceList: Array<Utils.DeviceInfo> | undefined;
}>();

const emits = defineEmits(['update']);

const route = useRoute();
const router = useRouter();
const { t } = useI18n();

const deviceId = computed<string>(() => {
    return route.params.deviceId as string;
});

const instanceId = computed<string>(() => {
    return route.params.instanceId as string;
});

const deviceInfo = computed<Utils.DeviceInfo | undefined | null>(() => {
    return deviceId.value ? props.deviceList?.find((device) => device.machine_id === deviceId.value) : null;
});

const selectedInstanceId = computed({
    get() {
        return instanceId.value;
    },
    set(value: string) {
        router.push({ name: 'deviceManagement', params: { deviceId: deviceId.value, instanceId: value } });
    }
});

const remoteClient = computed<Api.RemoteClient>(() => props.api.get_remote_client(deviceId.value));

const newConfigGenerator = () => {
    const config = NetworkTypes.DEFAULT_NETWORK_CONFIG();
    config.hostname = deviceInfo.value?.hostname;
    return config;
}

</script>

<template>
    <section class="managed-device-workspace">
        <div v-if="deviceInfo" class="managed-device-context">
            <span class="managed-device-presence" :class="{ online: deviceInfo.online }"><i class="pi pi-circle-fill" aria-hidden="true"></i>{{ deviceInfo.online ? t('web.device.online') : t('web.device.offline') }}</span>
            <span v-if="deviceInfo.easytier_version"><i class="pi pi-code" aria-hidden="true"></i>{{ deviceInfo.easytier_version }}</span>
            <span><i class="pi pi-share-alt" aria-hidden="true"></i>{{ t('web.console.instances') }}<strong>{{ deviceInfo.running_network_count }}</strong></span>
        </div>
        <RemoteManagement :key="deviceId" :api="remoteClient" v-model:instance-id="selectedInstanceId"
            :new-config-generator="newConfigGenerator" @update="emits('update')" />
    </section>
</template>

<style scoped>
.managed-device-workspace { display: flex; flex-direction: column; gap: 20px; min-width: 0; }
.managed-device-context { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 20px; padding: 13px 16px; border: 1px solid var(--console-border); border-radius: 10px; background: var(--console-panel-raised, var(--console-panel)); }
.managed-device-context > span { display: inline-flex; align-items: center; gap: 8px; color: var(--console-muted); font-size: 11px; overflow-wrap: anywhere; }
.managed-device-context > span > .pi { color: var(--p-primary-color); font-size: 12px; }
.managed-device-context strong { color: var(--p-text-color); font-weight: 600; font-variant-numeric: tabular-nums; }
.managed-device-context .managed-device-presence { font-weight: 550; }
.managed-device-context .managed-device-presence > .pi { font-size: 6px; color: inherit; }
.managed-device-context .managed-device-presence.online { color: var(--p-green-700); }
html.app-dark .managed-device-context .managed-device-presence.online { color: var(--p-green-300); }
:deep(.network-header) { padding: 0 0 18px; border-bottom: 1px solid var(--console-border); }
:deep(.network-content) { min-width: 0; }
:deep(.network-status-header), :deep(.network-creation-header) { padding: 4px 0 12px; }
:deep(.network-status-header > h2), :deep(.network-creation-header > h2) { font-size: 16px; font-weight: 600; }
:deep(.network-status-header > .pi), :deep(.network-creation-header > .pi) { color: var(--p-primary-color); font-size: 16px; }
:deep(.p-card) { border: 1px solid var(--console-border); border-radius: 12px; background: var(--console-panel-raised, var(--console-panel)); box-shadow: none; }
:deep(.p-card-title) { font-size: 14px; font-weight: 600; }
:deep(.p-card-subtitle) { font-size: 12px; color: var(--console-muted); }
:deep(.empty-state) { border: 1px dashed var(--console-border-strong, var(--console-border)); border-radius: 12px; background: var(--console-panel-raised, var(--console-panel)); padding-inline: 20px; }
:deep(.empty-state > .pi) { color: var(--p-primary-color); font-size: 32px; opacity: .8; }
:deep(.empty-state > .text-xl) { font-size: 16px; }
:deep(.empty-state > p) { color: var(--console-muted); font-size: 13px; line-height: 1.7; }
@media (max-width: 767px) {
    .managed-device-context { padding: 12px; gap: 10px 14px; }
    :deep(.network-header) { padding-bottom: 15px; }
    :deep(.network-content) { padding: 0; }
}
</style>
