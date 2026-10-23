import type { CentralNetworkMember, NodeRouteInfo } from './api';
import {
    buildMapTopology,
    type MapCounter,
    type MapRuntimeSample,
    type MapTopologyLink,
    type MapTopologyRoute,
} from './map-topology.ts';

/** Only machines visible in the current user's registered-node inventory. */
export interface RegisteredMapNode {
    device_id: string;
    online: boolean;
    running?: boolean | null;
}

export interface RegisteredMapRuntimeSample extends MapRuntimeSample {
    instanceId: string;
}

const normalizedInstance = (value: string | undefined) => {
    const id = value?.trim().toLowerCase() ?? '';
    return id === '00000000-0000-0000-0000-000000000000' ? '' : id;
};
const validPeer = (id: number | undefined): id is number => id != null && Number.isInteger(id) && id >= 0 && id <= 0xffff_ffff;
const sampleKey = (sample: RegisteredMapRuntimeSample) => `${encodeURIComponent(sample.deviceId)}@${encodeURIComponent(sample.instanceId)}`;
const peerKey = (instance: string, peer: number) => JSON.stringify([instance, peer]);
const currentSample = (sample: RegisteredMapRuntimeSample, now: number) => !sample.failed && now - sample.sampledAt < 45_000;

/**
 * A peer ID belongs to one running instance's routing table, not to the account.
 * Resolve each observation using that table's remote instance ID and the remote
 * instance's own peer ID. IP addresses, hostnames and central network membership
 * never establish identity here. Ambiguous or missing evidence remains unresolved.
 *
 * The existing topology builder then operates on uniquely identified instances.
 * This preserves its counter arithmetic and transport-backed relay paths before
 * the resulting connections are folded onto the registered-machine globe.
 */
export function buildRegisteredMapTopology(
    nodes: RegisteredMapNode[],
    observations: RegisteredMapRuntimeSample[],
    previous: ReadonlyMap<string, MapCounter> = new Map(),
    now = Date.now(),
) {
    const nodesById = new Map(nodes.map(node => [node.device_id, node]));
    const latest = new Map<string, RegisteredMapRuntimeSample>();
    for (const observation of observations) {
        const instanceId = normalizedInstance(observation.instanceId);
        if (!instanceId || !nodesById.has(observation.deviceId) || !validPeer(observation.peerId) || !Number.isFinite(observation.sampledAt)) continue;
        const sample = { ...observation, instanceId };
        const key = sampleKey(sample);
        const old = latest.get(key);
        if (!old || sample.sampledAt >= old.sampledAt) latest.set(key, sample);
    }
    const entries = [...latest.entries()].sort(([a], [b]) => a.localeCompare(b));
    // These IDs exist only inside this invocation. Persistent counters below use
    // stable machine/instance identities, so inventory changes cannot mix rates.
    const graphPeerByKey = new Map(entries.map(([key], index) => [key, index + 1]));
    const candidates = new Map<string, string[]>();
    for (const [key, sample] of entries) {
        const identity = peerKey(sample.instanceId, sample.peerId!);
        candidates.set(identity, [...(candidates.get(identity) ?? []), key]);
    }
    const localRoutes = new Map<string, Map<number, Set<string>>>();
    for (const [key, sample] of entries) {
        const routes = new Map<number, Set<string>>();
        for (const route of sample.routes) {
            const instances = routes.get(route.peer_id) ?? new Set<string>();
            instances.add(normalizedInstance(route.inst_id));
            routes.set(route.peer_id, instances);
        }
        localRoutes.set(key, routes);
    }
    const resolve = (source: string, peer: number): string | undefined => {
        const instances = localRoutes.get(source)?.get(peer);
        if (!instances || instances.size !== 1) return;
        const instance = [...instances][0];
        if (!instance) return;
        const matches = candidates.get(peerKey(instance, peer));
        return matches?.length === 1 ? matches[0] : undefined;
    };

    const graphMembers: CentralNetworkMember[] = [];
    const graphSamples: MapRuntimeSample[] = [];
    const previousGraphCounters = new Map<string, MapCounter>();
    const counterKeys = new Map<string, string>();
    const unresolved = new Set<string>();
    for (const [key, sample] of entries) {
        const node = nodesById.get(sample.deviceId)!;
        graphMembers.push({
            member_id: key, device_id: key, hostname: null, hostname_override: null,
            virtual_ipv4: null, runtime_virtual_ipv4: null, version: null, error_msg: null,
            online: node.online && node.running !== false && currentSample(sample, now),
            running: node.running ?? null,
        });
        const routes: NodeRouteInfo[] = [];
        for (const route of sample.routes) {
            const target = resolve(key, route.peer_id);
            if (!target) continue;
            const next = resolve(key, route.next_hop_peer_id);
            routes.push({
                ...route,
                peer_id: graphPeerByKey.get(target)!,
                // Zero is reserved for a missing next hop. Graph peer IDs start at 1.
                next_hop_peer_id: next ? graphPeerByKey.get(next)! : 0,
            });
        }
        const peers: MapRuntimeSample['peers'] = [];
        for (const peer of sample.peers) {
            const connections = (peer.conns ?? []).filter(connection => !connection.is_closed);
            if (!connections.length) continue;
            const target = resolve(key, peer.peer_id);
            if (!target) {
                unresolved.add(JSON.stringify([key, peer.peer_id]));
                continue;
            }
            const targetSample = latest.get(target)!;
            const graphPeer = graphPeerByKey.get(target)!;
            peers.push({ ...peer, peer_id: graphPeer, conns: connections });
            for (const connection of connections) {
                const graphKey = `${key}:${graphPeer}:${connection.conn_id}`;
                const stableKey = `registered:${JSON.stringify([
                    sample.deviceId, sample.instanceId, sample.peerId,
                    targetSample.deviceId, targetSample.instanceId, peer.peer_id, connection.conn_id,
                ])}`;
                counterKeys.set(graphKey, stableKey);
                const counter = previous.get(stableKey);
                if (counter) previousGraphCounters.set(graphKey, counter);
            }
        }
        graphSamples.push({ ...sample, deviceId: key, peerId: graphPeerByKey.get(key)!, routes, peers });
    }
    const topology = buildMapTopology(graphMembers, graphSamples, previousGraphCounters, now);
    const counters = new Map<string, MapCounter>();
    for (const [key, counter] of topology.counters) {
        const stableKey = counterKeys.get(key);
        if (stableKey) counters.set(stableKey, counter);
    }

    const grouped = new Map<string, MapTopologyLink[]>();
    for (const link of topology.links) {
        const source = latest.get(link.source)!.deviceId;
        const target = latest.get(link.target)!.deviceId;
        // Two instances on one host do not create a geographical transport arc.
        if (source === target) continue;
        const pair = [source, target].sort();
        const id = `direct:${pair[0]}:${pair[1]}`;
        const oriented = {
            ...link, id, source: pair[0], target: pair[1],
            rx_bps: source === pair[0] ? link.rx_bps : link.tx_bps,
            tx_bps: source === pair[0] ? link.tx_bps : link.rx_bps,
        };
        grouped.set(id, [...(grouped.get(id) ?? []), oriented]);
    }
    const links: MapTopologyLink[] = [...grouped.values()].map(instances => {
        const live = instances.filter(link => !link.stale);
        const preferred = live.length ? live : instances;
        const latencies = preferred.flatMap(link => link.latency_ms == null ? [] : [link.latency_ms]);
        const losses = preferred.flatMap(link => link.lossRate == null ? [] : [link.lossRate]);
        return {
            ...instances[0],
            // Each instance connection was already deduplicated across its two
            // endpoint reports. Separate instance transports may now be summed.
            rx_bps: live.reduce((sum, link) => sum + link.rx_bps, 0),
            tx_bps: live.reduce((sum, link) => sum + link.tx_bps, 0),
            protocols: [...new Set(preferred.flatMap(link => link.protocols))],
            latency_ms: latencies.length ? Math.min(...latencies) : null,
            lossRate: losses.length ? Math.max(...losses) : null,
            sampledAt: Math.max(...preferred.map(link => link.sampledAt)),
            stale: live.length === 0,
        };
    });
    const routes: MapTopologyRoute[] = topology.routes.flatMap(route => {
        const source = latest.get(route.source)!.deviceId;
        const target = latest.get(route.target)!.deviceId;
        if (source === target) return [];
        const path = route.path.map(key => latest.get(key)!.deviceId)
            .filter((device, index, devices) => index === 0 || device !== devices[index - 1]);
        // Keep the instance-qualified route ID: the same hosts can carry more
        // than one real route, even when their geographic endpoints coincide.
        return [{ ...route, source, target, path }];
    });
    return { links, routes, counters, unresolvedPeers: unresolved.size };
}
