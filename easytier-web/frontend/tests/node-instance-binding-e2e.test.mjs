import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { test } from 'node:test';
import { chromium } from 'playwright';

// Only a disposable local fixture is accepted. All success assertions exercise the
// real embedded frontend and HTTP backend. One explicitly labelled test aborts a
// GET to verify transient-failure UI; it does not fabricate a successful response.
const fixturePath = process.env.NODE_ASSETS_E2E_FIXTURE_FILE;
const assetPath = '/api/v1/assets';
const esc = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const idLabel = record => new RegExp(`^${esc(record.name)} · ${record.id.slice(0, 8)}(?: · |$)`);
const scope = fixture => ({ owner_user_id: String(fixture.owner.id), machine_id: fixture.node.machine_id });
const detailURL = fixture => `/#/h/node-assets?owner=${fixture.owner.id}&node=${fixture.node.machine_id}`;
async function json(response, statuses = [200, 201]) {
    assert.ok(statuses.includes(response.status()), `${new URL(response.url()).pathname}: HTTP ${response.status()}; ${await response.text()}`);
    return response.status() === 204 ? undefined : response.json();
}
async function responseFor(page, method, path, action, statuses) {
    const pending = page.waitForResponse(response => response.request().method() === method && new URL(response.url()).pathname === assetPath + path);
    await action(); return json(await pending, statuses);
}
async function select(page, control, label) {
    await control.click(); await page.getByRole('option', { name: label, exact: typeof label === 'string' }).click();
}
async function login(page, account) {
    await page.goto('/');
    await page.locator('#username').fill(account.username);
    await page.locator('input#password').fill(account.password);
    await page.getByRole('button', { name: 'Login', exact: true }).click();
    await page.locator('.console-page').waitFor();
}
function mutateInstance(patch) {
    execFileSync('python3', [resolve('tests/node-instance-binding-fixture.py'), 'mutate', '--fixture', fixturePath], {
        input: JSON.stringify({ action: 'patch', node: 'node', instance: 'ordinary2', patch }), encoding: 'utf8',
    });
}
function isolatedAdmin(action, username) {
    // Test setup/teardown only: a separate temporary actor avoids revoking the
    // shared fixture administrator while the parallel API suite is running.
    return JSON.parse(execFileSync('python3', ['-c', `
import json, sqlite3, sys
f=json.load(open(sys.argv[1])); assert f['isolated'] is True
con=sqlite3.connect(f['database']); con.execute('PRAGMA foreign_keys=ON')
action,name=sys.argv[2:4]
if action=='create':
  password=con.execute('SELECT password FROM users WHERE id=?',(f['admin']['id'],)).fetchone()[0]
  uid=con.execute('INSERT INTO users(username,password) VALUES(?,?)',(name,password)).lastrowid
  group=con.execute("SELECT id FROM groups WHERE name='admins'").fetchone()[0]
  con.execute('INSERT INTO users_groups(user_id,group_id) VALUES(?,?)',(uid,group))
else:
  uid=con.execute('SELECT id FROM users WHERE username=?',(name,)).fetchone()[0]
  con.execute('DELETE FROM users_groups WHERE user_id=?',(uid,))
  if action=='delete': con.execute('DELETE FROM users WHERE id=?',(uid,))
con.commit(); con.close(); print(json.dumps({'id':uid}))
`, fixturePath, action, username], { encoding: 'utf8' }));
}

test('instance binding: source conflicts and R02–R05 regressions against the isolated backend', { timeout: 300000 }, async t => {
    assert.ok(fixturePath, 'Supply the explicit disposable fixture path.');
    const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));
    assert.equal(fixture.isolated, true);
    assert.equal(new URL(fixture.baseURL).hostname, '127.0.0.1');
    const directory = resolve(process.env.NODE_ASSETS_E2E_SCREENSHOTS_DIR || fixture.screenshotsDir);
    await mkdir(directory, { recursive: true });
    const browser = await chromium.launch({ headless: true, executablePath: '/usr/bin/chromium' });
    const context = await browser.newContext({ baseURL: fixture.baseURL, viewport: { width: 1440, height: 1000 } });
    await context.addInitScript(() => localStorage.setItem('lang', 'en'));
    const page = await context.newPage(); page.setDefaultTimeout(15000);
    const suffix = randomUUID().slice(0, 8);
    const report = { startedAt: new Date().toISOString(), checks: [], screenshots: [], pageErrors: [], cleanupErrors: [] };
    const clean = value => [fixture.admin.password, fixture.owner.password].reduce((text, secret) => text.split(secret).join('[REDACTED]'), String(value));
    page.on('pageerror', error => report.pageErrors.push(clean(error.message)));
    const devices = [], types = [];
    let originalNode, temporaryActor;
    let failed = false;
    const get = async (path, params = scope(fixture)) => json(await context.request.get(assetPath + path, { params }));
    const step = async (name, body, extra = {}) => {
        await t.test(name, async () => {
            const started = Date.now();
            try { await body(); report.checks.push({ name, passed: true, durationMs: Date.now() - started, ...extra }); }
            catch (error) { report.checks.push({ name, passed: false, error: clean(error.message), ...extra }); throw error; }
        });
    };
    let created;
    try {
        await login(page, fixture.admin);
        await page.goto(detailURL(fixture)); await page.locator('.assets-list-card').waitFor();
        await step('R02: duplicate instance and type names expose distinct stable short IDs', async () => {
            for (let n = 0; n < 2; n++) {
                types.push(await json(await context.request.post(assetPath + '/device-types', {
                    params: { owner_user_id: fixture.owner.id }, data: { name: `同名类型-${suffix}`, request_id: randomUUID() },
                })));
            }
            await page.getByRole('button', { name: 'Refresh current data', exact: true }).click();
            await page.getByRole('button', { name: 'Add device', exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Add device', exact: true });
            for (const key of ['ordinary', 'ordinary2']) {
                const candidate = dialog.getByRole('radio').filter({ hasText: fixture.instances.node[key].instance_id.slice(0, 8) });
                await candidate.waitFor(); assert.match(await candidate.innerText(), /fixture/);
            }
            await dialog.locator('#asset-device-type').click();
            for (const type of types) await page.getByRole('option', { name: idLabel(type) }).waitFor();
            await page.keyboard.press('Escape');
            await dialog.getByRole('button', { name: 'Gateway', exact: true }).click();
            for (const key of ['mapped', 'mapped2']) {
                const candidate = dialog.getByRole('radio').filter({ hasText: fixture.instances.node[key].instance_id.slice(0, 8) });
                await candidate.waitFor(); assert.match(await candidate.innerText(), /fixture/);
                assert.equal(await candidate.isDisabled(), false, 'Saved disabled/offline instances remain usable asset sources.');
            }
            await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
        });
        await step('V06: changing the saved instance after preview returns a real conflict and preserves draft input', async () => {
            await page.getByRole('button', { name: 'Add device', exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Add device', exact: true });
            await dialog.locator('#asset-device-name').fill(`配置变化设备-${suffix}`);
            await dialog.getByRole('radio').filter({ hasText: fixture.instances.node.ordinary2.instance_id.slice(0, 8) }).click();
            const preview = await responseFor(page, 'POST', '/validation/instance-address',
                () => dialog.getByRole('button', { name: 'Preview changes', exact: true }).click());
            assert.equal(preview.addresses.overlay_ip, '10.70.0.11');
            mutateInstance({ virtual_ipv4: '10.70.0.91' });
            const conflict = await responseFor(page, 'POST', '/devices',
                () => dialog.getByRole('button', { name: 'Confirm and save', exact: true }).click(), [409]);
            assert.match(conflict.code, /conflict|changed|stale|version/);
            assert.equal(await dialog.locator('#asset-device-name').inputValue(), `配置变化设备-${suffix}`);
            await dialog.getByRole('button', { name: 'Refresh instances', exact: true }).click();
            await dialog.getByRole('radio').filter({ hasText: fixture.instances.node.ordinary2.instance_id.slice(0, 8) }).click();
            const updated = await responseFor(page, 'POST', '/validation/instance-address',
                () => dialog.getByRole('button', { name: 'Preview changes', exact: true }).click());
            assert.equal(updated.addresses.overlay_ip, '10.70.0.91');
            created = await responseFor(page, 'POST', '/devices',
                () => dialog.getByRole('button', { name: 'Confirm and save', exact: true }).click());
            devices.push(created.id); assert.equal(created.overlay_ip, '10.70.0.91');
            await dialog.waitFor({ state: 'hidden' });
        });
        await step('R05: Unicode code-point limits match the server; control characters and over-limit values cannot submit', async () => {
            originalNode = (await get('/nodes', { owner_user_id: fixture.owner.id })).items.find(node => node.machine_id === fixture.node.machine_id);
            await page.getByRole('button', { name: 'Edit node properties', exact: true }).click();
            let dialog = page.getByRole('dialog', { name: 'Edit node properties', exact: true });
            await dialog.locator('#asset-node-location').fill('🏠'.repeat(256));
            await dialog.locator('#asset-node-model').fill('📱'.repeat(128));
            assert.equal(await dialog.getByRole('button', { name: 'Save', exact: true }).isDisabled(), false);
            const saved = await responseFor(page, 'PATCH', `/nodes/${fixture.node.machine_id}/metadata`,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
            assert.equal(Array.from(saved.site_location).length, 256); assert.equal(Array.from(saved.device_model).length, 128);
            await dialog.waitFor({ state: 'hidden' });
            await page.getByRole('button', { name: 'Edit node properties', exact: true }).click();
            dialog = page.getByRole('dialog', { name: 'Edit node properties', exact: true });
            await dialog.locator('#asset-node-model').fill('📱'.repeat(129));
            assert.equal(await dialog.getByRole('button', { name: 'Save', exact: true }).isDisabled(), true);
            await dialog.locator('#asset-node-model-error').waitFor();
            await dialog.locator('#asset-node-model').fill('bad\u007fmodel');
            assert.equal(await dialog.getByRole('button', { name: 'Save', exact: true }).isDisabled(), true);
            await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
            await json(await context.request.patch(assetPath + `/nodes/${fixture.node.machine_id}/metadata`, {
                params: { owner_user_id: fixture.owner.id }, data: { site_location: originalNode.site_location, device_model: originalNode.device_model, is_public_server: originalNode.is_public_server },
            }));
        });
        await step('R03: a deleted device reports a resource error without clearing the valid node', async () => {
            const row = page.locator('tr').filter({ has: page.locator(`[data-asset-device-id="${created.id}"]`) });
            await row.getByRole('button', { name: `Edit device ${created.name}`, exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Edit device', exact: true });
            await dialog.locator('#asset-device-model').fill('stale editor');
            await json(await context.request.delete(assetPath + '/devices/' + created.id, { params: scope(fixture) }), [204]);
            await responseFor(page, 'PUT', '/devices/' + created.id,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click(), [404]);
            await dialog.getByRole('alert').waitFor();
            assert.equal(await dialog.locator('#asset-device-model').inputValue(), 'stale editor');
            assert.ok(page.url().includes(fixture.node.machine_id));
            await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
            await responseFor(page, 'GET', '/devices',
                () => page.getByRole('button', { name: 'Refresh current data', exact: true }).click());
            await page.locator('.assets-list-card').waitFor();
            await row.waitFor({ state: 'hidden' });
        });
        await step('R03 transient network failure keeps the last successful rows and manual retry recovers', async () => {
            const count = await page.locator('[data-asset-device-id]').count(); assert.ok(count > 0);
            const pattern = '**/api/v1/assets/devices?*';
            await page.route(pattern, route => route.abort('failed'));
            await page.getByRole('button', { name: 'Search', exact: true }).click();
            await page.locator('.assets-alert').waitFor();
            assert.equal(await page.locator('[data-asset-device-id]').count(), count);
            await page.unroute(pattern);
            await responseFor(page, 'GET', '/devices', () => page.getByRole('button', { name: 'Retry', exact: true }).click());
            await page.locator('.assets-alert').waitFor({ state: 'hidden' });
        }, { injectedFailure: 'Aborted one real device-list request; no successful response was mocked.' });
        await step('R04: returning from a scrolled detail page resets the node-list scroll position', async () => {
            await page.setViewportSize({ width: 390, height: 844 });
            await page.evaluate(() => {
                for (let element = document.querySelector('.node-assets-page')?.parentElement; element; element = element.parentElement) element.scrollTop = 700;
                window.scrollTo(0, 700);
            });
            const before = await page.evaluate(() => Math.max(scrollY, ...[...document.querySelectorAll('*')].map(element => element.scrollTop)));
            assert.ok(before > 0, 'The fixture detail page must actually be scrolled.');
            // Avoid Playwright auto-scrolling the header button into view before
            // activation; dispatching its click exercises the actual route action.
            await page.getByRole('button', { name: 'Back to network nodes', exact: true }).evaluate(button => button.click());
            await page.waitForURL('**/#/h/deviceList');
            await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
            const after = await page.evaluate(() => Math.max(scrollY, ...[...document.querySelectorAll('*')].map(element => element.scrollTop)));
            assert.equal(after, 0);
            await page.setViewportSize({ width: 1440, height: 1000 });
        });
        await step('R03: revoking a separate administrator in the real database clears cross-account forms and data', async () => {
            temporaryActor = `ui-binding-admin-${suffix}`; isolatedAdmin('create', temporaryActor);
            const revokedContext = await browser.newContext({ baseURL: fixture.baseURL });
            await revokedContext.addInitScript(() => localStorage.setItem('lang', 'en'));
            const revoked = await revokedContext.newPage(); revoked.setDefaultTimeout(15000);
            revoked.on('pageerror', error => report.pageErrors.push(clean(error.message)));
            try {
                await login(revoked, { username: temporaryActor, password: fixture.admin.password });
                await revoked.goto(detailURL(fixture));
                await revoked.locator('.assets-list-card').waitFor();
                await revoked.getByRole('button', { name: 'Manage device types', exact: true }).click();
                const dialog = revoked.getByRole('dialog', { name: 'Manage device types', exact: true });
                await dialog.getByRole('button', { name: 'Add type', exact: true }).click();
                await dialog.locator('#asset-type-name').fill(`revoked-${suffix}`);
                isolatedAdmin('revoke', temporaryActor);
                await responseFor(revoked, 'POST', '/device-types',
                    () => dialog.getByRole('button', { name: 'Save', exact: true }).click(), [403]);
                await dialog.waitFor({ state: 'hidden' });
                await revoked.locator('.assets-list-card').waitFor({ state: 'hidden' });
                assert.equal(await revoked.locator('#asset-account').count(), 0);
                assert.equal(await revoked.locator('[data-asset-device-id]').count(), 0);
                assert.ok(!revoked.url().includes(fixture.node.machine_id));
            } finally { await revokedContext.close(); }
        });
        assert.deepEqual(report.pageErrors, []);
    } catch (error) {
        failed = true;
        const path = resolve(directory, 'node-instance-binding-failure.png');
        await page.screenshot({ path, fullPage: true }).catch(() => {}); report.screenshots.push(path);
        throw new Error(clean(error.message));
    } finally {
        for (const id of devices) {
            const result = await context.request.delete(assetPath + '/devices/' + id, { params: scope(fixture) }).catch(() => null);
            if (!result || ![204, 404].includes(result.status())) report.cleanupErrors.push(`device ${id}`);
        }
        for (const type of types) {
            const result = await context.request.delete(assetPath + '/device-types/' + type.id, { params: { owner_user_id: fixture.owner.id } }).catch(() => null);
            if (!result || ![204, 404].includes(result.status())) report.cleanupErrors.push(`type ${type.id}`);
        }
        try { mutateInstance({ virtual_ipv4: fixture.instances.node.ordinary2.ipv4 }); } catch { report.cleanupErrors.push('instance restore'); }
        if (temporaryActor) { try { isolatedAdmin('delete', temporaryActor); } catch { report.cleanupErrors.push('temporary actor'); } }
        if (originalNode) await context.request.patch(assetPath + `/nodes/${fixture.node.machine_id}/metadata`, {
            params: { owner_user_id: fixture.owner.id }, data: { site_location: originalNode.site_location, device_model: originalNode.device_model, is_public_server: originalNode.is_public_server },
        }).catch(() => report.cleanupErrors.push('node metadata restore'));
        report.passed = !failed && report.checks.every(check => check.passed) && report.cleanupErrors.length === 0; report.finishedAt = new Date().toISOString();
        await writeFile(resolve(directory, 'node-instance-binding-e2e-report.json'), JSON.stringify(report, null, 2));
        await browser.close(); if (!failed) assert.deepEqual(report.cleanupErrors, []);
    }
});
