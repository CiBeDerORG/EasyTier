import { useI18n } from 'vue-i18n';
import type { AssetRole, InstanceCandidate } from '../types/node-assets';

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
    const candidateReason = (candidate: InstanceCandidate, requestedRole?: AssetRole): string => {
        // Keep configuration errors ahead of role and occupancy explanations.
        if (candidate.role === 'unknown' || !candidate.ipv4) {
            return candidate.unavailable_reason || bindingReason('instance_configuration_unavailable');
        }
        if (requestedRole && candidate.role !== requestedRole) {
            return t(candidate.role === 'gateway'
                ? 'assets.binding.gatewayInstanceOnly' : 'assets.binding.ordinaryInstanceOnly');
        }
        // The catalog identifies occupancy structurally; never compare translated server text.
        if (candidate.role === 'gateway' && candidate.bound_gateway) {
            return bindingReason('instance_already_bound');
        }
        return candidate.unavailable_reason || t('assets.binding.unavailable');
    };
    return { bindingReason, instanceSource, candidateReason };
}
