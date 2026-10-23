<script lang="ts" setup>
import { computed, inject, ref } from 'vue';
import { Password, Button, Message, useToast } from 'primevue';
import { useI18n } from 'vue-i18n';
import ApiClient from '../modules/api';
import { shellMessages } from '../modules/shell-messages';
const { t } = useI18n({ useScope: 'local', messages: shellMessages });
const dialogRef = inject<any>('dialogRef');
const api = computed<ApiClient>(() => dialogRef.value.data.api);
const toast = useToast();
const password = ref('');
const saving = ref(false);
const error = ref('');
async function changePassword() {
    if (saving.value || !password.value) return;
    saving.value = true;
    error.value = '';
    try {
        await api.value.change_password(password.value);
        toast.add({ severity: 'success', summary: t('passwordSaved'), life: 3000 });
        dialogRef.value.close();
    } catch { error.value = t('passwordFailed'); }
    finally { saving.value = false; }
}
</script>
<template>
    <form class="account-password-form" @submit.prevent="changePassword" :aria-busy="saving">
        <p>{{ t('passwordHint') }}</p>
        <Message v-if="error" severity="error" :closable="false" role="alert">{{ error }}</Message>
        <label for="account-new-password">{{ t('newPassword') }}</label>
        <Password input-id="account-new-password" v-model="password" autocomplete="new-password" :feedback="false" toggleMask required :disabled="saving" />
        <Button type="submit" icon="pi pi-check" :label="t('savePassword')" :loading="saving" :disabled="saving || !password" />
    </form>
</template>
<style scoped>
.account-password-form { display: flex; flex-direction: column; gap: 16px; min-width: 0; padding-bottom: 4px; }
.account-password-form > p { color: var(--console-muted); font-size: 13px; }
.account-password-form > label { font-size: 13px; font-weight: 550; margin-bottom: -8px; }
.account-password-form > .p-button { margin-top: 8px; align-self: flex-end; }
</style>
