import type { AutomationHierarchyNode, AutomationSelector } from '../types/automation';

export type LocatorCandidate = {
    id: string;
    label: string;
    selector: AutomationSelector;
    matches: number;
    unique: boolean;
    score: number;
    warning?: string;
};

type Snapshot = { truncated: boolean; nodes: AutomationHierarchyNode[] };
const fields = ['resource_id', 'description', 'text', 'class_name', 'package'] as const;
type Field = typeof fields[number];
const labels: Record<Field, string> = {
    resource_id: 'Resource ID', description: '无障碍描述', text: '文字',
    class_name: '类名', package: '应用包名',
};
const weights: Record<Field, number> = {
    resource_id: 100, description: 80, text: 65, class_name: 25, package: 15,
};
const bytes = (value: string) => new TextEncoder().encode(value).length;
const valid = (value: string) => !!value.trim() && bytes(value) <= 256;

/**
 * AUT-10: purely local suggestions based on the CURRENT hierarchy. The list is
 * a hint, never a persisted locator or proof of stability across screens.
 * No index/instance, positional XPath, or sensitive password text is used.
 */
export function rankLocatorCandidates(snapshot: Snapshot, selectedId: number): LocatorCandidate[] {
    const selected = snapshot.nodes.find(node => node.id === selectedId);
    if (!selected || snapshot.truncated || selected.password) return [];
    const available = fields.filter(key => valid(selected[key]));
    const candidates: LocatorCandidate[] = [];
    const add = (keys: Field[]) => {
        const selector = Object.fromEntries(keys.map(key => [key, selected[key]])) as AutomationSelector;
        const matches = snapshot.nodes.filter(node => keys.every(key => node[key] === selected[key])).length;
        const unique = matches === 1;
        // Prefer stable identifiers over content, then minimal combinations.
        const score = (unique ? 1000 : 0) + keys.reduce((sum, key) => sum + weights[key], 0)
            - (keys.length - 1) * 65;
        candidates.push({
            id: keys.join('+'), label: keys.map(key => labels[key]).join(' + '),
            selector, matches, unique, score,
            warning: unique ? undefined : `当前快照匹配 ${matches} 个控件，禁止直接使用`,
        });
    };
    for (const key of available) add([key]);
    // A bounded combinatorial search; avoid broad 4/5-field selectors.
    for (let i = 0; i < available.length; i++) {
        for (let j = i + 1; j < available.length; j++) add([available[i], available[j]]);
    }
    for (let i = 0; i < available.length; i++) {
        for (let j = i + 1; j < available.length; j++) {
            for (let k = j + 1; k < available.length; k++) add([available[i], available[j], available[k]]);
        }
    }
    return candidates.sort((a, b) => b.score - a.score
        || a.id.split('+').length - b.id.split('+').length || a.id.localeCompare(b.id));
}

export function locatorIsUnique(snapshot: Snapshot, selectedId: number, selector: AutomationSelector): boolean {
    if (snapshot.truncated || selector.xpath || !snapshot.nodes.some(node => node.id === selectedId)) return false;
    const entries = Object.entries(selector);
    if (!entries.length || entries.some(([key, value]) => !fields.includes(key as Field)
        || typeof value !== 'string' || !valid(value))) return false;
    const selected = snapshot.nodes.find(node => node.id === selectedId)!;
    return entries.every(([key, value]) => selected[key as Field] === value)
        && snapshot.nodes.filter(node => entries.every(([key, value]) => node[key as Field] === value)).length === 1;
}
