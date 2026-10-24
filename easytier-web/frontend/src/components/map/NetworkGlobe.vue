<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, watch, type ComponentPublicInstance } from 'vue';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { MapSceneLink, MapSceneNode } from '../../modules/network-space';
import { GeographicArc, geographicPosition, separateNearbyMarkers, assignRouteLanes } from './globe-geometry';
import { createAdministrativeLayer, type AdministrativeLabel } from './administrative-layer';
import dayMapUrl from '../../assets/map/earth-blue-marble.jpg';
import nightMapUrl from '../../assets/map/earth-night.jpg';

const props = withDefaults(defineProps<{
    nodes: MapSceneNode[]; links: MapSceneLink[]; selectedNodeId?: string | null;
    selectedLinkId?: string | null; reducedMotion?: boolean; locale?: string;
}>(), { selectedNodeId: null, selectedLinkId: null, reducedMotion: false, locale: 'cn' });
const emit = defineEmits<{ 'select-node': [id: string]; 'select-link': [id: string]; unavailable: [reason: string] }>();
const root = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
const unavailable = ref(false), textureFailed = ref(false), ready = ref(false);
const systemReducedMotion = ref(false), paused = ref(false);
const english = computed(() => props.locale.startsWith('en'));
const text = (cn: string, en: string) => english.value ? en : cn;
const reducedMotion = computed(() => props.reducedMotion || systemReducedMotion.value);
const altitudeKm = ref(14971.85);
const altitudeText = computed(() => altitudeKm.value >= 100 ? Math.round(altitudeKm.value).toLocaleString() : altitudeKm.value.toFixed(1));
const viewLevel = computed(() => altitudeKm.value < 400 ? text('近地视图', 'Local view') : altitudeKm.value < 4500 ? text('区域视图', 'Regional view') : text('全球视图', 'Global view'));
const viewport = ref({ width: 1, height: 1 });
interface MarkerDisplay { id: string; name: string; online: boolean; x: number; y: number; originalX: number; originalY: number; expanded: boolean }
interface RouteDisplay { id: string; path: string; color: string; dashed: boolean; selected: boolean }
interface Rectangle { left: number; top: number; width: number; height: number }
const markers = ref<MarkerDisplay[]>([]);
const displayedRoutes = ref<RouteDisplay[]>([]);
const administrativeLabels = shallowRef<readonly AdministrativeLabel[]>([]);
const expandedMarkers = computed(() => markers.value.filter(marker => marker.expanded));
const linkPicker = ref<{ x: number; y: number; ids: string[] } | null>(null);
const pickerElement = ref<HTMLElement>();
const pickerLinks = computed(() => (linkPicker.value?.ids ?? []).flatMap(id => {
    const link = props.links.find(item => item.id === id);
    return link ? [{ ...link, sourceName: props.nodes.find(node => node.id === link.source)?.name ?? link.source,
        targetName: props.nodes.find(node => node.id === link.target)?.name ?? link.target }] : [];
}));
const labelElements = new Map<string, HTMLElement>();
const minimumAltitude = .0008;
const surfaceRadius = 1.000012;
let renderer: THREE.WebGLRenderer | undefined, camera: THREE.PerspectiveCamera | undefined;
let scene: THREE.Scene | undefined, controls: OrbitControls | undefined, atmosphere: THREE.Mesh | undefined;
let trafficLayer: THREE.Group | undefined;
let administrativeLayer: ReturnType<typeof createAdministrativeLayer> | undefined;
let frame = 0, destroyed = false, inView = true, layoutDirty = true, initialFocusApplied = false, sizeInitialized = false;
let targetCamera: THREE.Vector3 | null = null;
let resizeObserver: ResizeObserver | undefined, visibilityObserver: IntersectionObserver | undefined, motionQuery: MediaQueryList | undefined;
let pointerStart: { x: number; y: number; id: number } | null = null;
let multiTouch = false, pinchDistance = 0, lastPointerUp = 0;
const activePointers = new Map<number, { x: number; y: number }>();
const textures: THREE.Texture[] = [];
const nodePositions = new Map<string, THREE.Vector3>();
const originalPositions = new Map<string, THREE.Vector3>();
const curves = new Map<string, GeographicArc>();
const particles: { mesh: THREE.Mesh; linkId: string; speed: number; reverse: boolean; offset: number }[] = [];
const routeSegments = new Map<string, { x1: number; y1: number; x2: number; y2: number }[]>();
let routeLabelExclusions: Rectangle[] = [];
const lastRenderedPosition = new THREE.Vector3(Infinity, Infinity, Infinity);
const lastRenderedQuaternion = new THREE.Quaternion();
const beforeControlsPosition = new THREE.Vector3();
const beforeControlsQuaternion = new THREE.Quaternion();
const raycaster = new THREE.Raycaster();
const pointer = new THREE.Vector2();
const projected = new THREE.Vector3();
const direction = new THREE.Vector3();

function setLabelRef(id: string, element: Element | ComponentPublicInstance | null) {
    if (element instanceof HTMLElement) labelElements.set(id, element); else labelElements.delete(id);
}
function disposeObject(object: THREE.Object3D) {
    object.traverse(child => {
        const drawable = child as THREE.Mesh;
        drawable.geometry?.dispose();
        if (drawable.material) (Array.isArray(drawable.material) ? drawable.material : [drawable.material]).forEach(material => material.dispose());
    });
}
function requestRender() {
    if (destroyed || frame || !renderer || !inView || document.hidden || unavailable.value) return;
    frame = requestAnimationFrame(renderFrame);
}
function invalidateLayout() { layoutDirty = true; requestRender(); }
function screenPosition(position: THREE.Vector3) {
    projected.copy(position).project(camera!);
    return { x: (projected.x * .5 + .5) * viewport.value.width, y: (-projected.y * .5 + .5) * viewport.value.height, z: projected.z };
}
function visibleFromCamera(position: THREE.Vector3) {
    if (!camera) return false;
    direction.copy(position).sub(camera.position);
    const length = direction.length();
    if (length < .000001) return false;
    direction.divideScalar(length);
    const b = camera.position.dot(direction);
    const discriminant = b * b - (camera.position.lengthSq() - 1);
    if (discriminant < 0) return true;
    const firstSurface = -b - Math.sqrt(discriminant);
    return firstSurface <= 0 || firstSurface >= length - .000015;
}
function worldUnitsPerPixel(position: THREE.Vector3) {
    return 2 * Math.tan(THREE.MathUtils.degToRad(camera!.fov) / 2) * camera!.position.distanceTo(position) / viewport.value.height;
}
function surfaceAtScreen(x: number, y: number) {
    pointer.set(x / viewport.value.width * 2 - 1, 1 - y / viewport.value.height * 2);
    raycaster.setFromCamera(pointer, camera!);
    return raycaster.ray.intersectSphere(new THREE.Sphere(new THREE.Vector3(), surfaceRadius), new THREE.Vector3());
}
function updateLabels(): Rectangle[] {
    const occupied: Rectangle[] = [
        { left: 0, top: 0, width: Math.min(280, viewport.value.width), height: 55 },
        { left: viewport.value.width - 65, top: viewport.value.height - 205, width: 65, height: 205 },
        { left: 0, top: viewport.value.height - 50, width: viewport.value.width, height: 50 },
        // The parent map's legend occupies the bottom-left corner. Keep the
        // altitude/readout above it, and keep geographic labels off both.
        { left: 0, top: viewport.value.height - 132, width: Math.min(340, viewport.value.width - 65), height: 132 },
        ...markers.value.map(marker => ({ left: marker.x - 12, top: marker.y - 12, width: 24, height: 24 })),
        ...routeLabelExclusions,
    ];
    const sorted = [...props.nodes].sort((a, b) => Number(b.id === props.selectedNodeId) - Number(a.id === props.selectedNodeId) || a.id.localeCompare(b.id));
    let count = 0;
    for (const node of sorted) {
        const element = labelElements.get(node.id), position = nodePositions.get(node.id);
        if (!element) continue;
        if (!position || !visibleFromCamera(position)) { element.style.visibility = 'hidden'; continue; }
        const { x, y, z } = screenPosition(position);
        if (z < -1 || z > 1 || x < 0 || x > viewport.value.width || y < 0 || y > viewport.value.height) { element.style.visibility = 'hidden'; continue; }
        const width = Math.min(150, element.offsetWidth || 120), height = 29;
        const candidates = [[17, -height / 2], [17, 20], [17, -49], [-width - 17, -height / 2], [-width - 17, 20], [17, 51], [17, -80]];
        let placement: Rectangle | undefined;
        for (const [dx, dy] of candidates) {
            const rectangle = { left: x + dx!, top: y + dy!, width, height };
            if (rectangle.left < 6 || rectangle.top < 6 || rectangle.left + width > viewport.value.width - 6 || rectangle.top + height > viewport.value.height - 46) continue;
            if (occupied.some(other => rectangle.left < other.left + other.width + 5 && rectangle.left + width + 5 > other.left && rectangle.top < other.top + other.height + 4 && rectangle.top + height + 4 > other.top)) continue;
            placement = rectangle; break;
        }
        if (!placement || count >= 32) { element.style.visibility = 'hidden'; continue; }
        count++; occupied.push(placement);
        element.style.visibility = 'visible';
        element.style.transform = `translate(${Math.round(placement.left)}px, ${Math.round(placement.top)}px)`;
    }
    return occupied;
}
function updateDisplayGeometry() {
    if (!camera) return;
    const altitude = Math.max(minimumAltitude, camera.position.length() - 1);
    // A city-level near plane is only needed near the ground. Keeping it that
    // small from orbit would waste depth precision and flicker real borders.
    const nearPlane = Math.max(.000005, altitude * .25);
    const farPlane = Math.max(3, camera.position.length() + 2);
    if (camera.near !== nearPlane || camera.far !== farPlane) {
        camera.near = nearPlane; camera.far = farPlane; camera.updateProjectionMatrix();
    }
    camera.updateMatrixWorld();
    nodePositions.clear();
    const byId = new Map(props.nodes.map(node => [node.id, node]));
    const originalScreens = new Map<string, ReturnType<typeof screenPosition>>();
    const candidates: { id: string; x: number; y: number }[] = [];
    for (const [id, position] of originalPositions) {
        nodePositions.set(id, position.clone());
        const point = screenPosition(position);
        originalScreens.set(id, point);
        if (visibleFromCamera(position) && point.z >= -1 && point.z <= 1 && point.x >= 0 && point.x <= viewport.value.width && point.y >= 0 && point.y <= viewport.value.height) candidates.push({ id, x: point.x, y: point.y });
    }
    markers.value = separateNearbyMarkers(candidates, viewport.value.width, viewport.value.height, (x, y) => !!surfaceAtScreen(x, y)).map(marker => {
        const original = originalPositions.get(marker.id)!;
        const display = marker.expanded ? surfaceAtScreen(marker.x, marker.y) ?? original.clone() : original.clone();
        nodePositions.set(marker.id, display);
        const actual = screenPosition(display), node = byId.get(marker.id)!;
        return { ...marker, x: actual.x, y: actual.y, expanded: Math.hypot(actual.x - marker.originalX, actual.y - marker.originalY) > 1,
            name: node.name, online: node.online };
    });
    const routes = props.links.filter(link => nodePositions.has(link.source) && nodePositions.has(link.target));
    const lanes = assignRouteLanes(routes.map(link => ({ id: link.id,
        source: originalScreens.get(link.source)!, target: originalScreens.get(link.target)!,
        displaySource: screenPosition(nodePositions.get(link.source)!), displayTarget: screenPosition(nodePositions.get(link.target)!),
    })));
    curves.clear(); routeSegments.clear(); routeLabelExclusions = [];
    const rendered: RouteDisplay[] = [];
    for (const link of routes) {
        const source = nodePositions.get(link.source)!, target = nodePositions.get(link.target)!;
        const sourceScreen = screenPosition(source), targetScreen = screenPosition(target);
        const near = Math.hypot(sourceScreen.x - targetScreen.x, sourceScreen.y - targetScreen.y) < 45;
        const pixels = worldUnitsPerPixel(source.clone().add(target).multiplyScalar(.5).normalize().multiplyScalar(surfaceRadius));
        const { lane, direction: orientation } = lanes.get(link.id) ?? { lane: 0, direction: 1 };
        // Reversing an edge reverses its arc normal. Use its bundle's geometric
        // direction, including different nearby endpoint pairs with unrelated IDs.
        // Twenty-pixel marker expansion still leaves almost no clickable chord
        // between the hit targets. Give a short route its own visible arc apex.
        const bendPixels = (lane * 11) + (near ? (lane < 0 ? -24 : 24) : 0);
        const curveAtOffset = (offset: number) => new GeographicArc(source, target, {
            laneOffset: THREE.MathUtils.clamp(offset * pixels * orientation, near ? -.75 : -.25, near ? .75 : .25),
            maximumHeight: altitude * .25, loopRadius: pixels * 12,
        });
        let curve = curveAtOffset(bendPixels);
        if (near && markers.value.length) {
            const apexClearance = (candidate: GeographicArc) => {
                const point = candidate.getPoint(.5), apex = screenPosition(point);
                // Keep a usable arc apex on the visible map, clear of the
                // corner controls/readouts and every node's pointer target.
                if (!visibleFromCamera(point) || apex.z < -1 || apex.z > 1 || apex.x < 16 || apex.x > viewport.value.width - 16
                    || apex.y < 66 || apex.y > viewport.value.height - 50
                    || (apex.x < 340 && apex.y > viewport.value.height - 132)
                    || (apex.x > viewport.value.width - 65 && apex.y > viewport.value.height - 205)) return -1;
                return Math.min(...markers.value.map(marker => Math.hypot(marker.x - apex.x, marker.y - apex.y)));
            };
            let clearance = apexClearance(curve);
            // A fixed bend can run into a third node in a compact cluster.
            // Expand along the same stable lane, with a bounded search and a
            // best-visible fallback at the horizon or the edge of the map.
            for (let step = 1; clearance < 24 && step <= 8; step++) {
                const candidate = curveAtOffset(bendPixels + Math.sign(bendPixels || 1) * step * 12);
                const candidateClearance = apexClearance(candidate);
                if (candidateClearance > clearance) { curve = candidate; clearance = candidateClearance; }
            }
        }
        curves.set(link.id, curve);
        if (near) {
            const apex = screenPosition(curve.getPoint(.5));
            if (apex.z >= -1 && apex.z <= 1) routeLabelExclusions.push({ left: apex.x - 13, top: apex.y - 13, width: 26, height: 26 });
        }
        const segments: { x1: number; y1: number; x2: number; y2: number }[] = [];
        let path = '', pathOpen = false, previous: { x: number; y: number; visible: boolean } | null = null;
        const samples = 144;
        for (let i = 0; i <= samples; i++) {
            // More samples near the actual endpoints preserve local routes when
            // a continental link is inspected from a city-level camera.
            const t = (1 - Math.cos(i / samples * Math.PI)) / 2;
            const point = curve.getPoint(t), screen = screenPosition(point);
            const visible = visibleFromCamera(point) && screen.z >= -1 && screen.z <= 1
                && Number.isFinite(screen.x) && Number.isFinite(screen.y)
                && Math.abs(screen.x) < viewport.value.width * 8 && Math.abs(screen.y) < viewport.value.height * 8;
            if (visible && previous?.visible) {
                segments.push({ x1: previous.x, y1: previous.y, x2: screen.x, y2: screen.y });
                if (!pathOpen) path += `M${previous.x.toFixed(2)},${previous.y.toFixed(2)}`;
                path += `L${screen.x.toFixed(2)},${screen.y.toFixed(2)}`;
                pathOpen = true;
            } else pathOpen = false;
            previous = { x: screen.x, y: screen.y, visible };
        }
        routeSegments.set(link.id, segments);
        const available = !link.stale && byId.get(link.source)?.online && byId.get(link.target)?.online;
        rendered.push({ id: link.id, path, color: !available ? '#647787' : link.kind === 'relay' ? '#f5b94d' : '#5edfeb',
            dashed: link.kind === 'relay' || link.stale, selected: link.id === props.selectedLinkId });
    }
    displayedRoutes.value = rendered.sort((a, b) => Number(a.selected) - Number(b.selected));
    altitudeKm.value = altitude * 6371;
    controls!.rotateSpeed = Math.max(.00022, Math.min(.55, altitude * .7));
    if (atmosphere) atmosphere.visible = altitude > .12;
    administrativeLabels.value = administrativeLayer?.update(camera, viewport.value, props.locale, updateLabels()) ?? [];
    layoutDirty = false;
}
function renderFrame(time: number) {
    frame = 0;
    if (!renderer || !scene || !camera || destroyed || document.hidden || !inView) return;
    if (targetCamera) {
        const distance = camera.position.length(), targetDistance = targetCamera.length();
        const from = camera.position.clone().normalize(), to = targetCamera.clone().normalize();
        if (reducedMotion.value || (from.angleTo(to) < .000001 && Math.abs(distance - targetDistance) < .000001)) {
            camera.position.copy(targetCamera); targetCamera = null;
        } else {
            const rotation = new THREE.Quaternion().setFromUnitVectors(from, to);
            from.applyQuaternion(new THREE.Quaternion().slerp(rotation, .12));
            camera.position.copy(from).multiplyScalar(Math.max(1 + minimumAltitude, THREE.MathUtils.lerp(distance, targetDistance, .12)));
        }
        layoutDirty = true;
    }
    beforeControlsPosition.copy(camera.position);
    beforeControlsQuaternion.copy(camera.quaternion);
    controls?.update();
    // OrbitControls' fixed change-event epsilon is kilometres wide on a unit
    // globe. Track the actual pose so a small local drag and its damping repaint
    // the same map/labels even when no event (or traffic animation) is emitted.
    const controlsMoved = beforeControlsPosition.distanceToSquared(camera.position) > 1e-18
        || 1 - Math.abs(beforeControlsQuaternion.dot(camera.quaternion)) > 1e-14;
    if (lastRenderedPosition.distanceToSquared(camera.position) > 1e-18
        || 1 - Math.abs(lastRenderedQuaternion.dot(camera.quaternion)) > 1e-14) layoutDirty = true;
    if (camera.position.length() < 1 + minimumAltitude) camera.position.setLength(1 + minimumAltitude);
    if (layoutDirty) updateDisplayGeometry();
    const animateTraffic = !reducedMotion.value && !paused.value;
    for (const particle of particles) {
        const curve = curves.get(particle.linkId);
        particle.mesh.visible = animateTraffic && !!curve;
        if (animateTraffic && curve) {
            const progress = (time * .0001 * particle.speed + particle.offset) % 1;
            curve.getPoint(particle.reverse ? 1 - progress : progress, particle.mesh.position);
            particle.mesh.scale.setScalar(worldUnitsPerPixel(particle.mesh.position) * 1.8);
        }
    }
    renderer.render(scene, camera);
    lastRenderedPosition.copy(camera.position); lastRenderedQuaternion.copy(camera.quaternion);
    if (targetCamera || controlsMoved || (animateTraffic && particles.length)) requestRender();
}
function rebuildData() {
    if (!scene || !trafficLayer) return;
    disposeObject(trafficLayer); trafficLayer.clear(); particles.length = 0;
    originalPositions.clear();
    const validNodes = props.nodes.filter(node => Number.isFinite(node.latitude) && Number.isFinite(node.longitude) && Math.abs(node.latitude) <= 90 && Math.abs(node.longitude) <= 180);
    validNodes.forEach(node => originalPositions.set(node.id, geographicPosition(node.latitude, node.longitude, surfaceRadius)));
    const byId = new Map(validNodes.map(node => [node.id, node]));
    for (const link of props.links) {
        if (link.stale || !byId.get(link.source)?.online || !byId.get(link.target)?.online) continue;
        for (const [rate, reverse] of [[link.tx_bps, false], [link.rx_bps, true]] as const) {
            if (!Number.isFinite(rate) || rate <= 0) continue;
            const mesh = new THREE.Mesh(new THREE.SphereGeometry(1, 8, 6), new THREE.MeshBasicMaterial({ color: 0xd9ffff, toneMapped: false }));
            trafficLayer.add(mesh);
            particles.push({ mesh, linkId: link.id, speed: Math.min(3, Math.max(.5, Math.log10(rate + 1) / 2)), reverse, offset: reverse ? .45 : 0 });
        }
    }
    if (!initialFocusApplied && validNodes.length) { focusNode(props.selectedNodeId ?? validNodes[0]!.id, false); initialFocusApplied = true; }
    if (linkPicker.value) linkPicker.value.ids = linkPicker.value.ids.filter(id => props.links.some(link => link.id === id));
    if (!linkPicker.value?.ids.length) linkPicker.value = null;
    invalidateLayout();
    void nextTick().then(invalidateLayout);
}
function defaultDistance() { return 3.35 / Math.min(1, viewport.value.width / viewport.value.height); }
function focusNode(id: string | null, animate = true) {
    if (!camera || !id) return;
    const position = originalPositions.get(id);
    if (!position) return;
    targetCamera = position.clone().normalize().multiplyScalar(Math.max(1 + minimumAltitude, camera.position.length()));
    if (!animate || reducedMotion.value) { camera.position.copy(targetCamera); targetCamera = null; controls?.update(); }
    invalidateLayout();
}
function resetView() {
    if (!camera) return;
    const selected = props.selectedNodeId && originalPositions.get(props.selectedNodeId);
    targetCamera = (selected || geographicPosition(25, 110)).clone().normalize().multiplyScalar(defaultDistance());
    controls?.target.set(0, 0, 0); closePicker(false); invalidateLayout();
}
function zoom(factor: number) {
    if (!camera || !controls) return;
    targetCamera = null;
    const altitude = THREE.MathUtils.clamp((camera.position.length() - 1) * factor, minimumAltitude, controls.maxDistance - 1);
    camera.position.setLength(1 + altitude); controls.update(); closePicker(false); invalidateLayout();
}
const zoomIn = () => zoom(.65), zoomOut = () => zoom(1 / .65);
defineExpose({ resetView, zoomIn, zoomOut });
function wheel(event: WheelEvent) { event.preventDefault(); zoom(Math.exp(THREE.MathUtils.clamp(event.deltaY * .0015, -1, 1))); }
function keyboardControl(event: KeyboardEvent) {
    if (!camera || !controls) return;
    if (event.key === '+' || event.key === '=') { event.preventDefault(); zoomIn(); return; }
    if (event.key === '-') { event.preventDefault(); zoomOut(); return; }
    if (event.key === 'Home') { event.preventDefault(); resetView(); return; }
    if (event.key === 'Escape') { closePicker(); return; }
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault(); targetCamera = null;
    const position = new THREE.Spherical().setFromVector3(camera.position);
    const step = Math.max(.00008, Math.min(.12, (position.radius - 1) * .18));
    if (event.key === 'ArrowLeft') position.theta -= step;
    if (event.key === 'ArrowRight') position.theta += step;
    if (event.key === 'ArrowUp') position.phi -= step;
    if (event.key === 'ArrowDown') position.phi += step;
    position.makeSafe(); camera.position.setFromSpherical(position); controls.update(); invalidateLayout();
}
function resize() {
    if (!root.value || !renderer || !camera) return;
    const width = root.value.clientWidth, height = root.value.clientHeight;
    if (!width || !height) return;
    const initial = !sizeInitialized;
    sizeInitialized = true;
    viewport.value = { width, height };
    camera.aspect = viewport.value.width / viewport.value.height; camera.updateProjectionMatrix();
    renderer.setSize(viewport.value.width, viewport.value.height, false);
    if (initial) camera.position.setLength(defaultDistance());
    invalidateLayout();
}
function pointerDown(event: PointerEvent) {
    closePicker(false);
    requestRender();
    activePointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    pointerStart = { x: event.clientX, y: event.clientY, id: event.pointerId };
    if (activePointers.size > 1) {
        multiTouch = true; pinchDistance = pointerDistance();
        // OrbitControls leaves its one-finger rotation state active when its
        // two-finger dolly/pan actions are disabled. The custom pinch owns this
        // gesture, so keep rotation paused until every finger has been lifted.
        if (controls) controls.enableRotate = false;
    }
}
function pointerDistance() {
    const [a, b] = [...activePointers.values()];
    return a && b ? Math.hypot(a.x - b.x, a.y - b.y) : 0;
}
function pointerMove(event: PointerEvent) {
    if (!activePointers.has(event.pointerId)) return;
    activePointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
    if (activePointers.size < 2) { requestRender(); return; }
    const next = pointerDistance();
    if (pinchDistance > 0 && next > 0) zoom(pinchDistance / next);
    pinchDistance = next;
}
function pointerCancel(event: PointerEvent) {
    activePointers.delete(event.pointerId); pointerStart = null;
    if (!activePointers.size) { multiTouch = false; if (controls) controls.enableRotate = true; }
    pinchDistance = pointerDistance();
}
function closePicker(focus = true) { const open = !!linkPicker.value; linkPicker.value = null; if (open && focus) canvas.value?.focus({ preventScroll: true }); }
function chooseLink(id: string) { emit('select-link', id); closePicker(); }
function pickerKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); closePicker(); return; }
    if (event.key !== 'Tab') return;
    const buttons = pickerElement.value?.querySelectorAll<HTMLButtonElement>('button');
    if (!buttons?.length) return;
    const first = buttons[0]!, last = buttons[buttons.length - 1]!;
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}
function pointerUp(event: PointerEvent) {
    requestRender();
    activePointers.delete(event.pointerId);
    if (multiTouch) {
        pointerStart = null;
        if (!activePointers.size) { multiTouch = false; if (controls) controls.enableRotate = true; }
        return;
    }
    if (!pointerStart || pointerStart.id !== event.pointerId || !canvas.value || !camera) return;
    const moved = Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y); pointerStart = null;
    if (moved > 5 || event.button !== 0 || performance.now() - lastPointerUp < 150) return;
    lastPointerUp = performance.now();
    const rectangle = canvas.value.getBoundingClientRect(), x = event.clientX - rectangle.left, y = event.clientY - rectangle.top;
    const nearestNode = markers.value.map(marker => ({ marker, distance: Math.hypot(marker.x - x, marker.y - y) })).sort((a, b) => a.distance - b.distance)[0];
    if (nearestNode && nearestNode.distance <= 10) { emit('select-node', nearestNode.marker.id); return; }
    const hits: { id: string; distance: number }[] = [];
    for (const [id, segments] of routeSegments) {
        let distance = Infinity;
        for (const segment of segments) {
            const dx = segment.x2 - segment.x1, dy = segment.y2 - segment.y1, length = dx * dx + dy * dy;
            const t = length ? THREE.MathUtils.clamp(((x - segment.x1) * dx + (y - segment.y1) * dy) / length, 0, 1) : 0;
            distance = Math.min(distance, Math.hypot(x - segment.x1 - t * dx, y - segment.y1 - t * dy));
        }
        if (distance <= 8) hits.push({ id, distance });
    }
    hits.sort((a, b) => a.distance - b.distance || a.id.localeCompare(b.id));
    if (!hits.length) return;
    const choices = hits.filter(hit => hit.distance <= hits[0]!.distance + 4);
    if (choices.length === 1) { emit('select-link', choices[0]!.id); return; }
    linkPicker.value = { x: Math.max(8, Math.min(x + 10, viewport.value.width - 288)), y: Math.max(8, Math.min(y + 10, viewport.value.height - 235)), ids: choices.map(hit => hit.id) };
    void nextTick().then(() => pickerElement.value?.querySelector<HTMLButtonElement>('button')?.focus());
}
function visibilityChange() { if (document.hidden || !inView) { cancelAnimationFrame(frame); frame = 0; } else invalidateLayout(); }
function motionChange() { systemReducedMotion.value = motionQuery?.matches ?? false; if (controls) controls.enableDamping = !reducedMotion.value; requestRender(); }
function contextLost(event: Event) { event.preventDefault(); unavailable.value = true; cancelAnimationFrame(frame); frame = 0; emit('unavailable', 'webgl-context-lost'); }

onMounted(() => {
    if (!canvas.value || !root.value) return;
    try {
        renderer = new THREE.WebGLRenderer({ canvas: canvas.value, antialias: true, alpha: false, powerPreference: 'low-power' });
        renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2)); renderer.setClearColor(0x080f17, 1);
        renderer.outputColorSpace = THREE.SRGBColorSpace; renderer.toneMapping = THREE.ACESFilmicToneMapping; renderer.toneMappingExposure = 1.2;
        scene = new THREE.Scene(); camera = new THREE.PerspectiveCamera(40, 1, .000005, 50);
        camera.position.copy(geographicPosition(25, 110, 3.35));
        controls = new OrbitControls(camera, canvas.value);
        controls.enablePan = false; controls.enableZoom = false;
        controls.touches.TWO = THREE.TOUCH.DOLLY_PAN;
        controls.enableDamping = !reducedMotion.value; controls.dampingFactor = .09; controls.rotateSpeed = .55;
        controls.minDistance = 1 + minimumAltitude; controls.maxDistance = 9;
        controls.addEventListener('change', invalidateLayout);
        controls.addEventListener('start', () => { targetCamera = null; closePicker(false); });
        scene.add(new THREE.AmbientLight(0x9fb9da, 1.35));
        const sun = new THREE.DirectionalLight(0xe6eff8, 1.9); sun.position.set(-3, 3, -4); scene.add(sun);
        const earthMaterial = new THREE.MeshPhongMaterial({ color: 0xc4cfde, specular: 0x20364d, shininess: 8, emissive: 0xffdfac, emissiveIntensity: 1.35 });
        scene.add(new THREE.Mesh(new THREE.SphereGeometry(1, 512, 256), earthMaterial));
        const loader = new THREE.TextureLoader();
        const loadTexture = (url: string, apply: (texture: THREE.Texture) => void) => {
            const texture = loader.load(url, loaded => {
                if (destroyed) { loaded.dispose(); return; }
                loaded.colorSpace = THREE.SRGBColorSpace; loaded.anisotropy = Math.min(renderer?.capabilities.getMaxAnisotropy() ?? 1, 8);
                apply(loaded); earthMaterial.needsUpdate = true; ready.value = true; requestRender();
            }, undefined, () => { if (!destroyed) { textureFailed.value = true; ready.value = true; requestRender(); } });
            textures.push(texture);
        };
        loadTexture(dayMapUrl, texture => { earthMaterial.map = texture; });
        loadTexture(nightMapUrl, texture => { earthMaterial.emissiveMap = texture; });
        atmosphere = new THREE.Mesh(new THREE.SphereGeometry(1.08, 80, 56), new THREE.ShaderMaterial({
            vertexShader: `varying vec3 vPosition; void main() { vec4 world = modelMatrix * vec4(position, 1.0); vPosition = world.xyz; gl_Position = projectionMatrix * viewMatrix * world; }`,
            fragmentShader: `varying vec3 vPosition; void main() { vec3 ray = normalize(vPosition - cameraPosition); float altitude = max(0.0, length(cross(cameraPosition, ray)) - 1.0); float haze = exp(-altitude * 105.0); haze *= 1.0 - smoothstep(0.055, 0.075, altitude); gl_FragColor = vec4(0.22, 0.66, 1.0, haze * 0.4); }`,
            transparent: true, side: THREE.BackSide, blending: THREE.AdditiveBlending, depthWrite: false,
        }));
        scene.add(atmosphere);
        administrativeLayer = createAdministrativeLayer(); scene.add(administrativeLayer.object);
        trafficLayer = new THREE.Group(); scene.add(trafficLayer);
        resize(); rebuildData();
        resizeObserver = new ResizeObserver(resize); resizeObserver.observe(root.value);
        visibilityObserver = new IntersectionObserver(entries => { inView = entries[0]?.isIntersecting ?? true; visibilityChange(); }); visibilityObserver.observe(root.value);
        motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)'); motionQuery.addEventListener('change', motionChange); motionChange();
        document.addEventListener('visibilitychange', visibilityChange); canvas.value.addEventListener('webglcontextlost', contextLost);
    } catch (error) { unavailable.value = true; emit('unavailable', error instanceof Error ? error.message : 'webgl-unavailable'); }
});
watch(() => [props.nodes, props.links], rebuildData, { deep: true });
watch(() => props.selectedNodeId, id => { focusNode(id); invalidateLayout(); });
watch(() => props.selectedLinkId, invalidateLayout);
watch(reducedMotion, () => { if (controls) controls.enableDamping = !reducedMotion.value; requestRender(); });
watch(paused, requestRender);
watch(() => props.locale, () => void nextTick().then(invalidateLayout));
onBeforeUnmount(() => {
    destroyed = true; cancelAnimationFrame(frame); resizeObserver?.disconnect(); visibilityObserver?.disconnect();
    motionQuery?.removeEventListener('change', motionChange); document.removeEventListener('visibilitychange', visibilityChange);
    canvas.value?.removeEventListener('webglcontextlost', contextLost); controls?.dispose();
    if (administrativeLayer && scene) { scene.remove(administrativeLayer.object); administrativeLayer.dispose(); }
    if (scene) disposeObject(scene); textures.forEach(texture => texture.dispose()); renderer?.dispose(); renderer?.forceContextLoss();
    labelElements.clear(); activePointers.clear();
});
</script>

<template>
    <div ref="root" class="network-globe" :class="{ 'network-globe-unavailable': unavailable }" data-testid="network-globe">
        <canvas ref="canvas" tabindex="0" :aria-label="text('三维网络地图：拖动或方向键旋转，滚轮或加减键缩放，Home 重置；也可从列表选择节点', '3D network globe: drag or arrow keys to orbit, scroll or plus/minus keys to zoom, Home to reset. Nodes are also available in the list.')"
            @keydown="keyboardControl" @wheel="wheel" @pointerdown="pointerDown" @pointermove="pointerMove" @pointerup="pointerUp" @pointercancel="pointerCancel" />
        <template v-if="!unavailable">
            <svg class="globe-routes" :viewBox="`0 0 ${viewport.width} ${viewport.height}`" aria-hidden="true">
                <path v-for="link in displayedRoutes" :key="link.id" class="globe-route-path" :class="{ selected: link.selected, dashed: link.dashed }"
                    :data-link-id="link.id" :d="link.path" :stroke="link.color" />
                <g v-for="marker in expandedMarkers" :key="`leader:${marker.id}`" class="globe-marker-leader">
                    <line :x1="marker.originalX" :y1="marker.originalY" :x2="marker.x" :y2="marker.y" />
                    <circle :cx="marker.originalX" :cy="marker.originalY" r="1.5" />
                </g>
            </svg>
            <div class="globe-administrative-labels" aria-label="Administrative regions">
                <span v-for="label in administrativeLabels" :key="label.id" class="globe-administrative-label"
                    :class="{ context: label.context }" :data-level="label.level" :title="label.text"
                    :style="{ transform: `translate(${label.x}px, ${label.y}px)` }">{{ label.text }}</span>
            </div>
            <div class="globe-markers">
                <button v-for="marker in markers" :key="marker.id" type="button" class="globe-node-marker"
                    :class="{ selected: marker.id === selectedNodeId, offline: !marker.online }"
                    :data-node-id="marker.id" :style="{ left: `${marker.x}px`, top: `${marker.y}px` }"
                    :aria-label="`${text('选择节点', 'Select node')} ${marker.name}`" :aria-pressed="marker.id === selectedNodeId"
                    :title="marker.name" @click="emit('select-node', marker.id)"><i aria-hidden="true"></i></button>
            </div>
        </template>
        <div v-if="!unavailable" class="globe-labels">
            <button v-for="node in nodes" :key="node.id" :ref="element => setLabelRef(node.id, element)" type="button"
                class="globe-node-label" :class="{ selected: node.id === selectedNodeId, offline: !node.online }"
                :title="node.name" :aria-label="`${node.name} · ${node.online ? text('在线', 'Online') : text('离线', 'Offline')}`"
                :aria-pressed="node.id === selectedNodeId" @click="emit('select-node', node.id)">
                <i :class="['pi', node.role === 'gateway' ? 'pi-sitemap' : 'pi-circle-fill']" aria-hidden="true"></i>
                <span>{{ node.name }}</span>
            </button>
        </div>
        <div v-if="unavailable" class="globe-fallback" role="status">
            <i class="pi pi-globe" aria-hidden="true"></i>
            <strong>{{ text('当前环境无法显示 3D 地球', '3D globe is unavailable in this environment') }}</strong>
            <p>{{ text('你仍可从节点列表查看设备和连接，或启用浏览器硬件加速后重试。', 'You can still use the node list to inspect devices and connections, or enable browser hardware acceleration and try again.') }}</p>
            <div class="globe-fallback-nodes"><button v-for="node in nodes" :key="node.id" type="button" @click="emit('select-node', node.id)">{{ node.name }}</button></div>
        </div>
        <span v-else-if="!ready" class="globe-loading" role="status">{{ text('正在加载地球…', 'Loading Earth…') }}</span>
        <span v-if="textureFailed && !unavailable" class="globe-texture-warning" role="status">{{ text('地球纹理加载失败，节点与连接仍可查看', 'Earth imagery could not load. Nodes and connections remain available.') }}</span>
        <div v-if="linkPicker && !unavailable" ref="pickerElement" class="globe-link-picker" role="dialog" :aria-label="text('选择连接线路', 'Choose a connection')"
            :style="{ left: `${linkPicker.x}px`, top: `${linkPicker.y}px` }" @keydown="pickerKeydown">
            <header><strong>{{ text('选择连接线路', 'Choose a connection') }}</strong><button type="button" :aria-label="text('关闭线路选择', 'Close connection choices')" @click="closePicker()">×</button></header>
            <p>{{ text('此处有多条线路', 'Several connections meet here') }}</p>
            <button v-for="link in pickerLinks" :key="link.id" type="button" class="globe-link-choice" :data-link-id="link.id" @click="chooseLink(link.id)">
                <span>{{ link.sourceName }} ↔ {{ link.targetName }}</span><small>{{ link.kind === 'relay' ? text('中继', 'Relay') : text('直连', 'Direct') }}<template v-if="link.stale"> · {{ text('已过期', 'Stale') }}</template></small>
            </button>
        </div>
        <div v-if="!unavailable" class="globe-controls" :aria-label="text('地图控制', 'Map controls')">
            <button type="button" :title="text('放大', 'Zoom in')" :aria-label="text('放大地图', 'Zoom in map')" @click="zoomIn"><i class="pi pi-plus" aria-hidden="true"></i></button>
            <button type="button" :title="text('缩小', 'Zoom out')" :aria-label="text('缩小地图', 'Zoom out map')" @click="zoomOut"><i class="pi pi-minus" aria-hidden="true"></i></button>
            <button type="button" :title="text('重置视角', 'Reset view')" :aria-label="text('重置地图视角', 'Reset map view')" @click="resetView"><i class="pi pi-compass" aria-hidden="true"></i></button>
            <button type="button" :title="paused ? text('播放流量动画', 'Play traffic animation') : text('暂停流量动画', 'Pause traffic animation')" :aria-label="paused ? text('播放流量动画', 'Play traffic animation') : text('暂停流量动画', 'Pause traffic animation')" :aria-pressed="paused" @click="paused = !paused"><i :class="['pi', paused ? 'pi-play' : 'pi-pause']" aria-hidden="true"></i></button>
        </div>
        <div v-if="!unavailable" class="globe-view-status" :aria-label="text('地图视角', 'Map view')"><span>{{ viewLevel }} · {{ text('距地', 'Altitude') }} {{ altitudeText }} km</span><span v-if="expandedMarkers.length" class="globe-cluster-note">{{ text('同址或邻近节点已展开', 'Co-located or nearby nodes are spread out') }}</span></div>
        <span v-if="!unavailable" class="globe-help">{{ text('拖动旋转 · 滚轮或双指缩放', 'Drag to orbit · Scroll or pinch to zoom') }}</span>
    </div>
</template>

<style scoped>
.network-globe { position: relative; width: 100%; height: 100%; min-height: 420px; overflow: hidden; isolation: isolate; background: #080f17; color: #dce8f1; }
.network-globe canvas { display: block; width: 100%; height: 100%; outline: none; touch-action: none; cursor: grab; }
.network-globe canvas:active { cursor: grabbing; }
.network-globe canvas:focus-visible { outline: 2px solid #57efc7; outline-offset: -3px; }
.globe-routes, .globe-administrative-labels, .globe-markers { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; overflow: hidden; }
.globe-route-path { fill: none; stroke-width: 1.2; stroke-opacity: .86; stroke-linecap: round; vector-effect: non-scaling-stroke; }
.globe-route-path.selected { stroke-width: 2.2; stroke-opacity: 1; }
.globe-route-path.dashed { stroke-dasharray: 6 4; }
.globe-marker-leader { stroke: #a4b8ca; stroke-width: .75; stroke-opacity: .65; fill: #bad6df; }
.globe-marker-leader line { stroke-dasharray: 2 3; }
.globe-node-marker { position: absolute; transform: translate(-50%, -50%); display: grid; place-items: center; width: 24px; height: 24px; padding: 0; border: 0; border-radius: 50%; background: transparent; cursor: pointer; pointer-events: auto; }
.globe-node-marker i { display: block; width: 5px; height: 5px; border-radius: 50%; background: #77e6fa; box-shadow: 0 0 0 1px #092b3c, 0 0 5px #52cde280; }
.globe-node-marker.selected i { width: 7px; height: 7px; background: #63f5cb; box-shadow: 0 0 0 2px #173c39, 0 0 0 3px #39dfb99e; }
.globe-node-marker.offline i { background: #8d9ba8; }
.globe-node-marker:focus-visible { outline: 1px solid #57efc7; }
.globe-administrative-label { position: absolute; left: 0; top: 0; max-width: 220px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 11px; line-height: 20px; color: #b2cad7; text-shadow: 0 1px 3px #000, 0 0 5px #051421; }
.globe-administrative-label[data-level="country"] { color: #cbdde7; letter-spacing: .04em; font-weight: 550; }
.globe-administrative-label.context { padding: 2px 7px; border-radius: 4px; background: #0d2235d9; border: 1px solid #54718980; color: #d6e8f3; }
.globe-view-status { position: absolute; left: 12px; bottom: 84px; display: flex; flex-direction: column; gap: 3px; pointer-events: none; color: #bacfdd; font-size: 10px; line-height: 13px; padding: 4px 7px; border-radius: 4px; background: #0a1724c9; }
.globe-cluster-note { color: #86b8c9; }
.globe-link-picker { position: absolute; z-index: 8; width: 280px; max-width: calc(100% - 16px); max-height: 220px; overflow: auto; padding: 10px; border: 1px solid #4e788b; border-radius: 7px; color: #dce8f1; background: #0d1d2bf5; box-shadow: 0 8px 28px #0008; }
.globe-link-picker header { display: flex; align-items: center; justify-content: space-between; gap: 10px; font-size: 12px; }
.globe-link-picker header button { padding: 0 6px; border: 0; background: transparent; color: #dce8f1; cursor: pointer; font-size: 20px; }
.globe-link-picker p { margin: 4px 0 8px; font-size: 11px; color: #8ea8b9; }
.globe-link-choice { display: flex; flex-direction: column; gap: 4px; width: 100%; padding: 8px; text-align: left; border: 1px solid #314d60; border-radius: 4px; background: #142b3a; color: #dce8f1; font: inherit; font-size: 12px; margin-top: 5px; cursor: pointer; overflow-wrap: anywhere; }
.globe-link-choice small { color: #9ab3c2; }
.globe-link-choice:hover { border-color: #5acdbd; }
.globe-link-picker button:focus-visible { outline: 2px solid #57efc7; outline-offset: 1px; }
.globe-labels { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
.globe-node-label { position: absolute; inset: 0 auto auto 0; visibility: hidden; display: flex; align-items: center; gap: 6px; max-width: 150px; min-height: 29px; padding: 4px 9px; background: rgba(13, 26, 37, .93); color: #e4edf5; border: 1px solid #334b5c; border-radius: 5px; font: inherit; font-size: 12px; line-height: 19px; white-space: nowrap; pointer-events: auto; cursor: pointer; box-shadow: 0 2px 8px #0006; }
.globe-node-label span { overflow: hidden; text-overflow: ellipsis; }
.globe-node-label .pi { color: #72ddee; font-size: 9px; flex: none; }
.globe-node-label.selected { color: #a0fce3; border-color: #2ce0b2; background: rgba(11, 45, 43, .95); }
.globe-node-label.selected .pi { color: #57efc7; }
.globe-node-label.offline .pi { color: #a1abb6; }
.globe-node-label:hover { border-color: #81e7ec; }
.globe-node-label:focus-visible, .globe-controls button:focus-visible, .globe-fallback button:focus-visible { outline: 2px solid #57efc7; outline-offset: 3px; }
.globe-controls { position: absolute; right: 18px; bottom: 44px; display: flex; flex-direction: column; background: rgba(15, 27, 38, .94); border: 1px solid #334655; border-radius: 7px; overflow: hidden; }
.globe-controls button { width: 36px; height: 36px; color: #dbe9f3; background: transparent; border: 0; border-bottom: 1px solid #263a4b; cursor: pointer; }
.globe-controls button:last-child { border-bottom: 0; }
.globe-controls button:hover { color: #64efcd; background: #1c3341; }
.globe-help { position: absolute; right: 18px; bottom: 13px; color: #93a8b9; font-size: 11px; pointer-events: none; }
.globe-loading, .globe-texture-warning { position: absolute; top: 18px; left: 50%; transform: translateX(-50%); max-width: 90%; padding: 6px 10px; font-size: 12px; background: #132330; border-radius: 5px; }
.globe-texture-warning { color: #edc783; }
.globe-fallback { position: absolute; inset: 0; display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 32px; text-align: center; }
.globe-fallback > .pi { font-size: 42px; color: #58d0d1; margin-bottom: 20px; }
.globe-fallback strong { font-size: 16px; }
.globe-fallback p { margin: 12px 0 18px; max-width: 380px; color: #a7b9c8; font-size: 13px; line-height: 1.7; }
.globe-fallback-nodes { display: flex; justify-content: center; flex-wrap: wrap; gap: 8px; max-height: 180px; overflow: auto; }
.globe-fallback button { padding: 7px 12px; color: #b4f6e6; background: #183c3b; border: 1px solid #346362; border-radius: 5px; cursor: pointer; font: inherit; font-size: 12px; }
@media (max-width: 767px) { .network-globe { min-height: 360px; } .globe-node-label { max-width: 120px; font-size: 11px; } .globe-controls { right: 10px; } .globe-help { display: none; } .globe-view-status { max-width: calc(100% - 70px); } }
</style>
