import type { ServiceEntry, ServiceInput } from './network-space';

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
