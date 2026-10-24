#!/usr/bin/env python3
"""Create the bundled Natural Earth map from the pinned, locally downloaded sources.

This is a data preparation command, not part of the frontend build. It performs
no network requests. See src/assets/map/administrative-data.md for provenance.
"""
from pathlib import Path
import argparse
import hashlib
import json
import math

SOURCE_COMMIT = 'ca96624a56bd078437bca8184e78163e5039ad19'
SOURCES = {
    'ne_50m_admin_0_countries.geojson': '3e458fc036ad0a66411f2c1e6cac49c5d7bfb81cb1123bc513b22511a2b7fdeb',
    'ne_50m_admin_0_boundary_lines_land.geojson': '2faac4f6b34386f3d21b6e018cf151f241f00e5c936d44dd17d7d9bfb147fa48',
    'ne_10m_admin_1_states_provinces.geojson': '22d0e3ad85eb3e27f17cabf8ba2d50e554fbc27a87796ff891d958185da62fb5',
    'ne_10m_admin_1_states_provinces_lines.geojson': '1a1f30ccaaf4cc9c4bde34266f0b8cbb955d3a4cf254b756912255f2ec7c75b6',
}
SCALE = 10000


def unwrap(points):
    output = []
    for point in points:
        x, y = point[:2]
        if output:
            while x - output[-1][0] > 180:
                x -= 360
            while x - output[-1][0] < -180:
                x += 360
        if not output or [x, y] != output[-1]:
            output.append([x, y])
    return output


def simplify(points, tolerance):
    """Deterministic Ramer-Douglas-Peucker in unwrapped lon/lat space."""
    if len(points) < 3:
        return points
    keep = {0, len(points) - 1}
    pending = [(0, len(points) - 1)]
    longitude_scale = max(.08, math.cos(math.radians(sum(p[1] for p in points) / len(points))))
    while pending:
        first, last = pending.pop()
        a, b = points[first], points[last]
        dx, dy = (b[0] - a[0]) * longitude_scale, b[1] - a[1]
        squared_length = dx * dx + dy * dy
        farthest, greatest = None, tolerance * tolerance
        for index in range(first + 1, last):
            px, py = (points[index][0] - a[0]) * longitude_scale, points[index][1] - a[1]
            t = max(0, min(1, (px * dx + py * dy) / squared_length)) if squared_length else 0
            distance = (px - t * dx) ** 2 + (py - t * dy) ** 2
            if distance > greatest:
                greatest, farthest = distance, index
        if farthest is not None:
            keep.add(farthest)
            pending.extend([(first, farthest), (farthest, last)])
    return [points[index] for index in sorted(keep)]


def encode(points):
    output, previous = [], (0, 0)
    for x, y in points:
        current = (round(x * SCALE), round(y * SCALE))
        if output and current == previous:
            continue
        output.extend([current[0] - previous[0], current[1] - previous[1]])
        previous = current
    return output


def polygons(geometry):
    if not geometry:
        return []
    if geometry['type'] == 'Polygon':
        return [geometry['coordinates']]
    return geometry['coordinates'] if geometry['type'] == 'MultiPolygon' else []


def boundaries(features, tolerance, disputed=False):
    output = []
    for feature in features:
        geometry = feature['geometry']
        if not geometry or geometry['type'] not in ['LineString', 'MultiLineString']:
            continue
        lines = [geometry['coordinates']] if geometry['type'] == 'LineString' else geometry['coordinates']
        properties = feature['properties']
        classification = str(properties.get('FEATURECLA', '')).lower()
        is_disputed = any(word in classification for word in ['disputed', 'indefinite', 'control'])
        if disputed != is_disputed:
            continue
        for line in lines:
            points = simplify(unwrap(line), tolerance)
            if len(points) >= 2:
                output.append(encode(points))
    return output


def regions(features, level):
    output = []
    for feature in features:
        p = feature['properties']
        shapes = []
        largest_area, span, main_bounds = -1, 0, None
        for polygon in polygons(feature['geometry']):
            rings = []
            for ring in polygon:
                original = unwrap(ring)
                simplified = simplify(original, .008 if level == 'region' else .015)
                if len(simplified) < 4:
                    simplified = original
                encoded = encode(simplified)
                if len(encoded) >= 8:
                    rings.append(encoded)
            if not rings:
                continue
            outer = unwrap(polygon[0])
            west, east = min(c[0] for c in outer), max(c[0] for c in outer)
            south, north = min(c[1] for c in outer), max(c[1] for c in outer)
            bounds = [round(v, 4) for v in [west, south, east, north]]
            shapes.append({'bounds': bounds, 'rings': rings})
            area = (east - west) * (north - south) * max(.08, math.cos(math.radians((north + south) / 2)))
            if area > largest_area:
                largest_area, span, main_bounds = area, math.sqrt(area), bounds
        if not shapes:
            continue
        if level == 'country':
            identifier = str(p['ADM0_A3'])
            en, zh = p.get('NAME_EN') or p.get('NAME'), p.get('NAME_ZH')
            point = [p.get('LABEL_X'), p.get('LABEL_Y')]
        else:
            identifier = str(p['adm1_code'])
            en, zh = p.get('name_en') or p.get('name'), p.get('name_zh')
            point = [p.get('longitude'), p.get('latitude')]
        if not all(isinstance(v, (int, float)) and math.isfinite(v) for v in point):
            point = [(main_bounds[0] + main_bounds[2]) / 2, (main_bounds[1] + main_bounds[3]) / 2]
        output.append({
            'id': identifier, 'en': en or identifier, 'zh': zh or en or identifier,
            'at': [round(v, 4) for v in point], 'span': round(span, 4),
            'parent': p.get('adm0_a3') if level == 'region' else None,
            'polygons': shapes,
        })
    return sorted(output, key=lambda item: item['id'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source_directory', type=Path)
    parser.add_argument('--output', type=Path, default=Path(__file__).resolve().parents[1] / 'src/assets/map/administrative-boundaries.json')
    args = parser.parse_args()
    inputs = {}
    for filename, expected in SOURCES.items():
        content = (args.source_directory / filename).read_bytes()
        if hashlib.sha256(content).hexdigest() != expected:
            raise SystemExit(f'Unexpected source content: {filename}. Review provenance before updating the pinned source.')
        inputs[filename] = json.loads(content)['features']
    country_lines = inputs['ne_50m_admin_0_boundary_lines_land.geojson']
    data = {
        'version': 1, 'coordinateScale': SCALE, 'source': 'Natural Earth', 'sourceCommit': SOURCE_COMMIT,
        'countryLines': boundaries(country_lines, .012),
        'disputedLines': boundaries(country_lines, .012, disputed=True),
        'regionLines': boundaries(inputs['ne_10m_admin_1_states_provinces_lines.geojson'], .006),
        'countries': regions(inputs['ne_50m_admin_0_countries.geojson'], 'country'),
        'regions': regions(inputs['ne_10m_admin_1_states_provinces.geojson'], 'region'),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, ensure_ascii=False, separators=(',', ':')) + '\n')
    print(f'Wrote {args.output}: {len(data["countries"])} countries, {len(data["regions"])} regions, {args.output.stat().st_size} bytes')


if __name__ == '__main__':
    main()
