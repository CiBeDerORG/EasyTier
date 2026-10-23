import { definePreset } from '@primeuix/themes';
import Aura from '@primeuix/themes/aura';

const surface = {
    0: '#ffffff', 50: '#f3f7fc', 100: '#eaf0f8', 200: '#dce5ef',
    300: '#c4d1e1', 400: '#94a7c0', 500: '#60758e', 600: '#475e78',
    700: '#30445e', 800: '#21314a', 900: '#0e192a', 950: '#070d19',
};

export default definePreset(Aura, {
    semantic: {
        primary: {
            50: '#ecfdff', 100: '#cff8fc', 200: '#9cecf5',
            300: '#5fdfec', 400: '#2bc7db', 500: '#0fa8c3',
            600: '#0786a2', 700: '#096b84', 800: '#10576c',
            900: '#12485b', 950: '#062e40',
        },
        focusRing: { width: '2px', style: 'solid', color: '{primary.color}', offset: '3px' },
        formField: { borderRadius: '8px', paddingX: '.8rem', paddingY: '.65rem' },
        content: { borderRadius: '12px' },
        transitionDuration: '.18s',
        colorScheme: {
            light: {
                surface,
                primary: { color: '{primary.700}', inverseColor: '#ffffff', hoverColor: '{primary.800}', activeColor: '{primary.900}' },
                highlight: { background: '{primary.50}', focusBackground: '{primary.100}', color: '{primary.700}', focusColor: '{primary.800}' },
            },
            dark: {
                surface,
                primary: { color: '{primary.300}', inverseColor: '{surface.950}', hoverColor: '{primary.200}', activeColor: '{primary.100}' },
                highlight: { background: '#102d3e', focusBackground: '#153b4c', color: '{primary.200}', focusColor: '{primary.100}' },
                text: { color: '#e7f0fc', hoverColor: '#ffffff', mutedColor: '{surface.400}', hoverMutedColor: '{surface.300}' },
            },
        },
    },
});
