import { useI18n } from 'vue-i18n';
import type { InstanceCandidate } from '../types/node-assets';

/** Translate public reason codes without exposing internal identifiers as UI copy. */
export function useAssetBindingLabels() {
    const { t, te } = useI18n();
    const bindingReason = (reason: string | null | undefined): string => {
        if (!reason) return '';
        const key = `assets.binding.reasons.${reason}`;
        return t(te(key) ? key : 'assets.binding.reasons.unknown');
    };
    const instanceSource = (source: string): string => t(
        source === 'user' || source === 'web'
            ? `assets.binding.sources.${source}` : 'assets.binding.sources.unknown',
    );
    const candidateReason = (candidate: InstanceCandidate): string => {
        // The catalog identifies occupancy structurally; never compare translated server text.
        if (candidate.role === 'gateway' && candidate.bound_gateway) {
            return bindingReason('instance_already_bound');
        }
        return candidate.unavailable_reason || t('assets.binding.unavailable');
    };
    return { bindingReason, instanceSource, candidateReason };
}
