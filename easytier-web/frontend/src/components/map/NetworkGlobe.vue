<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type ComponentPublicInstance } from 'vue';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { mergeGeometries } from 'three/addons/utils/BufferGeometryUtils.js';
import type { MapSceneLink, MapSceneNode } from '../../modules/network-space';
import { GeographicArc, geographicPosition } from './globe-geometry';
import dayMapUrl from '../../assets/map/earth-blue-marble.jpg';
import nightMapUrl from '../../assets/map/earth-night.jpg';

const props = withDefaults(defineProps<{
    nodes: MapSceneNode[];
    links: MapSceneLink[];
    selectedNodeId?: string | null;
    selectedLinkId?: string | null;
    reducedMotion?: boolean;
    locale?: string;
}>(), { selectedNodeId: null, selectedLinkId: null, reducedMotion: false, locale: 'cn' });
const emit = defineEmits<{
    'select-node': [id: string];
    'select-link': [id: string];
    unavailable: [reason: string];
}>();
const root = ref<HTMLElement>();
const canvas = ref<HTMLCanvasElement>();
const unavailable = ref(false);
const textureFailed = ref(false);
const ready = ref(false);
const systemReducedMotion = ref(false);
const paused = ref(false);
const english = computed(() => props.locale.startsWith('en'));
const text = (cn: string, en: string) => english.value ? en : cn;
const reducedMotion = computed(() => props.reducedMotion || systemReducedMotion.value);
const labelElements = new Map<string, HTMLElement>();

let renderer: THREE.WebGLRenderer | undefined;
let camera: THREE.PerspectiveCamera | undefined;
let scene: THREE.Scene | undefined;
let controls: OrbitControls | undefined;
let earth: THREE.Mesh | undefined;
let nodeLayer: THREE.Group | undefined;
let linkLayer: THREE.Group | undefined;
let frame = 0;
let destroyed = false;
let inView = true;
let size = { width: 1, height: 1 };
let targetCamera: THREE.Vector3 | null = null;
let resizeObserver: ResizeObserver | undefined;
let visibilityObserver: IntersectionObserver | undefined;
let motionQuery: MediaQueryList | undefined;
let pointerStart: { x: number; y: number; id: number } | null = null;
let lastPointerUp = 0;
let initialFocusApplied = false;
const textures: THREE.Texture[] = [];
const nodeObjects: THREE.Object3D[] = [];
const linkObjects: THREE.Object3D[] = [];
const nodePositions = new Map<string, THREE.Vector3>();
const particles: { mesh: THREE.Mesh; curve: GeographicArc; speed: number; reverse: boolean; offset: number }[] = [];
const raycaster = new THREE.Raycaster();
raycaster.params.Line = { threshold: .018 };
const pointer = new THREE.Vector2();
const projected = new THREE.Vector3();
const temporary = new THREE.Vector3();

function setLabelRef(id: string, element: Element | ComponentPublicInstance | null) {
    if (element instanceof HTMLElement) labelElements.set(id, element);
    else labelElements.delete(id);
}

function disposeObject(object: THREE.Object3D) {
    object.traverse(child => {
        const drawable = child as THREE.Mesh;
        drawable.geometry?.dispose();
        if (drawable.material) {
            const materials = Array.isArray(drawable.material) ? drawable.material : [drawable.material];
            materials.forEach(material => material.dispose());
        }
    });
}

function clearLayer(layer: THREE.Group | undefined) {
    if (!layer) return;
    disposeObject(layer);
    layer.clear();
}

function requestRender() {
    if (destroyed || frame || !renderer || !inView || document.hidden || unavailable.value) return;
    frame = requestAnimationFrame(renderFrame);
}

function updateLabels() {
    if (!camera) return;
    const occupied: { left: number; top: number; width: number; height: number }[] = [];
    const sorted = [...props.nodes].sort((a, b) => Number(b.id === props.selectedNodeId) - Number(a.id === props.selectedNodeId));
    for (const node of sorted) {
        const element = labelElements.get(node.id);
        const position = nodePositions.get(node.id);
        if (!element) continue;
        if (!position) {
            element.style.visibility = 'hidden';
            continue;
        }
        const frontFacing = temporary.copy(camera.position).sub(position).dot(position) > .025;
        projected.copy(position).project(camera);
        if (!frontFacing || projected.z > 1 || Math.abs(projected.x) > 1 || Math.abs(projected.y) > 1) {
            element.style.visibility = 'hidden';
            continue;
        }
        const x = (projected.x * .5 + .5) * size.width;
        const y = (-projected.y * .5 + .5) * size.height;
        const width = Math.min(150, element.offsetWidth || 120);
        const height = 29;
        const candidates = [[14, -height / 2], [14, 19], [14, -49], [-width - 14, -height / 2], [-width - 14, 19], [14, 50], [14, -80]];
        let placement: { left: number; top: number; width: number; height: number } | undefined;
        for (const [dx, dy] of candidates) {
            const rectangle = { left: x + dx!, top: y + dy!, width, height };
            if (rectangle.left < 6 || rectangle.top < 6 || rectangle.left + width > size.width - 6 || rectangle.top + height > size.height - 46) continue;
            if (occupied.some(other => rectangle.left < other.left + other.width + 5 && rectangle.left + width + 5 > other.left && rectangle.top < other.top + other.height + 4 && rectangle.top + height + 4 > other.top)) continue;
            placement = rectangle;
            break;
        }
        if (!placement || occupied.length >= 32) {
            element.style.visibility = 'hidden';
            continue;
        }
        occupied.push(placement);
        element.style.visibility = 'visible';
        element.style.transform = `translate(${Math.round(placement.left)}px, ${Math.round(placement.top)}px)`;
    }
}

function renderFrame(time: number) {
    frame = 0;
    if (!renderer || !scene || !camera || destroyed || document.hidden || !inView) return;
    if (targetCamera) {
        if (reducedMotion.value || camera.position.distanceTo(targetCamera) < .003) {
            camera.position.copy(targetCamera);
            targetCamera = null;
        } else camera.position.lerp(targetCamera, .1);
    }
    controls?.update();
    const animateTraffic = !reducedMotion.value && !paused.value;
    for (const particle of particles) {
        particle.mesh.visible = animateTraffic;
        if (animateTraffic) {
            const progress = ((time * .0001 * particle.speed + particle.offset) % 1);
            particle.curve.getPoint(particle.reverse ? 1 - progress : progress, particle.mesh.position);
        }
    }
    updateLabels();
    renderer.render(scene, camera);
    if (targetCamera || (animateTraffic && particles.length > 0)) requestRender();
}

function rebuildData() {
    if (!scene || !nodeLayer || !linkLayer) return;
    clearLayer(nodeLayer);
    clearLayer(linkLayer);
    nodeObjects.length = 0;
    linkObjects.length = 0;
    particles.length = 0;
    nodePositions.clear();
    const validNodes = props.nodes.filter(node => Number.isFinite(node.latitude) && Number.isFinite(node.longitude) && Math.abs(node.latitude) <= 90 && Math.abs(node.longitude) <= 180);
    for (const node of validNodes) {
        const position = geographicPosition(node.latitude, node.longitude, 1.022);
        nodePositions.set(node.id, position);
        const selected = node.id === props.selectedNodeId;
        const color = node.online ? (selected ? 0x45f4c4 : 0x6ce7ff) : 0x82909e;
        const marker = new THREE.Mesh(new THREE.SphereGeometry(selected ? .017 : .01, 16, 12), new THREE.MeshBasicMaterial({ color, toneMapped: false }));
        marker.position.copy(position);
        marker.userData = { nodeId: node.id };
        nodeLayer.add(marker);
        nodeObjects.push(marker);
        const ring = new THREE.Mesh(
            new THREE.RingGeometry(selected ? .022 : .014, selected ? .026 : .017, 40),
            new THREE.MeshBasicMaterial({ color, toneMapped: false, side: THREE.DoubleSide, transparent: true, opacity: node.online ? .9 : .5, depthWrite: false }),
        );
        ring.position.copy(position.clone().multiplyScalar(1.003));
        ring.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), position.clone().normalize());
        nodeLayer.add(ring);
        if (selected) {
            const halo = new THREE.Mesh(new THREE.RingGeometry(.029, .047, 48), new THREE.MeshBasicMaterial({ color: 0x2be1b1, toneMapped: false, side: THREE.DoubleSide, transparent: true, opacity: .2, depthWrite: false }));
            halo.position.copy(ring.position);
            halo.quaternion.copy(ring.quaternion);
            nodeLayer.add(halo);
        }
    }
    const byId = new Map(validNodes.map(node => [node.id, node]));
    for (const link of props.links) {
        const source = nodePositions.get(link.source);
        const target = nodePositions.get(link.target);
        if (!source || !target || source.distanceToSquared(target) < .0000001) continue;
        const selected = link.id === props.selectedLinkId;
        const available = !link.stale && byId.get(link.source)?.online && byId.get(link.target)?.online;
        const color = !available ? 0x647787 : link.kind === 'relay' ? 0xf5b94d : 0x5edfeb;
        const curve = new GeographicArc(source, target);
        const geometry = new THREE.BufferGeometry().setFromPoints(curve.getPoints(80));
        const material = link.kind === 'relay' || link.stale
            ? new THREE.LineDashedMaterial({ color, toneMapped: false, dashSize: .025, gapSize: .015, transparent: true, opacity: selected ? 1 : .8 })
            : new THREE.LineBasicMaterial({ color, toneMapped: false, transparent: true, opacity: selected ? 1 : .85 });
        const line = new THREE.Line(geometry, material);
        line.computeLineDistances();
        line.userData = { linkId: link.id };
        linkLayer.add(line);
        linkObjects.push(line);
        if (available) {
            let geometry: THREE.BufferGeometry;
            if (link.kind === 'relay') {
                // Tube segments keep relay dashes legible on high-DPI displays.
                const count = Math.min(40, Math.max(6, Math.ceil(curve.getLength() / .07)));
                const segments: THREE.BufferGeometry[] = [];
                for (let i = 0; i < count; i++) {
                    const dash = new THREE.CatmullRomCurve3([0, .3, .6].map(offset => curve.getPoint((i + offset) / count)));
                    segments.push(new THREE.TubeGeometry(dash, 4, selected ? .0035 : .0025, 5, false));
                }
                geometry = mergeGeometries(segments)!;
                segments.forEach(segment => segment.dispose());
            } else geometry = new THREE.TubeGeometry(curve, 64, selected ? .004 : .0026, 5, false);
            const tube = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ color, toneMapped: false, transparent: true, opacity: selected ? .95 : .8 }));
            linkLayer.add(tube);
        }
        if (!available) continue;
        for (const [rate, reverse] of [[link.tx_bps, false], [link.rx_bps, true]] as const) {
            if (!Number.isFinite(rate) || rate <= 0) continue;
            const speed = Math.min(3, Math.max(.5, Math.log10(rate + 1) / 2));
            const particle = new THREE.Mesh(new THREE.SphereGeometry(.006, 8, 6), new THREE.MeshBasicMaterial({ color: 0xd9ffff, toneMapped: false, transparent: true, opacity: .98 }));
            linkLayer.add(particle);
            particles.push({ mesh: particle, curve, speed, reverse, offset: reverse ? .45 : 0 });
        }
    }
    if (!initialFocusApplied && validNodes.length > 0) {
        focusNode(props.selectedNodeId ?? validNodes[0]!.id, false);
        initialFocusApplied = true;
    }
    void nextTick().then(requestRender);
}

function defaultDistance() {
    return 3.35 / Math.min(1, size.width / size.height);
}

function focusNode(id: string | null, animate = true) {
    if (!camera || !id) return;
    const position = nodePositions.get(id);
    if (!position) return;
    const distance = Math.max(2.2, camera.position.length());
    targetCamera = position.clone().normalize().multiplyScalar(distance);
    if (!animate || reducedMotion.value) {
        camera.position.copy(targetCamera);
        targetCamera = null;
        controls?.update();
    }
    requestRender();
}

function resetView() {
    if (!camera) return;
    const selected = props.selectedNodeId && nodePositions.get(props.selectedNodeId);
    const direction = selected || geographicPosition(25, 110);
    targetCamera = direction.clone().normalize().multiplyScalar(defaultDistance());
    controls?.target.set(0, 0, 0);
    requestRender();
}

function zoom(factor: number) {
    if (!camera || !controls) return;
    targetCamera = null;
    camera.position.multiplyScalar(factor).clampLength(controls.minDistance, controls.maxDistance);
    controls.update();
    requestRender();
}
const zoomIn = () => zoom(.84);
const zoomOut = () => zoom(1.19);
defineExpose({ resetView, zoomIn, zoomOut });

function keyboardControl(event: KeyboardEvent) {
    if (!camera || !controls) return;
    if (event.key === '+' || event.key === '=') { event.preventDefault(); zoomIn(); return; }
    if (event.key === '-') { event.preventDefault(); zoomOut(); return; }
    if (event.key === 'Home') { event.preventDefault(); resetView(); return; }
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
    event.preventDefault();
    targetCamera = null;
    const position = new THREE.Spherical().setFromVector3(camera.position);
    if (event.key === 'ArrowLeft') position.theta -= .12;
    if (event.key === 'ArrowRight') position.theta += .12;
    if (event.key === 'ArrowUp') position.phi -= .12;
    if (event.key === 'ArrowDown') position.phi += .12;
    position.makeSafe();
    camera.position.setFromSpherical(position);
    controls.update();
    requestRender();
}

function resize() {
    if (!root.value || !renderer || !camera) return;
    const previous = size;
    size = { width: Math.max(1, root.value.clientWidth), height: Math.max(1, root.value.clientHeight) };
    camera.aspect = size.width / size.height;
    camera.updateProjectionMatrix();
    renderer.setSize(size.width, size.height, false);
    if (previous.width === 1) camera.position.setLength(defaultDistance());
    requestRender();
}

function pointerDown(event: PointerEvent) {
    pointerStart = { x: event.clientX, y: event.clientY, id: event.pointerId };
}

function pointerUp(event: PointerEvent) {
    if (!pointerStart || pointerStart.id !== event.pointerId || !canvas.value || !camera || !earth) return;
    const moved = Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y);
    pointerStart = null;
    // Avoid a touch gesture or OrbitControls drag becoming an accidental selection.
    if (moved > 5 || event.button !== 0 || performance.now() - lastPointerUp < 150) return;
    lastPointerUp = performance.now();
    const rectangle = canvas.value.getBoundingClientRect();
    pointer.set((event.clientX - rectangle.left) / rectangle.width * 2 - 1, -(event.clientY - rectangle.top) / rectangle.height * 2 + 1);
    raycaster.setFromCamera(pointer, camera);
    const earthDistance = raycaster.intersectObject(earth)[0]?.distance ?? Infinity;
    const nodeHit = raycaster.intersectObjects(nodeObjects).find(hit => hit.distance < earthDistance + .015);
    if (nodeHit) {
        emit('select-node', nodeHit.object.userData.nodeId as string);
        return;
    }
    const linkHit = raycaster.intersectObjects(linkObjects).find(hit => hit.distance < earthDistance);
    if (linkHit) emit('select-link', linkHit.object.userData.linkId as string);
}

function visibilityChange() {
    if (document.hidden || !inView) {
        cancelAnimationFrame(frame);
        frame = 0;
    } else requestRender();
}

function motionChange() {
    systemReducedMotion.value = motionQuery?.matches ?? false;
    if (controls) controls.enableDamping = !reducedMotion.value;
    requestRender();
}

function contextLost(event: Event) {
    event.preventDefault();
    unavailable.value = true;
    cancelAnimationFrame(frame);
    frame = 0;
    emit('unavailable', 'webgl-context-lost');
}

onMounted(() => {
    if (!canvas.value || !root.value) return;
    try {
        renderer = new THREE.WebGLRenderer({ canvas: canvas.value, antialias: true, alpha: false, powerPreference: 'low-power' });
        renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
        renderer.setClearColor(0x080f17, 1);
        renderer.outputColorSpace = THREE.SRGBColorSpace;
        renderer.toneMapping = THREE.ACESFilmicToneMapping;
        renderer.toneMappingExposure = 1.2;
        scene = new THREE.Scene();
        camera = new THREE.PerspectiveCamera(40, 1, .05, 50);
        camera.position.copy(geographicPosition(25, 110, 3.35));
        controls = new OrbitControls(camera, canvas.value);
        controls.enablePan = false;
        controls.enableDamping = !reducedMotion.value;
        controls.dampingFactor = .09;
        controls.rotateSpeed = .55;
        controls.zoomSpeed = .7;
        controls.minDistance = 1.45;
        controls.maxDistance = 9;
        controls.addEventListener('change', requestRender);
        controls.addEventListener('start', () => { targetCamera = null; });
        scene.add(new THREE.AmbientLight(0x9fb9da, 1.35));
        const sun = new THREE.DirectionalLight(0xe6eff8, 1.9);
        sun.position.set(-3, 3, -4);
        scene.add(sun);

        const earthMaterial = new THREE.MeshPhongMaterial({ color: 0xc4cfde, specular: 0x20364d, shininess: 8, emissive: 0xffdfac, emissiveIntensity: 1.35 });
        earth = new THREE.Mesh(new THREE.SphereGeometry(1, 112, 80), earthMaterial);
        scene.add(earth);
        const loader = new THREE.TextureLoader();
        const loadTexture = (url: string, apply: (texture: THREE.Texture) => void) => {
            const texture = loader.load(url, loaded => {
                if (destroyed) { loaded.dispose(); return; }
                loaded.colorSpace = THREE.SRGBColorSpace;
                loaded.anisotropy = Math.min(renderer?.capabilities.getMaxAnisotropy() ?? 1, 8);
                apply(loaded);
                earthMaterial.needsUpdate = true;
                ready.value = true;
                requestRender();
            }, undefined, () => {
                if (destroyed) return;
                textureFailed.value = true;
                ready.value = true;
                requestRender();
            });
            textures.push(texture);
        };
        loadTexture(dayMapUrl, texture => { earthMaterial.map = texture; });
        loadTexture(nightMapUrl, texture => { earthMaterial.emissiveMap = texture; });

        // Ray altitude, rather than shell-facing angle, makes the haze decay away
        // from the actual surface. The outer mesh has zero visible edge of its own.
        const atmosphere = new THREE.Mesh(new THREE.SphereGeometry(1.08, 80, 56), new THREE.ShaderMaterial({
            vertexShader: `varying vec3 vPosition;
                void main() { vec4 world = modelMatrix * vec4(position, 1.0); vPosition = world.xyz; gl_Position = projectionMatrix * viewMatrix * world; }`,
            fragmentShader: `varying vec3 vPosition;
                void main() {
                    vec3 ray = normalize(vPosition - cameraPosition);
                    float altitude = max(0.0, length(cross(cameraPosition, ray)) - 1.0);
                    float haze = exp(-altitude * 105.0);
                    haze *= 1.0 - smoothstep(0.055, 0.075, altitude);
                    gl_FragColor = vec4(0.22, 0.66, 1.0, haze * 0.4);
                }`,
            transparent: true, side: THREE.BackSide, blending: THREE.AdditiveBlending, depthWrite: false,
        }));
        scene.add(atmosphere);
        nodeLayer = new THREE.Group();
        linkLayer = new THREE.Group();
        scene.add(nodeLayer, linkLayer);
        resize();
        rebuildData();
        resizeObserver = new ResizeObserver(resize);
        resizeObserver.observe(root.value);
        visibilityObserver = new IntersectionObserver(entries => {
            inView = entries[0]?.isIntersecting ?? true;
            visibilityChange();
        });
        visibilityObserver.observe(root.value);
        motionQuery = window.matchMedia('(prefers-reduced-motion: reduce)');
        motionQuery.addEventListener('change', motionChange);
        motionChange();
        document.addEventListener('visibilitychange', visibilityChange);
        canvas.value.addEventListener('webglcontextlost', contextLost);
    } catch (error) {
        unavailable.value = true;
        emit('unavailable', error instanceof Error ? error.message : 'webgl-unavailable');
    }
});

watch(() => [props.nodes, props.links, props.selectedNodeId, props.selectedLinkId], rebuildData, { deep: true });
watch(() => props.selectedNodeId, id => focusNode(id));
watch(reducedMotion, () => {
    if (controls) controls.enableDamping = !reducedMotion.value;
    requestRender();
});
watch(paused, requestRender);
watch(() => props.locale, () => void nextTick().then(requestRender));

onBeforeUnmount(() => {
    destroyed = true;
    cancelAnimationFrame(frame);
    resizeObserver?.disconnect();
    visibilityObserver?.disconnect();
    motionQuery?.removeEventListener('change', motionChange);
    document.removeEventListener('visibilitychange', visibilityChange);
    canvas.value?.removeEventListener('webglcontextlost', contextLost);
    controls?.dispose();
    if (scene) disposeObject(scene);
    textures.forEach(texture => texture.dispose());
    renderer?.dispose();
    renderer?.forceContextLoss();
    labelElements.clear();
});
</script>

<template>
    <div ref="root" class="network-globe" :class="{ 'network-globe-unavailable': unavailable }" data-testid="network-globe">
        <canvas ref="canvas" tabindex="0" :aria-label="text('三维网络地图：拖动或方向键旋转，滚轮或加减键缩放，Home 重置；也可从列表选择节点', '3D network globe: drag or arrow keys to orbit, scroll or plus/minus keys to zoom, Home to reset. Nodes are also available in the list.')"
            @keydown="keyboardControl" @pointerdown="pointerDown" @pointerup="pointerUp" @pointercancel="pointerStart = null" />
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
        <div v-if="!unavailable" class="globe-controls" :aria-label="text('地图控制', 'Map controls')">
            <button type="button" :title="text('放大', 'Zoom in')" :aria-label="text('放大地图', 'Zoom in map')" @click="zoomIn"><i class="pi pi-plus" aria-hidden="true"></i></button>
            <button type="button" :title="text('缩小', 'Zoom out')" :aria-label="text('缩小地图', 'Zoom out map')" @click="zoomOut"><i class="pi pi-minus" aria-hidden="true"></i></button>
            <button type="button" :title="text('重置视角', 'Reset view')" :aria-label="text('重置地图视角', 'Reset map view')" @click="resetView"><i class="pi pi-compass" aria-hidden="true"></i></button>
            <button type="button" :title="paused ? text('播放流量动画', 'Play traffic animation') : text('暂停流量动画', 'Pause traffic animation')" :aria-label="paused ? text('播放流量动画', 'Play traffic animation') : text('暂停流量动画', 'Pause traffic animation')" :aria-pressed="paused" @click="paused = !paused"><i :class="['pi', paused ? 'pi-play' : 'pi-pause']" aria-hidden="true"></i></button>
        </div>
        <span v-if="!unavailable" class="globe-help">{{ text('拖动旋转 · 滚轮缩放', 'Drag to orbit · Scroll to zoom') }}</span>
    </div>
</template>

<style scoped>
.network-globe { position: relative; width: 100%; height: 100%; min-height: 420px; overflow: hidden; isolation: isolate; background: #080f17; color: #dce8f1; }
.network-globe canvas { display: block; width: 100%; height: 100%; outline: none; touch-action: none; cursor: grab; }
.network-globe canvas:active { cursor: grabbing; }
.network-globe canvas:focus-visible { outline: 2px solid #57efc7; outline-offset: -3px; }
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
@media (max-width: 767px) { .network-globe { min-height: 360px; } .globe-node-label { max-width: 120px; font-size: 11px; } .globe-controls { right: 10px; } }
</style>
