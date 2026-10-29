/** Confirm-time snapshot review. No forced re-pick or pixel-position identity check. */
import type { AutomationHierarchyNode, AutomationSelector } from '../types/automation';
import { buildFilterOptions, filterHierarchy, type FilterField, type FilterOption } from './automation-cross-filter.ts';
import { nativeSnapshotMatches } from './automation-trial-diagnostics.ts';

type Snapshot = { nodes: AutomationHierarchyNode[]; truncated: boolean };

/** Null means the legacy XPath cannot be faithfully enumerated from the frontend snapshot. */
export function confirmationMatches(
    shot: Snapshot, selector: AutomationSelector, options: FilterOption[], activeIds: string[],
    pickedId: number | null, xpathCandidateMatches?: number,
): AutomationHierarchyNode[] | null {
    if (shot.truncated || shot.nodes.length > 2500) return null;
    if (activeIds.length) return filterHierarchy(shot, options, activeIds);
    if (!selector.xpath) return nativeSnapshotMatches(shot.nodes, selector);
    // A generated XPath's known unique candidate is safe to confirm, but
    // never treat every node as an actual XPath match in an ambiguous case.
    if (xpathCandidateMatches === 1 && pickedId !== null) {
        const node = shot.nodes.find(item => item.id === pickedId);
        return node ? [node] : null;
    }
    return null;
}

/** Turn a compared cell into a genuine executable AND condition, never a node index. */
export function comparisonFilterOption(
    shot: Snapshot, nodeId: number, field: FilterField, parent: boolean,
): FilterOption | null {
    if (shot.truncated) return null;
    const node = shot.nodes.find(item => item.id === nodeId);
    if (!node) return null;
    const options = buildFilterOptions(shot, nodeId);
    const parentNode = node.parent_id === null ? null : shot.nodes.find(item => item.id === node.parent_id);
    const distance = parentNode ? node.depth - parentNode.depth : 0;
    const option = options.find(item => item.field === field && (parent
        ? item.scope === 'ancestor' && item.distance === distance && distance > 0
        : item.scope === 'self'));
    if (!option) return null;
    return {
        ...option,
        // The same field may differ on other rows: keep its value in the ID.
        id: 'compare:' + option.scope + ':' + option.distance + ':' + field + ':' + JSON.stringify(option.value),
    };
}

/** New value for an already filtered field replaces the old one instead of forming an impossible AND. */
export function replaceComparisonCondition(
    options: FilterOption[], activeIds: string[], next: FilterOption,
): string[] {
    const selected = activeIds.filter(id => {
        const previous = options.find(option => option.id === id);
        return !previous || previous.scope !== next.scope ||
            previous.distance !== next.distance || previous.field !== next.field;
    });
    return [...selected, next.id];
}
