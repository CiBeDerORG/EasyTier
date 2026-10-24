<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import type { MapLanDevice } from '../../modules/network-space';
import { nodeSpaceCenterGateway } from '../../modules/node-map-assets';

const props = withDefaults(defineProps<{
    devices: MapLanDevice[];
    gatewayName: string;
    selectedDeviceId?: string | null;
    layout: 'home' | 'topology';
    locale?: string;
}>(), { selectedDeviceId: null, locale: 'cn' });
const emit = defineEmits<{
    'select-device': [id: string];
    unavailable: [reason: string];
}>();

const host = ref<HTMLDivElement>();
const failure = ref(false);
const chinese = computed(() => !props.locale.startsWith('en'));
const words = computed(() => chinese.value ? {
    home: '示意户型', topology: '设备拓扑', configured: '设备归属关系',
    hint: '拖动旋转 · 滚轮缩放', reset: '重置视角', zoomIn: '放大', zoomOut: '缩小',
    unavailable: '此浏览器暂时无法显示 3D 空间', fallback: '仍可从设备列表查看关联设备。',
    retry: '重新加载', gateway: '网络节点', empty: '该节点还没有资产设备',
    region: '网络节点、网关与资产设备的三维关系', illustrative: '示意摆放，不代表实际房间',
    merged: '网关与自身设备合并显示', site: '站点连接点',
} : {
    home: 'Illustrative layout', topology: 'Device topology', configured: 'Asset relationships',
    hint: 'Drag to orbit · Scroll to zoom', reset: 'Reset view', zoomIn: 'Zoom in', zoomOut: 'Zoom out',
    unavailable: '3D space is unavailable in this browser', fallback: 'Connected devices remain available in the list.',
    retry: 'Reload', gateway: 'Network node', empty: 'This node has no asset devices',
    region: '3D network node, gateway and asset device relationships', illustrative: 'Illustrative placement, not actual rooms',
    merged: 'Gateway and self device shown together', site: 'Site connection point',
});

const centerGateway = computed(() => nodeSpaceCenterGateway(props.devices));
const outerDevices = computed(() => props.devices.filter(device => device.id !== centerGateway.value?.id));

interface LabelAnchor { id: string; name: string; title?: string; position: THREE.Vector3; gateway?: boolean; node?: boolean; selectable?: boolean }
interface ScreenLabel { id: string; name: string; title?: string; x: number; y: number; gateway?: boolean; node?: boolean; selectable?: boolean }
const labels = shallowRef<ScreenLabel[]>([]);
let anchors: LabelAnchor[] = [];
let scene: THREE.Scene | undefined;
let camera: THREE.PerspectiveCamera | undefined;
let renderer: THREE.WebGLRenderer | undefined;
let controls: OrbitControls | undefined;
let content: THREE.Group | undefined;
let deviceObjects: THREE.Object3D[] = [];
let resizeObserver: ResizeObserver | undefined;
let intersectionObserver: IntersectionObserver | undefined;
let motionQuery: MediaQueryList | undefined;
let frame = 0;
let inViewport = true;
let destroyed = false;
let pointerStart: { x: number; y: number } | undefined;
const raycaster = new THREE.Raycaster();
const pointer = new THREE.Vector2();
const viewport = { width: 0, height: 0 };

function material(color: THREE.ColorRepresentation, roughness = .65, metalness = .08) {
    return new THREE.MeshStandardMaterial({ color, roughness, metalness });
}

function box(group: THREE.Group, dimensions: [number, number, number],
    position: [number, number, number], color: THREE.ColorRepresentation, radius = 0) {
    // Small bevels on furniture are made by the lit geometry, rather than a raster overlay.
    const mesh = new THREE.Mesh(new THREE.BoxGeometry(...dimensions), material(color));
    mesh.position.set(...position);
    mesh.rotation.y = radius;
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    group.add(mesh);
    return mesh;
}

function cylinder(group: THREE.Group, top: number, bottom: number, height: number,
    position: [number, number, number], color: THREE.ColorRepresentation, segments = 18) {
    const mesh = new THREE.Mesh(new THREE.CylinderGeometry(top, bottom, height, segments), material(color));
    mesh.position.set(...position);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    group.add(mesh);
    return mesh;
}

function plant(group: THREE.Group, x: number, z: number, size = 1) {
    cylinder(group, .15 * size, .11 * size, .23 * size, [x, .13 * size, z], '#d7d0bd');
    for (let i = 0; i < 5; i++) {
        const leaf = new THREE.Mesh(new THREE.SphereGeometry(.13 * size, 10, 8), material(i % 2 ? '#496950' : '#68855d'));
        leaf.scale.set(.6, 2.5, .8);
        leaf.position.set(x + Math.cos(i * 1.25) * .08 * size, .42 * size, z + Math.sin(i * 1.25) * .08 * size);
        leaf.rotation.z = Math.cos(i * 1.25) * .38;
        group.add(leaf);
    }
}

function addApartment(group: THREE.Group) {
    box(group, [7.6, .2, 5.6], [0, -.12, 0], '#777a78');
    // Individual boards supply texture and scale even when no external assets are available.
    const timber = ['#a68b6e', '#b69a7a', '#b19576', '#aa8e6e'];
    for (let row = 0; row < 20; row++) {
        const z = -2.64 + row * .278;
        for (let column = 0; column < 4; column++) {
            const width = 1.87;
            box(group, [width - .012, .028, .267], [-2.805 + column * width, .005, z], timber[(row + column * 3) % timber.length]!);
        }
    }
    // Cutaway outer walls stay low on the camera side. Interior doorways remain open.
    const wall = '#e0ded5';
    box(group, [7.6, 1.05, .15], [0, .5, -2.78], wall);
    box(group, [.15, 1.05, 5.6], [-3.78, .5, 0], wall);
    box(group, [.15, .27, 5.6], [3.78, .1, 0], '#cccac2');
    box(group, [7.6, .27, .15], [0, .1, 2.78], '#cccac2');
    box(group, [.12, .8, 2.8], [-.42, .38, -1.36], wall);
    box(group, [1.8, .8, .12], [-2.82, .38, -.15], wall);
    box(group, [1.0, .8, .12], [-.92, .38, -.15], wall);
    box(group, [1.4, .8, .12], [.26, .38, -.45], wall);
    box(group, [1.5, .8, .12], [3.03, .38, -.45], wall);
    box(group, [.12, .65, 1.6], [-1.2, .3, 1.95], wall);

    // Bedroom: low bed, pillows, headboard, side table.
    box(group, [1.65, .25, 2.08], [-2.18, .15, -1.5], '#b8aa95');
    box(group, [1.64, .2, 1.96], [-2.18, .36, -1.48], '#eeeae1');
    box(group, [1.67, .08, 1.1], [-2.18, .49, -1.03], '#9da6a3');
    box(group, [1.78, .65, .13], [-2.18, .38, -2.58], '#93816c');
    box(group, [.66, .12, .4], [-2.63, .51, -2.14], '#fbf5e9');
    box(group, [.66, .12, .4], [-1.74, .51, -2.14], '#fbf5e9');
    box(group, [.48, .38, .48], [-3.31, .2, -2.1], '#aa957a');
    cylinder(group, .16, .22, .2, [-3.31, .64, -2.1], '#f4dfb6');

    // Study: walnut desk against the back wall and an upholstered chair.
    box(group, [2.25, .11, .68], [1.9, .69, -2.25], '#765c44');
    box(group, [.12, .68, .56], [.86, .34, -2.25], '#696e70');
    box(group, [.12, .68, .56], [2.94, .34, -2.25], '#696e70');
    box(group, [.64, .12, .55], [1.9, .44, -1.38], '#596f73');
    box(group, [.64, .54, .1], [1.9, .68, -1.1], '#596f73');
    cylinder(group, .05, .05, .4, [1.9, .2, -1.38], '#535a5e');

    // Living room: sofa, rug, coffee table, and a deliberately blank TV.
    box(group, [2.0, .03, 1.55], [1.3, .055, 1.21], '#c3b9a2');
    box(group, [1.92, .3, .74], [2.24, .22, 2.13], '#d7c8b0');
    box(group, [1.92, .55, .18], [2.24, .45, 2.46], '#b8ab94');
    box(group, [.2, .53, .8], [1.26, .36, 2.1], '#c4b59c');
    box(group, [.2, .53, .8], [3.22, .36, 2.1], '#c4b59c');
    box(group, [.78, .12, .62], [1.77, .43, 2.04], '#e7dac4');
    box(group, [.78, .12, .62], [2.69, .43, 2.04], '#e2d4bc');
    box(group, [1.1, .09, .65], [.25, .37, .83], '#e3d1ad');
    cylinder(group, .23, .3, .31, [.25, .19, .83], '#8d7860');
    box(group, [1.25, .24, .36], [1.65, .15, -.18], '#8c7357');
    box(group, [1.02, .62, .055], [1.65, .61, -.22], '#242e38');
    box(group, [.88, .49, .025], [1.65, .62, -.181], '#44545d');

    // Entry cabinets and greenery make the spatial template legible.
    box(group, [.47, .65, 1.2], [-3.39, .34, 1.45], '#b6a58b');
    box(group, [.53, .07, 1.28], [-3.39, .71, 1.45], '#e0d5bd');
    plant(group, 3.25, -2.17, 1.15);
    plant(group, -.83, 2.28, .85);
    plant(group, -3.28, -.55, .7);
}

function hardware(kind: MapLanDevice['kind'], isGateway = false) {
    const group = new THREE.Group();
    if (kind === 'router') {
        box(group, [.58, .13, .36], [0, .12, 0], isGateway ? '#526a6d' : '#65707a');
        box(group, [.51, .025, .3], [0, .2, 0], '#253640');
        for (const x of [-.23, .23]) {
            cylinder(group, .016, .022, .4, [x, .34, -.12], '#23323d', 8);
        }
        box(group, [.13, .012, .015], [0, .13, .185], '#a7bec4');
    } else if (kind === 'server') {
        box(group, [.42, .65, .42], [0, .36, 0], '#293946');
        box(group, [.33, .55, .025], [0, .36, .222], '#405361');
        for (let i = 0; i < 3; i++) {
            box(group, [.24, .045, .013], [-.02, .2 + i * .15, .24], '#1c2933');
            box(group, [.025, .025, .016], [.12, .2 + i * .15, .24], '#9aaeb5');
        }
    } else if (kind === 'desktop') {
        box(group, [.66, .04, .42], [0, .06, .03], '#909da4');
        const screen = box(group, [.64, .41, .04], [0, .29, -.15], '#34424d');
        screen.rotation.x = -.14;
        const panel = box(group, [.55, .32, .012], [0, .29, -.119], '#527988');
        panel.rotation.x = -.14;
        box(group, [.4, .008, .14], [0, .087, .015], '#56636d');
    } else if (kind === 'camera') {
        cylinder(group, .16, .19, .06, [0, .04, 0], '#dce3de');
        cylinder(group, .045, .065, .2, [0, .15, 0], '#bdc7c4');
        const cameraBody = new THREE.Mesh(new THREE.SphereGeometry(.2, 20, 16), material('#e7eee9'));
        cameraBody.scale.set(1, 1.15, .88);
        cameraBody.position.set(0, .35, 0);
        group.add(cameraBody);
        const lens = cylinder(group, .095, .095, .04, [0, .36, .16], '#1a2c39');
        lens.rotation.x = Math.PI / 2;
    } else {
        box(group, [.43, .3, .43], [0, .18, 0], '#7d9196');
        box(group, [.28, .055, .022], [0, .2, .23], '#314852');
    }
    return group;
}

function addMarker(group: THREE.Group, position: THREE.Vector3, selected: boolean, gateway = false, scale = 1) {
    const ring = new THREE.Mesh(
        new THREE.RingGeometry(.34, selected ? .4 : .365, 48),
        new THREE.MeshBasicMaterial({ color: gateway || selected ? '#34d3ad' : '#80aeb6', transparent: true, opacity: selected ? .95 : .55, side: THREE.DoubleSide }),
    );
    ring.rotation.x = -Math.PI / 2;
    ring.scale.setScalar(scale);
    ring.position.copy(position).add(new THREE.Vector3(0, .012, 0));
    group.add(ring);
}

const illustrativeRooms: MapLanDevice['room'][] = ['living', 'study', 'bedroom', 'entry'];
function displayRoom(device: MapLanDevice, index: number): MapLanDevice['room'] {
    return device.asset_device_id || device.is_gateway
        ? illustrativeRooms[index % illustrativeRooms.length]! : device.room;
}

function devicePositions(devices: readonly MapLanDevice[]): THREE.Vector3[] {
    if (props.layout === 'topology') {
        const count = devices.length;
        return devices.map((_, i) => {
            const layer = Math.floor(i / 12);
            const inLayer = Math.min(12, count - layer * 12);
            const angle = ((i % 12) / inLayer) * Math.PI * 2 - Math.PI / 2;
            const radius = 2 + layer * 1.15;
            return new THREE.Vector3(Math.cos(angle) * radius, .06, Math.sin(angle) * radius);
        });
    }
    const rooms: Record<MapLanDevice['room'], [number, number, number, number]> = {
        living: [2.1, 1.2, 2, 1], study: [1.9, -2.25, 1.7, .4],
        bedroom: [-2.12, -1.33, 1.3, 1.2], entry: [-2.2, 1.4, 1.4, 1.3],
    };
    // Asset records have no physical room coordinates. Distribute them across
    // this template solely for legibility and label the view as illustrative.
    const roomCounts = new Map<string, number>();
    const roomTotals = devices.reduce<Record<string, number>>((counts, device, index) => {
        const room = displayRoom(device, index);
        counts[room] = (counts[room] ?? 0) + 1;
        return counts;
    }, {});
    return devices.map((device, deviceIndex) => {
        const room = displayRoom(device, deviceIndex);
        const [x, z, width, depth] = rooms[room] ?? rooms.living;
        const index = roomCounts.get(room) ?? 0;
        roomCounts.set(room, index + 1);
        const count = roomTotals[room] ?? 1;
        const columns = Math.ceil(Math.sqrt(count));
        const rows = Math.ceil(count / columns);
        return new THREE.Vector3(x + (index % columns - (columns - 1) / 2) * width / columns,
            room === 'study' ? .74 : room === 'bedroom' ? .54 : .08,
            z + (Math.floor(index / columns) - (rows - 1) / 2) * depth / rows);
    });
}

function disposeGroup(group: THREE.Group) {
    const geometries = new Set<THREE.BufferGeometry>();
    const materials = new Set<THREE.Material>();
    group.traverse(object => {
        if (object instanceof THREE.Mesh || object instanceof THREE.Line || object instanceof THREE.Points) {
            geometries.add(object.geometry);
            (Array.isArray(object.material) ? object.material : [object.material]).forEach(value => materials.add(value));
        }
    });
    geometries.forEach(value => value.dispose());
    materials.forEach(value => value.dispose());
}

function rebuild() {
    if (!scene) return;
    if (content) { scene.remove(content); disposeGroup(content); }
    content = new THREE.Group();
    scene.add(content);
    anchors = [];
    deviceObjects = [];
    if (props.layout === 'home') addApartment(content);
    else {
        const radius = Math.max(3.2, 3 + Math.floor(Math.max(0, outerDevices.value.length - 1) / 12) * 1.15);
        const floor = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius, .11, 72), material('#1b303b', .95));
        floor.position.y = -.08;
        floor.receiveShadow = true;
        content.add(floor);
        const rim = new THREE.Mesh(new THREE.RingGeometry(radius - .035, radius, 96), new THREE.MeshBasicMaterial({ color: '#38515d', side: THREE.DoubleSide }));
        rim.rotation.x = -Math.PI / 2;
        rim.position.y = -.015;
        content.add(rim);
    }
    const gatewayPosition = props.layout === 'home' ? new THREE.Vector3(.25, .42, .83) : new THREE.Vector3(0, .05, 0);
    if (!centerGateway.value) {
        // The node is a logical site anchor, not an extra physical server. Each
        // gateway keeps its own model and self asset when several groups exist.
        const hub = new THREE.Group();
        const disc = new THREE.Mesh(new THREE.CylinderGeometry(.18, .18, .035, 32),
            new THREE.MeshBasicMaterial({ color: '#31bda6', transparent: true, opacity: .75 }));
        hub.add(disc);
        const frame = new THREE.Mesh(new THREE.TorusGeometry(.26, .012, 8, 40),
            new THREE.MeshBasicMaterial({ color: '#67e5ce' }));
        frame.rotation.x = -Math.PI / 2;
        hub.add(frame);
        hub.position.copy(gatewayPosition);
        content.add(hub);
        anchors.push({ id: '__node__', name: props.gatewayName || words.value.gateway,
            title: `${props.gatewayName || words.value.gateway} · ${words.value.site}`,
            position: gatewayPosition.clone().add(new THREE.Vector3(0, .3, 0)), node: true, selectable: false });
    }

    const positions = devicePositions(outerDevices.value);
    const positionById = new Map(outerDevices.value.map((device, index) => [device.id, positions[index]!]));
    if (centerGateway.value) positionById.set(centerGateway.value.id, gatewayPosition);
    props.devices.forEach((device, index) => {
        const position = positionById.get(device.id)!;
        const central = device.id === centerGateway.value?.id;
        const outerIndex = outerDevices.value.findIndex(other => other.id === device.id);
        const roomCount = central ? 1 : outerDevices.value.filter((other, otherIndex) => displayRoom(other, otherIndex) === displayRoom(device, outerIndex)).length;
        const scale = props.layout === 'home' && !central ? Math.min(1, Math.max(.22, 2 / Math.sqrt(Math.max(1, roomCount)))) : 1;
        const model = hardware(device.kind, device.is_gateway);
        model.position.copy(position);
        model.scale.setScalar(scale);
        if (device.asset_device_id) {
            model.traverse(object => { object.userData.deviceId = device.asset_device_id; });
        }
        // Non-selectable group geometry still blocks picks on devices behind it.
        deviceObjects.push(model);
        content!.add(model);
        const selected = props.selectedDeviceId === device.id;
        addMarker(content!, position, selected, device.is_gateway, scale);
        const parentPosition = device.parent_id ? positionById.get(device.parent_id) : gatewayPosition;
        if (index < 12 || selected || device.is_gateway) anchors.push({
            id: device.id, name: device.name,
            title: device.is_gateway && device.group_name && device.group_name !== device.name
                ? `${device.group_name} · ${device.name}` : device.name,
            position: position.clone().add(new THREE.Vector3(0, (device.kind === 'server' ? .8 : .6) * scale, 0)),
            gateway: device.is_gateway, selectable: !!device.asset_device_id,
        });
        // A missing parent is not evidence of a direct node connection.
        if (central || !parentPosition || device.parent_id === device.id) return;
        const start = parentPosition.clone().add(new THREE.Vector3(0, .1, 0));
        const end = position.clone().add(new THREE.Vector3(0, .1 * scale, 0));
        const middle = start.clone().lerp(end, .5);
        middle.y = Math.max(start.y, end.y) + .25;
        const curve = new THREE.QuadraticBezierCurve3(start, middle, end);
        const geometry = new THREE.BufferGeometry().setFromPoints(curve.getPoints(32));
        const line = new THREE.Line(geometry, new THREE.LineDashedMaterial({
            color: selected ? '#18c6b1' : '#459cae', transparent: true, opacity: selected ? .95 : .68,
            dashSize: .1, gapSize: .08, depthWrite: false, depthTest: false,
        }));
        line.computeLineDistances();
        line.renderOrder = 2;
        content!.add(line);
    });
    requestFrame();
}

function projectLabels() {
    if (!camera || !viewport.width || !viewport.height) return;
    const screen: ScreenLabel[] = [];
    const priority = (anchor: LabelAnchor) => anchor.node ? 3 : anchor.id === props.selectedDeviceId ? 2 : anchor.gateway ? 1 : 0;
    const sorted = [...anchors].sort((a, b) => priority(b) - priority(a));
    for (const anchor of sorted) {
        const projected = anchor.position.clone().project(camera);
        if (projected.z < -1 || projected.z > 1) continue;
        const x = THREE.MathUtils.clamp((projected.x * .5 + .5) * viewport.width, 65, Math.max(65, viewport.width - 65));
        let y = (-projected.y * .5 + .5) * viewport.height - 12;
        if (y < 42 || y > viewport.height - 34) continue;
        let overlap = false;
        for (let attempt = 0; attempt < 4; attempt++) {
            overlap = screen.some(label => Math.abs(label.x - x) < 110 && Math.abs(label.y - y) < 27);
            if (!overlap) break;
            y -= 28;
        }
        if (y < 42 || overlap) continue;
        screen.push({ id: anchor.id, name: anchor.name, title: anchor.title, x, y, gateway: anchor.gateway, node: anchor.node, selectable: anchor.selectable });
    }
    labels.value = screen;
}

function requestFrame() {
    if (frame || destroyed || failure.value || !inViewport || document.hidden) return;
    frame = requestAnimationFrame(() => {
        frame = 0;
        if (!renderer || !scene || !camera || destroyed || failure.value || !inViewport || document.hidden) return;
        controls?.update();
        renderer.render(scene, camera);
        projectLabels();
    });
}

function resize() {
    if (!host.value || !renderer || !camera) return;
    const width = host.value.clientWidth;
    const height = host.value.clientHeight;
    if (!width || !height) return;
    viewport.width = width;
    viewport.height = height;
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.updateProjectionMatrix();
    requestFrame();
}

function pointerDown(event: PointerEvent) { pointerStart = { x: event.clientX, y: event.clientY }; }
function pointerUp(event: PointerEvent) {
    const start = pointerStart;
    pointerStart = undefined;
    if (!start || !renderer || !camera || Math.hypot(event.clientX - start.x, event.clientY - start.y) > 5) return;
    const rect = renderer.domElement.getBoundingClientRect();
    pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -(event.clientY - rect.top) / rect.height * 2 + 1);
    raycaster.setFromCamera(pointer, camera);
    const hit = raycaster.intersectObjects(deviceObjects, true)[0];
    const id = hit?.object.userData.deviceId;
    if (typeof id === 'string') emit('select-device', id);
}
function pointerCancel() { pointerStart = undefined; }
function contextLost(event: Event) {
    event.preventDefault();
    failure.value = true;
    labels.value = [];
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    emit('unavailable', 'webgl-context-lost');
}

function resetView() {
    if (!camera || !controls) return;
    const radius = 3 + Math.floor(Math.max(0, outerDevices.value.length - 1) / 12) * 1.15;
    const scale = props.layout === 'topology' ? Math.max(1, radius / 3.2) : 1;
    camera.position.set(8.6 * scale, 8.4 * scale, 10.6 * scale);
    controls.target.set(0, .15, 0);
    controls.update();
    requestFrame();
}
function zoom(factor: number) {
    if (!camera || !controls) return;
    const offset = camera.position.clone().sub(controls.target);
    const distance = THREE.MathUtils.clamp(offset.length() * factor, controls.minDistance, controls.maxDistance);
    camera.position.copy(controls.target).add(offset.setLength(distance));
    controls.update();
    requestFrame();
}
function visibilityChanged() {
    if (document.hidden && frame) { cancelAnimationFrame(frame); frame = 0; }
    else requestFrame();
}
function motionChanged() {
    if (controls) controls.enableDamping = !motionQuery?.matches;
    requestFrame();
}

function destroyRenderer() {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    controls?.removeEventListener('change', requestFrame);
    controls?.dispose();
    controls = undefined;
    if (content) { disposeGroup(content); content = undefined; }
    scene?.traverse(object => { if (object instanceof THREE.Light) object.dispose(); });
    scene?.clear();
    scene = undefined;
    if (renderer) {
        const canvas = renderer.domElement;
        canvas.removeEventListener('pointerdown', pointerDown);
        canvas.removeEventListener('pointerup', pointerUp);
        canvas.removeEventListener('pointercancel', pointerCancel);
        canvas.removeEventListener('webglcontextlost', contextLost);
        renderer.dispose();
        renderer.forceContextLoss();
        canvas.remove();
    }
    renderer = undefined;
    camera = undefined;
    anchors = [];
    deviceObjects = [];
}

function initialize() {
    if (!host.value || destroyed) return;
    failure.value = false;
    try {
        renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'low-power' });
        renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.8));
        renderer.outputColorSpace = THREE.SRGBColorSpace;
        renderer.toneMapping = THREE.ACESFilmicToneMapping;
        renderer.toneMappingExposure = 1.28;
        renderer.shadowMap.enabled = true;
        renderer.shadowMap.type = THREE.PCFSoftShadowMap;
        renderer.domElement.className = 'node-space-canvas';
        renderer.domElement.setAttribute('aria-hidden', 'true');
        host.value.prepend(renderer.domElement);
        scene = new THREE.Scene();
        camera = new THREE.PerspectiveCamera(34, 1, .1, 1000);
        controls = new OrbitControls(camera, renderer.domElement);
        controls.enableDamping = !motionQuery?.matches;
        controls.dampingFactor = .12;
        controls.enablePan = false;
        controls.minDistance = 6;
        controls.maxDistance = 400;
        controls.minPolarAngle = .22;
        controls.maxPolarAngle = Math.PI / 2.15;
        controls.addEventListener('change', requestFrame);
        scene.add(new THREE.HemisphereLight('#d7e8f0', '#9a8169', 2.3));
        const key = new THREE.DirectionalLight('#ffe4bd', 3.6);
        key.position.set(-4, 8, 5);
        key.castShadow = true;
        key.shadow.mapSize.set(1024, 1024);
        key.shadow.camera.left = -7;
        key.shadow.camera.right = 7;
        key.shadow.camera.top = 7;
        key.shadow.camera.bottom = -7;
        key.shadow.normalBias = .04;
        scene.add(key);
        const fill = new THREE.DirectionalLight('#8fc6e6', 1.3);
        fill.position.set(5, 3, -4);
        scene.add(fill);
        renderer.domElement.addEventListener('pointerdown', pointerDown);
        renderer.domElement.addEventListener('pointerup', pointerUp);
        renderer.domElement.addEventListener('pointercancel', pointerCancel);
        renderer.domElement.addEventListener('webglcontextlost', contextLost);
        rebuild();
        resetView();
        resize();
    } catch (error) {
        destroyRenderer();
        failure.value = true;
        labels.value = [];
        emit('unavailable', error instanceof Error ? error.message : 'webgl-unavailable');
    }
}

function retry() { destroyRenderer(); initialize(); }
defineExpose({ resetView, zoomIn: () => zoom(.85), zoomOut: () => zoom(1.18) });

watch(() => [props.devices, props.selectedDeviceId, props.gatewayName, props.locale], rebuild, { deep: true });
// Asset reads arrive after the scene mounts. Fit the changed device extent,
// while preserving camera interaction for selection and same-size refreshes.
watch(() => outerDevices.value.length, resetView);
watch(() => props.layout, () => { rebuild(); resetView(); });
onMounted(() => {
    motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
    motionQuery.addEventListener('change', motionChanged);
    initialize();
    resizeObserver = new ResizeObserver(resize);
    if (host.value) resizeObserver.observe(host.value);
    intersectionObserver = new IntersectionObserver(entries => {
        inViewport = entries[0]?.isIntersecting ?? true;
        if (!inViewport && frame) { cancelAnimationFrame(frame); frame = 0; }
        if (inViewport) requestFrame();
    });
    if (host.value) intersectionObserver.observe(host.value);
    document.addEventListener('visibilitychange', visibilityChanged);
});
onBeforeUnmount(() => {
    destroyed = true;
    resizeObserver?.disconnect();
    intersectionObserver?.disconnect();
    motionQuery?.removeEventListener('change', motionChanged);
    document.removeEventListener('visibilitychange', visibilityChanged);
    destroyRenderer();
});
</script>

<template>
    <div ref="host" class="node-space" role="group" :aria-label="words.region" :data-layout="layout" :data-center-gateway="centerGateway?.id">
        <div class="space-caption"><span>{{ layout === 'home' ? words.home : words.topology }}</span><span class="relationship-caption">{{ centerGateway ? words.merged : words.configured }}</span></div>
        <template v-if="!failure">
            <div class="space-label-layer">
                <template v-for="label in labels" :key="label.id">
                    <span v-if="!label.selectable" class="space-label gateway-label" :style="{ left: `${label.x}px`, top: `${label.y}px` }" :title="label.title || label.name"><i :class="['pi', label.node ? 'pi-sitemap' : 'pi-wifi']" aria-hidden="true"></i>{{ label.name }}</span>
                    <button v-else type="button" class="space-label" :class="{ selected: selectedDeviceId === label.id, 'gateway-label': label.gateway }"
                        :style="{ left: `${label.x}px`, top: `${label.y}px` }" :title="label.title || label.name"
                        :aria-pressed="selectedDeviceId === label.id" @pointerdown.stop @click.stop="emit('select-device', label.id)">
                        <i :class="['pi', label.gateway ? 'pi-wifi' : 'pi-server']" aria-hidden="true"></i>{{ label.name }}
                    </button>
                </template>
            </div>
            <div class="space-controls">
                <button type="button" :aria-label="words.zoomIn" :title="words.zoomIn" @click="zoom(.85)"><i class="pi pi-plus" aria-hidden="true"></i></button>
                <button type="button" :aria-label="words.zoomOut" :title="words.zoomOut" @click="zoom(1.18)"><i class="pi pi-minus" aria-hidden="true"></i></button>
                <button type="button" :aria-label="words.reset" :title="words.reset" @click="resetView"><i class="pi pi-refresh" aria-hidden="true"></i></button>
            </div>
            <p class="space-hint">{{ devices.length ? words.hint : words.empty }}<span v-if="layout === 'home'"> · {{ words.illustrative }}</span></p>
        </template>
        <div v-else class="space-unavailable" role="status">
            <i class="pi pi-box" aria-hidden="true"></i><strong>{{ words.unavailable }}</strong><p>{{ words.fallback }}</p>
            <button type="button" class="space-retry" @click="retry"><i class="pi pi-refresh" aria-hidden="true"></i>{{ words.retry }}</button>
            <div class="space-fallback-devices"><template v-for="device in devices" :key="device.id"><button v-if="device.asset_device_id" type="button" @click="emit('select-device', device.asset_device_id)">{{ device.name }}<i class="pi pi-angle-right" aria-hidden="true"></i></button><span v-else>{{ device.name }}</span></template></div>
        </div>
    </div>
</template>

<style scoped>
.node-space { position: relative; width: 100%; height: 100%; min-height: 340px; overflow: hidden; isolation: isolate; background: #101b24; color: #e4ecef; border-radius: 8px; }
.node-space :deep(.node-space-canvas) { display: block; width: 100%; height: 100%; touch-action: none; outline: none; }
.space-caption { position: absolute; top: 12px; left: 12px; right: 12px; display: flex; align-items: center; justify-content: space-between; gap: 8px; pointer-events: none; z-index: 2; font-size: 11px; line-height: 1.5; color: #b8c9d3; }
.space-caption > span:first-child { padding: 3px 7px; background: #172833e8; border: 1px solid #34454f; border-radius: 4px; }
.relationship-caption { opacity: .85; }
.space-label-layer { position: absolute; inset: 0; pointer-events: none; z-index: 2; }
.space-label { display: inline-flex; align-items: center; gap: 5px; position: absolute; transform: translate(-50%, -100%); max-width: 140px; min-height: 25px; padding: 4px 7px; border: 1px solid #536772; border-radius: 5px; background: #15252eea; color: #e8f0f2; font: inherit; font-size: 11px; line-height: 1.3; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; pointer-events: auto; cursor: pointer; box-shadow: 0 2px 6px #0003; }
.space-label .pi { font-size: 11px; }
.space-label:hover, .space-label.selected { border-color: #43d9b6; color: #b8ffeb; background: #143e3ceF; z-index: 3; }
.space-label:focus-visible, .space-controls button:focus-visible, .space-retry:focus-visible, .space-fallback-devices button:focus-visible { outline: 2px solid #5fe9c8; outline-offset: 3px; }
.gateway-label { color: #a2f6df; border-color: #35776f; cursor: default; }
button.gateway-label { cursor: pointer; }
.space-controls { position: absolute; right: 10px; bottom: 35px; z-index: 3; display: flex; flex-direction: column; overflow: hidden; border: 1px solid #344b58; border-radius: 6px; background: #13232eea; }
.space-controls button { display: grid; place-items: center; width: 32px; height: 32px; padding: 0; border: 0; border-bottom: 1px solid #344b58; color: #d0e1e8; background: transparent; cursor: pointer; }
.space-controls button:last-child { border: 0; }
.space-controls button:hover { color: #80e6c7; background: #24404b; }
.space-hint { position: absolute; left: 12px; right: 45px; bottom: 10px; margin: 0; font-size: 10px; color: #a3b6c1; pointer-events: none; }
.space-unavailable { min-height: 340px; padding: 64px 24px 28px; display: flex; align-items: center; justify-content: center; flex-direction: column; gap: 12px; text-align: center; font-size: 13px; }
.space-unavailable > .pi { color: #87a4b1; font-size: 30px; }
.space-unavailable p { margin: 0; color: #9eb0bc; }
.space-retry { display: inline-flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: 5px; border: 1px solid #427c74; color: #b9f1df; background: #1a383b; cursor: pointer; }
.space-fallback-devices { width: 100%; display: flex; flex-direction: column; gap: 2px; margin-top: 8px; max-height: 220px; overflow-y: auto; }
.space-fallback-devices button { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 10px; border: 0; border-radius: 4px; background: #1e303c; color: #d7e3e9; text-align: left; cursor: pointer; }
@media (max-width: 480px) { .node-space { min-height: 300px; } .relationship-caption { font-size: 10px; } .space-label { max-width: 116px; } }
</style>
