import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { test } from 'node:test';
import { chromium } from 'playwright';

/**
 * Real-server browser verification. This script neither builds nor starts servers,
 * seeds a database, mocks HTTP responses, or uses the existing deployment's login.
 * Run against a disposable database that already has an administrator, another
 * account, and registered (including offline) nodes with saved ordinary and gateway network instances:
 *
 * NODE_ASSETS_E2E_FIXTURE_FILE=/absolute/path/fixture.json \
 *   node --test tests/node-assets-e2e.test.mjs
 *
 * Fixture: { baseURL, admin: { username, password }, owner: { id, username, password },
 *   node: { machine_id, label }, offlineNode: { machine_id, label }, screenshotsDir? }
 * The fixture must declare isolated: true. Individual values can be overridden
 * through the NODE_ASSETS_E2E_* variables below.
 * The fixture is read only when its explicit path is supplied; never print it.
 */
async function configuration() {
    const env = process.env;
    const fixture = env.NODE_ASSETS_E2E_FIXTURE_FILE
        ? JSON.parse(await readFile(env.NODE_ASSETS_E2E_FIXTURE_FILE, 'utf8')) : {};
    assert.equal(fixture.isolated, true, 'Only an explicitly isolated test fixture may be used.');
    const config = {
        baseURL: env.NODE_ASSETS_E2E_BASE_URL || fixture.baseURL,
        admin: {
            username: env.NODE_ASSETS_E2E_ADMIN_USERNAME || fixture.admin?.username,
            password: env.NODE_ASSETS_E2E_ADMIN_PASSWORD || fixture.admin?.password,
        },
        owner: {
            id: Number(env.NODE_ASSETS_E2E_OWNER_ID || fixture.owner?.id),
            username: env.NODE_ASSETS_E2E_OWNER_USERNAME || fixture.owner?.username,
            password: env.NODE_ASSETS_E2E_OWNER_PASSWORD || fixture.owner?.password,
        },
        node: {
            machine_id: env.NODE_ASSETS_E2E_NODE_ID || fixture.node?.machine_id,
            label: env.NODE_ASSETS_E2E_NODE_LABEL || fixture.node?.label,
        },
        offlineNode: {
            machine_id: env.NODE_ASSETS_E2E_OFFLINE_NODE_ID || fixture.offlineNode?.machine_id,
            label: env.NODE_ASSETS_E2E_OFFLINE_NODE_LABEL || fixture.offlineNode?.label,
        },
        instances: fixture.instances,
        legacy: fixture.legacy,
        screenshotsDir: resolve(env.NODE_ASSETS_E2E_SCREENSHOTS_DIR
            || fixture.screenshotsDir || '.node-assets-e2e-artifacts'),
    };
    assert.ok(config.baseURL && config.admin.username && config.admin.password,
        'Provide an isolated server URL and temporary administrator credentials through the explicit fixture or environment.');
    assert.ok(Number.isSafeInteger(config.owner.id) && config.owner.username && config.node.machine_id,
        'Provide the target owner ID, username and registered node machine ID.');
    assert.ok(config.offlineNode.machine_id, 'Provide an offline registered node for offline management coverage.');
    assert.ok(config.owner.password, 'Provide the temporary owner password to independently confirm the node is offline.');
    const url = new URL(config.baseURL);
    assert.equal(url.hostname, '127.0.0.1', 'This mutating suite is limited to the isolated local test server.');
    assert.ok(!url.username && !url.password, 'Use the explicit administrator fields, not credentials embedded in the URL.');
    return config;
}

const assetPath = '/api/v1/assets';
const scopeParams = (config, node = config.node) => ({
    owner_user_id: String(config.owner.id), machine_id: node.machine_id,
});
const detailsPath = (config, node = config.node) =>
    `/#/h/node-assets?owner=${config.owner.id}&node=${encodeURIComponent(node.machine_id)}`;
function machineId(value) {
    if (typeof value === 'string') return value;
    if (!value) return '';
    const hex = [1, 2, 3, 4].map(part => Number(value[`part${part}`] ?? 0).toString(16).padStart(8, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

async function jsonResponse(response, statuses = [200, 201]) {
    const status = response.status();
    if (!statuses.includes(status)) throw new Error(
        `${new URL(response.url()).pathname}: HTTP ${status}; ${await response.text()}`);
    return status === 204 ? undefined : response.json();
}

async function api(context, config, path, node = config.node) {
    const params = ['/context', '/accounts'].includes(path) ? {}
        : ['/nodes', '/device-types'].includes(path) ? { owner_user_id: String(config.owner.id) }
            : scopeParams(config, node);
    return jsonResponse(await context.request.get(`${assetPath}${path}`, { params }));
}

async function actionResponse(page, method, path, action, statuses = [200, 201]) {
    const pending = page.waitForResponse(response => response.request().method() === method
        && new URL(response.url()).pathname === `${assetPath}${path}`);
    await action();
    return jsonResponse(await pending, statuses);
}

async function selectOption(page, control, label) {
    await control.click();
    await page.getByRole('option', { name: label, exact: typeof label === 'string' }).click();
}

function deviceRow(page, device) {
    return page.locator('tr').filter({ has: page.locator(`[data-asset-device-id="${device.id}"]`) });
}

function servicesPanel(page, name) {
    return page.getByRole('region', { name: `Services on ${name}`, exact: true });
}

async function ensureExpanded(page, device) {
    const row = deviceRow(page, device);
    await row.waitFor();
    const toggle = row.locator('button[aria-expanded]');
    if (await toggle.getAttribute('aria-expanded') !== 'true') await toggle.click();
    await servicesPanel(page, device.name).waitFor();
}

async function login(page, config, account = config.admin) {
    await page.goto('/');
    await page.locator('#username').fill(account.username);
    await page.locator('input#password').fill(account.password);
    await page.getByRole('button', { name: 'Login', exact: true }).click();
    await page.locator('.console-page').waitFor();
}

const escaped = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const namedId = record => new RegExp(`^${escaped(record.name)} · ${record.id.slice(0, 8)}(?: · |$)`);
async function chooseInstance(page, dialog, config, role, node = config.node, instanceId) {
    const url = new URL(`${assetPath}/network-instances`, config.baseURL);
    for (const [key, value] of Object.entries({ ...scopeParams(config, node), role })) url.searchParams.set(key, value);
    const candidates = await jsonResponse(await page.context().request.get(url.href));
    const candidate = candidates.items.find(item => item.selectable && (!instanceId || item.instance_id === instanceId));
    assert.ok(candidate, `A selectable ${role} persisted instance is required on the isolated node.`);
    await dialog.getByRole('radio').filter({ hasText: candidate.instance_id.slice(0, 8) }).click();
    return candidate;
}
async function createDevice(page, config, input, tracked, node = config.node) {
    await page.getByRole('button', { name: 'Add device', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add device', exact: true });
    assert.equal((await dialog.locator('#asset-device-group').innerText()).trim(), 'Directly under this node',
        'New devices must default to direct ownership even when a group filter is selected.');
    await dialog.locator('#asset-device-name').fill(input.name);
    await dialog.locator('#asset-device-model').fill(input.model || 'Pixel 测试型号');
    if (input.type) await selectOption(page, dialog.locator('#asset-device-type'), namedId(input.type));
    let candidate;
    if (input.group) {
        await selectOption(page, dialog.locator('#asset-device-group'), namedId(input.group));
        const rule = input.group.binding.snapshot.rules.find(rule => input.mapped ? !!rule.mapped_cidr : true);
        assert.ok(rule, 'The selected gateway must have the requested rule.');
        await selectOption(page, dialog.locator('#asset-address-rule'), rule.mapped_cidr ? `${rule.real_cidr} → ${rule.mapped_cidr}` : rule.real_cidr);
        await dialog.locator('#asset-address-host').fill(String(input.hostOffset ?? 20));
    } else {
        candidate = await chooseInstance(page, dialog, config, 'ordinary', node, input.instance_id);
        if (input.lan_ip) await dialog.locator('#asset-direct-lan').fill(input.lan_ip);
    }
    if (input.remarks) await dialog.locator('#asset-device-remarks').fill(input.remarks);
    const preview = await actionResponse(page, 'POST', '/validation/instance-address',
        () => dialog.getByRole('button', { name: 'Preview changes', exact: true }).click());
    assert.equal(preview.can_save, true);
    const created = await actionResponse(page, 'POST', '/devices',
        () => dialog.getByRole('button', { name: 'Confirm and save', exact: true }).click());
    tracked.devices.push({ id: created.id, node });
    assert.equal(created.owner_user_id, config.owner.id, 'Administrator-created records belong to the selected owner.');
    assert.equal(created.machine_id, node.machine_id);
    assert.equal(created.gateway_group_id, input.group?.id ?? null);
    assert.equal(created.address_mode, input.group ? 'gateway_rule' : 'direct_instance');
    assert.equal(created.overlay_ip, preview.addresses.overlay_ip);
    assert.equal(created.lan_ip, preview.addresses.lan_ip);
    if (candidate) assert.equal(created.overlay_ip, candidate.ipv4);
    await dialog.waitFor({ state: 'hidden' });
    await ensureExpanded(page, created);
    return created;
}

async function createService(page, device, input) {
    const panel = servicesPanel(page, device.name);
    await panel.getByRole('button', { name: 'Add service', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Add service', exact: true });
    await dialog.getByLabel('Service name', { exact: false }).fill(input.name);
    await dialog.getByLabel('Service port', { exact: false }).fill(String(input.port));
    if (input.udp) await dialog.getByRole('checkbox', { name: 'UDP', exact: true }).check();
    if (input.access_url) await dialog.getByLabel('Access link', { exact: true }).fill(input.access_url);
    if (input.icon) {
        await dialog.getByRole('combobox', { name: 'Service icon', exact: true }).waitFor();
        await selectOption(page, dialog.locator('[id$="-icon"]'), input.icon);
    }
    const service = await actionResponse(page, 'POST', `/devices/${device.id}/services`,
        () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
    await dialog.waitFor({ state: 'hidden' });
    await panel.getByRole('row').filter({ hasText: service.name }).waitFor();
    return service;
}

test('real backend: node asset management, offline nodes, and Chinese responsive screenshots', { timeout: 360000 }, async t => {
    const config = await configuration();
    await mkdir(config.screenshotsDir, { recursive: true });
    const suffix = randomUUID().slice(0, 8);
    const names = {
        group: `测试网关组-${suffix}`, type: `测试手机类型-${suffix}`, renamedType: `演示手机类型-${suffix}`,
        direct: `演示手机-${suffix}`, grouped: `演示 NAS-${suffix}`, offline: `离线节点设备-${suffix}`,
        service: `演示网页服务-${suffix}`, editedService: `演示控制台-${suffix}`, cascadeService: `待级联删除服务-${suffix}`,
    };
    const browser = await chromium.launch({ headless: true,
        executablePath: process.env.NODE_ASSETS_E2E_CHROMIUM || process.env.WEB_TEST_CHROMIUM || '/usr/bin/chromium' });
    const context = await browser.newContext({ baseURL: config.baseURL, viewport: { width: 1440, height: 1100 } });
    await context.addInitScript(() => localStorage.setItem('lang', 'en'));
    const page = await context.newPage();
    page.setDefaultTimeout(15000);
    let lastDeviceQuery;
    page.on('request', request => {
        const url = new URL(request.url());
        if (request.method() === 'GET' && url.pathname === `${assetPath}/devices`) lastDeviceQuery = url.searchParams;
    });
    const tracked = { devices: [], groups: [], types: [] };
    const report = { startedAt: new Date().toISOString(), checks: [], screenshots: [], pageErrors: [], layoutIssues: [], cleanupErrors: [] };
    const redact = value => [config.admin.password, config.owner.password]
        .reduce((text, secret) => text.split(secret).join('[REDACTED]'), String(value));
    page.on('pageerror', error => report.pageErrors.push(redact(error.message)));
    let group, type, directDevice, groupedDevice, service;
    let failed = false;
    async function step(label, work) {
        t.diagnostic(label);
        const started = Date.now();
        try {
            await work();
            report.checks.push({ name: label, passed: true, durationMs: Date.now() - started });
        } catch (error) {
            report.checks.push({ name: label, passed: false, error: redact(error.message), durationMs: Date.now() - started });
            throw error;
        }
    }
    try {
        await step('Administrator selects another account and a registered node', async () => {
            await login(page, config);
            const capability = await api(context, config, '/context');
            assert.equal(capability.can_manage_others, true, 'Fixture administrator must have cross-account asset authorization.');
            assert.notEqual(capability.actor.id, config.owner.id, 'The test owner must be another account.');
            await page.goto('/#/h/node-assets');
            await selectOption(page, page.locator('#asset-account'), config.owner.username);
            const nodes = (await api(context, config, '/nodes')).items;
            const node = nodes.find(item => item.machine_id === config.node.machine_id);
            assert.ok(node, 'Fixture node must be visible to the selected owner.');
            const label = `${node.alias || node.hostname || node.machine_id} · ${node.machine_id.slice(0, 8)}`;
            await selectOption(page, page.locator('#asset-node'), label);
            await page.locator('.assets-list-card').waitFor();
            await page.getByText(`Managing account: ${config.owner.username}`, { exact: true }).waitFor();
            assert.ok(page.url().includes(`node=${config.node.machine_id}`));
        });

        await step('Gateway manager requires a persisted gateway instance and preserves metadata editing', async () => {
            await page.getByRole('button', { name: 'Manage gateway groups', exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Manage gateway groups', exact: true });
            await dialog.getByRole('button', { name: 'Add gateway group', exact: true }).click();
            await dialog.locator('#gateway-editor-name').fill(names.group);
            const candidate = await chooseInstance(page, dialog, config, 'gateway', config.node, config.instances?.node?.mapped?.instance_id);
            const preview = await actionResponse(page, 'POST', '/validation/binding',
                () => dialog.getByRole('button', { name: 'Preview changes', exact: true }).click());
            assert.equal(preview.can_save, true);
            group = await actionResponse(page, 'POST', '/gateway-groups',
                () => dialog.getByRole('button', { name: 'Confirm and save', exact: true }).click());
            tracked.groups.push(group.id);
            assert.deepEqual(group.subnets.slice().sort(), [...new Set(candidate.rules.map(rule => rule.real_cidr))].sort());
            assert.equal(group.overlay_ip, candidate.ipv4);
            assert.equal(group.binding.state, 'current');
            await dialog.getByRole('button', { name: `Edit gateway group ${names.group}`, exact: true }).click();
            await dialog.locator('#gateway-editor-remarks').fill('显式测试资料；网段来自已绑定实例。');
            assert.equal(await dialog.getByRole('textbox', { name: 'LAN subnet 1', exact: true }).count(), 0,
                'A bound gateway must not expose a manual subnet override.');
            group = await actionResponse(page, 'PUT', `/gateway-groups/${group.id}`,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
            assert.equal(group.remarks, '显式测试资料；网段来自已绑定实例。');
            await dialog.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
        });

        await step('Add-device Gateway role creates a gateway and highlights it without counting it as an entity device', async () => {
            const before = (await api(context, config, '/devices')).total;
            await page.getByRole('button', { name: 'Add device', exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Add device', exact: true });
            await dialog.getByRole('button', { name: 'Gateway', exact: true }).click();
            await dialog.locator('#gateway-editor-name').fill(`普通代理网关-${suffix}`);
            await chooseInstance(page, dialog, config, 'gateway', config.node, config.instances.node.proxy.instance_id);
            const preview = await actionResponse(page, 'POST', '/validation/binding',
                () => dialog.getByRole('button', { name: 'Preview changes', exact: true }).click());
            assert.equal(preview.can_save, true);
            const result = await actionResponse(page, 'POST', '/gateway-groups',
                () => dialog.getByRole('button', { name: 'Confirm and save', exact: true }).click());
            tracked.groups.push(result.id);
            const manager = page.getByRole('dialog', { name: 'Manage gateway groups', exact: true });
            await manager.locator('.group-item.highlighted').filter({ hasText: result.id.slice(0, 8) }).waitFor();
            assert.equal((await api(context, config, '/devices')).total, before);
            assert.equal(result.binding.snapshot.rules[0].mapped_cidr, null);
            await manager.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
        });

        await step('Custom device type can be created and renamed', async () => {
            await page.getByRole('button', { name: 'Manage device types', exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Manage device types', exact: true });
            await dialog.getByRole('button', { name: 'Add type', exact: true }).click();
            await dialog.locator('#asset-type-name').fill(names.type);
            type = await actionResponse(page, 'POST', '/device-types',
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
            tracked.types.push(type.id);
            await dialog.getByRole('button', { name: `Edit type ${names.type}`, exact: true }).click();
            await dialog.locator('#asset-type-name').fill(names.renamedType);
            type = await actionResponse(page, 'PUT', `/device-types/${type.id}`,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
            assert.equal(type.name, names.renamedType);
            await dialog.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
        });

        await step('Search is explicit; creating outside current filters resets them and expands the new direct device', async () => {
            await selectOption(page, page.locator('#asset-group'), namedId(group));
            await selectOption(page, page.locator('#asset-type'), namedId(type));
            await page.locator('#asset-keyword').fill(`cannot-match-${suffix}`);
            await actionResponse(page, 'GET', '/devices',
                () => page.getByRole('button', { name: 'Search', exact: true }).click());
            await page.getByText('No devices match the current filters', { exact: true }).waitFor();
            directDevice = await createDevice(page, config, { name: names.direct, type }, tracked);
            assert.equal(await page.locator('#asset-keyword').inputValue(), '');
            assert.equal(lastDeviceQuery.has('group_id'), false, 'A device outside the old group filter must clear that filter.');
            assert.equal(lastDeviceQuery.has('type_id'), false, 'Creating outside the old filters must also reset the type filter.');
            assert.equal((await page.locator('#asset-group').innerText()).trim(), 'All devices');
            assert.equal((await page.locator('#asset-type').innerText()).trim(), 'All types');
            let listRequests = 0;
            const count = request => {
                if (request.method() === 'GET' && new URL(request.url()).pathname === `${assetPath}/devices`) listRequests++;
            };
            page.on('request', count);
            await page.locator('#asset-keyword').fill(`no-match-${suffix}`);
            await page.locator('#asset-keyword').press('Tab');
            // Flush input/change handlers and Vue's render cycle; no mocked API or polling timer.
            await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
            assert.equal(listRequests, 0, 'Changing a search field must not submit a device query automatically.');
            await deviceRow(page, directDevice).waitFor();
            await actionResponse(page, 'GET', '/devices',
                () => page.getByRole('button', { name: 'Search', exact: true }).click());
            assert.equal(listRequests, 1);
            page.off('request', count);
            await page.getByText('No devices match the current filters', { exact: true }).waitFor();
            await page.locator('#asset-keyword').fill('');
            await actionResponse(page, 'GET', '/devices',
                () => page.getByRole('button', { name: 'Search', exact: true }).click());
            await ensureExpanded(page, directDevice);
        });

        await step('Grouped device derives mapped addresses from its rule; multiple service lists stay expanded', async () => {
            groupedDevice = await createDevice(page, config, { name: names.grouped, group, type,
                model: 'NAS 测试型号', hostOffset: 20, mapped: true }, tracked);
            assert.equal(groupedDevice.overlay_ip, '10.60.0.20');
            assert.equal(groupedDevice.lan_ip, '192.168.60.20');
            assert.equal(groupedDevice.binding.instance_id, group.binding.instance_id);
            assert.equal(groupedDevice.warnings.length, 0);
            await ensureExpanded(page, directDevice);
            await servicesPanel(page, groupedDevice.name).waitFor();
            await servicesPanel(page, directDevice.name).waitFor();
        });

        await step('Service CRUD uses real API; dual protocols, HTTP link, icon, and conflict handling', async () => {
            service = await createService(page, directDevice, { name: names.service, port: 8080, udp: true,
                access_url: `${config.baseURL.replace(/\/$/, '')}/#/h/node-assets`, icon: 'Website' });
            assert.deepEqual(service.protocols, ['tcp', 'udp']);
            assert.equal(service.icon, 'globe');
            let row = servicesPanel(page, directDevice.name).getByRole('row').filter({ hasText: service.name });
            const link = row.getByRole('link');
            assert.equal(await link.getAttribute('target'), '_blank');
            assert.match(await link.getAttribute('rel'), /noopener/);
            assert.match(await link.getAttribute('rel'), /noreferrer/);
            assert.match(await link.getAttribute('href'), /^https?:\/\//);
            assert.equal(await row.locator('.pi-globe').count(), 1);
            await row.getByRole('button', { name: 'Edit', exact: true }).click();
            let dialog = page.getByRole('dialog', { name: 'Edit service', exact: true });
            await dialog.getByLabel('Service name', { exact: false }).fill(names.editedService);
            await dialog.getByLabel('Notes', { exact: true }).fill('真实后端测试服务');
            service = await actionResponse(page, 'PUT', `/services/${service.id}`,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
            await dialog.waitFor({ state: 'hidden' });
            row = servicesPanel(page, directDevice.name).getByRole('row').filter({ hasText: service.name });
            await row.waitFor();
            assert.equal(service.remarks, '真实后端测试服务');
            await servicesPanel(page, directDevice.name).getByRole('button', { name: 'Add service', exact: true }).click();
            dialog = page.getByRole('dialog', { name: 'Add service', exact: true });
            await dialog.getByLabel('Service name', { exact: false }).fill(`冲突服务-${suffix}`);
            await dialog.getByLabel('Service port', { exact: false }).fill('8080');
            await actionResponse(page, 'POST', `/devices/${directDevice.id}/services`,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click(), [409]);
            assert.equal(await dialog.getByLabel('Service name', { exact: false }).inputValue(), `冲突服务-${suffix}`);
            await dialog.getByRole('alert').waitFor();
            await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
            assert.equal((await api(context, config, `/devices/${directDevice.id}/services`)).items.length, 1);
        });

        await step('Device editing preserves ownership; reopening a collapsed service panel does not reopen an old add dialog', async () => {
            const row = deviceRow(page, directDevice);
            await row.getByRole('button', { name: `Edit device ${directDevice.name}`, exact: true }).click();
            const dialog = page.getByRole('dialog', { name: 'Edit device', exact: true });
            assert.equal(await dialog.locator('#asset-device-group').getAttribute('role'), null);
            assert.equal((await dialog.locator('#asset-device-group').innerText()).trim(), 'Directly under this node');
            await dialog.locator('#asset-device-model').fill('已编辑的演示型号');
            directDevice = await actionResponse(page, 'PUT', `/devices/${directDevice.id}`,
                () => dialog.getByRole('button', { name: 'Save', exact: true }).click());
            await dialog.waitFor({ state: 'hidden' });
            await row.getByRole('button', { name: `Add a service to ${directDevice.name}`, exact: true }).click();
            await page.getByRole('dialog', { name: 'Add service', exact: true })
                .getByRole('button', { name: 'Cancel', exact: true }).click();
            await row.locator('button[aria-expanded="true"]').click();
            await row.locator('button[aria-expanded="false"]').click();
            await servicesPanel(page, directDevice.name).waitFor();
            assert.equal(await page.getByRole('dialog', { name: 'Add service', exact: true }).count(), 0);
        });

        await step('Referenced groups and types cannot be deleted', async () => {
            await page.getByRole('button', { name: 'Manage gateway groups', exact: true }).click();
            let dialog = page.getByRole('dialog', { name: 'Manage gateway groups', exact: true });
            await dialog.getByRole('button', { name: `Delete gateway group ${group.name}`, exact: true }).click();
            await actionResponse(page, 'DELETE', `/gateway-groups/${group.id}`,
                () => dialog.getByRole('button', { name: 'Delete permanently', exact: true }).click(), [409]);
            await dialog.locator('.delete-confirm .error').waitFor();
            await dialog.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
            await page.getByRole('button', { name: 'Manage device types', exact: true }).click();
            dialog = page.getByRole('dialog', { name: 'Manage device types', exact: true });
            await dialog.getByRole('button', { name: `Delete type ${type.name}`, exact: true }).click();
            await actionResponse(page, 'DELETE', `/device-types/${type.id}`,
                () => dialog.getByRole('button', { name: 'Delete permanently', exact: true }).click(), [409]);
            await dialog.locator('.delete-confirm .error').waitFor();
            await dialog.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
        });

        await step('Chinese desktop and 390px screenshots show the saved data and service form', async () => {
            const chinese = await browser.newContext({ baseURL: config.baseURL,
                storageState: await context.storageState(), viewport: { width: 1440, height: 1100 } });
            await chinese.addInitScript(() => localStorage.setItem('lang', 'cn'));
            try {
                const visual = await chinese.newPage();
                await visual.goto(detailsPath(config));
                await deviceRow(visual, directDevice).waitFor();
                await deviceRow(visual, directDevice).locator('button[aria-expanded="false"]').click();
                await visual.locator('.service-table').getByText(service.name, { exact: true }).waitFor();
                await visual.evaluate(() => document.fonts.ready);
                const desktop = resolve(config.screenshotsDir, 'node-assets-desktop-cn.png');
                await visual.screenshot({ path: desktop, fullPage: true, animations: 'disabled' });
                report.screenshots.push(desktop);
                const desktopWidth = await visual.evaluate(() => ({ viewport: innerWidth, document: document.documentElement.scrollWidth }));
                if (desktopWidth.document > desktopWidth.viewport + 1) report.layoutIssues.push(
                    `Desktop document overflows horizontally (${desktopWidth.document}px / ${desktopWidth.viewport}px); tables must scroll internally.`);
                const screenshot = async filename => {
                    const path = resolve(config.screenshotsDir, filename);
                    await visual.screenshot({ path, fullPage: false, animations: 'disabled' });
                    report.screenshots.push(path);
                };
                await visual.getByRole('button', { name: '管理网关组', exact: true }).click();
                let manager = visual.getByRole('dialog', { name: '管理网关组', exact: true });
                await manager.locator('.group-item').first().waitFor();
                await screenshot('node-binding-gateway-manager-cn.png');
                await manager.getByRole('button', { name: '新增网关组', exact: true }).click();
                await manager.getByRole('radio').first().waitFor();
                await screenshot('node-binding-add-gateway-cn.png');
                await manager.locator('.p-dialog-footer').getByRole('button', { name: '关闭', exact: true }).click();
                await visual.getByRole('button', { name: '管理设备类型', exact: true }).click();
                manager = visual.getByRole('dialog', { name: '管理设备类型', exact: true });
                await manager.getByText(type.name, { exact: true }).waitFor();
                await screenshot('node-binding-type-manager-cn.png');
                await manager.locator('.p-dialog-footer').getByRole('button', { name: '关闭', exact: true }).click();
                await visual.getByRole('button', { name: '新增设备', exact: true }).click();
                const newDevice = visual.getByRole('dialog', { name: '新增设备', exact: true });
                await newDevice.locator('#asset-device-name').fill('手机演示：填写最后一段地址');
                await selectOption(visual, newDevice.locator('#asset-device-group'), namedId(group));
                const rule = group.binding.snapshot.rules[0];
                await selectOption(visual, newDevice.locator('#asset-address-rule'), `${rule.real_cidr} → ${rule.mapped_cidr}`);
                await newDevice.locator('#asset-address-host').fill('31');
                await actionResponse(visual, 'POST', '/validation/instance-address',
                    () => newDevice.getByRole('button', { name: '预览并核对', exact: true }).click());
                await screenshot('node-binding-derived-device-address-cn.png');
                await newDevice.getByRole('button', { name: '取消', exact: true }).click();
                await visual.setViewportSize({ width: 390, height: 844 });
                await visual.evaluate(() => window.scrollTo(0, 0));
                const mobile = resolve(config.screenshotsDir, 'node-assets-mobile-390-cn.png');
                await visual.screenshot({ path: mobile, fullPage: true, animations: 'disabled' });
                report.screenshots.push(mobile);
                const width = await visual.evaluate(() => ({ viewport: innerWidth, document: document.documentElement.scrollWidth }));
                if (width.document > width.viewport + 1) report.layoutIssues.push(
                    `Mobile document overflows horizontally (${width.document}px / ${width.viewport}px); tables must scroll internally.`);
                const actionBox = await deviceRow(visual, directDevice).locator('.device-action-column').boundingBox();
                assert.ok(actionBox && actionBox.x >= 0 && actionBox.x + actionBox.width <= 391,
                    'R01: the action column must remain inside the 390px viewport without horizontal table scrolling.');
                await deviceRow(visual, directDevice).getByRole('button', { name: `为 ${directDevice.name} 新增服务`, exact: true }).click();
                const form = visual.getByRole('dialog', { name: '新增服务', exact: true });
                await form.waitFor();
                const formShot = resolve(config.screenshotsDir, 'node-assets-service-form-390-cn.png');
                await visual.screenshot({ path: formShot, fullPage: false, animations: 'disabled' });
                report.screenshots.push(formShot);
            } finally { await chinese.close(); }
        });

        await step('An offline registered node accepts persisted-instance device and service management', async () => {
            const nodes = (await api(context, config, '/nodes')).items;
            const offline = nodes.find(item => item.machine_id === config.offlineNode.machine_id);
            assert.ok(offline, 'Offline fixture must be a registered node.');
            const ownerContext = await browser.newContext({ baseURL: config.baseURL });
            await ownerContext.addInitScript(() => localStorage.setItem('lang', 'en'));
            try {
                await login(await ownerContext.newPage(), config, config.owner);
                const registered = await jsonResponse(await ownerContext.request.get('/api/v1/machines'));
                const machine = registered.machines.find(item => machineId(item.info?.machine_id) === config.offlineNode.machine_id);
                assert.ok(machine, 'The owner must see this registered machine through the original device API.');
                assert.equal(machine.online, false, 'This coverage requires an actually offline node, not a mocked response.');
            } finally { await ownerContext.close(); }
            await page.goto(detailsPath(config, config.offlineNode));
            await page.locator('.assets-list-card').waitFor();
            const device = await createDevice(page, config, { name: names.offline }, tracked, config.offlineNode);
            await createService(page, device, { name: `离线服务-${suffix}`, port: 9090 });
            const saved = await api(context, config, `/devices/${device.id}/services`, config.offlineNode);
            assert.equal(saved.items.length, 1);
            await deviceRow(page, device).getByRole('button', { name: `Delete device ${device.name}`, exact: true }).click();
            await actionResponse(page, 'DELETE', `/devices/${device.id}`,
                () => page.getByRole('dialog', { name: 'Confirm deletion', exact: true })
                    .getByRole('button', { name: 'Delete', exact: true }).click(), [204]);
            const deletedServices = await context.request.get(`${assetPath}/devices/${device.id}/services`, { params: scopeParams(config, config.offlineNode) });
            assert.equal(deletedServices.status(), 404, 'Deleting a device removes access to its dependent services.');
            await page.goto(detailsPath(config));
            await ensureExpanded(page, directDevice);
        });

        await step('Service delete and device cascade delete persist; empty group and unused type can then be deleted', async () => {
            const panel = servicesPanel(page, directDevice.name);
            await panel.getByRole('row').filter({ hasText: service.name }).getByRole('button', { name: 'Delete', exact: true }).click();
            const deletedService = await actionResponse(page, 'DELETE', `/services/${service.id}`,
                () => page.getByRole('dialog', { name: 'Delete service', exact: true })
                    .getByRole('button', { name: 'Delete', exact: true }).click(), [200]);
            assert.equal(deletedService.deleted, true);
            assert.equal(deletedService.publication_sync.status, 'applied');
            assert.equal((await api(context, config, `/devices/${directDevice.id}/services`)).items.length, 0);
            await createService(page, directDevice, { name: names.cascadeService, port: 9091 });
            for (const device of [directDevice, groupedDevice]) {
                await deviceRow(page, device).getByRole('button', { name: `Delete device ${device.name}`, exact: true }).click();
                await actionResponse(page, 'DELETE', `/devices/${device.id}`,
                    () => page.getByRole('dialog', { name: 'Confirm deletion', exact: true })
                        .getByRole('button', { name: 'Delete', exact: true }).click(), [204]);
                await deviceRow(page, device).waitFor({ state: 'hidden' });
            }
            await page.getByRole('button', { name: 'Manage gateway groups', exact: true }).click();
            let dialog = page.getByRole('dialog', { name: 'Manage gateway groups', exact: true });
            await dialog.getByRole('button', { name: `Delete gateway group ${group.name}`, exact: true }).click();
            await actionResponse(page, 'DELETE', `/gateway-groups/${group.id}`,
                () => dialog.getByRole('button', { name: 'Delete permanently', exact: true }).click(), [204]);
            await dialog.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
            await page.getByRole('button', { name: 'Manage device types', exact: true }).click();
            dialog = page.getByRole('dialog', { name: 'Manage device types', exact: true });
            await dialog.getByRole('button', { name: `Delete type ${type.name}`, exact: true }).click();
            await actionResponse(page, 'DELETE', `/device-types/${type.id}`,
                () => dialog.getByRole('button', { name: 'Delete permanently', exact: true }).click(), [204]);
            await dialog.locator('.p-dialog-footer').getByRole('button', { name: 'Close', exact: true }).click();
            assert.deepEqual(report.pageErrors, [], 'Browser must not raise uncaught page errors.');
        });
        await step('Responsive layout has no document-wide horizontal overflow', async () => {
            assert.deepEqual(report.layoutIssues, []);
        });
    } catch (error) {
        failed = true;
        await page.screenshot({ path: resolve(config.screenshotsDir, 'node-assets-failure.png'), fullPage: true }).catch(() => {});
        throw new Error(redact(error.message), { cause: undefined });
    } finally {
        // Only IDs returned by this run are deleted. Existing fixtures and presets
        // are retained; services disappear with their owning test devices.
        for (const entry of tracked.devices) {
            const result = await context.request.delete(`${assetPath}/devices/${entry.id}`, { params: scopeParams(config, entry.node) }).catch(() => null);
            if (!result || ![204, 404].includes(result.status())) report.cleanupErrors.push(`device ${entry.id}: ${result ? `HTTP ${result.status()}` : 'request failed'}`);
        }
        for (const [collection, ids] of [['gateway-groups', tracked.groups], ['device-types', tracked.types]]) {
            for (const id of ids) {
                const params = collection === 'device-types' ? { owner_user_id: String(config.owner.id) } : scopeParams(config);
                const result = await context.request.delete(`${assetPath}/${collection}/${id}`, { params }).catch(() => null);
                if (!result || ![204, 404].includes(result.status())) report.cleanupErrors.push(`${collection} ${id}: ${result ? `HTTP ${result.status()}` : 'request failed'}`);
            }
        }
        report.finishedAt = new Date().toISOString();
        report.passed = !failed && report.cleanupErrors.length === 0;
        await writeFile(resolve(config.screenshotsDir, 'node-assets-e2e-report.json'), JSON.stringify(report, null, 2));
        await browser.close();
        if (!failed) assert.deepEqual(report.cleanupErrors, [], 'Temporary data cleanup must complete.');
    }
});
