import test from 'node:test';
import assert from 'node:assert/strict';
import { rankLocatorCandidates, locatorIsUnique } from '../src/modules/automation-locator-candidates.ts';

const node = (id, overrides = {}) => ({
    id, parent_id: null, depth: 0, class_name: 'android.widget.Button', package: 'demo.app',
    resource_id: '', text: '', description: '', clickable: true, enabled: true,
    scrollable: false, selected: false, checked: false, password: false,
    bounds: { left: 0, top: 0, right: 100, bottom: 50 }, ...overrides,
});
test('AUT-10 ranks unique resource ID first and explicitly validates uniqueness', () => {
    const shot = { truncated: false, nodes: [
        node(1, { resource_id: 'demo:id/submit', text: '确定' }),
        node(2, { resource_id: 'demo:id/cancel', text: '确定' }),
    ] };
    const items = rankLocatorCandidates(shot, 1);
    assert.equal(items[0].id, 'resource_id');
    assert.equal(items[0].unique, true);
    assert.equal(items.find(x => x.id === 'text').matches, 2);
    assert.equal(items.find(x => x.id === 'text').unique, false);
    assert.equal(locatorIsUnique(shot, 1, { resource_id: 'demo:id/submit' }), true);
    assert.equal(locatorIsUnique(shot, 1, { text: '确定' }), false);
    assert.equal(locatorIsUnique(shot, 1, { resource_id: 'demo:id/cancel' }), false);
});
test('AUT-10 suggests minimal composite when single fields repeat', () => {
    const shot = { truncated: false, nodes: [
        node(1, { resource_id: 'same', text: '保存' }),
        node(2, { resource_id: 'same', text: '取消' }),
        node(3, { resource_id: 'other', text: '保存' }),
    ] };
    const items = rankLocatorCandidates(shot, 1);
    assert.equal(items.find(x => x.id === 'resource_id+text').unique, true);
    assert.equal(locatorIsUnique(shot, 1, { resource_id: 'same', text: '保存' }), true);
});
test('AUT-10 rejects truncated trees, password fields and positional XPath', () => {
    const secret = node(2, { password: true, text: '123456', resource_id: 'pwd' });
    const shot = { truncated: false, nodes: [node(1, { resource_id: 'submit' }), secret] };
    assert.deepEqual(rankLocatorCandidates(shot, 2), []);
    assert.deepEqual(rankLocatorCandidates({ ...shot, truncated: true }, 1), []);
    assert.equal(locatorIsUnique(shot, 1, { xpath: '(//button)[1]' }), false);
    assert.equal(locatorIsUnique({ ...shot, truncated: true }, 1, { resource_id: 'submit' }), false);
});
test('AUT-10 never fabricates instance/index fallback', () => {
    const shot = { truncated: false, nodes: [node(1), node(2)] };
    assert.equal(rankLocatorCandidates(shot, 1).some(x => x.unique), false);
    assert.ok(rankLocatorCandidates(shot, 1).every(x => !('instance' in x.selector && !('index' in x.selector))));
});
