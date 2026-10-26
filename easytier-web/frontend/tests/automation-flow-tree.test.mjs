import assert from 'node:assert/strict';
import test from 'node:test';
import { analyzeAutomationFlow } from '../src/modules/automation-flow.ts';
import { parseFlowTree, compileFlowTree, removeFlowBlock, addFlowBranchStep, createFlowCondition } from '../src/modules/automation-flow-tree.ts';

const step = id => ({ id, type: 'key', key: 'BACK' });
const conditional = (id, then_count, else_count) =>
  ({ id, type: 'if_else', then_count, else_count, selector: { text: 'Dialog' } });

test('decode and compile preserve nested branching and identities', () => {
  // Root THEN contains a nested condition and one normal action; root ELSE one action.
  const flat = [
    conditional('outer', 4, 1),
    conditional('inner', 1, 1), step('a'), step('b'), step('c'), step('d'),
  ];
  const tree = parseFlowTree(flat);
  assert.ok(tree);
  assert.equal(tree[0].kind, 'condition');
  assert.equal(tree[0].thenNodes[0].kind, 'condition');
  assert.deepEqual(compileFlowTree(tree), flat.map(s => ({ ...s, selector: s.selector ? { ...s.selector } : undefined, scroll_container: undefined })));
  assert.equal(analyzeAutomationFlow(compileFlowTree(tree)).problem, undefined);
});

test('deleting an inner branch block adjusts outer THEN count atomically', () => {
  const flat = [conditional('outer', 4, 1), conditional('inner', 1, 1), step('a'), step('b'), step('c'), step('d')];
  const result = removeFlowBlock(flat, 'inner');
  assert.ok(result);
  assert.deepEqual(result.map(s => s.id), ['outer', 'c', 'd']);
  assert.equal(result[0].then_count, 1);
  assert.equal(result[0].else_count, 1);
  assert.equal(flat[0].then_count, 4);
});

test('removing only item in a branch refuses rather than reassigning another lane', () => {
  const flat = [conditional('outer', 1, 1), step('a'), step('b')];
  assert.equal(removeFlowBlock(flat, 'a'), null);
  assert.equal(removeFlowBlock(flat, 'b'), null);
  assert.deepEqual(removeFlowBlock(flat, 'outer'), []);
  assert.equal(removeFlowBlock(flat, 'missing'), null);
});

test('nested Else insertion recompiles all ancestors', () => {
  const flat = [conditional('outer', 3, 1), conditional('inner', 1, 1), step('a'), step('b'), step('c')];
  const result = addFlowBranchStep(flat, 'inner', 'else', step('new'));
  assert.ok(result);
  assert.equal(result[0].then_count, 4);
  assert.equal(result[1].then_count, 1);
  assert.equal(result[1].else_count, 2);
  assert.deepEqual(result.map(s => s.id), ['outer','inner','a','b','new','c']);
  assert.equal(addFlowBranchStep(flat, 'inner', 'then', step('a')), null, 'duplicate id must fail');
});

test('new condition contains blank, non-executable key placeholders', () => {
  const flat = [step('root')];
  const replacement = { id:'root',type:'if_else',selector:{ text:'ready' },then_count:1,else_count:1 };
  const result = createFlowCondition(flat, 'root', replacement,
    (() => { let n=0;return () => ({id:'blank'+(++n),type:'key'});})());
  assert.ok(result);
  assert.deepEqual(result.map(s => s.key), [undefined,undefined,undefined]);
  assert.equal(result[0].then_count, 1);
  assert.equal(result[0].else_count, 1);
  assert.deepEqual(flat, [step('root')]);
});

test('invalid nested structure, fourth depth and overfull branches fail closed', () => {
  assert.equal(parseFlowTree([conditional('bad', 2, 1), step('a')]), null);
  const deep = [
    { id:'a',type:'if_gone',selector:{text:'x'},skip_count:4 },
    { id:'b',type:'if_gone',selector:{text:'x'},skip_count:3 },
    { id:'c',type:'if_gone',selector:{text:'x'},skip_count:2 },
    { id:'d',type:'if_gone',selector:{text:'x'},skip_count:1 },
    step('end'),
  ];
  assert.equal(parseFlowTree(deep), null);
  assert.equal(compileFlowTree([{kind:'action',step:step('same')},{kind:'action',step:step('same')}]), null);
  const flat = [{id:'condition',type:'if_gone',selector:{text:'x'},skip_count:20}, ...Array.from({length:20},(_,i)=>step('s'+i))];
  assert.equal(addFlowBranchStep(flat, 'condition', 'then', step('extra')), null);
});
