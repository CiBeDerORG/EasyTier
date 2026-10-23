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

after(async () => {
    await browser?.close();
    server?.kill();
});

const id = n => `00000000-0000-0000-0000-${n.toString(16).padStart(12, '0')}`;
const uuid = n => ({ part1: 0, part2: 0, part3: 0, part4: n });
const networkId = id(100);
const otherNetworkId = id(101);
const ipv4 = (n) => ({ address: { addr: (10 << 24) + (144 << 16) + n }, network_length: 24 });
const asMember = (n, hostname) => ({ member_id: id(200 + n), device_id: id(n), hostname, hostname_override: null, alias: null, virtual_ipv4: `10.144.0.${n}`, runtime_virtual_ipv4: `10.144.0.${n}`, online: n !== 5, running: n !== 5, version: '2.7.0', error_msg: null, proxy_cidrs: n === 1 ? ['192.168.31.0/24'] : [] });

function fixture() {
    const networks = [
        { network_id: networkId, display_name: '家庭网络 · 测试数据', network_name: 'home', networking_method: 'Manual', member_count: 5, online_member_count: 4 },
        { network_id: otherNetworkId, display_name: '实验网络 · 测试数据', network_name: 'lab', networking_method: 'Manual', member_count: 1, online_member_count: 1 },
    ];
    const members = {
        [networkId]: [asMember(1, '上海 · 家庭网关'), asMember(2, '北京 · 办公室'), asMember(3, '新加坡 · 中继'), asMember(4, '东京 · 节点'), asMember(5, '待定位电脑')],
        [otherNetworkId]: [asMember(6, '实验节点')],
    };
    const map = {
        [networkId]: {
            revision: 1,
            nodes: [
                { device_id: id(1), latitude: 31.23, longitude: 121.47, label: '上海 · 家庭网关', role: 'gateway', layout: 'home' },
                { device_id: id(2), latitude: 39.9, longitude: 116.4, label: '北京 · 办公室', role: 'node', layout: 'topology' },
                { device_id: id(3), latitude: 1.35, longitude: 103.82, label: '新加坡 · 中继', role: 'node', layout: 'topology' },
                { device_id: id(4), latitude: 35.68, longitude: 139.69, label: '东京 · 节点', role: 'node', layout: 'topology' },
            ],
            devices: [
                { id: id(300), gateway_device_id: id(1), name: 'NAS', address: '192.168.31.20', kind: 'server', room: 'study' },
                { id: id(301), gateway_device_id: id(1), name: '门口摄像头', address: '192.168.31.30', kind: 'camera', room: 'entry' },
            ],
        },
        [otherNetworkId]: { revision: 0, nodes: [{ device_id: id(6), latitude: 51.5, longitude: -0.12, label: '实验节点', role: 'node', layout: 'topology' }], devices: [] },
    };
    const service = (n, name, icon, url, lan_device_id = null) => ({ id: id(400 + n), network_id: networkId, device_id: id(1), lan_device_id, name, description: n === 1 ? '家庭自动化' : '常用应用', url, icon, group: n < 3 ? '智能家居' : '个人服务', show_in_navigation: true, open_in_new_tab: true, sort_order: n });
    const services = [service(1, 'Home Assistant', 'home', 'https://home.example.test/'), service(2, 'Immich', 'photos', 'https://photos.example.test/', id(300)), service(3, 'Jellyfin', 'media', 'https://media.example.test/', id(300)), service(4, 'Grafana', 'chart', 'https://metrics.example.test/')];
    return { networks, members, map, services, requests: [], writes: [], failures: new Set(), failWrites: false, mapConflict: false, delays: new Map(), peerCounter: 0, nextService: 500, externalConsole: false };
}

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
            HTMLCanvasElement.prototype.getContext = function (type, ...args) {
                return /webgl/i.test(type) ? null : original.call(this, type, ...args);
            };
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
    t.after(() => assert.deepEqual(errors, [], 'no uncaught browser errors'));
    await page.route('**/api_meta.js', route => route.fulfill({ contentType: 'text/javascript', body: 'window.apiMeta = { api_host: "" };' }));
    await page.route('**/api/v1/**', async route => {
        const request = route.request();
        const url = new URL(request.url());
        const path = url.pathname.replace('/api/v1', '');
        const method = request.method();
        const payload = request.postDataJSON();
        state.requests.push({ path, method, payload });
        const isMutation = method !== 'GET' && !path.endsWith('/proxy-rpc');
        if (isMutation) state.writes.push({ path, method, payload });
        if (state.delays.has(path)) await delay(state.delays.get(path));
        if (state.failures.has(path) || (isMutation && state.failWrites)) return route.fulfill({ status: 503, json: { message: 'Fixture request failed' } });
        let result = {};
        if (path === '/console-info') result = { username: 'browser-fixture', config_server_protocol: 'udp', config_server_port: 22020, webhook_auth: state.externalConsole };
        else if (path === '/summary') result = { device_count: 0 };
        else if (path === '/machines') result = { machines: [] };
        else if (path === '/blocked-devices') result = { blocked: [] };
        else if (path === '/networks') result = { networks: state.networks };
        else if (/^\/networks\/[^/]+\/members$/.test(path)) result = { members: state.members[path.split('/')[2]] || [], temporary_peers: [] };
        else if (/^\/networks\/[^/]+\/map$/.test(path)) {
            const net = path.split('/')[2];
            if (method === 'PUT') {
                if (state.mapConflict || payload.revision !== state.map[net].revision) return route.fulfill({ status: 409, json: { message: 'map has changed; reload before saving' } });
                const removed = state.map[net].devices.filter(device => !payload.devices.some(next => next.id === device.id));
                if (removed.some(device => state.services.some(service => service.network_id === net && service.lan_device_id === device.id))) {
                    return route.fulfill({ status: 409, json: { message: 'remove or reassign the associated services before removing or moving a LAN device' } });
                }
                state.map[net] = { ...structuredClone(payload), revision: payload.revision + 1 };
            }
            result = state.map[net];
        } else if (path === '/services') {
            if (method === 'POST') { result = { ...payload, id: id(state.nextService++) }; state.services.push(structuredClone(result)); }
            else result = { services: state.services.filter(service => !url.searchParams.has('network_id') || service.network_id === url.searchParams.get('network_id')) };
        } else if (path.startsWith('/services/')) {
            const serviceId = path.split('/')[2];
            if (method === 'DELETE') { state.services = state.services.filter(service => service.id !== serviceId); return route.fulfill({ status: 204 }); }
            const current = state.services.find(service => service.id === serviceId);
            if (!current) return route.fulfill({ status: 404, json: { message: 'Missing fixture service' } });
            if (method === 'PUT') Object.assign(current, payload);
            result = current;
        } else if (path.endsWith('/proxy-rpc')) {
            const source = Number.parseInt(path.split('/')[2].slice(-12), 16);
            const peers = source === 1 ? [2, 3] : source === 2 ? [1] : source === 3 ? [1, 4] : source === 4 ? [3] : [];
            if (payload.method_name === 'show_node_info') result = { node_info: { peer_id: source * 10, hostname: `node-${source}`, ipv4_addr: `10.144.0.${source}` } };
            else if (payload.method_name === 'list_peer') {
                state.peerCounter++;
                result = { peer_infos: peers.map(n => ({ peer_id: n * 10, conns: [{ conn_id: `${source}-${n}`, tunnel: { tunnel_type: 'udp' }, stats: { latency_us: String(n * 2000), rx_bytes: String(100000 + state.peerCounter * 10000), tx_bytes: String(200000 + state.peerCounter * 10000) } }] })) };
            } else if (payload.method_name === 'list_route') result = { routes: [1, 2, 3, 4].filter(n => n !== source).map(n => ({ peer_id: n * 10, hostname: `node-${n}`, ipv4_addr: ipv4(n), cost: peers.includes(n) ? 1 : 2, path_latency: n * 6, proxy_cidrs: [], version: '2.7.0', next_hop_peer_id: (peers.includes(n) ? n : source === 4 ? 3 : source === 2 ? 1 : 3) * 10 })) };
        }
        await route.fulfill({ json: result });
    });
    await page.goto(`${base}/#${route}`);
    await page.locator('.console-topbar').waitFor();
    return { page, state, context };
}

async function selectOption(page, label, option, scope = page) {
    await scope.getByRole('combobox', { name: label, exact: true }).click();
    await page.getByRole('option', { name: option, exact: true }).click();
}

async function capture(page, name) {
    if (!screenshotDir) return;
    await page.evaluate(() => { document.activeElement?.blur(); window.scrollTo(0, 0); });
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.screenshot({ path: `${screenshotDir}/${name}.png`, fullPage: true, animations: 'disabled' });
    if (page.viewportSize().width === 390) await page.screenshot({ path: `${screenshotDir}/${name}-viewport.png`, animations: 'disabled' });
}

async function assertNoOverflow(page) {
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'page must not overflow horizontally');
}

test('navigation cards open services, filter, and persist removal without deleting the service', async t => {
    const { page, state, context } = await open(t);
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 4);
    assert.equal(await page.locator('.service-card').getByText('Not checked', { exact: true }).count(), 4);
    const popupPromise = context.waitForEvent('page');
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).click();
    const popup = await popupPromise;
    await popup.waitForURL('https://photos.example.test/');
    await popup.getByRole('heading', { name: 'Fixture service reached' }).waitFor();
    await popup.close();
    await page.getByRole('textbox', { name: 'Search services' }).fill('Immich');
    assert.equal(await page.locator('.service-card').count(), 1);
    await page.getByRole('textbox', { name: 'Search services' }).fill('');
    await page.getByRole('button', { name: 'Service actions: Immich' }).click();
    await page.getByRole('menuitem', { name: 'Remove from navigation' }).click();
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).waitFor({ state: 'detached' });
    assert.equal(state.services.length, 4);
    assert.equal(state.services.find(s => s.name === 'Immich').show_in_navigation, false);
    await page.reload();
    await page.getByRole('link', { name: 'Open service: Home Assistant', exact: true }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 3);
    await page.goto(`${base}/#/h/services/manage`);
    await page.getByText('Immich', { exact: true }).waitFor();
    await page.getByRole('switch', { name: 'Show in service navigation: Immich', checked: false }).click();
    await page.getByRole('switch', { name: 'Show in service navigation: Immich', checked: true }).waitFor();
    assert.equal(state.services.find(service => service.name === 'Immich').show_in_navigation, true);
    await page.goto(`${base}/#/h/services`);
    await page.reload();
    await page.getByRole('link', { name: 'Open service: Immich', exact: true }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 4);
});

test('service editor validates destinations and URLs, preserves failures, and persists create/edit/delete', async t => {
    const { page, state } = await open(t, `/h/services/manage?create=1&network=${networkId}&node=${id(1)}`);
    const editor = page.locator('.service-editor-drawer');
    await editor.getByLabel('Service name', { exact: true }).fill('New service');
    await editor.getByLabel('Service address', { exact: true }).fill('javascript:alert(1)');
    await editor.getByRole('button', { name: 'Save service', exact: true }).click();
    await editor.getByText('Enter a valid HTTP or HTTPS URL without embedded credentials.').waitFor();
    assert.equal(state.writes.filter(w => w.path === '/services').length, 0);
    await editor.getByLabel('Service address', { exact: true }).fill('https://new.example.test/');
    await editor.getByLabel('Group', { exact: true }).fill('Automation');
    await selectOption(page, 'Device behind gateway', 'NAS · 192.168.31.20');
    state.failWrites = true;
    await editor.getByRole('button', { name: 'Save service', exact: true }).click();
    await editor.getByText('Fixture request failed', { exact: false }).waitFor();
    assert.equal(await editor.getByLabel('Service name', { exact: true }).inputValue(), 'New service');
    state.failWrites = false;
    await editor.getByRole('button', { name: 'Save service', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    const created = state.services.find(s => s.name === 'New service');
    assert.ok(created);
    assert.equal(created.network_id, networkId);
    assert.equal(created.device_id, id(1));
    assert.equal(created.lan_device_id, id(300));
    await page.waitForURL(`**/#/h/services/manage?network=${networkId}`);
    await page.goto(`${base}/#/h/services`);
    await page.getByRole('link', { name: 'Open service: New service' }).waitFor();
    await page.getByRole('button', { name: 'Service actions: New service' }).click();
    await page.getByRole('menuitem', { name: 'Edit service' }).click();
    await editor.getByLabel('Service name', { exact: true }).fill('Renamed service');
    await editor.getByRole('button', { name: 'Save service', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    assert.equal(state.services.find(s => s.id === created.id).name, 'Renamed service');
    await page.getByRole('button', { name: 'Service actions: Renamed service' }).click();
    await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Cancel', exact: true }).click();
    assert.ok(state.services.some(s => s.id === created.id), 'cancelled deletion keeps the service');
    await page.getByRole('button', { name: 'Service actions: Renamed service' }).click();
    await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
    await page.getByRole('alertdialog').getByRole('button', { name: 'Delete', exact: true }).click();
    await page.getByRole('button', { name: 'Renamed service', exact: true }).waitFor({ state: 'detached' });
    assert.ok(!state.services.some(s => s.id === created.id));
    await page.goto(`${base}/#/h/services`);
    await page.reload();
    await page.getByRole('link', { name: 'Open service: Home Assistant', exact: true }).waitFor();
    assert.equal(await page.getByRole('link', { name: 'Open service: Renamed service', exact: true }).count(), 0);
});

test('service navigation locates its exact network, node, and LAN device on the map', async t => {
    const { page, state } = await open(t);
    await page.getByRole('button', { name: 'Service actions: Immich' }).click();
    await page.getByRole('menuitem', { name: 'Locate node on map' }).click();
    await page.waitForURL(`**/#/h/map?network=${networkId}&node=${id(1)}&device=${id(300)}`);
    await page.locator('.map-device-row').getByText('192.168.31.20', { exact: false }).waitFor();
    assert.ok(state.requests.some(r => r.path === `/networks/${networkId}/map`));
    await page.waitForFunction(() => document.querySelector('canvas') || document.body.textContent.includes('3D rendering is unavailable'));
    assert.ok(state.requests.filter(r => r.path.endsWith('/proxy-rpc')).every(r => r.payload.payload.instance.id.part4 === 100), 'RPC instance must be network ID, never member ID');
});

test('sidebar collapse preserves accessible names and persists across reload', async t => {
    const { page } = await open(t);
    const nav = page.locator('.console-sidebar');
    await nav.getByRole('link', { name: 'Service navigation', exact: true }).waitFor();
    const links = await nav.getByRole('link').allTextContents();
    assert.ok(links.findIndex(s => s.includes('Service navigation')) < links.findIndex(s => s.includes('Dashboard')));
    await page.getByRole('button', { name: 'Collapse menu', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.console-sidebar').getBoundingClientRect().width <= 73);
    await nav.getByRole('link', { name: 'Network map', exact: true }).waitFor();
    await page.reload();
    await page.getByRole('button', { name: 'Expand menu', exact: true }).waitFor();
    assert.ok(await nav.evaluate(el => el.getBoundingClientRect().width <= 73));
    await page.getByRole('button', { name: 'Expand menu', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.console-sidebar').getBoundingClientRect().width >= 207);
});

test('external Console hides central navigation and makes no map or service requests', async t => {
    const { page, state } = await open(t, '/h/services', {}, state => { state.externalConsole = true; });
    await page.waitForURL('**/#/h');
    assert.equal(state.requests.some(r => r.path === '/services' || r.path.startsWith('/networks')), false);
    assert.equal(await page.locator('a[href*="/h/services"], a[href*="/h/map"]').count(), 0);
});

test('changing a service network resets gateway device associations and offers only that network’s members', async t => {
    const { page, state } = await open(t, `/h/services/manage?create=1&network=${networkId}&node=${id(1)}&device=${id(300)}`);
    const editor = page.locator('.service-editor-drawer');
    await editor.getByLabel('Service name', { exact: true }).fill('Lab service');
    await editor.getByLabel('Service address', { exact: true }).fill('https://lab.example.test/');
    await selectOption(page, 'Network', '实验网络 · 测试数据', editor);
    await editor.getByRole('combobox', { name: 'Network node', exact: true }).getByText('实验节点', { exact: true }).waitFor();
    assert.equal(await editor.getByRole('combobox', { name: 'Device behind gateway', exact: true }).count(), 0);
    await editor.getByRole('button', { name: 'Save service', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    const created = state.services.find(s => s.name === 'Lab service');
    assert.equal(created.network_id, otherNetworkId);
    assert.equal(created.device_id, id(6));
    assert.equal(created.lan_device_id, null);
});

test('service loading failures recover without showing a false empty state', async t => {
    const { page, state } = await open(t, '/h/services', {}, state => { state.failures.add('/services'); });
    await page.getByText('Fixture request failed', { exact: false }).waitFor();
    assert.equal(await page.getByRole('heading', { name: 'Your services start here' }).count(), 0);
    state.failures.clear();
    await page.getByRole('button', { name: 'Retry', exact: true }).click();
    await page.getByRole('link', { name: 'Open service: Home Assistant' }).waitFor();
    assert.equal(await page.locator('.service-card').count(), 4);
});

test('unlocated node coordinates persist and concurrent map edits retain the unsaved draft', async t => {
    const { page, state } = await open(t, `/h/map?network=${networkId}&node=${id(5)}`);
    await page.locator('.map-inspector h2').filter({ hasText: '待定位电脑' }).waitFor();
    await page.locator('.map-node-filters').getByRole('button', { name: 'Unlocated 1', exact: true }).click();
    assert.equal(await page.locator('.map-node-list-item').count(), 1);
    await page.locator('.map-coordinate-line').getByRole('button', { name: 'Edit node', exact: true }).click();
    const editor = page.locator('.map-edit-drawer:visible');
    await editor.getByLabel('Latitude', { exact: true }).fill('48.85');
    await editor.getByRole('button', { name: 'Save changes', exact: true }).click();
    await editor.getByText('Provide both latitude and longitude, or leave both empty.').waitFor();
    assert.equal(state.writes.length, 0);
    await editor.getByLabel('Longitude', { exact: true }).fill('2.35');
    await editor.getByRole('button', { name: 'Save changes', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    assert.deepEqual(state.map[networkId].nodes.find(node => node.device_id === id(5)).latitude, 48.85);
    await page.reload();
    await page.locator('.map-coordinate-line').getByText('48.8500°, 2.3500°', { exact: true }).waitFor();
    await page.locator('.map-coordinate-line').getByRole('button', { name: 'Edit node', exact: true }).click();
    await editor.getByLabel('Display name', { exact: true }).fill('Unsaved conflicting label');
    state.mapConflict = true;
    await editor.getByRole('button', { name: 'Save changes', exact: true }).click();
    await editor.getByText('This map was changed elsewhere. Reload the latest configuration before editing again.').waitFor();
    assert.equal(await editor.getByLabel('Display name', { exact: true }).inputValue(), 'Unsaved conflicting label');
    assert.equal(await editor.getByRole('button', { name: 'Save changes', exact: true }).isDisabled(), true);
    state.mapConflict = false;
    state.map[networkId].nodes.find(node => node.device_id === id(5)).label = 'Saved elsewhere';
    state.map[networkId].revision++;
    await editor.getByRole('button', { name: 'Reload latest', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    await page.locator('.map-inspector h2').filter({ hasText: 'Saved elsewhere' }).waitFor();
});

test('gateway LAN devices can be added, explored, linked to services, and removed', async t => {
    const { page, state } = await open(t, `/h/map?network=${networkId}&node=${id(1)}`);
    await page.getByRole('button', { name: 'Add device', exact: true }).click();
    const editor = page.locator('.map-edit-drawer:visible');
    await editor.getByLabel('Display name', { exact: true }).fill('Desk computer');
    await editor.getByLabel('Address or hostname', { exact: true }).fill('192.168.31.40');
    await editor.getByLabel('Device type').selectOption('desktop');
    await editor.getByLabel('Room').selectOption('study');
    await editor.getByRole('button', { name: 'Save changes', exact: true }).click();
    await editor.waitFor({ state: 'hidden' });
    const added = state.map[networkId].devices.find(device => device.name === 'Desk computer');
    assert.equal(added.gateway_device_id, id(1));
    assert.equal(added.room, 'study');
    assert.equal(added.kind, 'desktop');
    await page.locator('.map-device-row').filter({ hasText: 'Desk computer' }).waitFor();
    assert.ok((await page.getByRole('link', { name: 'Add service', exact: true }).getAttribute('href')).includes(`device=${added.id}`));
    await page.getByRole('button', { name: 'Explore node space', exact: true }).click();
    await page.locator('.map-space-caption').getByText('Illustrative floor plan', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Back to globe', exact: true }).click();
    await page.locator('[data-testid="network-globe"]').waitFor({ state: 'visible' });
    assert.ok(page.url().includes(`node=${id(1)}`));
    await page.getByRole('button', { name: 'Remove Desk computer', exact: true }).click();
    const removeDialog = page.getByRole('dialog', { name: 'Remove LAN device?', exact: true });
    await removeDialog.getByRole('button', { name: 'Remove', exact: true }).click();
    await removeDialog.waitFor({ state: 'hidden' });
    assert.ok(!state.map[networkId].devices.some(device => device.id === added.id));
    assert.ok(!page.url().includes(`device=${added.id}`));
});

test('deleting a referenced LAN device preserves configuration and opens the associated services', async t => {
    const { page, state } = await open(t, `/h/map?network=${networkId}&node=${id(1)}&device=${id(300)}`);
    await page.getByRole('button', { name: 'Remove NAS', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Remove LAN device?', exact: true });
    await dialog.getByRole('button', { name: 'Remove', exact: true }).click();
    await dialog.getByText('remove or reassign the associated services before removing or moving a LAN device', { exact: false }).waitFor();
    assert.ok(state.map[networkId].devices.some(device => device.id === id(300)));
    assert.equal(state.map[networkId].revision, 1);
    assert.equal(await dialog.getByRole('button', { name: 'Reload latest', exact: true }).count(), 0, 'association conflicts must not be mislabeled as revision conflicts');
    await dialog.getByRole('button', { name: 'Manage associated services', exact: true }).click();
    await page.waitForURL(`**/#/h/services/manage?edit=${id(402)}`);
    await page.locator('.service-editor-drawer').getByLabel('Service name', { exact: true }).waitFor();
    assert.equal(await page.locator('.service-editor-drawer').getByLabel('Service name', { exact: true }).inputValue(), 'Immich');
});

test('map renders measured direct links and verified relay paths, and marks failed telemetry stale', async t => {
    const { page, state } = await open(t, `/h/map?network=${networkId}&node=${id(1)}`);
    const direct = page.locator('.map-link-row').filter({ hasText: '北京 · 办公室' }).first();
    await direct.click();
    const details = page.locator('.map-path-details');
    await details.getByText('udp', { exact: true }).waitFor();
    assert.match(await details.textContent(), /0 B\/s/, 'first counters establish a baseline rather than fake throughput');
    await details.getByText(/^\d+\.\d ms$/, { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.waitForFunction(() => [...document.querySelectorAll('.map-path-details dd')].some(element => {
        const match = element.textContent.match(/↓ ([\d.]+) [kM]?B\/s/);
        return match && Number(match[1]) > 0;
    }));
    await page.locator('.map-link-row').filter({ hasText: '东京 · 节点' }).click();
    await details.getByText('上海 · 家庭网关 → 新加坡 · 中继 → 东京 · 节点', { exact: true }).waitFor();
    assert.equal(await details.getByText('Path information is incomplete.', { exact: false }).count(), 0);
    assert.ok(state.requests.filter(r => r.path.endsWith('/proxy-rpc')).every(r => r.payload.payload.instance.id.part4 === 100));
    state.failures.add(`/machines/${id(1)}/proxy-rpc`);
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.getByText('Some nodes could not be queried. Retained links are marked stale.', { exact: true }).waitFor();
    await page.locator('.map-link-row').filter({ hasText: '东京 · 节点' }).click();
    await details.getByText('Stale', { exact: true }).waitFor();
});

test('late responses from a previous network cannot replace the current selection', async t => {
    const { page, state } = await open(t, `/h/map?network=${networkId}`, {}, state => { state.delays.set(`/networks/${networkId}/map`, 700); });
    await page.getByRole('combobox', { name: 'Network', exact: true }).selectOption(otherNetworkId);
    await page.locator('.map-inspector h2').filter({ hasText: '实验节点' }).waitFor();
    await delay(850);
    assert.equal(await page.locator('.map-node-list-item').count(), 1);
    assert.match(await page.locator('.map-node-list-item').textContent(), /实验节点/);
    assert.ok(page.url().includes(`network=${otherNetworkId}`));
    assert.equal(await page.locator('.map-device-row').count(), 0);
    const currentRpc = state.requests.filter(r => r.path.endsWith('/proxy-rpc'));
    assert.ok(currentRpc.some(r => r.payload.payload.instance.id.part4 === 101));
});

test('WebGL failure keeps node configuration usable', async t => {
    const { page } = await open(t, `/h/map?network=${networkId}&node=${id(5)}`, { noWebGL: true });
    await page.getByText('3D rendering is unavailable. You can still use the node list and configuration controls.', { exact: true }).waitFor();
    await page.locator('.map-coordinate-line').getByRole('button', { name: 'Edit node', exact: true }).click();
    await page.locator('.map-edit-drawer:visible').getByLabel('Latitude', { exact: true }).fill('45');
    await page.locator('.map-edit-drawer:visible').getByLabel('Longitude', { exact: true }).fill('100');
    await page.locator('.map-edit-drawer:visible').getByRole('button', { name: 'Save changes', exact: true }).click();
    await page.locator('.map-coordinate-line').getByText('45.0000°, 100.0000°', { exact: true }).waitFor();
});

test('merged auto-refresh preference pauses map polling while manual refresh and later resumption remain usable', async t => {
    const { page, state } = await open(t, `/h/map?network=${networkId}`, { noWebGL: true, clock: true });
    await page.locator('.map-link-row').first().waitFor();
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    const refreshSwitch = page.getByRole('switch', { name: 'Auto refresh', exact: true });
    await refreshSwitch.uncheck();
    const samples = () => state.requests.filter(request => request.path.endsWith('/proxy-rpc') || request.path.endsWith('/members')).length;
    const paused = samples();
    await page.clock.fastForward(30_000);
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    assert.equal(samples(), paused, 'paused map does not sample on timer or visibility recovery');
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    assert.ok(samples() > paused, 'manual refresh samples while polling is paused');
    const beforeResume = samples();
    await refreshSwitch.check();
    const nextMembers = page.waitForResponse(response => new URL(response.url()).pathname === `/api/v1/networks/${networkId}/members`);
    await page.clock.fastForward(15_000);
    await nextMembers;
    await page.locator('.map-live-label .pi-spin').waitFor({ state: 'detached' });
    assert.ok(samples() > beforeResume, 're-enabled map resumes sampling');
});

test('Chinese dark desktop and mobile flows stay within the viewport', async t => {
    const { page } = await open(t, '/h/services', { language: 'cn' });
    await page.getByRole('link', { name: '访问服务: Immich', exact: true }).waitFor();
    await assertNoOverflow(page);
    await capture(page, 'fixture-service-navigation-cn-dark-desktop');
    await page.getByRole('button', { name: '收起菜单', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.console-sidebar').getBoundingClientRect().width <= 73);
    await capture(page, 'fixture-service-navigation-cn-dark-collapsed');
    await page.getByRole('button', { name: '展开菜单', exact: true }).click();
    await page.goto(`${base}/#/h/services/manage?edit=${id(402)}`);
    const serviceEditor = page.locator('.service-editor-drawer');
    await serviceEditor.getByLabel('服务名称', { exact: true }).waitFor();
    await serviceEditor.getByRole('combobox', { name: '网关下挂设备', exact: true }).waitFor();
    await capture(page, 'fixture-service-editor-cn-dark-desktop');
    await serviceEditor.getByRole('button', { name: '取消', exact: true }).click();
    await page.goto(`${base}/#/h/map?network=${networkId}&node=${id(1)}`);
    await page.locator('.map-inspector h2').filter({ hasText: '上海 · 家庭网关' }).waitFor();
    await page.locator('.network-globe .globe-loading').waitFor({ state: 'detached' });
    await page.locator('.map-link-row').first().waitFor();
    await page.locator('.map-stage canvas').waitFor();
    assert.ok((await page.locator('.map-space-preview').boundingBox()).height >= 200, 'gateway spatial preview must not collapse in a flex inspector');
    assert.ok((await page.getByRole('button', { name: '查看节点空间', exact: true }).boundingBox()).height >= 30, 'primary action must retain an accessible hit area');
    assert.ok(await page.locator('.map-stage canvas').evaluate(canvas => canvas.width > 0 && canvas.height > 0));
    await assertNoOverflow(page);
    await capture(page, 'fixture-network-map-cn-dark-desktop');
    await page.getByRole('button', { name: '查看节点空间', exact: true }).click();
    await page.locator('.map-stage .node-space-canvas').waitFor();
    await capture(page, 'fixture-node-space-cn-dark-desktop');
    await page.setViewportSize({ width: 390, height: 844 });
    await assertNoOverflow(page);
    await capture(page, 'fixture-node-space-cn-dark-mobile');
    await page.getByRole('button', { name: '返回地球', exact: true }).click();
    await assertNoOverflow(page);
    await capture(page, 'fixture-network-map-cn-dark-mobile');
    await page.goto(`${base}/#/h/services`);
    await page.getByRole('link', { name: '访问服务: Immich', exact: true }).waitFor();
    await assertNoOverflow(page);
    await capture(page, 'fixture-service-navigation-cn-dark-mobile');
    await page.getByRole('button', { name: '导航', exact: true }).click();
    await page.locator('.console-mobile-nav').getByRole('link', { name: '网络地图', exact: true }).click();
    await page.locator('.console-mobile-nav').waitFor({ state: 'hidden' });
    await page.waitForURL('**/#/h/map**');
    await page.setViewportSize({ width: 1440, height: 1024 });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.reload();
    await page.locator('.map-inspector h2').filter({ hasText: '上海 · 家庭网关' }).waitFor();
    await page.locator('.network-globe .globe-loading').waitFor({ state: 'detached' });
    assert.equal(await page.locator('html').evaluate(element => element.classList.contains('app-dark')), false);
    await assertNoOverflow(page);
    await capture(page, 'fixture-network-map-cn-light-desktop');
    await page.goto(`${base}/#/h/services`);
    await page.getByRole('link', { name: '访问服务: Immich', exact: true }).waitFor();
    await capture(page, 'fixture-service-navigation-cn-light-desktop');
});
