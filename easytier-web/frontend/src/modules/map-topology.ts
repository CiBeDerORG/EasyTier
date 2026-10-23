import type { CentralNetworkMember, NodePeerInfo, NodeRouteInfo } from './api';
import type { MapSceneLink } from './network-space';

export interface MapRuntimeSample {
    deviceId: string;
    peerId?: number;
    routes: NodeRouteInfo[];
    peers: NodePeerInfo[];
    sampledAt: number;
    failed: boolean;
}

export interface MapCounter {
    rx: bigint;
    tx: bigint;
    at: number;
}

export interface MapTopologyLink extends MapSceneLink {
    protocols: string[];
    lossRate: number | null;
    sampledAt: number;
}

export interface MapTopologyRoute {
    id: string;
    source: string;
    target: string;
    path: string[];
    complete: boolean;
    latency_ms: number | null;
    stale: boolean;
}

function ipv4(addr: NodeRouteInfo['ipv4_addr']): string | undefined {
    const value = addr?.address?.addr;
    return value == null ? undefined : [24, 16, 8, 0].map(shift => (value >>> shift) & 255).join('.');
}

const finite = (value: number | undefined): value is number => value != null && Number.isFinite(value);
const usable = (sample: MapRuntimeSample, now: number) => !sample.failed && now - sample.sampledAt < 45_000;
const maxUint64 = (1n << 64n) - 1n;
const maxSafeInteger = BigInt(Number.MAX_SAFE_INTEGER);

function counterValue(value: unknown): bigint | null {
    // Missing scalar fields in a present protobuf stats message mean zero.
    if (value == null) return 0n;
    if (typeof value === 'number') {
        return Number.isSafeInteger(value) && value >= 0 ? BigInt(value) : null;
    }
    if (typeof value !== 'string' || !/^\d{1,20}$/.test(value)) return null;
    const parsed = BigInt(value);
    return parsed <= maxUint64 ? parsed : null;
}

function counterRate(current: bigint, previous: bigint, seconds: number): number {
    const delta = current - previous;
    // Subtract exact counters before converting. Long-running connections can
    // exceed JS's integer precision while their actual interval traffic is small.
    if (delta < 0n || delta > maxSafeInteger) return 0;
    const rate = Number(delta) / seconds;
    return Number.isFinite(rate) && rate <= Number.MAX_SAFE_INTEGER ? rate : 0;
}

function latencyValue(value: unknown): number | undefined {
    const parsed = counterValue(value);
    // Zero is the protobuf default, not evidence of a measured zero-latency link.
    return parsed != null && parsed > 0n && parsed <= maxSafeInteger ? Number(parsed) : undefined;
}

/** Build only edges supported by peer observations. A route is kept separately
 * from physical adjacency, so a multi-hop route never becomes a fake direct edge. */
export function buildMapTopology(
    members: CentralNetworkMember[],
    samples: MapRuntimeSample[],
    previous: ReadonlyMap<string, MapCounter> = new Map(),
    now = Date.now(),
) {
    const membersById = new Map(members.map(member => [member.device_id, member]));
    const peerCandidates = new Map<number, Set<string>>();
    const nominate = (peerId: number, deviceId: string) => {
        if (!Number.isInteger(peerId) || !membersById.has(deviceId)) return;
        const candidates = peerCandidates.get(peerId) ?? new Set<string>();
        candidates.add(deviceId);
        peerCandidates.set(peerId, candidates);
    };
    const ipCandidates = new Map<string, string[]>();
    for (const member of members) {
        const ip = member.runtime_virtual_ipv4 || member.virtual_ipv4 || member.allocated_ipv4;
        if (ip) ipCandidates.set(ip.split('/')[0], [...(ipCandidates.get(ip.split('/')[0]) ?? []), member.device_id]);
    }
    for (const sample of samples) {
        if (sample.peerId != null) nominate(sample.peerId, sample.deviceId);
        for (const route of sample.routes) {
            const ip = ipv4(route.ipv4_addr);
            const candidates = ip ? ipCandidates.get(ip) : undefined;
            // Never resolve identity by hostname, which is not unique.
            if (candidates?.length === 1) nominate(route.peer_id, candidates[0]);
        }
    }
    const deviceByPeer = new Map<number, string>();
    for (const [peerId, candidates] of peerCandidates) {
        if (candidates.size === 1) deviceByPeer.set(peerId, [...candidates][0]);
    }
    const links = new Map<string, MapTopologyLink>();
    const counters = new Map<string, MapCounter>();
    const unresolvedPeers = new Set<number>();
    const byDevice = new Map(samples.map(sample => [sample.deviceId, sample]));

    for (const sample of samples) {
        if (!membersById.has(sample.deviceId)) continue;
        for (const peer of sample.peers) {
            const target = deviceByPeer.get(peer.peer_id);
            if (!target) { unresolvedPeers.add(peer.peer_id); continue; }
            if (target === sample.deviceId) continue;
            const connections = (peer.conns ?? []).filter(conn => !conn.is_closed);
            if (!connections.length) continue;
            const source = sample.deviceId;
            const sorted = [source, target].sort();
            const id = `direct:${sorted[0]}:${sorted[1]}`;
            const stale = !usable(sample, now) || !membersById.get(source)?.online || !membersById.get(target)?.online;
            let rx = 0;
            let tx = 0;
            for (const conn of connections) {
                // A failed/expired sample may mix newly returned stats with an
                // older timestamp. Re-baseline after recovery rather than use it
                // as a counter observation and distort the next traffic rate.
                if (!conn.stats || stale) continue;
                const currentRx = counterValue(conn.stats.rx_bytes);
                const currentTx = counterValue(conn.stats.tx_bytes);
                if (currentRx == null || currentTx == null) continue;
                const key = `${source}:${peer.peer_id}:${conn.conn_id}`;
                const before = previous.get(key);
                if (!stale && before && sample.sampledAt > before.at && sample.sampledAt - before.at < 90_000) {
                    const seconds = (sample.sampledAt - before.at) / 1000;
                    // Counter resets/reconnects yield zero until a new baseline exists.
                    rx += counterRate(currentRx, before.rx, seconds);
                    tx += counterRate(currentTx, before.tx, seconds);
                }
                counters.set(key, { rx: currentRx, tx: currentTx, at: sample.sampledAt });
            }
            const latencies = connections.map(conn => latencyValue(conn.stats?.latency_us)).filter(finite);
            const losses = connections.map(conn => conn.loss_rate ?? 0).filter(finite);
            const link: MapTopologyLink = {
                id, source: sorted[0], target: sorted[1], kind: 'direct',
                latency_ms: latencies.length ? Math.min(...latencies) / 1000 : null,
                rx_bps: source === sorted[0] ? rx : tx,
                tx_bps: source === sorted[0] ? tx : rx,
                stale,
                protocols: [...new Set(connections.map(conn => conn.tunnel?.tunnel_type).filter((value): value is string => !!value))],
                lossRate: losses.length ? Math.max(...losses) : null,
                sampledAt: sample.sampledAt,
            };
            const existing = links.get(id);
            // Both peers may report the same connection. Prefer one current sample;
            // never sum both reports, which would double the real throughput.
            if (!existing || (existing.stale && !stale) || (existing.stale === stale && sample.sampledAt > existing.sampledAt)) links.set(id, link);
        }
    }

    const routes: MapTopologyRoute[] = [];
    for (const sample of samples) {
        for (const route of sample.routes.filter(route => route.cost > 1)) {
            const target = deviceByPeer.get(route.peer_id);
            if (!target || target === sample.deviceId) continue;
            const path = [sample.deviceId];
            let cursor: MapRuntimeSample | undefined = sample;
            let stale = !usable(sample, now);
            for (let hop = 0; hop < samples.length && cursor; hop++) {
                const nextRoute = cursor.routes.find(candidate => candidate.peer_id === route.peer_id);
                if (!nextRoute) break;
                const nextPeer = nextRoute.cost <= 1 ? route.peer_id : nextRoute.next_hop_peer_id;
                const nextDevice = deviceByPeer.get(nextPeer);
                // Evidence of a direct transport is required for every segment.
                if (!nextDevice || path.includes(nextDevice) || !cursor.peers.some(peer => peer.peer_id === nextPeer && (peer.conns ?? []).some(conn => !conn.is_closed))) break;
                path.push(nextDevice);
                stale ||= !usable(cursor, now);
                if (nextDevice === target) break;
                cursor = byDevice.get(nextDevice);
            }
            // The terminal node does not own another route hop, but its observed
            // state still determines whether this path is current and usable.
            stale ||= path.some(id => {
                const member = membersById.get(id);
                const sample = byDevice.get(id);
                return !member?.online || member.running === false || !sample || !usable(sample, now);
            });
            routes.push({
                id: `route:${sample.deviceId}:${target}`,
                source: sample.deviceId, target, path,
                complete: path[path.length - 1] === target,
                latency_ms: finite(route.path_latency) ? route.path_latency : null,
                stale,
            });
        }
    }
    return { links: [...links.values()], routes, counters, unresolvedPeers: unresolvedPeers.size };
}
