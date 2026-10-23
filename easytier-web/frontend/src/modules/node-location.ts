import type { Utils } from 'easytier-frontend-lib';

export const NODE_LOCATION_TEXT_FIELDS = ['country', 'region', 'city', 'district'] as const;
export type NodeLocationTextField = typeof NODE_LOCATION_TEXT_FIELDS[number];
export type NodeLocationField = NodeLocationTextField | 'latitude' | 'longitude';
type Translate = (key: string, values?: Record<string, string | number>) => string;

export function emptyNodeLocation(): Utils.NodeGeoLocation {
    return { country: null, region: null, city: null, district: null,
        latitude: null, longitude: null, coordinate_system: 'WGS84' };
}

export function normalizeNodeLocation(value?: Utils.NodeGeoLocation | null): Utils.NodeGeoLocation | null {
    if (!value) return null;
    const result: Utils.NodeGeoLocation = {
        country: value.country?.trim() || null, region: value.region?.trim() || null,
        city: value.city?.trim() || null, district: value.district?.trim() || null,
        latitude: value.latitude ?? null, longitude: value.longitude ?? null, coordinate_system: 'WGS84',
    };
    return NODE_LOCATION_TEXT_FIELDS.some(field => result[field] !== null)
        || result.latitude !== null || result.longitude !== null ? result : null;
}

export function nodeLocationErrors(value: Utils.NodeGeoLocation | null | undefined, t: Translate): Partial<Record<NodeLocationField, string>> {
    const errors: Partial<Record<NodeLocationField, string>> = {};
    if (!value) return errors;
    for (const field of NODE_LOCATION_TEXT_FIELDS) {
        const text = value[field] ?? '';
        if (/[\u0000-\u001f\u007f-\u009f]/u.test(text)) errors[field] = t('web.device.attributes_invalid_control');
        else if (Array.from(text.trim()).length > 128) errors[field] = t('web.device.attributes_too_long', { limit: 128 });
    }
    const latitude = value.latitude ?? null, longitude = value.longitude ?? null;
    if ((latitude === null) !== (longitude === null)) {
        errors.latitude = errors.longitude = t('web.node_location.coordinate_pair');
    } else if (latitude !== null && longitude !== null) {
        if (!Number.isFinite(latitude) || latitude < -90 || latitude > 90) errors.latitude = t('web.node_location.latitude_invalid');
        if (!Number.isFinite(longitude) || longitude < -180 || longitude > 180) errors.longitude = t('web.node_location.longitude_invalid');
    }
    return errors;
}

export function nodeLocationLabel(value?: Utils.NodeGeoLocation | null): string {
    return [...new Set(NODE_LOCATION_TEXT_FIELDS.map(field => value?.[field]?.trim()).filter(Boolean))].join(' · ');
}

export function nodeLocationCoordinates(value?: Utils.NodeGeoLocation | null): string {
    if (value?.latitude == null || value.longitude == null
        || !Number.isFinite(value.latitude) || !Number.isFinite(value.longitude)) return '';
    return `${value.latitude.toFixed(6)}, ${value.longitude.toFixed(6)} · WGS84`;
}
