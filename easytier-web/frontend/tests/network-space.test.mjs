import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { spawn } from 'node:child_process';
import { mkdir } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';

const base = process.env.WEB_TEST_URL || 'http://127.0.0.1:5199';
const screenshotDir = process.env.WEB_SCREENSHOT_DIR;
let browser;
let server;
before(async () => {
    if (!process.env.WEB_TEST_URL) {
        server = spawn(process.execPath, ['node_modules/vite/bin/vite.js', 'preview', '--host', '127.0.0.1', '--port', '5199', '--strictPort'], { stdio: 'pipe' });
        let ready = false;
        for (let i = 0; i < 100; i++) {
            if (server.exitCode !== null) throw new Error('Vite exited before starting');
            if (await fetch(base).then(r => r.ok).catch(() => false)) { ready = true; break; }
            await delay(100);
        }
        assert.ok(ready, 'Vite preview must be ready before browser tests');
    }
    browser = await chromium.launch({ headless: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH, args: ['--no-sandbox', '--enable-unsafe-swiftshader'] });
    if (screenshotDir) await mkdir(screenshotDir, { recursive: true });
});
after(async () => { await browser?.close(); server?.kill(); });

const id = n => `00000000-0000-0000-0000-${n.toString(16).padStart(12, '0')}`;
const uuid = n => ({ part1: 0, part2: 0, part3: 0, part4: n });
const instanceId = n => id(1000 + n);
const nodeNames = ['上海 · 家庭节点', '北京 · 办公室', '新加坡 · 中继', '东京 · 节点', '待定位电脑'];
const ipv4 = n => ({ address: { addr: (10 << 24) + (144 << 16) + n }, network_length: 24 });
const legacyRequest = request => /^\/services(?:\/|$)/.test(request.path) || /^\/networks\/[^/]+\/(?:map|members)$/.test(request.path);
const binding = { state: 'legacy_unbound', reason: null, instance_id: null, snapshot: null, current: null, can_attach: true, can_refresh: false, can_edit_address: true };

function fixture() {
    const coordinates = [[31.23, 121.47], [39.9, 116.4], [1.35, 103.82], [35.68, 139.69], [null, null]];
    const machines = nodeNames.map((hostname, i) => ({
        owner_user_id: 1, client_url: `tcp://198.51.100.${i + 10}:11010`, alias: '', online: i !== 4,
        info: { hostname, machine_id: uuid(i + 1), easytier_version: '2.7.0', report_time: '2026-10-23T12:00:00Z', running_network_instances: i === 4 ? [] : [uuid(1001 + i)] },
        geo_location: { country: null, region: null, city: null, district: null, latitude: coordinates[i][0], longitude: coordinates[i][1], coordinate_system: 'WGS84' },
        site_location: null, device_model: 'Fixture model', is_public_server: false, networks: [],
        connection_ip: `198.51.100.${i + 10}`, ip_country: null, ip_region: null, ip_city: null,
        ip_location_status: 'not_found', ip_observed_at: '2026-10-23T12:00:00Z',
    }));
    const types = ['Router', 'Server', 'Camera', 'Computer'].map((name, i) => ({ id: id(700 + i), owner_user_id: 1, name, created_at: '2026-10-23T12:00:00Z', updated_at: '2026-10-23T12:00:00Z' }));
    const asset = (n, name, node, group, type, address, self = false, owner = 1) => ({
        id: id(n), owner_user_id: owner, machine_id: id(node), name, gateway_group_id: group ? id(group) : null,
        device_type_id: id(type), model: null, overlay_ip: address, lan_ip: group ? address.replace('10.31.', '192.168.') : null,
        is_gateway_self: self, remarks: null, address_mode: 'legacy_manual', rule_key: null, host_offset: null,
        network_instance_id: null, binding: structuredClone(binding), address_spec: null,
        created_at: '2026-10-23T12:00:00Z', updated_at: '2026-10-23T12:00:00Z', warnings: [],
    });
    const devices = [asset(299, 'Home router', 1, 600, 700, '10.31.31.1', true), asset(300, 'NAS', 1, 600, 701, '10.31.31.20'),
        asset(301, 'Entry camera', 1, 600, 702, '10.31.31.30'), asset(302, 'Office workstation', 2, null, 703, '10.144.0.2'),
        asset(399, 'Other owner device', 6, null, 701, '10.145.0.6', false, 2)];
    const groups = [{ id: id(600), owner_user_id: 1, machine_id: id(1), name: 'Home gateway', remarks: null,
        subnets: ['192.168.31.0/24'], overlay_ip: '10.144.0.1', binding: structuredClone(binding), warnings: [],
        created_at: '2026-10-23T12:00:00Z', updated_at: '2026-10-23T12:00:00Z' }];
    const service = (n, name, deviceId, icon, accessUrl, shown = true) => {
        const device = devices.find(item => item.id === id(deviceId));
        return { id: id(400 + n), owner_user_id: device.owner_user_id, machine_id: device.machine_id, device_id: device.id,
            name, protocols: ['tcp'], port: 8000 + n, remarks: 'Device-owned fixture service', access_url: accessUrl, icon,
            show_in_navigation: shown, publication: null, overlay_ip: device.overlay_ip, lan_ip: device.lan_ip,
            created_at: '2026-10-23T12:00:00Z', updated_at: '2026-10-23T12:00:00Z' };
    };
    const services = [service(1, 'Home Assistant', 299, 'home', 'https://home.example.test/'),
        service(2, 'Immich', 300, 'server', 'https://photos.example.test/'), service(3, 'Jellyfin', 300, 'play', 'https://media.example.test/'),
        service(4, 'Grafana', 300, 'globe', 'https://metrics.example.test/'), service(5, 'TCP-only service', 302, null, null, false),
        service(6, 'Other owner service', 399, 'globe', 'https://other.example.test/')];
    return { machines, devices, groups, types, services, requests: [], writes: [], failures: new Set(), failWrites: false,
        delays: new Map(), gates: new Map(), peerCounter: 0, nextService: 500, externalConsole: false, admin: false };
}

const assetNode = (state, machine) => ({ ...machine, machine_id: id(machine.info.machine_id.part4), hostname: machine.info.hostname,
    alias: machine.alias || '', owner_user_id: machine.owner_user_id, device_count: state.devices.filter(device => device.machine_id === id(machine.info.machine_id.part4)).length });
const serviceCatalog = (state, owner) => state.services.filter(service => service.owner_user_id === owner).map(service => ({ ...service,
    node_name: state.machines.find(machine => id(machine.info.machine_id.part4) === service.machine_id)?.info.hostname || 'Other owner node',
    device_name: state.devices.find(device => device.id === service.device_id)?.name || 'Missing device' }));

async function open(t, route = '/h/services', options = {}, configure = () => {}) {
    const state = fixture();
    configure(state);
    const { language = 'en', noWebGL = false, clock = false, ...contextOptions } = options;
    const context = await browser.newContext({ viewport: { width: 1440, height: 1024 }, colorScheme: 'dark', reducedMotion: 'reduce', ...contextOptions });
    t.after(() => context.close());
    await context.addInitScript(({ language, noWebGL }) => {
        localStorage.setItem('lang', language);
        if (noWebGL) {
            const original = HTMLCanvasElement.prototype.getContext;
            HTMLCanvasElement.prototype.getContext = function (type, ...args) { return /webgl/i.test(type) ? null : original.call(this, type, ...args); };
        }
    }, { language, noWebGL });
    await context.route(/^https:\/\/[^/]+\.example\.test\//, route => route.fulfill({ contentType: 'text/html', body: '<h1>Fixture service reached</h1>' }));
    const page = await context.newPage();
    if (clock) await page.clock.install();
    page.setDefaultTimeout(15000);
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => {
        if (['error', 'warning'].includes(message.type()) && /(?:TypeError|ReferenceError|Unhandled error during)/.test(message.text())) errors.push(message.text());
    });
    t.after(() => {
        assert.deepEqual(errors, [], 'no uncaught browser errors');
        assert.deepEqual(state.requests.filter(legacyRequest), [], 'map and services must not consume the retired central-network datasets');
    });
    await page.route('**/api_meta.js', route => route.fulfill({ contentType: 'text/javascript', body: 'window.apiMeta = { api_host: "" };' }));
    await page.route('**/tiles/**', route => route.fulfill({ contentType: 'image/png', body: Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aT9sAAAAASUVORK5CYII=', 'base64') }));
    await page.route('**/api/v1/**', async intercepted => {
        const request = intercepted.request();
        const url = new URL(request.url());
        const path = url.pathname.replace('/api/v1', '');
        const method = request.method();
        const payload = request.postDataJSON();
        const query = Object.fromEntries(url.searchParams);
        const owner = Number(query.owner_user_id || 1);
        const machineId = query.machine_id;
        const record = { path, method, payload, query };
        state.requests.push(record);
        const isMutation = method !== 'GET' && !path.endsWith('/proxy-rpc');
        if (isMutation) state.writes.push(record);
        const milliseconds = state.delays.get(`${path}:${machineId || owner}`) ?? state.delays.get(path);
        if (milliseconds) await delay(milliseconds);
        const gate = state.gates.get(`${path}:${machineId || owner}`) ?? state.gates.get(path);
        if (gate) await gate;
        if (page.isClosed()) return;
        if (state.failures.has(path) || (isMutation && state.failWrites)) return intercepted.fulfill({ status: 503, json: { message: 'Fixture request failed' } });
        if (legacyRequest(record)) return intercepted.fulfill({ status: 410, json: { message: 'No central-network map or legacy services in this fixture' } });
        let result = {};
        if (path === '/console-info') result = { username: 'browser-fixture', config_server_protocol: 'udp', config_server_port: 22020, webhook_auth: state.externalConsole };
        else if (path === '/admin/users/context') result = { actor: { id: 1, username: 'browser-fixture' }, available: !state.externalConsole, can_manage_users: false };
        else if (path === '/summary') result = { device_count: state.machines.length };
        else if (path === '/machines') result = { machines: state.machines };
        else if (path === '/blocked-devices') result = { blocked: [] };
        // The shell still loads the bottom menu; its empty value must not hide nodes or services.
        else if (path === '/networks') result = { networks: [] };
        else if (path === '/assets/context') result = { actor: { id: 1, username: 'browser-fixture' }, can_manage_others: state.admin, available: !state.externalConsole };
        else if (path === '/assets/accounts') result = { items: [{ id: 1, username: 'browser-fixture' }, { id: 2, username: 'other-owner' }] };
        else if (path.startsWith('/assets/') && owner !== 1 && !state.admin) return intercepted.fulfill({ status: 403, json: { message: 'Owner access denied' } });
        else if (path === '/assets/nodes') result = { items: owner === 1 ? state.machines.map(machine => assetNode(state, machine)) : [{ owner_user_id: 2, machine_id: id(6), hostname: 'Other owner node', alias: '', geo_location: null, is_public_server: false, device_count: 1 }] };
        else if (path === '/assets/device-counts') result = { items: state.machines.map(machine => ({ machine_id: id(machine.info.machine_id.part4), device_count: assetNode(state, machine).device_count })) };
        else if (path === '/assets/device-types' || path === '/assets/device-types/initialize') result = { items: state.types.filter(type => type.owner_user_id === owner) };
        else if (path === '/assets/gateway-groups') result = { items: state.groups.filter(group => group.owner_user_id === owner && group.machine_id === machineId) };
        else if (path === '/assets/devices') {
            let items = state.devices.filter(device => device.owner_user_id === owner && device.machine_id === machineId);
            if (query.group_id) items = items.filter(device => device.gateway_group_id === query.group_id);
            if (query.type_id) items = items.filter(device => device.device_type_id === query.type_id);
            if (query.keyword) items = items.filter(device => `${device.name} ${device.overlay_ip} ${device.lan_ip || ''}`.toLowerCase().includes(query.keyword.toLowerCase()));
            const size = Math.max(1, Number(query.page_size || 20));
            let selectedPage = Math.max(1, Number(query.page || 1));
            if (query.locate_id) {
                const index = items.findIndex(device => device.id === query.locate_id);
                if (index < 0) return intercepted.fulfill({ status: 404, json: { code: 'asset_device_not_found', message: 'Device not found' } });
                selectedPage = Math.floor(index / size) + 1;
            }
            result = { items: items.slice((selectedPage - 1) * size, selectedPage * size), total: items.length, page: selectedPage, page_size: size };
        } else if (/^\/assets\/devices\/[^/]+$/.test(path)) {
            result = state.devices.find(device => device.id === path.split('/')[3] && device.machine_id === machineId && device.owner_user_id === owner);
            if (!result) return intercepted.fulfill({ status: 404, json: { code: 'asset_device_not_found', message: 'Device not found' } });
        } else if (/^\/assets\/devices\/[^/]+\/services$/.test(path)) {
            const device = state.devices.find(device => device.id === path.split('/')[3] && device.machine_id === machineId && device.owner_user_id === owner);
            if (!device) return intercepted.fulfill({ status: 404, json: { code: 'asset_device_not_found', message: 'Device not found' } });
            if (method === 'POST') {
                const { request_id, ...input } = payload;
                assert.ok(request_id, 'service creation retains the idempotency key');
                result = { ...input, id: id(state.nextService++), owner_user_id: owner, machine_id: machineId, device_id: device.id,
                    overlay_ip: device.overlay_ip, lan_ip: device.lan_ip, publication: null, created_at: '2026-10-23T13:00:00Z', updated_at: '2026-10-23T13:00:00Z' };
                state.services.push(structuredClone(result));
            } else result = { items: state.services.filter(service => service.device_id === device.id) };
        } else if (path === '/assets/services') result = { items: serviceCatalog(state, owner) };
        else if (/^\/assets\/services\/[^/]+(?:\/navigation)?$/.test(path)) {
            const current = state.services.find(service => service.id === path.split('/')[3] && service.owner_user_id === owner && service.machine_id === machineId);
            if (!current) return intercepted.fulfill({ status: 404, json: { message: 'Service not found' } });
            if (method === 'DELETE') { state.services = state.services.filter(service => service.id !== current.id); return intercepted.fulfill({ status: 204 }); }
            if (path.endsWith('/navigation')) {
                assert.equal(method, 'PATCH');
                assert.deepEqual(Object.keys(payload), ['show_in_navigation'], 'navigation toggles do not rewrite service/publication fields');
                if (payload.show_in_navigation && !current.access_url) return intercepted.fulfill({ status: 422, json: { message: 'A valid access address is required' } });
            }
            Object.assign(current, payload);
            result = current;
        } else if (/^\/assets\/nodes\/[^/]+\/metadata$/.test(path)) {
            assert.equal(method, 'PATCH');
            const machine = state.machines.find(machine => id(machine.info.machine_id.part4) === path.split('/')[3]);
            Object.assign(machine, payload);
            result = assetNode(state, machine);
        } else if (/^\/machines\/[^/]+\/alias$/.test(path)) {
            const machine = state.machines.find(machine => id(machine.info.machine_id.part4) === path.split('/')[2]);
            machine.alias = payload.alias;
        } else if (path === '/geocoding/config') result = { tile_url: `${base}/tiles/{z}/{x}/{y}.png`, attribution_text: 'Fixture tiles', attribution_url: 'https://example.test/', max_zoom: 19, reverse_enabled: false };
        else if (path.endsWith('/proxy-rpc')) {
            const source = Number.parseInt(path.split('/')[2].slice(-12), 16);
            assert.equal(payload.payload.instance.id.part4, 1000 + source, 'runtime queries use the actual registered machine instance');
            const peers = source === 1 ? [2, 3] : source === 2 ? [1] : source === 3 ? [1, 4] : source === 4 ? [3] : [];
            if (payload.method_name === 'show_node_info') result = { node_info: { peer_id: source * 10, inst_id: instanceId(source), hostname: nodeNames[source - 1], ipv4_addr: `10.144.0.${source}`, proxy_cidrs: source === 1 ? ['192.168.31.0/24'] : [] } };
            else if (payload.method_name === 'list_peer') {
                state.peerCounter++;
                result = { peer_infos: peers.map(n => ({ peer_id: n * 10, conns: [{ conn_id: `${source}-${n}`, tunnel: { tunnel_type: 'udp' }, stats: { latency_us: String(n * 2000), rx_bytes: String(100000 + state.peerCounter * 10000), tx_bytes: String(200000 + state.peerCounter * 10000) } }] })) };
            } else if (payload.method_name === 'list_route') result = { routes: [1, 2, 3, 4].filter(n => n !== source).map(n => ({ peer_id: n * 10, inst_id: instanceId(n), hostname: nodeNames[n - 1], ipv4_addr: ipv4(n), cost: peers.includes(n) ? 1 : 2, path_latency: n * 6, proxy_cidrs: [], version: '2.7.0', next_hop_peer_id: (peers.includes(n) ? n : source === 4 ? 3 : source === 2 ? 1 : 3) * 10 })) };
        }
        await intercepted.fulfill({ json: result });
    });
    await page.goto(`${base}/#${route}`);
    await page.locator('.console-topbar').waitFor();
    return { page, state, context };
}

async function selectOption(page, label, option) {
    await page.getByRole('combobox', { name: label, exact: true }).click();
    await page.getByRole('option', { name: option, exact: true }).click();
}
async function capture(page, name) {
    if (!screenshotDir) return;
    await page.evaluate(() => { document.activeElement?.blur(); window.scrollTo(0, 0); });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.screenshot({ path: `${screenshotDir}/${name}.png`, fullPage: true, animations: 'disabled' });
    if (page.viewportSize().width === 390) await page.screenshot({ path: `${screenshotDir}/${name}-viewport.png`, animations: 'disabled' });
}
async function assertNoOverflow(page) { assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'page must not overflow horizontally'); }
const assetDialog = page => page.getByRole('dialog', { name: 'Edit service', exact: true });
const serviceRow = (page, serviceId) => page.locator(`.service-management-card[data-service-id="${serviceId}"]`);

test('device-owned navigation works without central networks and toggles only the same service flag', async t => {
    const { page, state, context } = await open(t);
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 4);
    const original = structuredClone(state.services.find(service => service.id === id(402)));
    const popupPromise = context.waitForEvent('page');
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).click();
    const popup = await popupPromise;
    await popup.waitForURL('https://photos.example.test/');
    await popup.getByRole('heading', { name: 'Fixture service reached' }).waitFor();
    await popup.close();
    await page.getByRole('textbox', { name: 'Search services' }).fill('Immich');
    await page.locator('.service-card').filter({ hasText: 'Home Assistant' }).waitFor({ state: 'detached' });
    assert.equal(await page.locator('.service-card').count(), 1);
    await page.getByRole('textbox', { name: 'Search services' }).fill('');
    await page.getByRole('button', { name: 'Service actions: Immich' }).click();
    await page.getByRole('menuitem', { name: 'Remove from navigation' }).click();
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).waitFor({ state: 'detached' });
    assert.deepEqual(state.services.find(service => service.id === id(402)), { ...original, show_in_navigation: false });
    await page.reload();
    await page.getByRole('link', { name: 'Open service: Home Assistant', exact: true }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 3);
    await page.goto(`${base}/#/h/services/manage`);
    await page.getByRole('switch', { name: 'Show in service navigation: Immich', checked: false }).click();
    await page.getByRole('switch', { name: 'Show in service navigation: Immich', checked: true }).waitFor();
    assert.deepEqual(state.services.find(service => service.id === id(402)), original);
    assert.ok(state.requests.some(request => request.path === '/assets/services' && request.query.owner_user_id === '1'));
    assert.equal(state.writes.filter(request => request.path.endsWith('/navigation')).length, 2);
});

test('global editing updates the device service, preserves a failed draft, and deletion removes its navigation card', async t => {
    const { page, state } = await open(t, `/h/services/manage?edit=${id(402)}`);
    const editor = assetDialog(page);
    await editor.locator('input[id$="-name"]').fill('Renamed Immich');
    await editor.getByLabel('Access link', { exact: true }).fill('javascript:alert(1)');
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.getByText('Enter a complete web address starting with http:// or https://.', { exact: true }).waitFor();
    assert.equal(state.writes.length, 0);
    await editor.getByLabel('Access link', { exact: true }).fill('https://photos.example.test/library');
    state.failWrites = true;
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.getByRole('alert').waitFor();
    assert.equal(await editor.locator('input[id$="-name"]').inputValue(), 'Renamed Immich');
    state.failWrites = false;
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    await serviceRow(page, id(402)).getByRole('button', { name: 'Renamed Immich', exact: true }).waitFor();
    const saved = state.services.find(service => service.id === id(402));
    assert.equal(saved.device_id, id(300));
    assert.equal(saved.machine_id, id(1));
    assert.equal(saved.access_url, 'https://photos.example.test/library');
    await page.goto(`${base}/#/h/node-assets?owner=1&node=${id(1)}&device=${id(300)}`);
    await page.locator('.service-panel').getByText('Renamed Immich', { exact: true }).waitFor();
    await page.goto(`${base}/#/h/services/manage`);
    await page.getByRole('button', { name: 'Service actions: Renamed Immich' }).click();
    await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Cancel', exact: true }).click();
    assert.ok(state.services.some(service => service.id === id(402)));
    await page.getByRole('button', { name: 'Service actions: Renamed Immich' }).click();
    await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Delete', exact: true }).click();
    await serviceRow(page, id(402)).waitFor({ state: 'detached' });
    assert.ok(!state.services.some(service => service.id === id(402)));
    await page.goto(`${base}/#/h/services`);
    await page.getByRole('link', { name: 'Open service: Home Assistant' }).waitFor();
    assert.equal(await page.getByRole('link', { name: 'Open service: Renamed Immich' }).count(), 0);
});

test('adding a service under its actual device appears in management and navigation without copying records', async t => {
    const { page, state } = await open(t, `/h/node-assets?owner=1&node=${id(1)}&device=${id(300)}`);
    await page.locator('.service-panel').getByText('Immich', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Add a service to NAS', exact: true }).click();
    const editor = page.getByRole('dialog', { name: 'Add service', exact: true });
    await editor.locator('input[id$="-name"]').fill('Device-created service');
    await editor.locator('input[id$="-port"]').pressSequentially('9090');
    await editor.locator('input[id$="-port"]').press('Tab');
    assert.ok(await editor.getByRole('checkbox', { name: 'Show in service navigation', exact: true }).isDisabled());
    await editor.getByLabel('Access link', { exact: true }).fill('https://new.example.test/');
    await editor.getByRole('checkbox', { name: 'Show in service navigation', exact: true }).check();
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    const created = state.services.find(service => service.name === 'Device-created service');
    assert.ok(created);
    assert.equal(created.device_id, id(300));
    assert.equal(created.show_in_navigation, true);
    assert.equal(state.writes.filter(request => request.path === `/assets/devices/${id(300)}/services` && request.method === 'POST').length, 1);
    await page.goto(`${base}/#/h/services/manage`);
    await serviceRow(page, created.id).getByRole('button', { name: created.name, exact: true }).waitFor();
    await page.goto(`${base}/#/h/services`);
    await page.getByRole('link', { name: `Open service: ${created.name}`, exact: true }).waitFor();
    assert.equal(state.services.filter(service => service.name === created.name).length, 1);
});

test('navigation without an access address opens the actual service editor and clearing the address disables the card', async t => {
    const { page, state } = await open(t, '/h/services/manage');
    await page.getByRole('switch', { name: 'Show in service navigation: TCP-only service', checked: false }).click();
    const editor = assetDialog(page);
    await editor.waitFor();
    assert.equal(await editor.locator('input[id$="-name"]').inputValue(), 'TCP-only service');
    assert.ok(await editor.getByRole('checkbox', { name: 'Show in service navigation', exact: true }).isDisabled());
    assert.equal(state.writes.length, 0);
    await editor.getByLabel('Access link', { exact: true }).fill('https://office.example.test/');
    await editor.getByRole('checkbox', { name: 'Show in service navigation', exact: true }).check();
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    await page.getByRole('switch', { name: 'Show in service navigation: TCP-only service', checked: true }).waitFor();
    await serviceRow(page, id(405)).getByRole('button', { name: 'TCP-only service', exact: true }).click();
    await editor.getByLabel('Access link', { exact: true }).fill('');
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    await page.getByRole('switch', { name: 'Show in service navigation: TCP-only service', checked: false }).waitFor();
    assert.equal(state.services.find(service => service.id === id(405)).access_url, null);
});

test('service navigation locates the registered node and same asset device without a central network selector', async t => {
    const { page, state } = await open(t);
    await page.getByRole('button', { name: 'Service actions: Immich' }).click();
    await page.getByRole('menuitem', { name: 'Locate node on map' }).click();
    await page.waitForURL(url => url.hash.includes('/h/map?') && url.hash.includes(`node=${id(1)}`) && url.hash.includes(`device=${id(300)}`));
    await page.locator('.map-device-row.selected').getByText('NAS', { exact: true }).waitFor();
    await page.locator('.map-inspector h2').getByText(nodeNames[0], { exact: true }).waitFor();
    assert.equal(await page.locator('#map-network').count(), 0);
    assert.equal(await page.locator('.map-node-list-item').count(), 5);
    assert.ok(!page.url().includes('network='));
    await page.locator('.map-link-row').first().waitFor();
    assert.ok(state.requests.some(request => request.path === '/machines'));
    assert.ok(state.requests.some(request => request.path === '/assets/devices' && request.query.machine_id === id(1)));
});

test('map locations edit registered metadata and survive reopening without writing separate map configuration', async t => {
    const { page, state } = await open(t, `/h/map?node=${id(5)}`, { noWebGL: true });
    await page.locator('.map-inspector h2').getByText(nodeNames[4], { exact: true }).waitFor();
    await page.locator('.map-node-filters').getByRole('button', { name: 'Unlocated 1', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('.map-node-list-item').length === 1);
    await page.locator('.map-coordinate-line').getByRole('button').click();
    const editor = page.getByRole('dialog', { name: 'Edit attributes', exact: true });
    await editor.locator('#device-node-geo-latitude').pressSequentially('48.85');
    await editor.locator('#device-node-geo-latitude').press('Tab');
    assert.ok(await editor.getByRole('button', { name: 'Save', exact: true }).isDisabled());
    await editor.locator('#device-node-geo-longitude').pressSequentially('2.35');
    await editor.locator('#device-node-geo-longitude').press('Tab');
    state.failWrites = true;
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.getByRole('alert').waitFor();
    assert.equal(await editor.locator('#device-node-geo-latitude').inputValue(), '48.85');
    state.failWrites = false;
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    assert.equal(state.machines[4].geo_location.latitude, 48.85);
    assert.equal(state.machines[4].geo_location.longitude, 2.35);
    assert.equal(state.machines[4].geo_location.coordinate_system, 'WGS84');
    assert.ok(state.writes.every(request => request.path === `/assets/nodes/${id(5)}/metadata`));
    await page.reload();
    await page.locator('.map-coordinate-line').getByText('48.8500°, 2.3500°', { exact: true }).waitFor();
    await page.goto(`${base}/#/h/deviceList`);
    await page.locator('.device-list-page').getByText('48.850000, 2.350000 · WGS84', { exact: true }).first().waitFor();
});

test('actual running instances produce direct links and relay routes while failed samples become stale', async t => {
    const { page, state } = await open(t, `/h/map?node=${id(1)}`);
    await page.locator('.map-link-row').filter({ hasText: nodeNames[1] }).first().click();
    const details = page.locator('.map-path-details');
    await details.getByText('udp', { exact: true }).waitFor();
    assert.match(await details.textContent(), /0 B\/s/, 'initial counters establish a baseline');
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.waitForFunction(() => [...document.querySelectorAll('.map-path-details dd')].some(element => {
        const match = element.textContent.match(/↓ ([\d.]+) [kM]?B\/s/);
        return match && Number(match[1]) > 0;
    }));
    await page.locator('.map-link-row').filter({ hasText: nodeNames[3] }).click();
    await details.getByText(`${nodeNames[0]} → ${nodeNames[2]} → ${nodeNames[3]}`, { exact: true }).waitFor();
    state.failures.add(`/machines/${id(1)}/proxy-rpc`);
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.getByText('Some nodes could not be queried. Retained links are marked stale.', { exact: true }).waitFor();
    await page.locator('.map-link-row').filter({ hasText: nodeNames[3] }).click();
    await details.getByText('Stale', { exact: true }).waitFor();
});

test('node selection fences late device responses from a previously selected node', async t => {
    const { page, state } = await open(t, `/h/map?node=${id(1)}`, { noWebGL: true }, state => { state.delays.set(`/assets/devices:${id(1)}`, 800); });
    if (!state.requests.some(request => request.path === '/assets/devices' && request.query.machine_id === id(1))) {
        await page.waitForRequest(request => {
            const url = new URL(request.url());
            return url.pathname === '/api/v1/assets/devices' && url.searchParams.get('machine_id') === id(1);
        });
    }
    await page.locator('.map-node-list-item').filter({ hasText: nodeNames[1] }).click();
    await page.locator('.map-inspector h2').getByText(nodeNames[1], { exact: true }).waitFor();
    await page.locator('.map-device-row').getByText('Office workstation', { exact: true }).waitFor();
    await delay(950);
    assert.equal(await page.locator('.map-device-row').getByText('NAS', { exact: true }).count(), 0);
    assert.equal(await page.locator('.map-device-row').getByText('Office workstation', { exact: true }).count(), 1);
    assert.ok(page.url().includes(`node=${id(2)}`));
    assert.ok(state.requests.some(request => request.path === '/assets/devices' && request.query.machine_id === id(1)));
});

test('WebGL fallback keeps node actions and links to real device management', async t => {
    const { page } = await open(t, `/h/map?node=${id(1)}&device=${id(300)}`, { noWebGL: true });
    await page.getByText('3D rendering is unavailable. You can still use the node list and configuration controls.', { exact: true }).waitFor();
    await page.locator('.map-device-row').filter({ hasText: 'NAS' }).getByRole('button', { name: 'Edit NAS', exact: true }).click();
    await page.waitForURL(url => url.hash.includes('/h/node-assets?') && url.hash.includes(`node=${id(1)}`) && url.hash.includes(`device=${id(300)}`));
    await page.locator('.service-panel').getByText('Immich', { exact: true }).waitFor();
    assert.equal(await page.getByRole('heading', { name: 'Device management', exact: true }).count(), 1);
});

test('auto refresh pauses map sampling while manual refresh and later resumption remain usable', async t => {
    const { page, state } = await open(t, `/h/map?node=${id(1)}`, { noWebGL: true, clock: true });
    await page.locator('.map-link-row').first().waitFor();
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    await page.locator('.map-link-row').filter({ hasText: nodeNames[1] }).first().click();
    const details = page.locator('.map-path-details');
    const toggle = page.getByRole('switch', { name: 'Auto refresh', exact: true });
    await toggle.uncheck();
    const samples = () => state.requests.filter(request => request.path.endsWith('/proxy-rpc') || request.path === '/machines').length;
    const paused = samples();
    await page.clock.fastForward(30_000);
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    assert.equal(samples(), paused);
    await details.getByText('Stale', { exact: true }).waitFor();
    await page.clock.fastForward(15_000);
    await details.getByText('Stale', { exact: true }).waitFor();
    assert.equal(samples(), paused, 'expiry never turns paused samples current again');
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    assert.ok(samples() > paused, 'manual refresh stays available while polling is paused');
    const resumed = samples();
    await toggle.check();
    const nextMachines = page.waitForResponse(response => new URL(response.url()).pathname === '/api/v1/machines');
    await page.clock.fastForward(15_000);
    await nextMachines;
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    assert.ok(samples() > resumed);
});

test('service loading errors recover without pretending the device catalog is empty', async t => {
    const { page, state } = await open(t, '/h/services', {}, state => { state.failures.add('/assets/services'); });
    await page.getByText('Fixture request failed', { exact: false }).waitFor();
    assert.equal(await page.getByRole('heading', { name: 'Your services start here' }).count(), 0);
    state.failures.clear();
    await page.getByRole('button', { name: 'Retry', exact: true }).click();
    await page.getByRole('link', { name: 'Open service: Home Assistant' }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 4);
});

test('administrator catalogs remain owner-specific and revoked access clears the other owner records', async t => {
    const { page, state } = await open(t, '/h/services/manage', {}, state => { state.admin = true; });
    await serviceRow(page, id(402)).waitFor();
    await selectOption(page, 'Account', 'other-owner');
    await serviceRow(page, id(406)).waitFor();
    assert.equal(await page.locator('.service-management-card').count(), 1);
    await page.getByRole('button', { name: 'Service actions: Other owner service' }).click();
    assert.equal(await page.getByRole('menuitem', { name: 'Locate node on map' }).count(), 0);
    await page.getByRole('menuitem', { name: 'Open owning device' }).waitFor();
    await page.keyboard.press('Escape');
    await page.getByRole('switch', { name: 'Show in service navigation: Other owner service', checked: true }).click();
    await page.getByRole('switch', { name: 'Show in service navigation: Other owner service', checked: false }).waitFor();
    const write = state.writes.find(request => request.path.endsWith('/navigation'));
    assert.equal(write.query.owner_user_id, '2');
    assert.equal(write.query.machine_id, id(6));
    assert.equal(state.services.find(service => service.id === id(402)).show_in_navigation, true);
    state.admin = false;
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await serviceRow(page, id(402)).waitFor();
    assert.equal(await serviceRow(page, id(406)).count(), 0);
});

test('sidebar collapse persists and external mode does not request the asset service catalog', async t => {
    const { page } = await open(t);
    const nav = page.locator('.console-sidebar');
    await nav.getByRole('link', { name: 'Service navigation', exact: true }).waitFor();
    const links = await nav.getByRole('link').allTextContents();
    assert.ok(links.findIndex(value => value.includes('Service navigation')) < links.findIndex(value => value.includes('Dashboard')));
    await page.getByRole('button', { name: 'Collapse menu', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.console-sidebar').getBoundingClientRect().width <= 73);
    await page.reload();
    await page.getByRole('button', { name: 'Expand menu', exact: true }).waitFor();
    assert.ok(await nav.evaluate(element => element.getBoundingClientRect().width <= 73));
    const external = await open(t, '/h/services', {}, state => { state.externalConsole = true; });
    await external.page.waitForURL('**/#/h');
    assert.equal(external.state.requests.some(request => request.path === '/assets/services'), false);
    assert.equal(await external.page.locator('a[href*="/h/services"], a[href*="/h/map"]').count(), 0);
});

test('node space automatically fits one hundred devices arriving after its empty scene mounts', async t => {
    let releaseDevices;
    const deviceResponse = new Promise(resolve => { releaseDevices = resolve; });
    t.after(() => releaseDevices());
    const { page, state } = await open(t, `/h/map?node=${id(1)}`, {}, state => {
        const template = state.devices.find(device => device.id === id(300));
        state.devices = Array.from({ length: 100 }, (_, index) => ({ ...structuredClone(template),
            id: id(2000 + index), name: `Load device ${String(index + 1).padStart(3, '0')}`,
            gateway_group_id: null, overlay_ip: `10.31.32.${index + 1}`, lan_ip: null }));
        state.groups = [];
        state.services = [];
        state.gates.set(`/assets/devices:${id(1)}`, deviceResponse);
    });
    await page.getByRole('button', { name: 'Node space', exact: true }).click();
    const stage = page.locator('.map-stage');
    await stage.locator('.node-space-canvas').waitFor();
    await stage.getByText('This node has no asset devices', { exact: true }).waitFor();
    assert.equal(await page.locator('.map-device-row').count(), 0);
    releaseDevices();
    await page.locator('.map-device-row').getByText('Load device 100', { exact: true }).waitFor();
    assert.equal(await page.locator('.map-device-row').count(), 100);
    await stage.getByText('This node has no asset devices', { exact: true }).waitFor({ state: 'detached' });
    await assertNoOverflow(page);
    // Outer-ring devices must remain in the automatically fitted view. Selecting
    // them changes only the highlighted marker; no reset or zoom is performed.
    for (const number of [97, 98, 99, 100]) {
        const name = `Load device ${String(number).padStart(3, '0')}`;
        await page.locator('.map-device-select').filter({ hasText: name }).click();
        const label = stage.getByRole('button', { name, exact: true });
        await label.waitFor();
        const [bounds, canvas] = await Promise.all([label.boundingBox(), stage.locator('.node-space-canvas').boundingBox()]);
        assert.ok(bounds && canvas && bounds.x >= canvas.x && bounds.x + bounds.width <= canvas.x + canvas.width
            && bounds.y >= canvas.y && bounds.y + bounds.height <= canvas.y + canvas.height, `${name} stays inside the fitted scene`);
    }
    assert.ok(state.requests.some(request => request.path === '/assets/devices' && request.query.page_size === '100'));
    if (screenshotDir) await stage.screenshot({ path: `${screenshotDir}/asset-node-space-100-devices-auto-fit.png`, animations: 'disabled' });
});

test('asset-backed 3D map and services remain usable on Chinese desktop and mobile layouts', async t => {
    const { page, state } = await open(t, '/h/services', { language: 'cn' });
    await page.getByRole('link', { name: '访问服务: Immich', exact: true }).waitFor();
    assert.equal(await page.locator('.service-network-filter [role="combobox"]').textContent(), '全部网络节点');
    await assertNoOverflow(page);
    await capture(page, 'asset-service-navigation-cn-dark-desktop');
    await page.goto(`${base}/#/h/services/manage`);
    await serviceRow(page, id(402)).waitFor();
    assert.deepEqual(await page.locator('.service-management-filter [role="combobox"]').allTextContents(), ['全部网络节点', '全部设备']);
    await assertNoOverflow(page);
    await capture(page, 'asset-service-management-cn-dark-desktop');
    await page.goto(`${base}/#/h/map?node=${id(1)}&device=${id(300)}`);
    await page.locator('.map-inspector h2').getByText(nodeNames[0], { exact: true }).waitFor();
    await page.locator('.map-link-row').first().waitFor();
    await page.locator('.map-stage canvas').waitFor();
    assert.ok(await page.locator('.map-stage canvas').evaluate(canvas => canvas.width > 0 && canvas.height > 0));
    await assertNoOverflow(page);
    await capture(page, 'asset-network-map-cn-dark-desktop');
    await page.getByRole('button', { name: '查看节点空间', exact: true }).click();
    await page.locator('.map-stage .node-space-canvas').waitFor();
    await capture(page, 'asset-node-space-cn-dark-desktop');
    await page.getByRole('combobox', { name: '空间视图', exact: true }).selectOption('home');
    await page.locator('.map-stage .node-space[data-layout="home"]').waitFor();
    await capture(page, 'asset-node-home-cn-dark-desktop');
    await page.getByRole('combobox', { name: '空间视图', exact: true }).selectOption('topology');
    assert.equal(state.writes.length, 0, 'space layout is a local view preference, not independent map data');
    await page.setViewportSize({ width: 390, height: 844 });
    await assertNoOverflow(page);
    await capture(page, 'asset-node-space-cn-dark-mobile');
    await page.getByRole('button', { name: '返回地球', exact: true }).click();
    await assertNoOverflow(page);
    await capture(page, 'asset-network-map-cn-dark-mobile');
    await page.goto(`${base}/#/h/services`);
    await page.getByRole('link', { name: '访问服务: Immich', exact: true }).waitFor();
    await assertNoOverflow(page);
    await capture(page, 'asset-service-navigation-cn-dark-mobile');
    await page.goto(`${base}/#/h/services/manage`);
    await serviceRow(page, id(402)).waitFor();
    await assertNoOverflow(page);
    await capture(page, 'asset-service-management-cn-dark-mobile');
    await page.getByRole('button', { name: '导航', exact: true }).click();
    await page.locator('.console-mobile-nav').getByRole('link', { name: '设备管理', exact: true }).waitFor();
    await page.locator('.console-mobile-nav').getByRole('link', { name: '网络地图', exact: true }).click();
    await page.locator('.console-mobile-nav').waitFor({ state: 'hidden' });
    await page.setViewportSize({ width: 1440, height: 1024 });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.reload();
    await page.locator('.map-inspector h2').getByText(nodeNames[0], { exact: true }).waitFor();
    assert.equal(await page.locator('html').evaluate(element => element.classList.contains('app-dark')), false);
    await assertNoOverflow(page);
    await capture(page, 'asset-network-map-cn-light-desktop');
});
