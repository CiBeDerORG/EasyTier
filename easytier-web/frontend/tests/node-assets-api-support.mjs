import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Suites sharing apiNode must run sequentially (--test-concurrency=1).
const fixturePath = process.env.NODE_ASSETS_E2E_FIXTURE_FILE;
assert.ok(fixturePath, 'NODE_ASSETS_E2E_FIXTURE_FILE is required');
export const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));
assert.equal(fixture.isolated, true, 'Only disposable test fixtures are supported');
assert.equal(new URL(fixture.baseURL).hostname, '127.0.0.1');
export const scope = { owner_user_id: fixture.owner.id, machine_id: fixture.apiNode.machine_id };
export const fresh = body => ({ ...body, request_id: randomUUID() });
export const instance = name => fixture.instances.apiNode[name].instance_id;

export async function request(cookie, method, path, query = {}, body, expected = 200) {
    const url = new URL(`/api/v1${path}`, fixture.baseURL);
    for (const [key, value] of Object.entries(query)) url.searchParams.set(key, String(value));
    const response = await fetch(url, {
        method, headers: { ...(cookie ? { cookie } : {}), ...(body === undefined ? {} : { 'content-type': 'application/json' }) },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await response.text();
    assert.equal(response.status, expected, `${method} ${url.pathname}: ${text}`);
    return text ? JSON.parse(text) : null;
}
export async function login(user) {
    const response = await fetch(`${fixture.baseURL}/api/v1/auth/login`, {
        method: 'POST', headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ username: user.username, password: createHash('md5').update(user.password).digest('hex') }),
    });
    assert.equal(response.status, 200, 'Fixture account login');
    return response.headers.getSetCookie().map(value => value.split(';')[0]).join('; ');
}
export async function bindingPreview(cookie, subject, instanceId, operation = 'attach', extra = {}, context = scope) {
    const response = await request(cookie, 'POST', '/assets/validation/binding', context,
        { subject, operation, instance_id: instanceId, ...extra });
    assert.equal(response.can_save, true, JSON.stringify(response.field_errors));
    assert.ok(response.preview_version);
    return response;
}
export async function addressPreview(cookie, spec, id = null, context = scope) {
    const response = await request(cookie, 'POST', '/assets/validation/instance-address', context,
        { subject: { resource_kind: 'device', id }, address_spec: spec });
    assert.equal(response.can_save, true, JSON.stringify(response.field_errors));
    assert.ok(response.preview_version);
    return response;
}
export async function createGroup(cookie, instanceId, name = 'API gateway', context = scope) {
    const preview = await bindingPreview(cookie, { resource_kind: 'gateway', id: null }, instanceId, 'attach', {}, context);
    const input = fresh({ name, remarks: null, binding: { instance_id: instanceId }, preview_version: preview.preview_version });
    const record = await request(cookie, 'POST', '/assets/gateway-groups', context, input);
    return { record, input, preview };
}
export async function createDevice(cookie, spec, metadata = {}, context = scope) {
    const preview = await addressPreview(cookie, spec, null, context);
    const input = fresh({ name: 'API device', model: null, remarks: null, device_type_id: null,
        ...metadata, address_spec: spec, preview_version: preview.preview_version });
    const record = await request(cookie, 'POST', '/assets/devices', context, input);
    return { record, input, preview };
}
export async function groupRecord(cookie, id, context = scope) {
    return (await request(cookie, 'GET', '/assets/gateway-groups', context)).items.find(group => group.id === id);
}
export const deviceRecord = (cookie, id) => request(cookie, 'GET', `/assets/devices/${id}`, scope);
export function mutateInstance(name, action, patch = {}, node = 'apiNode') {
    execFileSync('python3', [fileURLToPath(new URL('./node-instance-binding-fixture.py', import.meta.url)), 'mutate', '--fixture', fixturePath],
        { input: JSON.stringify({ instance: name, action, patch, node }), stdio: ['pipe', 'pipe', 'pipe'] });
}
export function restoreInstance(name) {
    const original = fixture.instances.apiNode[name];
    mutateInstance(name, 'patch', { network_name: original.name, virtual_ipv4: original.ipv4,
        proxy_cidrs: original.rules, dhcp: false, dhcp_ipv4_cidr: null });
}
export function resetApiFixture() {
    execFileSync('python3', [fileURLToPath(new URL('./node-instance-binding-fixture.py', import.meta.url)), 'reset-api', '--fixture', fixturePath],
        { stdio: ['ignore', 'pipe', 'pipe'] });
}
