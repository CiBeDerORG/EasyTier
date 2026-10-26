import type { AutomationHierarchy, AutomationHierarchyNode, AutomationSelector } from '../types/automation';

export type LocatorMode = 'resource_id' | 'description' | 'text' | 'composite' | 'xpath';
export interface LocatorCandidate {
    key: string;
    mode: LocatorMode;
    label: string;
    selector: AutomationSelector;
    matchCount: number;
    unique: boolean;
    score: number;
    explanation: string;
}

type NativeKey = 'resource_id' | 'description' | 'text' | 'class_name' | 'package';
type Snapshot = AutomationHierarchy['snapshot'];
const FIELDS: readonly NativeKey[] = ['resource_id', 'description', 'text', 'class_name', 'package'];
const XPATH_ATTR: Record<NativeKey, string> = {
    resource_id: 'resource-id', description: 'content-desc',
    text: 'text', class_name: 'class', package: 'package',
};
const LABELS: Record<NativeKey, string> = {
    resource_id: 'Resource ID', description: '无障碍描述', text: '控件文字',
    class_name: '类名', package: '应用包名',
};

function safeField(node: AutomationHierarchyNode, field: NativeKey): boolean {
    const value = node[field];
    // Password widgets must not expose secret text/description in suggested locators.
    if (node.password && (field === 'text' || field === 'description')) return false;
    return typeof value === 'string' && !!value.trim() && new TextEncoder().encode(value).length <= 256;
}

function xpathLiteral(value: string): string {
    if (!value.includes("'")) return "'" + value + "'";
    if (!value.includes('"')) return '"' + value + '"';
    return 'concat(' + value.split("'").map(part => "'" + part + "'").join(', "\'" , ') + ')';
}

function selectorFor(node: AutomationHierarchyNode, keys: NativeKey[]): AutomationSelector {
    return Object.fromEntries(keys.map(key => [key, node[key]])) as AutomationSelector;
}
function matchCount(shot: Snapshot, node: AutomationHierarchyNode, keys: NativeKey[]): number {
    return shot.nodes.filter(candidate => keys.every(key => candidate[key] === node[key])).length;
}
function combinationScore(keys: NativeKey[]): number {
    const base = keys.includes('resource_id') ? 90 : keys.includes('description') ? 84 : 72;
    return base - (keys.length - 2) * 4;
}

/**
 * Rank only locators proven unique in this complete snapshot. The result also
 * contains unavailable single-field options to explain ambiguity to the user.
 * No positional XPath, instance(index), or automatic coordinate fallback.
 * A snapshot is not a runtime guarantee; the executor re-checks uniqueness.
 */
export function recommendAutomationLocators(shot: Snapshot | null, nodeId: number | null): LocatorCandidate[] {
    if (!shot || nodeId === null || shot.width <= 0 || shot.height <= 0) return [];
    const targets = shot.nodes.filter(node => node.id === nodeId);
    if (targets.length !== 1) return [];
    const node = targets[0];
    const available = FIELDS.filter(key => safeField(node, key));
    const result: LocatorCandidate[] = [];

    for (const key of ['resource_id', 'description', 'text'] as const) {
        if (!available.includes(key)) continue;
        const count = matchCount(shot, node, [key]);
        const unique = !shot.truncated && count === 1;
        result.push({
            key, mode: key, label: LABELS[key], selector: selectorFor(node, [key]),
            matchCount: count, unique, score: unique ? ({ resource_id: 100, description: 94, text: 78 })[key] : 0,
            explanation: shot.truncated ? '控件树已截断，无法证明唯一性'
                : unique ? '本次控件树中唯一匹配' : `当前有 ${count} 个匹配，不能直接使用`,
        });
    }

    // Use the smallest uniquely identifying attribute groups, keeping only the
    // highest-ranked options to prevent combinatorial UI noise.
    const composite: LocatorCandidate[] = [];
    const visit = (start: number, keys: NativeKey[]) => {
        if (keys.length >= 2) {
            const count = matchCount(shot, node, keys);
            if (!shot.truncated && count === 1) {
                composite.push({
                    key: 'composite:' + keys.join('+'), mode: 'composite',
                    label: '组合：' + keys.map(key => LABELS[key]).join(' + '),
                    selector: selectorFor(node, keys), matchCount: 1, unique: true,
                    score: combinationScore(keys),
                    explanation: '组合条件在本次控件树中唯一匹配',
                });
            }
        }
        if (keys.length === 3) return;
        for (let i = start; i < available.length; i++) visit(i + 1, [...keys, available[i]]);
    };
    visit(0, []);
    composite.sort((a, b) => b.score - a.score || a.key.localeCompare(b.key));
    // A larger group that adds properties to an already-unique subset adds no
    // value and makes matching more fragile.
    const minimal = composite.filter(candidate => !composite.some(other => other !== candidate
        && other.matchCount === 1 && Object.keys(other.selector).length < Object.keys(candidate.selector).length
        && Object.keys(other.selector).every(key => key in candidate.selector))).slice(0, 5);
    result.push(...minimal);

    const bestNative = [...result].filter(item => item.unique).sort((a, b) => b.score - a.score)[0];
    if (bestNative) {
        const xpath = '//*[' + Object.entries(bestNative.selector)
            .map(([key, value]) => '@' + XPATH_ATTR[key as NativeKey] + '=' + xpathLiteral(value!)).join(' and ') + ']';
        if (new TextEncoder().encode(xpath).length <= 512) result.push({
            key: 'xpath', mode: 'xpath', label: 'XPath 1.0（按属性，无序号）',
            selector: { xpath }, matchCount: 1, unique: true, score: 55,
            explanation: '由已唯一匹配的属性生成，未使用节点序号',
        });
    }
    return result.sort((a, b) => Number(b.unique) - Number(a.unique)
        || b.score - a.score || a.key.localeCompare(b.key));
}
