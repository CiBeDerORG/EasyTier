import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createServer as createPortReservation } from 'node:net';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import vue from '@vitejs/plugin-vue';
import yaml from '@modyfi/vite-plugin-yaml';
import { chromium } from 'playwright';

// Actual Vue components/locales in Chromium. Fixture state is not backend evidence.
const root = fileURLToPath(new URL('../', import.meta.url));
let server;
let browser;
let base;
let cacheDirectory;
const fixture = `
import { createApp, h, reactive } from 'vue';
import { createI18n } from 'vue-i18n';
import PrimeVue from 'primevue/config';
import theme from '/src/theme.ts';
import QueryNotice from '/src/components/ecosystem/shared/EcosystemQueryNotice.vue';
import CapabilityNotice from '/src/components/ecosystem/shared/EcosystemCapabilityNotice.vue';
import en from '/@fs/${root}../frontend-lib/src/locales/en.yaml';
import cn from '/@fs/${root}../frontend-lib/src/locales/cn.yaml';
const params = new URLSearchParams(location.search);
if (params.get('theme') === 'dark') document.documentElement.classList.add('app-dark');
const state = reactive({status:'idle',error:null,queriedAt:null,capability:{status:'unsupported',reason:'resource_api_missing'}});
const calls = [];
window.fixture = {state,calls,messages:{en,cn}};
createApp({render:()=>h('main', [
 h(QueryNotice,{status:state.status,error:state.error,queriedAt:state.queriedAt,onRefresh:()=>{calls.push('refresh');state.status='loading';}}),
 h(CapabilityNotice,{capability:state.capability}),
])}).use(PrimeVue,{theme:{preset:theme,options:{darkModeSelector:'.app-dark'}}})
.use(createI18n({legacy:false,locale:params.get('lang')||'en',messages:{en,cn}})).mount('#app');`;

before(async () => {
    cacheDirectory = await mkdtemp(join(tmpdir(), 'easytier-eco-shared-'));
    const reservation = createPortReservation();
    await new Promise((resolve, reject) => { reservation.once('error', reject); reservation.listen(0, '127.0.0.1', resolve); });
    const port = reservation.address().port;
    await new Promise((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));
    server = await createServer({
        root, configFile: false, cacheDir: cacheDirectory,
        plugins: [vue(), yaml(), {
            name: 'ecosystem-shared-fixture',
            resolveId(id) { if (id === '/ecosystem-shared-fixture.js') return id; },
            load(id) { if (id === '/ecosystem-shared-fixture.js') return fixture; },
            configureServer(vite) {
                vite.middlewares.use((request, response, next) => {
                    if (request.url?.split('?')[0] !== '/ecosystem-shared-test') return next();
                    response.setHeader('Content-Type', 'text/html');
                    response.end('<!doctype html><meta name="viewport" content="width=device-width, initial-scale=1"><div id="app"></div><script type="module" src="/ecosystem-shared-fixture.js"></script>');
                });
            },
        }],
        server: { host: '127.0.0.1', port, strictPort: true, fs: { allow: [fileURLToPath(new URL('../../', import.meta.url))] } },
    });
    await server.listen();
    base = `http://127.0.0.1:${server.httpServer.address().port}/ecosystem-shared-test`;
    browser = await chromium.launch({ headless: true, executablePath: process.env.WEB_TEST_CHROMIUM || '/usr/bin/chromium' });
});

after(async () => {
    await browser?.close();
    await server?.close();
    if (cacheDirectory) await rm(cacheDirectory, { recursive: true, force: true });
});

async function open(t, { lang = 'en', theme = 'light', width = 768 } = {}) {
    const page = await browser.newPage({ viewport: { width, height: 900 } });
    const errors = [];
    const apiRequests = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('request', request => { if (new URL(request.url()).pathname.startsWith('/api/')) apiRequests.push(request.url()); });
    t.after(async () => { await page.close(); assert.deepEqual(errors, []); assert.deepEqual(apiRequests, []); });
    page.setDefaultTimeout(10000);
    await page.goto(`${base}?lang=${lang}&theme=${theme}`);
    await page.getByRole('button').waitFor();
    return page;
}

test('refresh is an explicit keyboard event; loading prevents duplicates', async t => {
    const page = await open(t);
    const button = page.getByRole('button', { name: 'Refresh' });
    await button.focus();
    await page.keyboard.press('Enter');
    assert.equal(await button.isDisabled(), true);
    await page.keyboard.press('Enter');
    assert.deepEqual(await page.evaluate(() => window.fixture.calls), ['refresh']);
    await page.getByText('Loading…', { exact: true }).waitFor();
    assert.equal(await page.locator('[aria-busy="true"]').count(), 1);
});

test('failed reads preserve a visible prior observation and never show raw diagnostics', async t => {
    const page = await open(t);
    await page.evaluate(() => Object.assign(window.fixture.state, {
        status: 'error', error: { kind: 'network', status: null, message: 'fictional-sensitive-response' }, queriedAt: '2026-10-21T10:00:00Z',
    }));
    assert.match(await page.getByRole('alert').innerText(), /could not be reached/);
    assert.equal(await page.locator('time').getAttribute('datetime'), '2026-10-21T10:00:00Z');
    await page.getByText('Last successful result', { exact: false }).waitFor();
    assert.equal(await page.getByText('fictional-sensitive-response').count(), 0);
    assert.equal(await page.getByRole('button').isEnabled(), true);
    await page.evaluate(() => Object.assign(window.fixture.state, { status: 'ready', error: null }));
    await page.getByText('Loaded on request.', { exact: true }).waitFor();
    assert.equal(await page.getByRole('alert').count(), 0);
    assert.equal(await page.getByText('Last successful result', { exact: false }).count(), 0);
});

test('permission, missing object, timeout and unknown error remain distinct', async t => {
    const page = await open(t);
    for (const [kind, text] of [
        ['forbidden', 'Access was denied.'],
        ['password_change_required', 'Change your password before continuing.'],
        ['not_found', 'The selected object was not found.'],
        ['timeout', 'The request timed out. Refresh manually to try again.'],
        ['invalid_response', 'The response could not be read safely.'],
        ['request_failed', 'The query failed. Refresh manually to try again.'],
    ]) {
        await page.evaluate(kind => Object.assign(window.fixture.state, { status: 'error', error: { kind, status: null } }), kind);
        await page.getByRole('alert').getByText(text, { exact: true }).waitFor();
    }
});

test('unsupported capability is explicit and supported capability does not invent success', async t => {
    const page = await open(t);
    for (const reason of ['external_console', 'node_metadata_api_missing', 'resource_api_missing', 'public_access_api_missing', 'port_mapping_api_missing', 'navigation_api_missing']) {
        await page.evaluate(reason => { window.fixture.state.capability = { status: 'unsupported', reason }; }, reason);
        const expected = await page.evaluate(reason => window.fixture.messages.en.web.ecosystem.unsupported[reason], reason);
        await page.getByText(expected, { exact: true }).waitFor();
        await page.getByText('Not supported', { exact: true }).waitFor();
        const text = await page.locator('.ecosystem-capability-notice').innerText();
        assert.ok(text.length > 30);
        assert.ok(!text.includes('web.ecosystem.'));
        assert.equal(await page.locator('.ecosystem-capability-notice button, .ecosystem-capability-notice input').count(), 0);
    }
    await page.evaluate(() => { window.fixture.state.capability = { status: 'supported' }; });
    await page.locator('.ecosystem-capability-notice').waitFor({ state: 'detached' });
});

test('CN/EN, light/dark and 360/768/1440 layouts use real locale keys without overflow', async t => {
    for (const lang of ['cn', 'en']) {
        for (const theme of ['light', 'dark']) {
            for (const width of [360, 768, 1440]) {
                const page = await open(t, { lang, theme, width });
                await page.getByRole('button', { name: lang === 'cn' ? '刷新' : 'Refresh', exact: true }).waitFor();
                await page.evaluate(() => { window.fixture.state.queriedAt = '2026-10-21T10:00:00Z'; });
                assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
                assert.ok(!(await page.locator('main').innerText()).includes('web.ecosystem.'));
                assert.equal(await page.evaluate(() => document.documentElement.classList.contains('app-dark')), theme === 'dark');
            }
        }
    }
});


test('ecosystem CN/EN leaf keys match and all translated values are nonempty', async t => {
    const page = await open(t);
    const catalogs = await page.evaluate(() => ({ en: window.fixture.messages.en.web.ecosystem, cn: window.fixture.messages.cn.web.ecosystem }));
    function flatten(value, prefix = '') {
        return Object.entries(value).flatMap(([key, item]) => typeof item === 'string'
            ? [[`${prefix}${key}`, item]] : flatten(item, `${prefix}${key}.`));
    }
    const en = flatten(catalogs.en);
    const cn = flatten(catalogs.cn);
    assert.deepEqual(en.map(([key]) => key).sort(), cn.map(([key]) => key).sort());
    assert.ok([...en, ...cn].every(([, value]) => value.trim().length > 0));
});
