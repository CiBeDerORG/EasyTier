import assert from 'node:assert/strict';
import { test } from 'node:test';
import { GeographicArc, geographicPosition, separateNearbyMarkers, assignRouteLanes } from '../src/components/map/globe-geometry.ts';

test('nearby markers separate deterministically while keeping their true anchors unchanged', () => {
    const input = [{ id: 'b', x: 200, y: 150 }, { id: 'a', x: 200, y: 150 }, { id: 'c', x: 203, y: 152 }];
    const snapshot = structuredClone(input);
    const markers = separateNearbyMarkers(input, 500, 350);
    assert.deepEqual(input, snapshot);
    assert.deepEqual(markers, separateNearbyMarkers([...input].reverse(), 500, 350));
    for (let i = 0; i < markers.length; i++) {
        const original = input.find(item => item.id === markers[i].id);
        assert.equal(markers[i].originalX, original.x);
        assert.equal(markers[i].originalY, original.y);
        for (let j = 0; j < i; j++) assert.ok(Math.hypot(markers[i].x - markers[j].x, markers[i].y - markers[j].y) >= 18);
    }
    assert.equal(markers.filter(marker => marker.expanded).length, 2);
});

test('marker expansion respects the visible globe near the horizon', () => {
    const anchors = ['a', 'b', 'c', 'd'].map(id => ({ id, x: 202, y: 80 }));
    const insideGlobe = (x, y) => Math.hypot(x - 120, y - 120) <= 92;
    const result = separateNearbyMarkers(anchors, 300, 240, insideGlobe);
    assert.ok(result.every(marker => insideGlobe(marker.x, marker.y)));
    assert.equal(new Set(result.map(marker => `${marker.x}:${marker.y}`)).size, 4);
});

test('route lanes are stable for parallel and reversed endpoint pairs', () => {
    const source = { x: 80, y: 100 }, target = { x: 300, y: 150 };
    const routes = [{ id: 'one', source, target }, { id: 'two', source: target, target: source },
        { id: 'near', source: { x: 82, y: 101 }, target: { x: 303, y: 150 } },
        { id: 'far', source: { x: 80, y: 300 }, target: { x: 300, y: 350 } }];
    const lanes = assignRouteLanes(routes);
    assert.deepEqual([...lanes].sort(), [...assignRouteLanes([...routes].reverse())].sort());
    assert.equal(new Set(['one', 'two', 'near'].map(id => lanes.get(id).lane)).size, 3);
    assert.equal(lanes.get('far').lane, 0);
    const from = geographicPosition(31, 121, 1.000012), to = geographicPosition(35, 140, 1.000012);
    // Reversed curves invert the normal, so rendering uses a canonical sign.
    const forward = new GeographicArc(from, to, { laneOffset: -.01 });
    const reverse = new GeographicArc(to, from, { laneOffset: -.01 });
    assert.ok(forward.getPoint(.5).distanceTo(reverse.getPoint(.5)) > .01);
});

test('nearby routes with different endpoint IDs use geometric direction to remain separate', () => {
    const routes = [
        { id: '1-to-2', source: { x: 100, y: 100 }, target: { x: 300, y: 100 } },
        { id: '3-to-4', source: { x: 301, y: 101 }, target: { x: 101, y: 101 } },
    ];
    const lanes = assignRouteLanes(routes);
    const first = lanes.get('1-to-2'), second = lanes.get('3-to-4');
    assert.equal(first.direction, -second.direction);
    const forward = new GeographicArc(geographicPosition(30, 120, 1.000012), geographicPosition(35, 140, 1.000012),
        { laneOffset: first.lane * first.direction * .02 });
    const reverse = new GeographicArc(geographicPosition(35.001, 140.001, 1.000012), geographicPosition(30.001, 120.001, 1.000012),
        { laneOffset: second.lane * second.direction * .02 });
    assert.ok(forward.getPoint(.5).distanceTo(reverse.getPoint(.5)) > .015);
});

test('co-located reversed routes share a bundle and use their expanded display direction', () => {
    const anchor = { x: 200, y: 150 }, expanded = { x: 200, y: 130 };
    const lanes = assignRouteLanes([
        { id: 'a', source: anchor, target: anchor, displaySource: anchor, displayTarget: expanded },
        { id: 'b', source: anchor, target: anchor, displaySource: expanded, displayTarget: anchor },
    ]);
    assert.equal(lanes.get('a').direction, -lanes.get('b').direction);
    assert.equal(lanes.get('a').lane, -.5);
    assert.equal(lanes.get('b').lane, .5);
    const from = geographicPosition(31.23, 121.47, 1.000012), to = geographicPosition(31.231, 121.47, 1.000012);
    const first = lanes.get('a'), second = lanes.get('b');
    const forward = new GeographicArc(from, to, { laneOffset: first.lane * first.direction * .0001 });
    const reverse = new GeographicArc(to, from, { laneOffset: second.lane * second.direction * .0001 });
    assert.ok(forward.getPoint(.5).distanceTo(reverse.getPoint(.5)) > .00009);
});

test('local curves stay above the surface and below a close camera while preserving exact endpoints', () => {
    const from = geographicPosition(31.23, 121.47, 1.000012), to = geographicPosition(31.231, 121.472, 1.000012);
    const curve = new GeographicArc(from, to, { maximumHeight: .0002, laneOffset: .00002 });
    assert.ok(curve.getPoint(0).distanceTo(from) < 1e-12);
    assert.ok(curve.getPoint(1).distanceTo(to) < 1e-12);
    for (const point of curve.getPoints(64)) { assert.ok(point.length() > 1); assert.ok(point.length() < 1.0008); }
});

test('co-located nodes keep an anchored, finite relationship loop', () => {
    const point = geographicPosition(-33, 151, 1.000012);
    const curve = new GeographicArc(point, point, { loopRadius: .00003, maximumHeight: .0002 });
    assert.ok(curve.getPoint(0).distanceTo(point) < 1e-12);
    assert.ok(curve.getPoint(1).distanceTo(point) < 1e-12);
    assert.ok(curve.getPoint(.5).distanceTo(point) > .00002);
    assert.ok(curve.getPoints(40).every(value => Number.isFinite(value.x) && Number.isFinite(value.y) && Number.isFinite(value.z) && value.length() > 1));
});

test('date-line and nearly antipodal arcs retain their actual destinations', () => {
    for (const [a, b] of [[[20, 179.99], [20, -179.99]], [[.000001, 0], [0, 179.999999]], [[90, 0], [-90, 0]]]) {
        const from = geographicPosition(...a, 1.000012), to = geographicPosition(...b, 1.000012);
        const curve = new GeographicArc(from, to, { laneOffset: .001 });
        assert.ok(curve.getPoint(0).distanceTo(from) < 1e-12);
        assert.ok(curve.getPoint(1).distanceTo(to) < 1e-12);
        assert.ok(curve.getPoints(40).every(value => Number.isFinite(value.x) && value.length() > 1));
    }
});
