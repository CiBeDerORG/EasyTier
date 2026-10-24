import * as THREE from 'three';
import source from '../../assets/map/administrative-boundaries.json?raw';
import { geographicPosition } from './globe-geometry';

type Level = 'country' | 'region';
interface PolygonRecord { bounds: number[]; rings: number[][] }
interface RegionRecord {
    id: string;
    en: string;
    zh: string;
    at: number[];
    span: number;
    parent: string | null;
    polygons: PolygonRecord[];
}
interface AdministrativeData {
    coordinateScale: number;
    countryLines: number[][];
    disputedLines: number[][];
    regionLines: number[][];
    countries: RegionRecord[];
    regions: RegionRecord[];
}
export interface AdministrativeRectangle { left: number; top: number; width: number; height: number }
export interface AdministrativeLabel {
    id: string;
    text: string;
    /** Left/top in CSS pixels relative to the globe canvas. */
    x: number;
    y: number;
    level: Level;
    /** A containing-region caption, rather than a geographic label anchor. */
    context?: boolean;
}

// Keep the dataset out of TypeScript's inferred literal types; Vite bundles the
// local text with this lazily loaded globe module, with no runtime provider URL.
const data: AdministrativeData = JSON.parse(source);
const boundaryRadius = 1.000008;
const coordinateScale = data.coordinateScale;

function decodeLine(encoded: number[]): [number, number][] {
    const points: [number, number][] = [];
    let longitude = 0, latitude = 0;
    for (let i = 0; i + 1 < encoded.length; i += 2) {
        longitude += encoded[i]!;
        latitude += encoded[i + 1]!;
        points.push([longitude / coordinateScale, latitude / coordinateScale]);
    }
    return points;
}

function lineGeometries(lines: number[][], partition = false): THREE.BufferGeometry[] {
    const chunks = new Map<string, number[]>();
    const from = new THREE.Vector3(), to = new THREE.Vector3();
    for (const line of lines) {
        const points = decodeLine(line);
        for (let i = 1; i < points.length; i++) {
            const a = points[i - 1]!, b = points[i]!;
            // Subdivide the source's lon/lat segments: long chords must not
            // disappear below the sphere when the camera approaches the ground.
            const pieces = Math.max(1, Math.ceil(Math.max(Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1])) / .15));
            from.copy(geographicPosition(a[1], a[0], boundaryRadius));
            for (let j = 1; j <= pieces; j++) {
                const t = j / pieces;
                to.copy(geographicPosition(a[1] + (b[1] - a[1]) * t, a[0] + (b[0] - a[0]) * t, boundaryRadius));
                const middle = (j - .5) / pieces;
                const longitude = ((a[0] + (b[0] - a[0]) * middle + 180) % 360 + 360) % 360 - 180;
                const latitude = a[1] + (b[1] - a[1]) * middle;
                const key = partition ? `${Math.floor((longitude + 180) / 20)}:${Math.floor((latitude + 90) / 20)}` : 'all';
                let coordinates = chunks.get(key);
                if (!coordinates) { coordinates = []; chunks.set(key, coordinates); }
                coordinates.push(from.x, from.y, from.z, to.x, to.y, to.z);
                from.copy(to);
            }
        }
    }
    return [...chunks.values()].map(coordinates => {
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.Float32BufferAttribute(coordinates, 3));
        geometry.computeBoundingSphere();
        return geometry;
    });
}

function overlaps(a: AdministrativeRectangle, b: AdministrativeRectangle) {
    return a.left < b.left + b.width + 5 && a.left + a.width + 5 > b.left
        && a.top < b.top + b.height + 4 && a.top + a.height + 4 > b.top;
}

function inRing(encoded: number[], longitude: number, latitude: number): boolean {
    const points = decodeLine(encoded);
    if (points.length < 4) return false;
    const extent = points.reduce((bounds, point) => [Math.min(bounds[0]!, point[0]), Math.max(bounds[1]!, point[0])],
        [points[0]![0], points[0]![0]]);
    const x = longitude + 360 * Math.round(((extent[0]! + extent[1]!) / 2 - longitude) / 360);
    let inside = false;
    for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
        const a = points[i]!, b = points[j]!;
        if ((a[1] > latitude) !== (b[1] > latitude)
            && x < (b[0] - a[0]) * (latitude - a[1]) / (b[1] - a[1]) + a[0]) inside = !inside;
    }
    return inside;
}

function contains(region: RegionRecord, longitude: number, latitude: number) {
    return region.polygons.some(polygon => {
        const [west = 0, south = 0, east = 0, north = 0] = polygon.bounds;
        const x = longitude + 360 * Math.round(((west + east) / 2 - longitude) / 360);
        if (latitude < south || latitude > north || x < west || x > east || !polygon.rings[0]) return false;
        return inRing(polygon.rings[0], longitude, latitude)
            && !polygon.rings.slice(1).some(ring => inRing(ring, longitude, latitude));
    });
}

interface LabelRecord { region: RegionRecord; level: Level; position: THREE.Vector3 }
// These immutable anchors are shared by all mounted globe instances. Geometry
// and materials remain instance-owned so disposal never affects another map.
const countryLabels: LabelRecord[] = data.countries.map(region => ({ region, level: 'country', position: geographicPosition(region.at[1]!, region.at[0]!, boundaryRadius) }));
const regionLabels: LabelRecord[] = data.regions.map(region => ({ region, level: 'region', position: geographicPosition(region.at[1]!, region.at[0]!, boundaryRadius) }));
const countryById = new Map(data.countries.map(region => [region.id, region]));

/** A bundled, offline administrative reference layer. Natural Earth describes
 * countries and first-level subdivisions; this is not a live or street map. */
export function createAdministrativeLayer() {
    const object = new THREE.Group();
    object.name = 'administrative-regions';
    const countryMaterial = new THREE.LineBasicMaterial({ color: 0xb3d9e6, transparent: true, opacity: .62, toneMapped: false, depthWrite: false });
    const regionMaterial = new THREE.LineBasicMaterial({ color: 0x83afc1, transparent: true, opacity: .36, toneMapped: false, depthWrite: false });
    const disputedMaterial = new THREE.LineDashedMaterial({ color: 0xb8b5a1, transparent: true, opacity: .55, toneMapped: false, depthWrite: false, dashSize: .002, gapSize: .0014 });
    const countries = new THREE.LineSegments(lineGeometries(data.countryLines)[0], countryMaterial);
    const disputed = new THREE.LineSegments(lineGeometries(data.disputedLines)[0], disputedMaterial);
    const regions = new THREE.Group();
    // A close view needs only a few geographic chunks. Their bounding spheres
    // let Three's camera frustum reject the rest before uploading draw calls.
    const regionChunks = lineGeometries(data.regionLines, true).map(geometry => new THREE.LineSegments(geometry, regionMaterial));
    regions.add(...regionChunks);
    disputed.computeLineDistances();
    object.add(countries, disputed, regions);
    const projected = new THREE.Vector3(), direction = new THREE.Vector3(), difference = new THREE.Vector3();
    let previousKey = '';
    let previousLabels: AdministrativeLabel[] = [];
    let previousContainingRegion: RegionRecord | undefined;
    let containingKey = '';

    function update(camera: THREE.PerspectiveCamera, viewport: { width: number; height: number }, locale: string,
        occupiedRects: AdministrativeRectangle[] = []): AdministrativeLabel[] {
        const altitude = Math.max(.00001, camera.position.length() - 1);
        regions.visible = altitude < .62;
        regionMaterial.opacity = THREE.MathUtils.clamp((.7 - altitude) * .85, .12, .5);
        countryMaterial.opacity = altitude < .08 ? .78 : .62;
        const width = Math.max(1, viewport.width), height = Math.max(1, viewport.height);
        const key = [camera.position.x, camera.position.y, camera.position.z, camera.quaternion.x, camera.quaternion.y,
            camera.quaternion.z, camera.quaternion.w, camera.fov, width, height].map(value => value.toFixed(6)).join(':')
            + locale + occupiedRects.map(rect => `${Math.round(rect.left)},${Math.round(rect.top)},${Math.round(rect.width)},${Math.round(rect.height)}`).join(';');
        if (previousKey === key) return previousLabels;
        previousKey = key;
        const english = locale.startsWith('en');
        const name = (region: RegionRecord) => english ? region.en : region.zh;
        const output: AdministrativeLabel[] = [];
        const occupied = [...occupiedRects,
            { left: 0, top: 0, width: Math.min(width, 265), height: 62 },
            { left: width - 72, top: height - 165, width: 72, height: 165 },
            { left: 0, top: height - 52, width, height: 52 }];
        const place = (label: AdministrativeLabel, textWidth: number, textHeight = 22) => {
            const rect = { left: Math.round(label.x), top: Math.round(label.y), width: textWidth, height: textHeight };
            if (rect.left < 8 || rect.top < 8 || rect.left + rect.width > width - 8 || rect.top + rect.height > height - 8
                || occupied.some(other => overlaps(rect, other))) return false;
            occupied.push(rect);
            output.push({ ...label, x: rect.left, y: rect.top });
            return true;
        };
        direction.copy(camera.position).normalize();
        if (altitude < .055) {
            const latitude = THREE.MathUtils.radToDeg(Math.asin(direction.y));
            const longitude = THREE.MathUtils.radToDeg(Math.atan2(-direction.z, direction.x));
            const pointKey = `${longitude.toFixed(3)},${latitude.toFixed(3)}`;
            if (pointKey !== containingKey) {
                containingKey = pointKey;
                previousContainingRegion = data.regions.find(region => contains(region, longitude, latitude))
                    ?? data.countries.find(region => contains(region, longitude, latitude));
            }
            if (previousContainingRegion) {
                const region = previousContainingRegion;
                const parent = region.parent ? countryById.get(region.parent) : undefined;
                // Keep the local administrative name readable first, even
                // when a country's source name is long and the caption clips.
                const text = `${name(region)}${parent ? ' · ' + name(parent) : ''}`;
                const textWidth = Math.min(width - 32, Math.max(70, [...text].reduce((sum, char) => sum + (char.charCodeAt(0) > 255 ? 12 : 7), 22)));
                for (const y of [72, 101, 130, height - 84]) {
                    if (place({ id: `context:${region.id}`, text, x: 16, y, level: region.parent ? 'region' : 'country', context: true }, textWidth, 24)) break;
                }
            }
        }
        const candidates: { record: LabelRecord; x: number; y: number; width: number; priority: number }[] = [];
        const focalPixels = height / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov) / 2));
        const records = regions.visible ? [...countryLabels, ...regionLabels] : countryLabels;
        for (const record of records) {
            if (record.level === 'country' && altitude < .018) continue;
            // A surface point is visible only above the true spherical horizon.
            difference.copy(camera.position).sub(record.position);
            if (difference.dot(record.position) <= 0) continue;
            projected.copy(record.position).project(camera);
            if (projected.z < -1 || projected.z > 1 || Math.abs(projected.x) > .97 || Math.abs(projected.y) > .97) continue;
            const text = name(record.region);
            const textWidth = Math.min(220, Math.max(30, [...text].reduce((sum, char) => sum + (char.charCodeAt(0) > 255 ? 11 : 6.2), 12)));
            const angularSpan = THREE.MathUtils.degToRad(Math.max(.002, record.region.span));
            const projectedSpan = angularSpan * focalPixels / Math.max(.0001, difference.length());
            if (projectedSpan < textWidth * (record.level === 'country' ? 1.15 : 1.45)) continue;
            candidates.push({ record, x: (projected.x * .5 + .5) * width, y: (-projected.y * .5 + .5) * height,
                width: textWidth, priority: (record.level === 'country' ? 2000 : 0) + Math.min(projectedSpan, 1800) });
        }
        candidates.sort((a, b) => b.priority - a.priority || a.record.region.id.localeCompare(b.record.region.id));
        const counts = { country: 0, region: 0 };
        for (const candidate of candidates) {
            const { record, x, y } = candidate;
            if (counts[record.level] >= (record.level === 'country' ? 18 : 28)) continue;
            if (place({ id: `${record.level}:${record.region.id}`, text: name(record.region),
                x: x - candidate.width / 2, y: y - 11, level: record.level }, candidate.width)) counts[record.level]++;
        }
        previousLabels = output;
        return output;
    }

    function dispose() {
        for (const lines of [countries, disputed, ...regionChunks]) lines.geometry.dispose();
        countryMaterial.dispose();
        regionMaterial.dispose();
        disputedMaterial.dispose();
        object.clear();
        previousLabels = [];
        previousContainingRegion = undefined;
    }
    return { object, update, dispose };
}
