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

test('AUT-10 fails closed on repeated hierarchy node IDs and password-node selectors', () => {
    const duplicated = { truncated: false, nodes: [
        node(1, { resource_id: 'demo:id/one' }),
        node(1, { resource_id: 'demo:id/two' }),
    ] };
    assert.deepEqual(rankLocatorCandidates(duplicated, 1), []);
    assert.equal(locatorIsUnique(duplicated, 1, { resource_id: 'demo:id/one' }), false);
    const secret = { truncated: false, nodes: [
        node(1, { password: true, resource_id: 'demo:id/pass', text: '1234' }),
    ] };
    assert.deepEqual(rankLocatorCandidates(secret, 1), []);
    assert.equal(locatorIsUnique(secret, 1, { resource_id: 'demo:id/pass' }), false);
});

test('AUT-10 rejects forbidden fields, empty data and changed snapshots', () => {
    const shot = { truncated: false, nodes: [
        node(1, { resource_id: 'demo:id/save', text: '保存' }),
        node(2, { resource_id: 'demo:id/cancel', text: '取消' }),
    ] };
    assert.equal(locatorIsUnique(shot, 1, { index: '0' }), false);
    assert.equal(locatorIsUnique(shot, 1, { text: '' }), false);
    assert.equal(locatorIsUnique(shot, 1, { resource_id: 'demo:id/cancel' }), false);
    assert.equal(locatorIsUnique({ ...shot, nodes: [
        node(1, { resource_id: 'demo:id/save' }),
        node(2, { resource_id: 'demo:id/save' }),
    ] }, 1, { resource_id: 'demo:id/save' }), false);
});


test('AUT-10 distinguishes duplicate labels with clickable=true and clickable=false', () => {
    const shot = { truncated: false, nodes: [
        node(1, { description: '领淘金币', clickable: true }),
        node(2, { description: '领淘金币', clickable: false }),
    ] };
    const clickable = rankLocatorCandidates(shot, 1).find(x => x.id === 'description+clickable');
    const notClickable = rankLocatorCandidates(shot, 2).find(x => x.id === 'description+clickable');
    assert.equal(clickable?.unique, true);
    assert.equal(clickable?.selector.clickable, true);
    assert.match(clickable?.label ?? '', /可点击/);
    assert.equal(notClickable?.unique, true);
    assert.equal(notClickable?.selector.clickable, false);
    assert.match(notClickable?.label ?? '', /不可点击/);
    assert.equal(locatorIsUnique(shot, 1, {description:'领淘金币', clickable:true}), true);
    assert.equal(locatorIsUnique(shot, 2, {description:'领淘金币', clickable:false}), true);
    assert.equal(locatorIsUnique(shot, 1, {description:'领淘金币', clickable:false}), false);
    assert.equal(locatorIsUnique(shot, 2, {description:'领淘金币', clickable:'false'}), false);
});

test('AUT-10 clickable does not bypass uniqueness when duplicate states match', () => {
    const shot = {truncated:false, nodes:[
        node(1, { description:'领淘金币', clickable:true }),
        node(2, { description:'领淘金币', clickable:true }),
        node(3, { description:'领淘金币', clickable:false }),
    ]};
    assert.equal(rankLocatorCandidates(shot, 1).find(x=>x.id==='description+clickable')?.unique,false);
    assert.equal(rankLocatorCandidates(shot, 3).find(x=>x.id==='description+clickable')?.unique,true);
    assert.equal(locatorIsUnique(shot,1,{description:'领淘金币',clickable:true}),false);
});
