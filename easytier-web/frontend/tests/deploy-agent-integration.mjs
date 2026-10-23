// Local integration against real Web + Agent binaries. Installs only a harmless
// fixture through deploy.sh; no Docker daemon or real applications are used.
// Requires Linux, Node 22+, DEPLOY_REPO. Root ownership commands are stubbed
// inside the isolated fixture PATH; all files stay in a temporary directory.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile, copyFile, cp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { createServer, connect } from 'node:net';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { createHash, randomUUID } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { request as playwrightRequest } from 'playwright';

assert.ok(process.env.DEPLOY_REPO, 'Set DEPLOY_REPO to the deploy checkout');
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const deploy = resolve(process.env.DEPLOY_REPO);
const agentBin = process.env.DEPLOY_AGENT_BINARY || join(deploy, 'agent/dist/deploy-agent-linux-amd64');
const webBin = process.env.EASYTIER_WEB_BINARY || join(repo, 'target/debug/easytier-web');
const dir = await mkdtemp(join(tmpdir(), 'deploy-agent-e2e-'));
const source = join(dir, 'source'), software = join(dir, 'software'), database = join(dir, 'web.db');
const children = [], contexts = [];
let web, agent;
let api, second;

async function freePort() {
    return new Promise((res, rej) => {
        const server = createServer(); server.once('error', rej);
        server.listen(0, '127.0.0.1', () => { const port = server.address().port; server.close(() => res(port)); });
    });
}
async function until(check, label) {
    for (let i = 0; i < 100; i++) {
        if (await check().catch(() => false)) return;
        await delay(100); // Test synchronization only; product code has no polling.
    }
    throw new Error(`Timed out: ${label}`);
}
function start(binary, args, env = process.env) {
    const child = spawn(binary, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
    child.output = ''; for (const stream of [child.stdout, child.stderr]) stream.on('data', b => { child.output += b; });
    child.on('error', e => { child.output += e.message; }); children.push(child); return child;
}
async function stop(child) {
    if (!child || child.exitCode !== null || child.signalCode !== null) return;
    const ended = new Promise(res => child.once('exit', res)); child.kill('SIGTERM'); await ended;
}
const webPort = await freePort(), agentPort = await freePort();
const base = `http://127.0.0.1:${webPort}`;
const startWeb = () => start(webBin, ['--db', database, '--api-server-addr', '127.0.0.1', '--api-server-port', String(webPort), '--config-server-protocol', 'tcp', '--config-server-port', '0', '--console-log-level', 'error']);
const readyWeb = () => until(async () => (await fetch(`${base}/api/v1/auth/check_login_status`)).status === 401, 'Web ready');
async function call(method, path, data, status = 200, client = api) {
    const response = await client.fetch(`/api/v1${path}`, { method, ...(data === undefined ? {} : { data }) });
    assert.equal(response.status(), status, `${method} ${path}: ${await response.text()}`);
    const body = await response.text(); return body ? JSON.parse(body) : undefined;
}
async function agentReady() {
    await until(() => new Promise(res => {
        const socket = connect(agentPort, '127.0.0.1');
        socket.once('connect', () => { socket.destroy(); res(true); }); socket.once('error', () => res(false));
    }), 'Agent ready');
}

try {
    await mkdir(join(source, 'agent'), { recursive: true });
    await mkdir(join(source, 'portainer/deploy'), { recursive: true });
    await mkdir(join(source, 'bin'), { recursive: true });
    for (const name of ['deploy.sh', '.env.example', '.secrets.example']) await copyFile(join(deploy, name), join(source, name));
    await cp(join(deploy, 'lib'), join(source, 'lib'), { recursive: true });
    const manifest = JSON.parse(await readFile(join(deploy, 'agent/catalog.json'), 'utf8')).filter(app => app.id === 'portainer');
    await writeFile(join(source, 'agent/catalog.json'), JSON.stringify(manifest));
    const installer = `#!/bin/bash
set -eu
test "$DEPLOY_LOCK_HELD" = 1
printf 'executed\\n' >> "$SOFTWARE_ROOT/executions"
printf ready > "$SOFTWARE_ROOT/installer-ready"
IFS= read -r release < "$SOFTWARE_ROOT/release-install"
cat "$SOFTWARE_ROOT/deploy/.secrets"
printf 'fixture completed\\n'
`;
    await writeFile(join(source, 'portainer/deploy/install.sh'), installer, { mode: 0o700 });
    await writeFile(join(source, 'portainer/deploy/update.sh'), installer, { mode: 0o700 });
    await writeFile(join(source, 'bin/docker'), '#!/bin/sh\n[ "$*" = "compose version" ]\n', { mode: 0o700 });
    await writeFile(join(source, 'bin/id'), '#!/bin/sh\nif [ "$*" = "-u" ]; then echo 0; else exec /usr/bin/id "$@"; fi\n', { mode: 0o700 });
    await writeFile(join(source, 'bin/chown'), '#!/bin/sh\nexit 0\n', { mode: 0o700 });
    const credential = ' exact ! " \' 空白 device credential ';
    await writeFile(join(dir, 'token'), credential, { mode: 0o600 });
    const configFile = join(dir, 'config.json');
    const initialized = spawnSync(agentBin, ['init', '--config', configFile, '--host', '127.0.0.1', '--listen', `127.0.0.1:${agentPort}`, '--deploy-dir', source, '--software-root', software, '--state-dir', join(dir, 'state'), '--token-file', join(dir, 'token')], { encoding: 'utf8' });
    assert.equal(initialized.status, 0, initialized.stderr);
    assert.ok(!initialized.stdout.includes(credential));
    const agentEnv = { ...process.env, PATH: `${join(source, 'bin')}:${process.env.PATH}` };
    const startAgent = () => start(agentBin, ['serve', '--config', configFile], agentEnv);
    agent = startAgent(); await agentReady(); web = startWeb(); await readyWeb();
    const db = new DatabaseSync(database);
    const hash = '$argon2id$v=19$m=4096,t=3,p=1$ZWFzeXRpZXItZTJlLXNhbHQ$FHbDjVElaTXuArgEEQrQrO4AdsnqNOcXGA5PFY8cG/s';
    const group = db.prepare("SELECT id FROM groups WHERE name='users'").get();
    for (const username of ['deploy-one', 'deploy-two']) {
        const user = db.prepare('INSERT INTO users(username,password) VALUES (?,?)').run(username, hash);
        db.prepare('INSERT INTO users_groups(user_id,group_id) VALUES (?,?)').run(user.lastInsertRowid, group.id);
    }
    db.close();
    for (const username of ['deploy-one', 'deploy-two']) {
        const context = await playwrightRequest.newContext({ baseURL: base }); contexts.push(context);
        const login = await context.post('/api/v1/auth/login', { data: { username, password: createHash('md5').update('e2e-password').digest('hex') } });
        assert.equal(login.status(), 200, 'login');
    }
    [api, second] = contexts;
    const ca = await readFile(join(dir, 'agent.crt'), 'utf8');
    const enrollment = { alias: 'Test NAS', endpoint: `https://127.0.0.1:${agentPort}`, ca_pem: ca, token: credential };
    const { id } = await call('POST', '/deploy/agents', enrollment);
    const path = `/deploy/agents/${id}`;
    const before = await call('GET', '/deploy/agents');
    assert.equal(before[0].identity, null, 'saving must not dial the Agent');
    assert.deepEqual(await call('GET', '/deploy/agents', undefined, 200, second), []);
    await call('POST', `${path}/probe`, undefined, 404, second);
    const wrongTrust = await call('POST', '/deploy/agents', { ...enrollment, ca_pem: '' });
    await call('POST', `/deploy/agents/${wrongTrust.id}/probe`, undefined, 502);
    const wrongToken = await call('POST', '/deploy/agents', { ...enrollment, token: 'wrong-device-credential' });
    await call('POST', `/deploy/agents/${wrongToken.id}/probe`, undefined, 502);
    const wrongHost = await call('POST', '/deploy/agents', { ...enrollment, endpoint: `https://localhost:${agentPort}` });
    await call('POST', `/deploy/agents/${wrongHost.id}/probe`, undefined, 502);
    const identity = await call('POST', `${path}/probe`);
    assert.equal(identity.protocol_version, 1);
    const catalog = await call('POST', `${path}/catalog`);
    assert.equal(catalog.applications[0].id, 'portainer');
    const services = await call('POST', `${path}/discover`);
    assert.ok(services.providers.docker && services.providers.systemd && services.providers.procd);
    const secret = 'chosen ! " \' 空格 password';
    const task = { task_id: randomUUID(), application_id: 'portainer', action: 'install', variant: 'docker-compose', catalog_revision: catalog.revision, parameters: { PORTAINER_HTTP_PORT: '7900' }, secrets: { PORTAINER_ADMIN_PASSWORD: secret } };
    await mkdir(software, { recursive: true });
    assert.equal(spawnSync('mkfifo', [join(software, 'release-install')]).status, 0);
    assert.equal((await call('POST', `${path}/tasks`, task)).status, 'accepted');
    await until(async () => (await readFile(join(software, 'installer-ready'), 'utf8')) === 'ready', 'fixture started');
    const cli = spawnSync('bash', [join(source, 'deploy.sh'), 'env'], { env: { ...agentEnv, SOFTWARE_ROOT: software }, timeout: 5000, encoding: 'utf8' });
    assert.equal(cli.status, 1, 'CLI must reject a concurrent Agent installation without waiting');
    assert.equal((await call('POST', `${path}/tasks`, task)).status, 'running');
    await writeFile(join(software, 'release-install'), 'continue\n');
    await until(async () => {
        const result = await call('POST', `${path}/tasks/${task.task_id}/refresh`);
        assert.notEqual(result.status, 'failed', result.message);
        return result.status === 'succeeded';
    }, 'fixture installation');
    assert.equal((await call('POST', `${path}/tasks`, task)).status, 'succeeded');
    const logs = await call('POST', `${path}/tasks/${task.task_id}/fetch-logs`);
    assert.ok(logs.text.includes('fixture completed'));
    assert.ok(!logs.text.includes(secret));
    assert.equal(await readFile(join(software, 'executions'), 'utf8'), 'executed\n');
    assert.ok((await readFile(join(software, 'deploy/.secrets'), 'utf8')).includes(secret));
    await call('POST', `${path}/tasks`, { ...task, action: 'update' }, 409);
    const publicData = JSON.stringify(await call('GET', `${path}/tasks`));
    assert.ok(!publicData.includes(secret) && !publicData.includes(credential));
    const stored = new DatabaseSync(database);
    assert.ok(!stored.prepare('SELECT request_cipher FROM deploy_tasks').get().request_cipher.includes(secret));
    assert.ok(!stored.prepare('SELECT credential FROM deploy_agents WHERE id=?').get(id).credential.includes(credential));
    stored.close();
    // Restart both processes: no replay, persisted encryption key/credentials work.
    await stop(agent); await stop(web); agent = startAgent(); await agentReady(); web = startWeb(); await readyWeb();
    // Existing Web session signing keys are process-local; log in again after restart.
    await call('POST', '/auth/login', { username: 'deploy-one', password: createHash('md5').update('e2e-password').digest('hex') });
    await call('POST', `${path}/probe`);
    assert.equal((await call('POST', `${path}/tasks/${task.task_id}/resubmit`)).status, 'succeeded');
    assert.equal(await readFile(join(software, 'executions'), 'utf8'), 'executed\n');
    // Replacing the destination device must never silently rebind the record.
    await stop(agent);
    const config = JSON.parse(await readFile(configFile, 'utf8')); config.agent_id = 'replacement-device';
    await writeFile(configFile, JSON.stringify(config), { mode: 0o600 }); agent = startAgent(); await agentReady();
    await call('POST', `${path}/probe`, undefined, 502);
    const saved = (await call('GET', '/deploy/agents')).find(row => row.id === id);
    assert.equal(saved.agent_id, identity.agent_id);
    assert.deepEqual(saved.services, services);
    await call('DELETE', path, undefined, 204);
    console.log('PASS: real TLS, authentication, tenant isolation, discovery, deploy.sh installation, durable deduplication, restart, logs and identity binding');
} finally {
    for (const child of children.reverse()) await stop(child);
    for (const context of contexts) await context.dispose();
    await rm(dir, { recursive: true, force: true });
}
