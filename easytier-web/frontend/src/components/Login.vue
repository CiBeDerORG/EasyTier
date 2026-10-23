<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { InputText, Password, Button, Message } from 'primevue';
import { useRouter } from 'vue-router';
import { useToast } from 'primevue/usetoast';
import { I18nUtils } from 'easytier-frontend-lib';
import { getApiBase } from '../modules/api-host';
import { useI18n } from 'vue-i18n';
import ApiClient from '../modules/api';
import { shellMessages } from '../modules/shell-messages';
import { applyThemeMode, storedThemeMode, type ThemeMode } from '../modules/theme';
import Icon from '../assets/easytier.png';
import Atmosphere from '../assets/console/network-atmosphere.webp';

const props = defineProps<{ isRegistering: boolean }>();
const { t } = useI18n();
const { t: ui } = useI18n({ useScope: 'local', messages: shellMessages });
const api = new ApiClient(getApiBase());
const router = useRouter();
const toast = useToast();
const username = ref('');
const password = ref('');
const registerUsername = ref('');
const registerPassword = ref('');
const captcha = ref('');
const captchaVersion = ref(0);
const captchaSrc = computed(() => api.captcha_url() + '?refresh=' + captchaVersion.value);
const submitting = ref(false);
const error = ref('');
const oidcEnabled = ref(false);
const themeMode = ref<ThemeMode>(storedThemeMode());
const themeIcon = computed(() => themeMode.value === 'dark' ? 'pi pi-moon' : themeMode.value === 'light' ? 'pi pi-sun' : 'pi pi-desktop');
function cycleTheme() {
    const modes: ThemeMode[] = ['system', 'light', 'dark'];
    themeMode.value = modes[(modes.indexOf(themeMode.value) + 1) % modes.length];
    applyThemeMode(themeMode.value);
}
function refreshCaptcha() { captchaVersion.value++; captcha.value = ''; }
watch(() => props.isRegistering, () => { error.value = ''; });
async function onSubmit() {
    if (submitting.value || !username.value || !password.value) return;
    error.value = '';
    submitting.value = true;
    try {
        const result = await api.login({ username: username.value, password: password.value });
        if (result.success) await router.push({ name: 'dashboard' });
        else error.value = result.message || ui('loginFailed');
    } catch { error.value = ui('loginFailed'); }
    finally { submitting.value = false; }
}
async function onRegister() {
    if (submitting.value || !registerUsername.value || !registerPassword.value || !captcha.value) return;
    error.value = '';
    submitting.value = true;
    try {
        const result = await api.register({ credentials: { username: registerUsername.value, password: registerPassword.value }, captcha: captcha.value });
        if (result.success) {
            username.value = registerUsername.value;
            registerPassword.value = '';
            captcha.value = '';
            toast.add({ severity: 'success', summary: ui('registerSuccess'), life: 3500 });
            await router.push({ name: 'login' });
        } else error.value = result.message || ui('registerFailed');
    } catch { error.value = ui('registerFailed'); }
    finally { submitting.value = false; }
}
function onSsoLogin() { if (!submitting.value) window.location.href = api.oidcLoginUrl(); }
onMounted(async () => {
    try { oidcEnabled.value = (await api.getOidcConfig()).enabled; }
    catch { oidcEnabled.value = false; }
});
</script>

<template>
    <main class="auth-page">
        <section class="auth-story">
            <img :src="Atmosphere" class="auth-atmosphere" alt="" aria-hidden="true" />
            <div class="auth-brand"><img :src="Icon" alt="" /><span>EasyTier<small>NETWORK WORKSPACE</small></span></div>
            <div class="auth-story-copy">
                <p class="auth-eyebrow">{{ ui('authEyebrow') }}</p>
                <h1>{{ ui('authTitle') }}</h1>
                <p class="auth-story-description">{{ ui('authDescription') }}</p>
            </div>
            <div class="auth-capabilities">
                <div><i class="pi pi-sitemap" aria-hidden="true"></i><span><strong>{{ ui('authNetworks') }}</strong><small>{{ ui('authNetworksHint') }}</small></span></div>
                <div><i class="pi pi-globe" aria-hidden="true"></i><span><strong>{{ ui('authMap') }}</strong><small>{{ ui('authMapHint') }}</small></span></div>
                <div><i class="pi pi-th-large" aria-hidden="true"></i><span><strong>{{ ui('authServices') }}</strong><small>{{ ui('authServicesHint') }}</small></span></div>
            </div>
            <a class="auth-story-docs" href="https://easytier.cn" target="_blank" rel="noopener noreferrer">{{ ui('documentation') }}<i class="pi pi-arrow-up-right" aria-hidden="true"></i></a>
        </section>
        <section class="auth-entry">
            <div class="auth-preferences">
                <Button :icon="themeIcon" text severity="secondary" :aria-label="t('web.console.theme_mode')" @click="cycleTheme" />
                <Button icon="pi pi-language" text severity="secondary" :aria-label="t('web.console.language')" @click="I18nUtils.toggleLanguage" />
            </div>
            <div class="auth-form-panel">
                <div class="auth-form-symbol"><i :class="isRegistering ? 'pi pi-user-plus' : 'pi pi-lock'" aria-hidden="true"></i></div>
                <header class="auth-heading"><h2>{{ ui(isRegistering ? 'registerTitle' : 'welcome') }}</h2><p>{{ ui(isRegistering ? 'registerHint' : 'welcomeHint') }}</p></header>
                <Message v-if="error" severity="error" :closable="false" role="alert" class="auth-error">{{ error }}</Message>
                <form v-if="!isRegistering" @submit.prevent="onSubmit" class="auth-form" :aria-busy="submitting">
                    <div class="auth-field"><label for="username">{{ t('web.login.username') }}</label><InputText id="username" v-model="username" autocomplete="username" required :disabled="submitting" /></div>
                    <div class="auth-field"><label for="password">{{ t('web.login.password') }}</label><Password input-id="password" v-model="password" autocomplete="current-password" required toggleMask :feedback="false" :disabled="submitting" /></div>
                    <Button :label="t('web.login.login')" icon="pi pi-arrow-right" icon-pos="right" type="submit" class="auth-submit" :loading="submitting" :disabled="submitting" />
                    <div v-if="oidcEnabled" class="auth-sso"><p>{{ ui('alternateSignIn') }}</p><Button :label="t('web.login.sso_login')" icon="pi pi-id-card" type="button" outlined severity="secondary" :disabled="submitting" @click="onSsoLogin" /></div>
                    <p class="auth-account-switch"><span>{{ ui('accountPrompt') }}</span><Button :label="t('web.login.register')" type="button" text :disabled="submitting" @click="router.replace({ name: 'register' })" /></p>
                </form>
                <form v-else @submit.prevent="onRegister" class="auth-form" :aria-busy="submitting">
                    <div class="auth-field"><label for="register-username">{{ t('web.login.username') }}</label><InputText id="register-username" v-model="registerUsername" autocomplete="username" required :disabled="submitting" /></div>
                    <div class="auth-field"><label for="register-password">{{ t('web.login.password') }}</label><Password input-id="register-password" v-model="registerPassword" autocomplete="new-password" required toggleMask :feedback="false" :disabled="submitting" /></div>
                    <div class="auth-field">
                        <label for="captcha">{{ t('web.login.captcha') }}</label>
                        <div class="auth-captcha-row"><InputText id="captcha" v-model="captcha" autocomplete="off" required :disabled="submitting" />
                            <button type="button" class="auth-captcha-image" :aria-label="ui('refreshCaptcha')" :disabled="submitting" @click="refreshCaptcha"><img :src="captchaSrc" :alt="ui('captchaAlt')" /></button>
                            <Button icon="pi pi-refresh" text severity="secondary" :aria-label="ui('refreshCaptcha')" :disabled="submitting" @click="refreshCaptcha" />
                        </div><small>{{ ui('captchaHint') }}</small>
                    </div>
                    <Button :label="t('web.login.register')" icon="pi pi-arrow-right" icon-pos="right" type="submit" class="auth-submit" :loading="submitting" :disabled="submitting" />
                    <p class="auth-account-switch"><span>{{ ui('existingAccount') }}</span><Button :label="t('web.login.back_to_login')" type="button" text :disabled="submitting" @click="router.replace({ name: 'login' })" /></p>
                </form>
            </div>
            <footer class="auth-footer">{{ ui('authFooter') }}</footer>
        </section>
    </main>
</template>

<style scoped>
.auth-page { min-height: 100dvh; display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(420px, .85fr); gap: 32px; padding: 24px; background: var(--console-ground); color: var(--p-text-color); font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Microsoft YaHei', sans-serif; }
.auth-story { min-height: 680px; position: relative; isolation: isolate; display: flex; flex-direction: column; justify-content: center; overflow: hidden; padding: 44px; border-radius: 22px; border: 1px solid #203249; background: #050d18; color: #eef6ff; }
.auth-atmosphere { position: absolute; inset: 0; z-index: -1; height: 100%; width: 100%; object-fit: cover; object-position: 58% center; opacity: .6; }
.auth-brand { display: flex; align-items: center; gap: 12px; margin-bottom: auto; font-size: 24px; font-weight: 700; letter-spacing: -.5px; }
.auth-brand img { width: 42px; height: 42px; filter: drop-shadow(0 0 12px #2563eb44); }
.auth-brand small { display: block; margin-top: 3px; color: #8ba7c2; font-size: 9px; font-weight: 500; letter-spacing: .18em; }
.auth-story-copy { margin: 84px 0 54px; max-width: 490px; }
.auth-eyebrow { color: #7adce7; font-size: 10px; font-weight: 600; letter-spacing: .16em; margin-bottom: 22px; }
.auth-story h1 { white-space: pre-line; font-size: clamp(32px, 3.4vw, 52px); line-height: 1.3; font-weight: 650; letter-spacing: -1.5px; text-wrap: balance; }
.auth-story-description { color: #b0c5d9; font-size: 14px; line-height: 1.9; max-width: 340px; margin-top: 24px; text-wrap: balance; }
.auth-capabilities { display: flex; flex-direction: column; gap: 24px; margin-bottom: 64px; }
.auth-capabilities > div { display: flex; align-items: center; gap: 16px; }
.auth-capabilities > div > .pi { display: inline-flex; align-items: center; justify-content: center; width: 38px; height: 38px; border-radius: 10px; border: 1px solid #254453; background: #0b2534; color: #6ee1e7; font-size: 16px; }
.auth-capabilities strong { display: block; font-size: 13px; font-weight: 550; }
.auth-capabilities small { display: block; margin-top: 3px; font-size: 11px; color: #94afc6; }
.auth-story-docs { display: inline-flex; align-items: center; align-self: flex-start; gap: 10px; margin-top: auto; color: #a7c0d4; font-size: 12px; }
.auth-story-docs:hover { color: #8eeef2; }
.auth-entry { display: flex; align-items: center; flex-direction: column; min-width: 0; }
.auth-preferences { display: flex; align-self: flex-end; gap: 6px; padding: 4px 8px; }
.auth-form-panel { width: min(100%, 400px); margin-block: auto; padding: 48px 0; }
.auth-form-symbol { width: 48px; height: 48px; display: flex; align-items: center; justify-content: center; border: 1px solid color-mix(in srgb, var(--p-primary-color) 28%, var(--console-border)); border-radius: 14px; color: var(--p-primary-color); background: var(--console-accent-soft); box-shadow: var(--console-glow); font-size: 20px; margin-bottom: 24px; }
.auth-heading h2 { font-size: 29px; line-height: 1.3; letter-spacing: -.6px; font-weight: 650; }
.auth-heading p { margin-top: 10px; color: var(--console-muted); font-size: 13px; line-height: 1.7; }
.auth-form { display: flex; flex-direction: column; gap: 24px; margin-top: 36px; }
.auth-field { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
.auth-field > label { font-size: 13px; font-weight: 550; }
.auth-field :deep(.p-inputtext) { width: 100%; min-height: 46px; background: var(--console-panel); border-color: var(--console-border-strong); }
.auth-field :deep(.p-inputtext:focus) { border-color: var(--p-primary-color); box-shadow: 0 0 0 3px var(--console-accent-soft); }
.auth-field > small { color: var(--console-muted); font-size: 11px; line-height: 1.5; }
.auth-submit.p-button { width: 100%; min-height: 46px; margin-top: 4px; border-radius: 9px; font-weight: 600; }
.auth-submit.p-button:hover { box-shadow: var(--console-glow); }
.auth-error { margin-top: 22px; overflow-wrap: anywhere; }
.auth-sso { padding-top: 6px; }
.auth-sso > p { display: flex; align-items: center; gap: 14px; color: var(--console-muted); font-size: 11px; margin-bottom: 16px; }
.auth-sso > p::before, .auth-sso > p::after { content: ''; flex: 1; border-top: 1px solid var(--console-border); }
.auth-sso > .p-button { width: 100%; min-height: 44px; }
.auth-account-switch { display: flex; justify-content: center; align-items: center; flex-wrap: wrap; gap: 4px; color: var(--console-muted); font-size: 12px; padding-top: 4px; }
.auth-account-switch > .p-button { font-size: 12px; padding: 6px 8px; }
.auth-captcha-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
.auth-captcha-row > .p-inputtext { min-width: 0; flex: 1; width: 0; }
.auth-captcha-image { flex: none; border: 1px solid var(--console-border); border-radius: 8px; background: white; overflow: hidden; width: 120px; height: 46px; }
.auth-captcha-image img { width: 100%; height: 100%; object-fit: contain; }
.auth-captcha-image:focus-visible { outline: 2px solid var(--p-primary-color); outline-offset: 3px; }
.auth-captcha-image:disabled { opacity: .6; cursor: wait; }
.auth-captcha-row > .p-button { flex: none; width: 32px; height: 36px; }
.auth-footer { padding: 24px 8px 8px; color: var(--console-muted); font-size: 11px; letter-spacing: .02em; }
@media (max-width: 1100px) {
    .auth-page { grid-template-columns: minmax(0, 1fr) minmax(360px, 1fr); gap: 24px; padding: 16px; }
    .auth-story { padding: 32px; }
    .auth-form-panel { padding: 40px 16px; }
    .auth-story h1 { font-size: 35px; }
}
@media (max-width: 767px) {
    .auth-page { display: flex; flex-direction: column; padding: 0; gap: 0; }
    .auth-story { min-height: 0; padding: 28px 24px; border-radius: 0; border: 0; border-bottom: 1px solid #203249; }
    .auth-brand { font-size: 22px; }
    .auth-brand img { width: 36px; height: 36px; }
    .auth-story-copy { margin: 26px 0 2px; max-width: 260px; }
    .auth-eyebrow, .auth-story-description, .auth-capabilities, .auth-story-docs { display: none; }
    .auth-story h1 { font-size: 28px; letter-spacing: -.8px; }
    .auth-atmosphere { object-position: right center; opacity: .5; }
    .auth-entry { padding: 16px 24px 0; }
    .auth-preferences { padding: 0; }
    .auth-form-panel { margin-block: 0; padding: 10px 0 24px; }
    .auth-form-symbol { display: none; }
    .auth-heading h2 { font-size: 24px; }
    .auth-form { gap: 22px; margin-top: 26px; }
    .auth-footer { padding: 8px 0 24px; }
}
@media (prefers-reduced-motion: no-preference) {
    .auth-form-panel { animation: auth-enter .3s ease-out; }
    @keyframes auth-enter { from { opacity: 0; transform: translateY(6px); } to { opacity: 1; transform: translateY(0); } }
}
</style>
