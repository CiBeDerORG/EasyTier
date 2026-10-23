import type { AssetIcon } from '../types/node-assets';

export const DEFAULT_ASSET_SERVICE_ICON: AssetIcon = 'server';

const iconClasses: Record<AssetIcon, string> = {
    server: 'pi pi-server',
    globe: 'pi pi-globe',
    database: 'pi pi-database',
    mobile: 'pi pi-mobile',
    desktop: 'pi pi-desktop',
    home: 'pi pi-home',
    cloud: 'pi pi-cloud',
    cog: 'pi pi-cog',
    play: 'pi pi-play',
    link: 'pi pi-link',
    shield: 'pi pi-shield',
    wifi: 'pi pi-wifi',
};

export const ASSET_SERVICE_ICON_OPTIONS = (Object.keys(iconClasses) as AssetIcon[]).map(value => ({
    value,
    icon: iconClasses[value],
    labelKey: `assets.serviceIcons.${value}`,
}));

/** Only render our own icon classes, never a class or URL supplied by a record. */
export function assetServiceIconClass(value: string | null | undefined): string {
    return value && Object.prototype.hasOwnProperty.call(iconClasses, value)
        ? iconClasses[value as AssetIcon]
        : iconClasses[DEFAULT_ASSET_SERVICE_ICON];
}
