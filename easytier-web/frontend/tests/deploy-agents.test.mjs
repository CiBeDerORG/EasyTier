import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';

let server, browser;
const base = 'http://127.0.0.1:5199';
before(async () => {
    server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', 'preview', '--host', '127.0.0.1', '--port', '5199', '--strictPort'], { stdio: 'pipe' });
    for (let i = 0; i < 100; i++) {
        if (server.exitCode !== null) throw new Error('Vite preview exited');
        if (await fetch(base).then(r => r.ok).catch(() => false)) break;
        await delay(100);
    }
    const executablePath = process.env.CHROMIUM_PATH || (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);
    browser = await chromium.launch({ headless: true, executablePath });
});
after(async () => { await browser?.close(); server?.kill(); });

test('device access, installation and task queries require explicit clicks', async t => {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
    t.after(() => context.close());
    await context.addInitScript(() => localStorage.setItem('lang', 'cn'));
    const page = await context.newPage();
    await page.clock.install();
    const errors = [], consoleMessages = [], requests = [], tasks = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', e => consoleMessages.push(e.text()));
    const id = '00000000-0000-0000-0000-000000000001';
    const agent = { id, alias: '飞牛 NAS', endpoint: 'https://192.168.1.20:7014', updated_at: '2026-10-24T00:00:00Z' };
    const secret = 'chosen ! " 空白 password';
    await page.route('**/api/v1/**', async route => {
        const req = route.request(), path = new URL(req.url()).pathname.replace('/api/v1', '');
        requests.push({ method: req.method(), path, data: req.postDataJSON() });
        let result = {};
        if (path === '/console-info') result = { webhook_auth: false, username: 'test', config_server_protocol: 'tcp', config_server_port: 7008 };
        else if (path === '/networks') result = { networks: [] };
        else if (path === '/blocked-devices') result = { blocked: [] };
        else if (path === '/admin/users/context') result = { available: true, can_manage_users: false };
        else if (path === '/deploy/agents') result = [agent];
        else if (path.endsWith('/probe')) {
            agent.agent_id = 'agent-identity';
            agent.identity = { agent_id: agent.agent_id, hostname: 'NAS', platform: 'fnos', arch: 'amd64', software_root: '/vol1/software', observed_at: agent.updated_at };
            result = agent.identity;
        } else if (path.endsWith('/catalog')) {
            agent.catalog = { revision: 'pinned-bundle', applications: [{ id: 'portainer', name: 'Portainer CE', supported: true, variants: ['docker-compose'], configuration: [{ key: 'PORTAINER_ADMIN_PASSWORD', secret: true, default: '' }] }] };
            result = agent.catalog;
        } else if (path.endsWith('/discover')) {
            agent.services = { observed_at: agent.updated_at, providers: { docker: { status: 'ok' } }, services: [{ id: 'docker:a', provider: 'docker', name: 'existing-app', state: 'exited', project: 'existing', observed_at: agent.updated_at }] };
            result = agent.services;
        } else if (path.endsWith('/tasks')) {
            if (req.method() === 'POST') {
                const data = req.postDataJSON();
                assert.equal(data.secrets.PORTAINER_ADMIN_PASSWORD, secret);
                assert.equal(data.catalog_revision, 'pinned-bundle');
                tasks.push({ task_id: data.task_id, application_id: data.application_id, action: data.action, status: 'accepted', queried_at: agent.updated_at });
                result = tasks[0];
            } else result = tasks;
        } else if (path.endsWith('/refresh')) { tasks[0].status = 'succeeded'; result = tasks[0]; }
        else if (path.endsWith('/fetch-logs')) { tasks[0].logs = 'Installer finished\n'; result = { text: tasks[0].logs, next_offset: 19 }; }
        await route.fulfill({ json: result });
    });
    await page.goto(`${base}/#/h/applications`);
    await page.getByRole('heading', { name: '应用与服务', exact: true }).waitFor();
    await page.getByRole('button', { name: '连接并验证', exact: true }).waitFor();
    await delay(1200);
    const afterInitial = requests.length;
    assert.equal(requests.filter(r => r.method === 'POST').length, 0, 'mount must not probe an Agent');
    await page.clock.fastForward(60_000);
    await delay(100);
    assert.equal(requests.length, afterInitial, 'idle page must not poll even the controller');
    await page.getByRole('button', { name: '连接并验证', exact: true }).click();
    await page.getByText('NAS · fnos / amd64', { exact: false }).waitFor();
    await page.getByRole('button', { name: '发现服务', exact: true }).click();
    await page.getByRole('cell', { name: 'existing-app', exact: true }).waitFor();
    await page.getByRole('cell', { name: 'exited', exact: true }).waitFor();
    await page.getByRole('button', { name: '读取应用目录', exact: true }).click();
    await page.getByRole('button', { name: 'Portainer CE', exact: true }).click();
    await page.locator('#enabled-PORTAINER_ADMIN_PASSWORD').check();
    await page.locator('#value-PORTAINER_ADMIN_PASSWORD').fill(secret);
    const afterInvalidPaste = await page.locator('#value-PORTAINER_ADMIN_PASSWORD').evaluate(element => {
        const data = new DataTransfer(); data.setData('text', 'first\nsecond');
        const event = new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true });
        element.dispatchEvent(event);
        return { prevented: event.defaultPrevented, value: element.value };
    });
    assert.equal(afterInvalidPaste.prevented, true, 'unsupported pasted input must not be silently normalized');
    assert.equal(afterInvalidPaste.value, secret);
    await page.getByRole('button', { name: '安装', exact: true }).click();
    await page.getByRole('button', { name: '查询状态', exact: true }).waitFor();
    await delay(150);
    const afterInstall = requests.length;
    await page.clock.fastForward(60_000);
    await delay(100);
    assert.equal(requests.length, afterInstall, 'an accepted task must not start status/log polling');
    assert.equal(requests.filter(r => r.path.endsWith('/tasks') && r.method === 'POST').length, 1);
    await page.getByRole('button', { name: '查询状态', exact: true }).click();
    await page.getByText('已完成', { exact: true }).waitFor();
    await page.getByRole('button', { name: '读取日志', exact: true }).click();
    await page.getByText('Installer finished', { exact: false }).waitFor();
    assert.deepEqual(errors, []);
    assert.ok(!consoleMessages.some(message => message.includes(secret)), 'secret must not be logged by the client');
    if (process.env.WEB_SCREENSHOT_DIR) {
        await page.getByRole('button', { name: 'Close', exact: true }).click();
        await page.getByRole('dialog').waitFor({ state: 'hidden' });
        await page.getByRole('heading', { name: '应用与服务', exact: true }).click();
        await page.evaluate(() => window.scrollTo(0, 0));
        await mkdir(process.env.WEB_SCREENSHOT_DIR, { recursive: true });
        await page.screenshot({ path: `${process.env.WEB_SCREENSHOT_DIR}/deploy-agents-desktop.png`, fullPage: true });
        await page.setViewportSize({ width: 390, height: 844 });
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1), 'mobile must not overflow');
        await page.screenshot({ path: `${process.env.WEB_SCREENSHOT_DIR}/deploy-agents-mobile.png`, fullPage: true });
    }
});

// The existing shell refresh preference must never enable timers on these routes.
test('manual pages share console themes, language, mobile layout and navigation', async t => {
    for (const lang of ['cn', 'en']) for (const theme of ['light', 'dark']) {
        const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
        t.after(() => context.close());
        await context.addInitScript(({ lang, theme }) => {
            localStorage.setItem('lang', lang);
            localStorage.setItem('console-theme-mode', theme);
            localStorage.setItem('webConsole.autoRefresh', 'true');
        }, { lang, theme });
        const page = await context.newPage();
        await page.clock.install();
        const requests = [], errors = [];
        page.on('pageerror', error => errors.push(error.message));
        await page.route('**/api/v1/**', async route => {
            const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
            requests.push(path);
            let result = {};
            if (path === '/console-info') result = { webhook_auth: false, username: 'test', config_server_protocol: 'tcp', config_server_port: 7008 };
            else if (path === '/networks') result = { networks: [] };
            else if (path === '/blocked-devices') result = { blocked: [] };
            else if (path === '/admin/users/context') result = { available: true, can_manage_users: true };
            else if (path === '/deploy/agents') result = [];
            else if (path === '/automation/context') result = { available: true, runner_configured: false, actor: { id: 1, username: 'test' }, can_manage_others: false, protocol_version: 1 };
            else if (path === '/automation/devices') result = { items: [], issues: [] };
            else if (path === '/automation/scripts' || path === '/automation/runs') result = { items: [] };
            await route.fulfill({ json: result });
        });
        const agentTitle = lang === 'cn' ? '应用与服务' : 'Applications & services';
        const automationTitle = lang === 'cn' ? '自动化中心' : 'Automation center';
        await page.goto(`${base}/#/h/applications`);
        await page.getByRole('heading', { name: agentTitle, exact: true }).waitFor();
        await page.getByRole('heading', { name: lang === 'cn' ? '接入你的第一台主机' : 'Connect your first host', exact: true }).waitFor();
        assert.equal(await page.locator('html').evaluate(el => el.classList.contains('app-dark')), theme === 'dark');
        assert.equal(await page.locator('#console-auto-refresh').count(), 0);
        await delay(200);
        let idle = requests.length;
        await page.clock.fastForward(60_000);
        await delay(100);
        assert.equal(requests.length, idle);
        await page.locator('#console-sidebar').getByRole('menuitem').filter({ hasText: automationTitle }).click();
        await page.getByRole('heading', { name: automationTitle, exact: true }).waitFor();
        await delay(100);
        idle = requests.length;
        await page.clock.fastForward(60_000);
        await delay(100);
        assert.equal(requests.length, idle, 'automation entry must not scan or poll');
        assert.equal(await page.getByText(lang === 'cn' ? '规划中' : 'Planned', { exact: true }).count(), 0);
        await page.getByRole('button', { name: lang === 'cn' ? '脚本库' : 'Script library', exact: true }).waitFor();
        await page.getByRole('heading', { name: lang === 'cn' ? '还没有登记实体设备' : 'No registered devices', exact: true }).waitFor();
        if (process.env.WEB_SCREENSHOT_DIR) {
            await mkdir(process.env.WEB_SCREENSHOT_DIR, { recursive: true });
            await page.screenshot({ path: `${process.env.WEB_SCREENSHOT_DIR}/automation-${lang}-${theme}-desktop.png`, fullPage: true });
        }
        await page.setViewportSize({ width: 390, height: 844 });
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1), 'automation mobile must not overflow');
        if (process.env.WEB_SCREENSHOT_DIR) await page.screenshot({ path: `${process.env.WEB_SCREENSHOT_DIR}/automation-${lang}-${theme}-mobile.png`, fullPage: true });
        await page.locator('.mobile-nav-toggle').click();
        await page.locator('.console-mobile-nav').getByRole('menuitem').filter({ hasText: agentTitle }).click();
        await page.getByRole('heading', { name: agentTitle, exact: true }).waitFor();
        await page.getByRole('button', { name: lang === 'cn' ? '添加设备' : 'Add device', exact: true }).first().click();
        await page.getByRole('dialog').waitFor();
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1), 'add-device dialog must fit mobile');
        assert.deepEqual(errors, []);
    }
});
