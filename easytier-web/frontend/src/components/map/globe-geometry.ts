import { Curve, MathUtils, Vector3 } from 'three';

/** Equirectangular coordinates matching Three's SphereGeometry UVs. */
export function geographicPosition(latitude: number, longitude: number, radius = 1): Vector3 {
    const lat = MathUtils.degToRad(latitude);
    const lon = MathUtils.degToRad(longitude);
    return new Vector3(
        Math.cos(lat) * Math.cos(lon),
        Math.sin(lat),
        -Math.cos(lat) * Math.sin(lon),
    ).multiplyScalar(radius);
}

export interface ScreenAnchor { id: string; x: number; y: number }
export interface DisplayAnchor extends ScreenAnchor { originalX: number; originalY: number; expanded: boolean }

/** Screen-only separation. The caller keeps geographic coordinates unchanged. */
export function separateNearbyMarkers(anchors: ScreenAnchor[], width: number, height: number,
    isAvailable: (x: number, y: number) => boolean = () => true): DisplayAnchor[] {
    const placed: DisplayAnchor[] = [];
    for (const anchor of [...anchors].sort((a, b) => a.id.localeCompare(b.id))) {
        let x = anchor.x, y = anchor.y;
        const clear = (px: number, py: number) => px >= 12 && py >= 12 && px <= width - 12 && py <= height - 12
            && isAvailable(px, py) && placed.every(other => Math.hypot(other.x - px, other.y - py) >= 18);
        if (!clear(x, y)) {
            let found = false;
            for (let ring = 1; ring <= 12 && !found; ring++) {
                const count = ring * 8;
                for (let step = 0; step < count; step++) {
                    const angle = step / count * Math.PI * 2 - Math.PI / 2;
                    const px = anchor.x + Math.cos(angle) * ring * 20;
                    const py = anchor.y + Math.sin(angle) * ring * 20;
                    if (clear(px, py)) { x = px; y = py; found = true; break; }
                }
            }
        }
        placed.push({ ...anchor, x, y, originalX: anchor.x, originalY: anchor.y,
            expanded: Math.hypot(x - anchor.x, y - anchor.y) > 1 });
    }
    return placed;
}

export interface ScreenRoute {
    id: string;
    source: { x: number; y: number };
    target: { x: number; y: number };
    displaySource?: { x: number; y: number };
    displayTarget?: { x: number; y: number };
}
export interface RouteLane { lane: number; direction: 1 | -1 }

/** Nearby endpoint pairs share a bundle; reversed links join the same bundle.
 * Stable IDs, rather than render order, determine each display lane. */
export function assignRouteLanes(routes: ScreenRoute[], threshold = 28): Map<string, RouteLane> {
    const sorted = [...routes].sort((a, b) => a.id.localeCompare(b.id));
    const parents = sorted.map((_, index) => index);
    const find = (index: number): number => {
        while (parents[index] !== index) { parents[index] = parents[parents[index]!]!; index = parents[index]!; }
        return index;
    };
    const near = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y) < threshold;
    for (let a = 0; a < sorted.length; a++) for (let b = 0; b < a; b++) {
        const left = sorted[a]!, right = sorted[b]!;
        if ((near(left.source, right.source) && near(left.target, right.target))
            || (near(left.source, right.target) && near(left.target, right.source))) parents[find(a)] = find(b);
    }
    const groups = new Map<number, ScreenRoute[]>();
    sorted.forEach((route, index) => { const key = find(index); groups.set(key, [...(groups.get(key) ?? []), route]); });
    const result = new Map<string, RouteLane>();
    const vector = (route: ScreenRoute) => {
        const source = route.displaySource ?? route.source, target = route.displayTarget ?? route.target;
        return { x: target.x - source.x, y: target.y - source.y };
    };
    for (const group of groups.values()) {
        // IDs order lanes, but cannot determine geometry orientation: different
        // nearby endpoint pairs may run in opposite directions despite their IDs.
        // Use expanded endpoints for direction when true anchors coincide; only
        // the original anchors determine bundle membership.
        const representative = [...group].sort((a, b) =>
            Math.hypot(vector(b).x, vector(b).y) - Math.hypot(vector(a).x, vector(a).y) || a.id.localeCompare(b.id))[0]!;
        const reference = vector(representative);
        group.forEach((route, index) => {
            const current = vector(route), dot = current.x * reference.x + current.y * reference.y;
            result.set(route.id, { lane: index - (group.length - 1) / 2, direction: dot < 0 ? -1 : 1 });
        });
    }
    return result;
}

export interface GeographicArcOptions { laneOffset?: number; maximumHeight?: number; loopRadius?: number }

/** A logical network edge following the shortest great circle, above the Earth. */
export class GeographicArc extends Curve<Vector3> {
    private readonly origin: Vector3;
    private readonly destination: Vector3;
    private readonly axis: Vector3;
    private readonly angle: number;
    private readonly height: number;
    private readonly radius: number;
    private readonly laneOffset: number;
    private readonly tangent: Vector3;
    private readonly loopRadius: number;

    constructor(from: Vector3, to: Vector3, options: GeographicArcOptions = {}) {
        super();
        this.origin = from.clone().normalize();
        const destination = to.clone().normalize();
        this.destination = destination;
        this.angle = Math.acos(MathUtils.clamp(this.origin.dot(destination), -1, 1));
        this.axis = new Vector3().crossVectors(this.origin, destination);
        // Antipodal points have many equally short paths. Choose a stable plane.
        if (this.axis.lengthSq() < 1e-20) {
            this.axis.crossVectors(this.origin, Math.abs(this.origin.y) < .9 ? new Vector3(0, 1, 0) : new Vector3(1, 0, 0));
        }
        this.axis.normalize();
        this.tangent = new Vector3().crossVectors(this.axis, this.origin).normalize();
        this.radius = Math.max(1.000012, Math.min(from.length(), to.length()));
        this.laneOffset = options.laneOffset ?? 0;
        this.loopRadius = options.loopRadius ?? .003;
        this.height = Math.min(options.maximumHeight ?? .6,
            Math.max(.000002, this.angle * .16, this.angle < 1e-7 ? this.loopRadius * .5 : 0) + Math.abs(this.laneOffset) * .15);
    }

    override getPoint(t: number, target = new Vector3()): Vector3 {
        if (t <= 0) return target.copy(this.origin).multiplyScalar(this.radius);
        if (t >= 1) return target.copy(this.destination).multiplyScalar(this.radius);
        const wave = Math.sin(Math.PI * t);
        target.copy(this.origin);
        if (this.angle < 1e-7) {
            // Co-located endpoints still have a real relationship: render a
            // small anchored loop instead of silently dropping that edge.
            target.addScaledVector(this.tangent, Math.sin(Math.PI * 2 * t) * this.loopRadius)
                .addScaledVector(this.axis, (1 - Math.cos(Math.PI * 2 * t)) * this.loopRadius);
        } else target.applyAxisAngle(this.axis, this.angle * t);
        return target.addScaledVector(this.axis, wave * this.laneOffset).normalize()
            .multiplyScalar(this.radius + wave * this.height);
    }
}
