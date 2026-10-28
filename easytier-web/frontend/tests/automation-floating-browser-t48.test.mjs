/** T48: production Vue floating panel/dialog under Chromium, no phone.
 * This browser fixture does not use WebRTC or any Android/ADB transport.
 */
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { after, before, test } from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';

const base='http://127.0.0.1:5199/tests/fixtures/automation-floating-t48.html';
let server,browser;
before(async()=>{
  server=spawn(process.execPath,
    ['node_modules/vite/bin/vite.js','--host','127.0.0.1',
     '--port','5199','--strictPort'],{stdio:'pipe'});
  let ready=false;
  for(let i=0;i<150;i++){
    if(server.exitCode!==null)throw new Error('Vite exited before fixture loaded');
    if(await fetch(base).then(r=>r.ok).catch(()=>false)){ready=true;break;}
    await delay(120);
  }
  if(!ready)throw new Error('Automation Vue browser fixture not available');
  browser=await chromium.launch({
    headless:true,executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH,
    args:['--no-sandbox'],
  });
});
after(async()=>{await browser?.close();server?.kill();});
async function setup(t){
  const context=await browser.newContext({viewport:{width:1150,height:800}});
  t.after(()=>context.close());
  const page=await context.newPage(),errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  t.after(()=>assert.deepEqual(errors,[],'no uncaught browser exceptions'));
  await page.goto(base);
  await page.waitForFunction(()=>typeof window.t48Probe==='function');
  return page;
}
const state=page=>page.evaluate(()=>window.t48Probe());

test('T48 no duplicate video; main and child use original card fullscreen host',async t=>{
  const page=await setup(t);
  assert.equal(await page.locator('#primary-video-sentinel').count(),1);
  assert.equal(await page.locator('video').count(),0,
    'the fixture mounts no secondary player while opening floating editors');
  assert.equal(await page.locator('#main-content').count(),1);
  await page.locator('#editor-open').click();
  const dialog=page.getByRole('dialog',{name:'步骤编辑'});
  await dialog.waitFor();
  assert.equal(await dialog.evaluate(node=>node.parentElement.id),'screen-host',
    'Teleport must stay inside the original fullscreen target, not body');
  await page.locator('#fullscreen-toggle').click();
  await page.waitForFunction(()=>document.fullscreenElement?.id==='screen-host');
  assert.equal((await state(page)).fullscreen,true);
  assert.equal(await dialog.isVisible(),true);
  assert.equal(await page.locator('#main-content').isVisible(),true);
  await page.locator('#fullscreen-toggle').click();
  await page.waitForFunction(()=>document.fullscreenElement===null);
  assert.equal(await dialog.isVisible(),true);
});

test('T48 Escape on dirty editor asks before discard and preserves typed input',async t=>{
  const page=await setup(t);
  await page.locator('#editor-open').click();
  const input=page.locator('#parameter-input');
  await input.fill('pending user input');
  await page.locator('#dirty-toggle').click();
  await input.focus();
  await page.keyboard.press('Escape');
  await page.getByRole('alert').getByText('当前修改尚未保存').waitFor();
  assert.equal((await state(page)).editor,true,'dirty Escape must not close immediately');
  await page.getByRole('button',{name:'继续编辑'}).click();
  assert.equal(await input.inputValue(),'pending user input',
    'cancel does not clear unconfirmed changes');
  await input.focus();await page.keyboard.press('Escape');
  await page.getByRole('button',{name:'放弃修改并关闭'}).click();
  assert.equal((await state(page)).editor,false);
  assert.equal((await state(page)).main,true,'closing the child never closes main');
  assert.equal((await state(page)).parameter,'');
  assert.deepEqual((await state(page)).closes,['editor:discard']);
});

test('T48 only topmost child handles Escape; keyboard positioning remains usable',async t=>{
  const page=await setup(t);
  await page.locator('#editor-open').click();
  const editor=page.getByRole('dialog',{name:'步骤编辑'});
  const header=editor.locator('.automation-child-titlebar');
  await header.focus();
  const before=await editor.getAttribute('style');
  await page.keyboard.press('ArrowRight');
  const afterMove=await editor.getAttribute('style');
  assert.notEqual(afterMove,before,'focused titlebar arrow moves the real component');
  await page.locator('#detail-open').click();
  const second=page.getByRole('dialog',{name:'条件子窗'});
  await second.waitFor();
  const result=await page.evaluate(()=>{
    const windows=[...document.querySelectorAll('[role="dialog"]')];
    return windows.map(x=>({name:x.getAttribute('aria-label'),
      z:Number.parseFloat(x.style.zIndex)}));
  });
  assert.ok(result.find(x=>x.name==='条件子窗').z>
    result.find(x=>x.name==='步骤编辑').z,
    'last-focused child must stack above earlier child');
  await second.focus();await page.keyboard.press('Escape');
  assert.equal((await state(page)).detail,false);
  assert.equal((await state(page)).editor,true);
  await editor.focus();await page.keyboard.press('Escape');
  assert.equal((await state(page)).editor,false);
  assert.deepEqual((await state(page)).closes,['detail:normal','editor:normal']);
  assert.equal((await state(page)).main,true);
});
