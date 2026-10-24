// Reproduce unsaved script loss and idle requests against the real isolated Web.
// Usage: NODE_ASSETS_E2E_FIXTURE_FILE=... node ui-gap-probe.mjs
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const require = createRequire(resolve(repo, 'easytier-web/frontend/package.json'));
const { chromium } = require('playwright');
const fixture = JSON.parse(await readFile(process.env.NODE_ASSETS_E2E_FIXTURE_FILE, 'utf8'));
assert.equal(fixture.isolated, true);
assert.equal(new URL(fixture.baseURL).hostname, '127.0.0.1');
const directory = '/tmp/integrity-ui-gaps';
await mkdir(directory, { recursive: true });
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
try {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
    await context.addInitScript(() => localStorage.setItem('lang', 'cn'));
    const response = await context.request.post(`${fixture.baseURL}/api/v1/auth/login`, {
        data: { username: fixture.owner.username, password: createHash('md5').update(fixture.owner.password).digest('hex') },
    });
    assert.equal(response.status(), 200);
    const page = await context.newPage();
    const dialogs = [];
    page.on('dialog', async dialog => { dialogs.push(dialog.type()); await dialog.dismiss(); });
    await page.goto(fixture.baseURL + '/#/h/automation');
    await page.getByRole('button', { name: '脚本库', exact: true }).click();
    await page.locator('#automation-script-name').fill('尚未保存的关键脚本');
    await page.getByRole('button', { name: '添加步骤', exact: true }).first().click();
    await page.screenshot({ path: directory + '/script-before.png', fullPage: true });
    const before = await page.locator('#automation-script-name').inputValue();
    await page.getByRole('button', { name: '执行记录', exact: true }).click();
    await page.getByRole('button', { name: '脚本库', exact: true }).click();
    const after = await page.locator('#automation-script-name').inputValue();
    await page.screenshot({ path: directory + '/script-after.png', fullPage: true });
    const result = { before, after, confirmationDialogs: dialogs.length, draftLost: before !== after };
    console.log('Script draft after switching tabs:', JSON.stringify(result));
    assert.equal(result.draftLost, true, 'Probe expects the audited gap; revise after fixing the product');
    assert.equal(dialogs.length, 0);
    const requests = [];
    page.on('request', request => {
        const url = new URL(request.url());
        if (url.pathname.startsWith('/api/v1/')) requests.push({ method: request.method(), path: url.pathname });
    });
    await page.clock.install();
    await page.goto(fixture.baseURL + '/#/h/devices');
    await page.locator('#console-auto-refresh').waitFor();
    await new Promise(resolve => setTimeout(resolve, 500));
    const start = requests.length;
    for (let i = 0; i < 3; i++) {
        await page.clock.fastForward(10000);
        await new Promise(resolve => setTimeout(resolve, 250));
    }
    result.idleRequests = requests.slice(start);
    console.log('No clicks, 30 simulated seconds:', JSON.stringify(result.idleRequests));
    assert.ok(result.idleRequests.length > 0, 'Probe expects default idle polling');
    await writeFile(directory + '/result.json', JSON.stringify(result, null, 2));
    await context.close();
} finally { await browser.close(); }
