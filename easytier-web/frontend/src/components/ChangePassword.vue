<script lang="ts" setup>
import { computed, inject, ref } from 'vue';
import { Card, Password, Button } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';
import ApiClient from '../modules/api';
import { toUserManagementError } from '../modules/userManagement';
const props = defineProps<{ api?: ApiClient; required?: boolean }>();
const dialogRef = inject<{ value: { data: { api: ApiClient }; close: () => void } } | null>('dialogRef', null);
const api = computed(() => props.api ?? dialogRef?.value.data.api);
const { t } = useI18n();
const router = useRouter();
const password = ref('');
const busy = ref(false);
const errorCode = ref<string | null>(null);
const valid = computed(() => password.value.length >= 12 && password.value.length <= 128);
async function changePassword() {
    if (!valid.value || busy.value || !api.value) return;
    busy.value = true; errorCode.value = null;
    try {
        await api.value.change_password(password.value);
        password.value = ''; dialogRef?.value.close();
        await router.replace({ name: 'login', query: { reason: 'password_changed' } });
    } catch (error) {
        const safe = toUserManagementError(error);
        errorCode.value = safe.code === 'network_error' ? 'network_error' : safe.status === 401 ? 'unauthenticated' : safe.status === 403 ? 'forbidden' : 'invalid_request';
    } finally { busy.value = false; }
}
</script>
<template>
    <div class="flex items-center justify-center">
        <Card class="w-full max-w-md p-6">
            <template #title>{{ t('web.main.change_password') }}</template>
            <template #content>
                <p v-if="required">{{ t('user_management.password_change_required') }}</p>
                <form class="flex flex-col gap-4" @submit.prevent="changePassword">
                    <label for="new-account-password">{{ t('web.login.password') }}</label>
                    <Password input-id="new-account-password" v-model="password" :feedback="false" toggleMask autocomplete="new-password" :disabled="busy" aria-describedby="password-policy" />
                    <p id="password-policy">{{ t('user_management.password_policy') }}</p>
                    <p v-if="errorCode" role="alert">{{ t(`user_management.errors.${errorCode}`) }}</p>
                    <Button type="submit" :label="t('web.main.change_password')" :disabled="busy || !valid" :loading="busy" />
                </form>
            </template>
        </Card>
    </div>
</template>
