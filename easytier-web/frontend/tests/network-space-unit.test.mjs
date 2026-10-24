import assert from 'node:assert/strict';
import { test } from 'node:test';
import { buildMapTopology } from '../src/modules/map-topology.ts';
import { safeServiceUrl } from '../src/modules/service-utils.ts';
import { geographicPosition, GeographicArc } from '../src/components/map/globe-geometry.ts';

const members = ['a', 'b', 'c'].map((device_id, i) => ({ device_id, online: true, runtime_virtual_ipv4: `10.0.0.${i + 1}` }));
const conn = (id, rx, tx) => ({ conn_id: id, stats: { rx_bytes: rx, tx_bytes: tx, latency_us: 12_000 }, tunnel: { tunnel_type: 'tcp' }, loss_rate: 0 });
const route = (peer_id, next_hop_peer_id, cost = 1) => ({ peer_id, next_hop_peer_id, cost, path_latency: 24 });
function snapshot(time, delta = 0) {
    return [
        { deviceId: 'a', peerId: 1, sampledAt: time, failed: false, peers: [{ peer_id: 2, conns: [conn('ab', 100 + delta, 200 + delta * 2)] }], routes: [route(2, 2), route(3, 2, 2)] },
        { deviceId: 'b', peerId: 2, sampledAt: time, failed: false, peers: [{ peer_id: 1, conns: [conn('ba', 200 + delta * 2, 100 + delta)] }, { peer_id: 3, conns: [conn('bc', 100, 100)] }], routes: [route(1, 1), route(3, 3)] },
        { deviceId: 'c', peerId: 3, sampledAt: time, failed: false, peers: [{ peer_id: 2, conns: [conn('cb', 100, 100)] }], routes: [route(2, 2)] },
    ];
}

test('relay routes retain every observed hop without inventing a direct edge', () => {
    const result = buildMapTopology(members, snapshot(1000), new Map(), 1000);
    assert.deepEqual(result.links.map(link => link.id).sort(), ['direct:a:b', 'direct:b:c']);
    assert.deepEqual(result.routes[0].path, ['a', 'b', 'c']);
    assert.equal(result.routes[0].complete, true);
    assert.equal(result.links[0].latency_ms, 12);
    const partial = snapshot(1000);
    partial[1].peers = partial[1].peers.filter(peer => peer.peer_id !== 3);
    const missing = buildMapTopology(members, partial, new Map(), 1000);
    assert.deepEqual(missing.routes[0].path, ['a', 'b']);
    assert.equal(missing.routes[0].complete, false);
});

test('traffic uses counter deltas, deduplicates two peer reports and handles resets', () => {
    const first = buildMapTopology(members, snapshot(1000), new Map(), 1000);
    assert.equal(first.links[0].tx_bps, 0);
    const next = buildMapTopology(members, snapshot(3000, 100), first.counters, 3000);
    const link = next.links.find(link => link.id === 'direct:a:b');
    assert.equal(link.rx_bps, 50);
    assert.equal(link.tx_bps, 100);
    const reset = buildMapTopology(members, snapshot(5000), next.counters, 5000);
    assert.equal(reset.links[0].rx_bps, 0);
    assert.equal(reset.links[0].tx_bps, 0);
});

function wireStatsSample(time, stats) {
    return [
        { deviceId: 'a', peerId: 1, sampledAt: time, failed: false, peers: [{ peer_id: 2, conns: [{ conn_id: 'ab', stats }] }], routes: [] },
        { deviceId: 'b', peerId: 2, sampledAt: time, failed: false, peers: [], routes: [] },
    ];
}

test('protobuf uint64 strings produce real traffic and latency, including omitted zero fields', () => {
    const first = buildMapTopology(members, wireStatsSample(1000, {}), new Map(), 1000);
    assert.equal(first.links[0].latency_ms, null, 'an omitted zero latency is not a measured latency');
    assert.equal(first.links[0].lossRate, 0, 'protobuf omits the zero loss rate');
    assert.equal(first.counters.get('a:2:ab').rx, 0n);
    const next = buildMapTopology(members, wireStatsSample(3000, { tx_bytes: '200', latency_us: '12000' }), first.counters, 3000);
    assert.equal(next.links[0].rx_bps, 0);
    assert.equal(next.links[0].tx_bps, 100);
    assert.equal(next.links[0].latency_ms, 12);
    const both = buildMapTopology(members, wireStatsSample(5000, { rx_bytes: '1000', tx_bytes: '4200', latency_us: '18000' }), next.counters, 5000);
    assert.equal(both.links[0].rx_bps, 500);
    assert.equal(both.links[0].tx_bps, 2000);
    assert.equal(both.links[0].latency_ms, 18);
});

test('large protobuf counters retain exact small deltas without unsafe-number spikes', () => {
    const base = 9_007_199_254_740_993_000n;
    const first = buildMapTopology(members, wireStatsSample(1000, { rx_bytes: String(base), tx_bytes: String(base) }), new Map(), 1000);
    const next = buildMapTopology(members, wireStatsSample(3000, { rx_bytes: String(base + 128n), tx_bytes: String(base + 256n) }), first.counters, 3000);
    assert.equal(next.links[0].rx_bps, 64);
    assert.equal(next.links[0].tx_bps, 128);
    assert.equal(next.counters.get('a:2:ab').rx, base + 128n);

    const zero = buildMapTopology(members, wireStatsSample(1000, {}), new Map(), 1000);
    const tooLargeDelta = buildMapTopology(members, wireStatsSample(3000, { tx_bytes: '18446744073709551615' }), zero.counters, 3000);
    assert.equal(tooLargeDelta.links[0].tx_bps, 0);
    const reset = buildMapTopology(members, wireStatsSample(5000, {}), tooLargeDelta.counters, 5000);
    assert.equal(reset.links[0].tx_bps, 0);

    for (const value of [Number.MAX_SAFE_INTEGER + 1, -1, '18446744073709551616', '-1', '1.5', 'NaN', 'Infinity', '']) {
        const invalid = buildMapTopology(members, wireStatsSample(3000, { rx_bytes: value }), zero.counters, 3000);
        assert.equal(invalid.links[0].rx_bps, 0, `invalid uint64 ${value} cannot animate traffic`);
        assert.equal(invalid.counters.size, 0, `invalid uint64 ${value} cannot poison a baseline`);
    }
    const missing = buildMapTopology(members, wireStatsSample(3000, undefined), zero.counters, 3000);
    assert.equal(missing.counters.size, 0, 'absent stats is unknown rather than an observed zero');
});

test('omitted protobuf connection arrays do not invent edges or interrupt aggregation', () => {
    const data = snapshot(1000);
    delete data[0].peers[0].conns;
    data[1].peers = [{ peer_id: 1 }, { peer_id: 3 }];
    data[2].peers = [{ peer_id: 2 }];
    const result = buildMapTopology(members, data, new Map(), 1000);
    assert.deepEqual(result.links, []);
    assert.deepEqual(result.routes[0].path, ['a']);
    assert.equal(result.routes[0].complete, false);
});

test('a relay path is stale when its terminal node is offline or failed', () => {
    const data = snapshot(1000);
    const offline = members.map(member => ({ ...member, online: member.device_id !== 'c' }));
    const result = buildMapTopology(offline, data, new Map(), 1000);
    assert.equal(result.routes[0].complete, true, 'historical hop evidence is still preserved');
    assert.equal(result.routes[0].stale, true);
    data[2].failed = true;
    const failed = buildMapTopology(members, data, new Map(), 1000);
    assert.equal(failed.routes[0].stale, true);
});

test('offline and expired observations cannot animate traffic', () => {
    const baseline = buildMapTopology(members, snapshot(1000), new Map(), 1000);
    const offline = members.map(member => ({ ...member, online: member.device_id !== 'b' }));
    const result = buildMapTopology(offline, snapshot(3000, 100), baseline.counters, 3000);
    assert.ok(result.links.every(link => link.stale && link.rx_bps === 0 && link.tx_bps === 0));
    const expired = buildMapTopology(members, snapshot(3000, 100), baseline.counters, 60_000);
    assert.ok(expired.links.every(link => link.stale && link.tx_bps === 0));
});

test('failed partial samples cannot become a traffic baseline when a node recovers', () => {
    const first = buildMapTopology(members, wireStatsSample(1000, { tx_bytes: '100' }), new Map(), 1000);
    const partial = wireStatsSample(1000, { tx_bytes: '10000' });
    partial[0].failed = true; // New stats arrived, but another RPC failed, so timestamp was retained.
    const failed = buildMapTopology(members, partial, first.counters, 3000);
    assert.equal(failed.counters.size, 0);
    const recovered = buildMapTopology(members, wireStatsSample(5000, { tx_bytes: '20000' }), failed.counters, 5000);
    assert.equal(recovered.links[0].tx_bps, 0, 'recovery establishes a fresh baseline');
    const measured = buildMapTopology(members, wireStatsSample(7000, { tx_bytes: '20200' }), recovered.counters, 7000);
    assert.equal(measured.links[0].tx_bps, 100);
});

test('ambiguous identities and unmanaged peers never become guessed map nodes', () => {
    const data = snapshot(1000);
    data[2].peerId = 2;
    data[1].peers = []; // No independent observation of the a–b transport.
    data[0].peers.push({ peer_id: 99, conns: [conn('unknown', 0, 0)] });
    const result = buildMapTopology(members, data, new Map(), 1000);
    assert.ok(!result.links.some(link => link.source === 'a' && link.target === 'b'));
    assert.ok(result.unresolvedPeers > 0);
    assert.ok(!result.links.some(link => link.source === '99' || link.target === '99'));
});

test('geographic arcs cross the date line and remain above the globe at the antipodes', () => {
    assert.ok(geographicPosition(90, 0).distanceTo(geographicPosition(90, 150)) < 1e-12);
    const dateLine = new GeographicArc(geographicPosition(0, 179), geographicPosition(0, -179));
    assert.ok(dateLine.getPoint(.5).x < -1, 'short path passes the date line, not Greenwich');
    for (const end of [geographicPosition(0, 180), geographicPosition(0, 0)]) {
        const arc = new GeographicArc(geographicPosition(0, 0), end);
        for (let step = 0; step <= 20; step++) {
            const point = arc.getPoint(step / 20);
            assert.ok(Number.isFinite(point.x + point.y + point.z));
            assert.ok(point.length() > 1, 'the route remains above the surface, including its ground-level endpoints');
        }
        assert.ok(arc.getPoint(1).clone().normalize().distanceTo(end) < 1e-9);
    }
});

test('service links accept web destinations and reject executable schemes or credentials', () => {
    assert.equal(safeServiceUrl(' http://10.144.0.1:8123 '), 'http://10.144.0.1:8123/');
    assert.equal(safeServiceUrl('https://[::1]:8443/'), 'https://[::1]:8443/');
    for (const url of ['javascript:alert(1)', 'data:text/html,x', '//example.test', 'file:///tmp/a', 'https://user:password@example.test', 'not a URL']) assert.equal(safeServiceUrl(url), null);
});
