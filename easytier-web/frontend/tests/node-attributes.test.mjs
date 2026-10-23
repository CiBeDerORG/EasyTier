import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';

const base = process.env.WEB_TEST_URL || 'http://127.0.0.1:5199';
let browser;
let server;

before(async () => {
    if (!process.env.WEB_TEST_URL) {
        server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', 'preview', '--host', '127.0.0.1', '--port', '5199', '--strictPort'], { stdio: 'pipe' });
        let ready = false;
        for (let i = 0; i < 100; i++) {
            if (server.exitCode !== null) throw new Error('Vite exited before starting');
            if (await fetch(base).then(response => response.ok).catch(() => false)) {
                ready = true;
                break;
            }
            await delay(100);
        }
        assert.ok(ready, 'Vite preview is ready');
    }
    browser = await chromium.launch({ headless: true, executablePath: process.env.WEB_TEST_CHROMIUM || undefined });
});

after(async () => {
    await browser?.close();
    server?.kill();
});

const uuid = n => ({ part1: 0, part2: 0, part3: 0, part4: n });
const id = n => `00000000-0000-0000-0000-${n.toString(16).padStart(12, '0')}`;
const attributesPath = n => `/machines/${id(n)}/attributes`;
const isMachinesResponse = response => new URL(response.url()).pathname === '/api/v1/machines';
const editButton = scope => scope.getByRole('button', { name: 'Edit attributes', exact: true });
const dialogFor = page => page.getByRole('dialog', { name: 'Edit attributes', exact: true });
const locationInput = page => page.locator('#device-site-location');
const modelInput = page => page.locator('#device-model');

function fixture() {
    return {
        machines: ['OpenWrt gateway', 'Office router'].map((hostname, i) => ({
            client_url: `tcp://198.51.100.${i + 10}:11010`,
            info: { hostname, machine_id: uuid(i + 1), easytier_version: '2.4.5', report_time: '2026-10-22 12:00:00', running_network_instances: [] },
            location: { country: 'Legacy country', region: '', city: 'Legacy city' },
            networks: [], online: true,
            site_location: i ? 'Office rack' : 'Home rack', device_model: i ? 'Office model' : 'Router model',
            connection_ip: `198.51.100.${i + 10}`, ip_country: 'Source country', ip_region: 'Source region', ip_city: 'Source city',
            ip_location_status: 'resolved', ip_observed_at: '2026-10-22T12:00:00Z',
        })),
        writes: [], saveStatus: 200, saveGate: null, externalConsole: false,
    };
}

async function open(t, { view = 'card', mobile = false, configure = () => {} } = {}) {
    const state = fixture();
    configure(state);
    const context = await browser.newContext({ viewport: mobile ? { width: 390, height: 844 } : { width: 1440, height: 960 } });
    t.after(() => context.close());
    await context.addInitScript(view => {
        localStorage.setItem('lang', 'en');
        if (view) localStorage.setItem('deviceList.viewMode', view);
    }, view);
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    t.after(() => assert.deepEqual(errors, [], 'no uncaught browser errors'));
    await page.route('**/api_meta.js', route => route.fulfill({ contentType: 'text/javascript', body: 'window.apiMeta = { api_host: "" };' }));
    await page.route('**/api/v1/**', async route => {
        const request = route.request();
        const path = new URL(request.url()).pathname.replace('/api/v1', '');
        const method = request.method();
        let result = {};
        if (method !== 'GET') state.writes.push({ path, method, payload: request.postDataJSON() });
        if (path === '/machines') result = { machines: state.machines };
        else if (path === '/console-info') result = { username: 'test-user', config_server_protocol: 'udp', config_server_port: 22020, webhook_auth: state.externalConsole };
        else if (path === '/networks') result = { networks: [] };
        else if (path === '/blocked-devices') result = { blocked: [] };
        else if (path === '/summary') result = { device_count: state.machines.length };
        else if (/^\/machines\/[^/]+\/attributes$/.test(path)) {
            assert.equal(method, 'PUT');
            if (state.saveGate) await state.saveGate;
            if (state.saveStatus !== 200) return route.fulfill({ status: state.saveStatus, json: { message: 'Attribute update failed' } });
            const machine = state.machines.find(machine => id(machine.info.machine_id.part4) === path.split('/')[2]);
            if (!machine) return route.fulfill({ status: 404, json: { message: 'Machine not found' } });
            const payload = request.postDataJSON();
            result = { site_location: payload.site_location || null, device_model: payload.device_model || null };
            Object.assign(machine, result);
        }
        await route.fulfill({ json: result });
    });
    await page.goto(`${base}/#/h/deviceList`);
    const scope = page.locator(mobile ? '.mobile-list-item' : view === 'card' ? '.device-card' : '.desktop-list tbody > tr').filter({ hasText: 'OpenWrt gateway' });
    await scope.waitFor();
    if (!state.externalConsole) await editButton(scope).waitFor();
    return { page, state, scope };
}

async function edit(page, scope) {
    await editButton(scope).click();
    const dialog = dialogFor(page);
    await dialog.waitFor();
    return dialog;
}

async function save(page) {
    await dialogFor(page).getByRole('button', { name: 'Save', exact: true }).click();
    await dialogFor(page).waitFor({ state: 'hidden' });
}

test('card edit is additive, separates automatic location and cancels without a request', async t => {
    const { page, state, scope } = await open(t);
    for (const name of ['Set Alias', 'Delete Device', 'Manage OpenWrt gateway']) {
        await scope.getByRole('button', { name, exact: true }).waitFor();
    }
    assert.match(await scope.textContent(), /Legacy country.*Legacy city/s);
    assert.match(await scope.textContent(), /Home rack/);
    const dialog = await edit(page, scope);
    assert.equal(await locationInput(page).inputValue(), 'Home rack');
    assert.equal(await modelInput(page).inputValue(), 'Router model');
    assert.equal(await dialog.locator('input').count(), 2, 'only manual fields are editable');
    assert.match(await dialog.textContent(), /198\.51\.100\.10/);
    assert.match(await dialog.textContent(), /Source country.*Source region.*Source city/s);
    await locationInput(page).fill('Unsaved');
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
    assert.deepEqual(state.writes, []);
    await edit(page, scope);
    assert.equal(await locationInput(page).inputValue(), 'Home rack', 'reopening discards cancelled draft');
});

test('save trims manual fields, persists clear, and keeps the selected view and search', async t => {
    const { page, state, scope } = await open(t);
    const search = page.getByRole('textbox', { name: 'Search name or address', exact: true });
    await search.fill('OpenWrt');
    await edit(page, scope);
    await locationInput(page).fill('  Upstairs cabinet  ');
    await modelInput(page).fill('  GL-MT6000  ');
    await save(page);
    assert.deepEqual(state.writes, [{ path: attributesPath(1), method: 'PUT', payload: { site_location: 'Upstairs cabinet', device_model: 'GL-MT6000' } }]);
    await scope.getByText('Upstairs cabinet', { exact: false }).waitFor();
    assert.equal(await search.inputValue(), 'OpenWrt');
    assert.equal(await page.locator('.device-card').count(), 1);
    assert.equal(await page.evaluate(() => localStorage.getItem('deviceList.viewMode')), 'card');
    await edit(page, scope);
    assert.equal(await modelInput(page).inputValue(), 'GL-MT6000');
    await locationInput(page).fill('   ');
    await modelInput(page).fill('');
    await save(page);
    assert.deepEqual(state.writes.at(-1).payload, { site_location: '', device_model: '' });
    await page.reload();
    const refreshed = page.locator('.device-card').filter({ hasText: 'OpenWrt gateway' });
    await edit(page, refreshed);
    assert.equal(await locationInput(page).inputValue(), '');
    assert.equal(await modelInput(page).inputValue(), '');
});

test('background list responses do not overwrite the open draft or leak it to another node', async t => {
    const { page, state, scope } = await open(t);
    const dialog = await edit(page, scope);
    await locationInput(page).fill('Draft location');
    await modelInput(page).fill('Draft model');
    state.machines[0].site_location = 'Changed elsewhere';
    const response = await page.waitForResponse(isMachinesResponse);
    await response.finished();
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    assert.equal(await locationInput(page).inputValue(), 'Draft location');
    assert.equal(await modelInput(page).inputValue(), 'Draft model');
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await dialog.waitFor({ state: 'hidden' });
    await edit(page, page.locator('.device-card').filter({ hasText: 'Office router' }));
    assert.equal(await locationInput(page).inputValue(), 'Office rack');
    assert.equal(await modelInput(page).inputValue(), 'Office model');
    assert.deepEqual(state.writes, []);
});

for (const status of [503, 404]) {
    test(`${status} keeps the draft visible for correction or retry`, async t => {
        const { page, state, scope } = await open(t, { configure: state => { state.saveStatus = status; } });
        const dialog = await edit(page, scope);
        await locationInput(page).fill('Keep this draft');
        const response = page.waitForResponse(response => new URL(response.url()).pathname.endsWith('/attributes'));
        await dialog.getByRole('button', { name: 'Save', exact: true }).click();
        assert.equal((await response).status(), status);
        await dialog.getByRole('alert').waitFor();
        assert.equal(await locationInput(page).inputValue(), 'Keep this draft');
        assert.ok(await dialog.isVisible());
        state.saveStatus = 200;
        await save(page);
        assert.equal(state.writes.length, 2);
    });
}

test('an in-flight save disables closing, editing, and duplicate submissions', async t => {
    let release;
    const gate = new Promise(resolve => { release = resolve; });
    t.after(() => release());
    const { page, state, scope } = await open(t, { configure: state => { state.saveGate = gate; } });
    const dialog = await edit(page, scope);
    await locationInput(page).fill('Pending location');
    const request = page.waitForRequest(request => new URL(request.url()).pathname.endsWith('/attributes'));
    await dialog.getByRole('button', { name: 'Save', exact: true }).click();
    await request;
    assert.ok(await locationInput(page).isDisabled());
    assert.ok(await modelInput(page).isDisabled());
    assert.ok(await dialog.getByRole('button', { name: 'Cancel', exact: true }).isDisabled());
    assert.ok(await dialog.getByRole('button', { name: 'Save', exact: true }).isDisabled());
    await page.keyboard.press('Escape');
    await page.keyboard.press('Enter');
    assert.ok(await dialog.isVisible());
    assert.equal(state.writes.length, 1);
    release();
    await dialog.waitFor({ state: 'hidden' });
    assert.equal(state.writes.length, 1);
});

test('Unicode limits count code points and reject excessive length and controls', async t => {
    const { page, state, scope } = await open(t);
    await edit(page, scope);
    await locationInput(page).fill('🏠'.repeat(256));
    await modelInput(page).fill('📱'.repeat(128));
    await save(page);
    assert.equal([...state.writes[0].payload.site_location].length, 256);
    assert.equal([...state.writes[0].payload.device_model].length, 128);
    for (const [location, model] of [['🏠'.repeat(257), 'Router'], ['Home', '📱'.repeat(129)], ['Home\u0001rack', 'Router']]) {
        const dialog = await edit(page, scope);
        await locationInput(page).fill(location);
        await modelInput(page).fill(model);
        const saveButton = dialog.getByRole('button', { name: 'Save', exact: true });
        if (await saveButton.isEnabled()) await saveButton.click();
        await dialog.locator('[role="alert"], [aria-invalid="true"]').first().waitFor();
        assert.equal(state.writes.length, 1, 'invalid input sends no update');
        await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
        await dialog.waitFor({ state: 'hidden' });
    }
});

test('default table keeps expansion after an attribute save', async t => {
    const { page, scope } = await open(t, { view: null });
    assert.equal(await page.locator('.device-card').count(), 0);
    await scope.getByRole('button').first().click();
    await page.locator('.desktop-list .device-details').waitFor();
    await edit(page, scope);
    await locationInput(page).fill('Rack B');
    await save(page);
    assert.ok(await page.locator('.desktop-list .device-details').isVisible());
    assert.equal(await page.locator('.device-card').count(), 0);
});

test('mobile list exposes the same edit flow without horizontal dialog overflow', async t => {
    const { page, state, scope } = await open(t, { mobile: true });
    const dialog = await edit(page, scope);
    const bounds = await dialog.boundingBox();
    assert.ok(bounds && bounds.x >= 0 && bounds.x + bounds.width <= 391);
    await locationInput(page).fill('Mobile edit');
    await save(page);
    assert.equal(state.writes[0].payload.site_location, 'Mobile edit');
    assert.ok(await scope.locator(':scope > details > summary').isVisible(), 'existing details entry is retained');
});

test('external console mode does not offer unsupported attribute editing', async t => {
    const { page } = await open(t, { configure: state => { state.externalConsole = true; } });
    assert.equal(await editButton(page).count(), 0);
});

test('card details keep all six fields visible and attribute editing beside the gear above them', async t => {
    const fullVersion = '2.4.5-main-e307dadb-release';
    const lastSeen = '2026-10-22T13:45:36.123456Z';
    const { scope } = await open(t, { configure: state => {
        state.machines[0].info.easytier_version = fullVersion;
        state.machines[0].info.running_network_instances = [uuid(100), uuid(101)];
        state.machines[0].last_seen = lastSeen;
    } });
    const details = scope.locator('.card-details');
    await details.waitFor();
    assert.deepEqual(await details.locator('.detail-label').allTextContents(), [
        'Hostname', 'Public IP', 'Network Count', 'Last Seen', 'Version', 'Machine ID',
    ]);
    const expected = {
        hostname: 'OpenWrt gateway',
        'public-ip': 'tcp://198.51.100.10:11010',
        'running-networks': '2',
        'last-seen': lastSeen,
        version: fullVersion,
        'machine-id': id(1),
    };
    for (const [field, value] of Object.entries(expected)) {
        const cell = details.locator(`.${field} .detail-value`);
        assert.ok(await cell.isVisible(), `${field} is visible without expansion`);
        assert.equal((await cell.textContent()).trim(), value);
    }
    assert.equal(await details.locator('details, summary').count(), 0, 'card information needs no expansion');
    const gear = await scope.getByRole('button', { name: 'Manage OpenWrt gateway', exact: true }).boundingBox();
    const edit = await editButton(scope).boundingBox();
    const body = await details.boundingBox();
    assert.ok(gear && edit && body);
    assert.ok(Math.abs((gear.y + gear.height / 2) - (edit.y + edit.height / 2)) <= 2, 'edit and gear share one row');
    assert.ok(edit.x >= gear.x + gear.width && edit.x - (gear.x + gear.width) <= 12, 'edit sits beside the gear');
    assert.ok(Math.max(gear.y + gear.height, edit.y + edit.height) <= body.y + 1, 'both actions sit above the six fields');
});

test('card Last Seen falls back from recorded time to report time to a dash for online and offline nodes', async t => {
    const cases = [true, false].flatMap(online => [
        { online, lastSeen: '2026-10-22T14:01:02Z', reportTime: '2026-10-21 11:00:00', expected: '2026-10-22T14:01:02Z' },
        { online, lastSeen: null, reportTime: '2026-10-21 11:00:00', expected: '2026-10-21 11:00:00' },
        { online, lastSeen: null, reportTime: undefined, expected: '—' },
    ]);
    const { page } = await open(t, { configure: state => {
        const source = state.machines[0];
        state.machines = cases.map((item, index) => ({
            ...source,
            online: item.online,
            last_seen: item.lastSeen,
            info: {
                ...source.info,
                machine_id: uuid(index + 1),
                hostname: index === 0 ? 'OpenWrt gateway' : `Timestamp node ${index}`,
                report_time: item.reportTime,
            },
        }));
    } });
    for (const [index, item] of cases.entries()) {
        const hostname = index === 0 ? 'OpenWrt gateway' : `Timestamp node ${index}`;
        const card = page.locator('.device-card').filter({ hasText: hostname });
        const lastSeen = card.locator('.card-details .last-seen .detail-value');
        assert.ok(await lastSeen.isVisible(), `Last Seen remains visible when online=${item.online}`);
        assert.equal((await lastSeen.textContent()).trim(), item.expected);
    }
});

test('card details wrap long IPv6, UUID and version values without overflow', async t => {
    const address = 'tcp://[ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535';
    const machineId = 'ffffffff-ffff-ffff-ffff-ffffffffffff';
    const fullVersion = '2.4.5-main-e307dadb-release-candidate-linux-aarch64';
    const { page, scope } = await open(t, { configure: state => {
        state.machines[0].client_url = address;
        state.machines[0].info.machine_id = { part1: 0xffffffff, part2: 0xffffffff, part3: 0xffffffff, part4: 0xffffffff };
        state.machines[0].info.easytier_version = fullVersion;
        state.machines[0].last_seen = '2026-10-22T14:01:02.123456789+00:00';
    } });
    await page.setViewportSize({ width: 1100, height: 960 });
    const details = scope.locator('.card-details');
    assert.equal((await details.locator('.public-ip .detail-value').textContent()).trim(), address);
    assert.equal((await details.locator('.machine-id .detail-value').textContent()).trim(), machineId);
    assert.equal((await details.locator('.version .detail-value').textContent()).trim(), fullVersion);
    const layout = await details.evaluate(element => {
        const card = element.closest('.device-card');
        const bounds = card.getBoundingClientRect();
        return {
            cardFits: card.scrollWidth <= card.clientWidth + 1,
            values: [...element.querySelectorAll('.detail-value')].map(value => {
                const box = value.getBoundingClientRect();
                const text = document.createRange();
                text.selectNodeContents(value);
                const rendered = text.getBoundingClientRect();
                return {
                    value: value.textContent,
                    fits: value.scrollWidth <= value.clientWidth + 1
                        && box.left >= bounds.left && box.right <= bounds.right
                        && rendered.right <= box.right + 1 && rendered.bottom <= box.bottom + 1,
                    ellipsis: getComputedStyle(value).textOverflow === 'ellipsis',
                };
            }),
        };
    });
    assert.ok(layout.cardFits, 'the card has no horizontal overflow');
    for (const value of layout.values) {
        assert.ok(value.fits, `complete value remains inside the card: ${value.value}`);
        assert.equal(value.ellipsis, false, `value is not truncated: ${value.value}`);
    }
});
