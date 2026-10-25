import type { ServiceEntry, ServiceInput } from './network-space';
import type { AssetService } from '../types/node-assets';

export const serviceIcons = {
    home: 'pi-home', photos: 'pi-images', media: 'pi-play-circle', chart: 'pi-chart-line',
    server: 'pi-server', globe: 'pi-globe', code: 'pi-code',
} as const;

/** Only web addresses can be launched from a navigation card. */
export function safeServiceUrl(value: string): string | null {
    if (!/^https?:\/\//i.test(value.trim()) || /[\u0000-\u001f\u007f]/.test(value)) return null;
    try {
        const url = new URL(value.trim());
        if (!['http:', 'https:'].includes(url.protocol) || !url.hostname || url.username || url.password) return null;
        return url.href;
    } catch {
        return null;
    }
}

export type ServiceAccessLink = { kind: 'internal' | 'public'; url: string };

 /** Browser access to a public mapping uses the actual selected public-server IP and listener port.
  * Keep the service's scheme, path, query and fragment: the port forward only changes its authority.
  * Do not invent a browser URL for a UDP-only mapping or a non-HTTP service.
  */
export function publicServiceUrl(service: Pick<AssetService, 'access_url' | 'publication'>): string | null {
    const internal = safeServiceUrl(service.access_url || '');
    const publication = service.publication;
    if (!internal || !publication?.public_ip || !publication.protocols?.includes('tcp')
        || !Number.isInteger(publication.bind_port) || publication.bind_port < 1 || publication.bind_port > 65535) return null;
    const ip = publication.public_ip.trim().replace(/^\[|\]$/g, '');
    if (!ip || !/^[a-z0-9.:-]+$/i.test(ip)) return null;
    try {
        const original = new URL(internal);
        const host = ip.includes(':') ? `[${ip}]` : ip;
        const publicUrl = new URL(`${original.protocol}//${host}:${publication.bind_port}${original.pathname}${original.search}${original.hash}`);
        const safe = safeServiceUrl(publicUrl.href);
        return safe && safe !== internal ? safe : null;
    } catch {
        return null;
    }
}

export function serviceAccessLinks(service: Pick<AssetService, 'access_url' | 'publication'>): ServiceAccessLink[] {
    const internal = safeServiceUrl(service.access_url || '');
    const published = publicServiceUrl(service);
    return [...(internal ? [{ kind: 'internal' as const, url: internal }] : []),
        ...(published ? [{ kind: 'public' as const, url: published }] : [])];
}

export function serviceInput(entry: ServiceEntry): ServiceInput {
    const { id: _id, ...input } = entry;
    return input;
}

export function sortServices(entries: ServiceEntry[]): ServiceEntry[] {
    return [...entries].sort((a, b) => a.sort_order - b.sort_order || a.name.localeCompare(b.name));
}

export function serviceError(error: unknown, fallback: string): string {
    const detail = (error as { response?: { data?: { message?: unknown } } })?.response?.data?.message;
    return typeof detail === 'string' && detail ? detail : fallback;
}
