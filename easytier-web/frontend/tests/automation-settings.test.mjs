import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { mkdir } from 'node:fs/promises';
import { chromium } from 'playwright';

const base = 'http://127.0.0.1:5207';
let browser, server;
before(async () => {
    server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', 'preview', '--host', '127.0.0.1', '--port', '5207', '--strictPort'], { stdio: 'pipe' });
    for (let i = 0; i < 100; i++) {
        if (await fetch(base).then(response => response.ok).catch(() => false)) break;
        await delay(100);
    }
    browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
});
after(async () => { await browser?.close(); server?.kill(); });

async function open(t, { admin = true, mobile = false, configured = false } = {}) {
    const context = await browser.newContext({ viewport: mobile ? { width: 390, height: 844 } : { width: 1440, height: 1100 } });
    t.after(() => context.close());
    await context.addInitScript(() => { localStorage.setItem('lang', 'cn'); localStorage.setItem('webConsole.autoRefresh', 'false'); });
    const page = await context.newPage(); page.setDefaultTimeout(10000); await page.clock.install();
    const errors = [], calls = [];
    page.on('pageerror', error => errors.push(error.message));
    t.after(() => assert.deepEqual(errors, []));
    const state = {
        saved: { endpoint: configured ? 'http://192.168.1.20:8766/' : '', token_configured: configured, configured,
            source: configured ? 'environment' : 'none', revision: 0, updated_at: null },
        payload: null, conflict: false, checkFailure: false,
    };
    await page.route('**/api/v1/**', async route => {
        const request = route.request(), path = new URL(request.url()).pathname.replace('/api/v1', '');
        const body = request.postDataJSON(), method = request.method();
        calls.push({ path, body, method });
        let result = {};
        if (path === '/console-info') result = { username: 'tester', webhook_auth: false, config_server_protocol: 'tcp', config_server_port: 7008 };
        else if (path === '/admin/users/context') result = { available: true, can_manage_users: admin };
        else if (path === '/automation/context') result = { available: true, runner_configured: state.saved.configured,
            can_manage_others: admin, can_manage_settings: admin, actor: { id: 1, username: 'tester' }, protocol_version: 1 };
        else if (path === '/assets/accounts') result = { items: [{ id: 1, username: 'tester' }] };
        else if (path === '/automation/devices') result = { items: [], issues: [] };
        else if (path === '/assets/discovery/events') { await route.abort(); return; }
        else if (path === '/automation/settings') {
            if (method === 'PUT') {
                state.payload = body;
                if (state.conflict) {
                    await route.fulfill({ status: 409, json: { code: 'runner_settings_changed', message: '设置已被其他管理员修改，请重新读取后保存。' } }); return;
                }
                state.saved = { endpoint: body.endpoint, token_configured: true, configured: true, source: 'database',
                    revision: state.saved.revision + 1, updated_at: '2026-10-24T12:00:00Z' };
            }
            result = state.saved;
        } else if (path === '/automation/settings/check') {
            if (state.checkFailure) {
                await route.fulfill({ status: 502, json: { code: 'runner_unreachable', message: '无法确认执行器响应。' } }); return;
            }
            result = { reachable: true, executor_id: '00000000-0000-4000-8000-000000000123', protocol_version: 1, checked_at: '2026-10-24T12:01:00Z' };
        } else if (path === '/automation/scripts' || path === '/automation/runs') result = { items: [] };
        else if (path === '/networks') result = { networks: [] };
        else if (path === '/blocked-devices') result = { blocked: [] };
        await route.fulfill({ json: result });
    });
    await page.goto(`${base}/#/h/automation`);
    await page.getByRole('heading', { name: '自动化中心', exact: true }).waitFor();
    await page.getByRole('navigation', { name: '自动化中心' }).waitFor();
    return { page, calls, state };
}

test('administrator saves exact credentials, sees immediate configuration and probes only on click', async t => {
    const { page, calls, state } = await open(t);
    assert.equal(calls.filter(call => call.path.startsWith('/automation/settings')).length, 0);
    await page.getByRole('navigation', { name: '自动化中心' }).getByRole('button', { name: '设置', exact: true }).click();
    await page.getByLabel('服务器地址', { exact: true }).fill('http://192.168.1.20:8766');
    const secret = '  exact ! $ ` token with spaces 123456789  ';
    await page.getByLabel('执行器 Token', { exact: true }).fill(secret);
    assert.equal(await page.getByRole('button', { name: '检查已保存连接' }).isDisabled(), true);
    await page.getByRole('button', { name: '保存设置', exact: true }).click();
    await page.getByText('执行器设置已保存并生效。', { exact: true }).waitFor();
    assert.equal(state.payload.token, secret);
    assert.equal(await page.getByLabel('执行器 Token', { exact: true }).count(), 0);
    await page.getByText('手机执行器尚未配置。', { exact: false }).waitFor({ state: 'hidden' });
    assert.equal(calls.filter(call => call.path.endsWith('/settings/check')).length, 0);
    const before = calls.filter(call => call.path.startsWith('/automation/settings')).length;
    await page.clock.fastForward(120000); await delay(100);
    assert.equal(calls.filter(call => call.path.startsWith('/automation/settings')).length, before);
    await page.getByRole('button', { name: '检查已保存连接' }).click();
    await page.getByText('接口连接成功', { exact: true }).waitFor();
    assert.equal(calls.filter(call => call.path.endsWith('/settings/check')).length, 1);
    await page.getByRole('button', { name: '保存设置', exact: true }).click();
    await page.getByText('执行器设置已保存并生效。', { exact: true }).waitFor();
    assert.equal(state.payload.token_action, 'preserve'); assert.equal('token' in state.payload, false);
    await mkdir('/tmp/automation-settings-screenshots', { recursive: true });
    await page.clock.runFor(400);
    await page.screenshot({ path: '/tmp/automation-settings-screenshots/desktop.png', fullPage: true, animations: 'disabled' });
});

test('mobile settings retain drafts across tabs and keep input after a version conflict', async t => {
    const { page, state } = await open(t, { mobile: true, configured: true });
    const tabs = page.getByRole('navigation', { name: '自动化中心' });
    await tabs.getByRole('button', { name: '设置', exact: true }).click();
    await page.getByText('当前使用环境变量。', { exact: false }).waitFor();
    await page.getByLabel('服务器地址', { exact: true }).fill('http://192.168.1.21:8766');
    await tabs.getByRole('button', { name: '执行记录', exact: true }).click();
    await tabs.getByRole('button', { name: '设置', exact: true }).click();
    assert.equal(await page.getByLabel('服务器地址', { exact: true }).inputValue(), 'http://192.168.1.21:8766');
    state.conflict = true;
    await page.getByRole('button', { name: '保存设置', exact: true }).click();
    await page.getByText('设置已被其他管理员修改，请重新读取后保存。', { exact: true }).waitFor();
    assert.equal(await page.getByLabel('服务器地址', { exact: true }).inputValue(), 'http://192.168.1.21:8766');
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    await mkdir('/tmp/automation-settings-screenshots', { recursive: true });
    await page.clock.runFor(400);
    await page.evaluate(() => { document.activeElement?.blur(); window.scrollTo(0, 0); });
    await page.screenshot({ path: '/tmp/automation-settings-screenshots/mobile.png', fullPage: true, animations: 'disabled' });
    await page.getByRole('button', { name: '放弃修改并重新读取' }).click();
    await page.getByLabel('服务器地址', { exact: true }).waitFor();
    await page.waitForFunction(() => document.querySelector('#executor-url').value.endsWith('20:8766/'));
});

test('connection errors remain visible without automatic retry', async t => {
    const { page, calls, state } = await open(t, { configured: true });
    await page.getByRole('navigation', { name: '自动化中心' }).getByRole('button', { name: '设置', exact: true }).click();
    state.checkFailure = true;
    await page.getByRole('button', { name: '检查已保存连接' }).click();
    await page.getByText('连接检查失败：无法确认执行器响应。', { exact: true }).waitFor();
    await page.clock.fastForward(120000); await delay(100);
    assert.equal(calls.filter(call => call.path.endsWith('/settings/check')).length, 1);
});

test('ordinary accounts have no global settings entry or requests', async t => {
    const { page, calls } = await open(t, { admin: false });
    assert.equal(await page.getByRole('button', { name: '设置', exact: true }).count(), 0);
    await page.clock.fastForward(120000); await delay(100);
    assert.equal(calls.filter(call => call.path.startsWith('/automation/settings')).length, 0);
});
