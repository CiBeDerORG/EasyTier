import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildRegisteredMapTopology } from '../src/modules/registered-map-topology.ts';

const nodes = (...ids) => ids.map(device_id => ({ device_id, online: true }));
const route = (peer_id, inst_id, next_hop_peer_id = peer_id, cost = 1) => ({
    peer_id, inst_id, next_hop_peer_id, cost, path_latency: 24, proxy_cidrs: [], hostname: 'duplicate-name', version: '',
    ipv4_addr: { address: { addr: 0x0a000002 }, network_length: 24 },
});
const conn = (rx = 100, tx = 200) => ({
    conn_id: 'same-connection-label', stats: { rx_bytes: String(rx), tx_bytes: String(tx), latency_us: '12000' },
    tunnel: { tunnel_type: 'tcp' },
});
const sample = (deviceId, instanceId, peerId, routes = [], peers = [], sampledAt = 1000) => ({
    deviceId, instanceId, peerId, routes, peers, sampledAt, failed: false,
});
const pair = (left, right, leftInstance, rightInstance, time = 1000, delta = 0) => [
    sample(left, leftInstance, 1, [route(2, rightInstance)], [{ peer_id: 2, conns: [conn(100 + delta, 200 + delta * 2)] }], time),
    sample(right, rightInstance, 2, [route(1, leftInstance)], [{ peer_id: 1, conns: [conn(200 + delta * 2, 100 + delta)] }], time),
];

test('registered map isolates overlapping peer IDs, addresses and hostnames by observed remote instance', () => {
    const data = [...pair('a', 'b', 'instance-a', 'instance-b'), ...pair('c', 'd', 'instance-c', 'instance-d')];
    const result = buildRegisteredMapTopology(nodes('a', 'b', 'c', 'd'), data, new Map(), 1000);
    assert.deepEqual(result.links.map(link => link.id).sort(), ['direct:a:b', 'direct:c:d']);
    assert.equal(result.unresolvedPeers, 0);
});

test('one shared instance UUID can identify different machines through their own peer IDs', () => {
    const result = buildRegisteredMapTopology(nodes('a', 'b'), pair('a', 'b', 'central-id', 'central-id'), new Map(), 1000);
    assert.equal(result.links.length, 1);
    assert.equal(result.links[0].id, 'direct:a:b');
});

test('missing, nil, conflicting or ambiguous instance evidence never guesses a target', () => {
    for (const remoteInstance of [undefined, '', '00000000-0000-0000-0000-000000000000']) {
        const data = [sample('a', 'instance-a', 1, [route(2, remoteInstance)], [{ peer_id: 2, conns: [conn()] }]), sample('b', 'instance-b', 2)];
        const result = buildRegisteredMapTopology(nodes('a', 'b'), data, new Map(), 1000);
        assert.deepEqual(result.links, []);
        assert.equal(result.unresolvedPeers, 1);
    }
    const ambiguous = [
        sample('a', 'instance-a', 1, [route(2, 'shared')], [{ peer_id: 2, conns: [conn()] }]),
        sample('b', 'shared', 2), sample('c', 'shared', 2),
    ];
    assert.deepEqual(buildRegisteredMapTopology(nodes('a', 'b', 'c'), ambiguous, new Map(), 1000).links, []);
    ambiguous[2].instanceId = 'other';
    ambiguous[0].routes.push(route(2, 'other'));
    assert.deepEqual(buildRegisteredMapTopology(nodes('a', 'b', 'c'), ambiguous, new Map(), 1000).links, []);
});

test('only registered nodes can appear, even when an observation proves another remote identity', () => {
    const result = buildRegisteredMapTopology(nodes('a'), pair('a', 'outside', 'instance-a', 'instance-outside'), new Map(), 1000);
    assert.deepEqual(result.links, []);
    assert.equal(result.unresolvedPeers, 1);
});

test('multiple instance transports sum once per connection and keep separate counter baselines', () => {
    const firstData = [...pair('a', 'b', 'a-1', 'b-1'), ...pair('a', 'b', 'a-2', 'b-2')];
    const first = buildRegisteredMapTopology(nodes('a', 'b'), firstData, new Map(), 1000);
    assert.equal(first.counters.size, 4, 'identical connection labels on separate instances remain distinct');
    const nextData = [...pair('a', 'b', 'a-1', 'b-1', 3000, 100), ...pair('a', 'b', 'a-2', 'b-2', 3000, 200)];
    const result = buildRegisteredMapTopology(nodes('a', 'b'), nextData, first.counters, 3000);
    assert.equal(result.links.length, 1);
    assert.equal(result.links[0].rx_bps, 150);
    assert.equal(result.links[0].tx_bps, 300, 'reports from both endpoints must not double traffic');
});

test('adding or removing unrelated samples does not change stable connection counter identity', () => {
    const first = buildRegisteredMapTopology(nodes('a', 'b'), pair('a', 'b', 'a-1', 'b-1'), new Map(), 1000);
    const next = buildRegisteredMapTopology(nodes('0', 'a', 'b'), [
        sample('0', 'earlier-sort-key', 2, [], [], 3000), ...pair('a', 'b', 'a-1', 'b-1', 3000, 100),
    ], first.counters, 3000);
    assert.equal(next.links[0].tx_bps, 100);
    const last = buildRegisteredMapTopology(nodes('a', 'b'), pair('a', 'b', 'a-1', 'b-1', 5000, 200), next.counters, 5000);
    assert.equal(last.links[0].tx_bps, 100);
});

test('relay traversal stays on the proven instance even when a host has another overlapping overlay', () => {
    const data = [
        sample('a', 'a-1', 1, [route(2, 'b-1'), route(3, 'c-1', 2, 2)], [{ peer_id: 2, conns: [conn()] }]),
        sample('b', 'b-1', 2, [route(1, 'a-1'), route(3, 'c-1')], [{ peer_id: 1, conns: [conn()] }, { peer_id: 3, conns: [conn()] }]),
        sample('c', 'c-1', 3),
        sample('b', 'b-2', 2, [route(3, 'd-2')], [{ peer_id: 3, conns: [conn()] }]),
        sample('d', 'd-2', 3),
    ];
    const result = buildRegisteredMapTopology(nodes('a', 'b', 'c', 'd'), data, new Map(), 1000);
    assert.deepEqual(result.routes[0].path, ['a', 'b', 'c']);
    assert.equal(result.routes[0].complete, true);
    assert.ok(!result.links.some(link => link.id === 'direct:a:c'));
    data[1].peers = data[1].peers.filter(peer => peer.peer_id !== 3);
    const partial = buildRegisteredMapTopology(nodes('a', 'b', 'c', 'd'), data, new Map(), 1000);
    assert.deepEqual(partial.routes[0].path, ['a', 'b']);
    assert.equal(partial.routes[0].complete, false, 'the other b instance cannot complete the route');
});

test('offline, expired and failed observations cannot animate registered-map traffic', () => {
    const data = pair('a', 'b', 'a-1', 'b-1');
    const first = buildRegisteredMapTopology(nodes('a', 'b'), data, new Map(), 1000);
    const nextData = pair('a', 'b', 'a-1', 'b-1', 3000, 100);
    const offline = nodes('a', 'b');
    offline[1].online = false;
    for (const result of [
        buildRegisteredMapTopology(offline, nextData, first.counters, 3000),
        buildRegisteredMapTopology(nodes('a', 'b'), nextData, first.counters, 60_000),
    ]) assert.ok(result.links.every(link => link.stale && link.rx_bps === 0 && link.tx_bps === 0));
    nextData[0].failed = true;
    const failed = buildRegisteredMapTopology(nodes('a', 'b'), nextData, first.counters, 3000);
    assert.equal(failed.counters.size, 0);
    const recovered = buildRegisteredMapTopology(nodes('a', 'b'), pair('a', 'b', 'a-1', 'b-1', 5000, 1000), failed.counters, 5000);
    assert.equal(recovered.links[0].tx_bps, 0, 'recovery starts a new observation interval');
});

test('uint64 counter precision and restarts remain safe through the instance adapter', () => {
    const base = 9_007_199_254_740_993_000n;
    const data = pair('a', 'b', 'a-1', 'b-1');
    for (const observation of data) observation.peers[0].conns[0].stats = { rx_bytes: String(base), tx_bytes: String(base) };
    const first = buildRegisteredMapTopology(nodes('a', 'b'), data, new Map(), 1000);
    for (const observation of data) {
        observation.sampledAt = 3000;
        observation.peers[0].conns[0].stats = { rx_bytes: String(base + 128n), tx_bytes: String(base + 128n) };
    }
    const next = buildRegisteredMapTopology(nodes('a', 'b'), data, first.counters, 3000);
    assert.equal(next.links[0].tx_bps, 64);
    const reset = buildRegisteredMapTopology(nodes('a', 'b'), pair('a', 'b', 'a-1', 'b-1', 5000), next.counters, 5000);
    assert.equal(reset.links[0].tx_bps, 0);
});
