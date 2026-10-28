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
async function setup(t,url=base){
  const context=await browser.newContext({viewport:{width:1150,height:800}});
  t.after(()=>context.close());
  const page=await context.newPage(),errors=[];
  page.on('pageerror',error=>errors.push(error.message));
  t.after(()=>assert.deepEqual(errors,[],'no uncaught browser exceptions'));
  await page.goto(url);
  if(url===base)await page.waitForFunction(()=>typeof window.t48Probe==='function');
  else await page.waitForFunction(()=>typeof window.phoneCardReview==='object');
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

const realCardFixture='http://127.0.0.1:5199/tests/fixtures/automation-phone-card-review.html';

test('T52 actual PhoneCard retains its primary video DOM across main/child floating UI and fullscreen',async t=>{
  const page=await setup(t,realCardFixture);
  const original=page.locator('.phone-card video.primary-video');
  assert.equal(await original.count(),1,'production PhoneCard has one original foreground player');
  const mockedStreamId=await original.evaluate(node=>{
    node.dataset.t52Original='still-owned';
    // A local empty MediaStream is enough to catch accidental srcObject
    // replacement; never negotiates a real WebRTC/ADB session.
    node.srcObject=new MediaStream();
    return node.srcObject.id;
  });
  await page.locator('#open-panel').click();
  await page.locator('#open-step').click();
  const editor=page.getByRole('dialog',{name:/编辑第 1 步/});
  await editor.waitFor();
  assert.equal(await editor.evaluate(node=>!!node.closest('.phone-screen')),true,
    'actual step editor must be teleported inside original PhoneCard fullscreen host');
  assert.equal(await original.getAttribute('data-t52-original'),'still-owned',
    'opening main and child floating UI must not replace the original video');
  assert.equal(await original.evaluate(node=>node.srcObject?.id),mockedStreamId,
    'opening floating UI must not replace the original MediaStream object');
  assert.equal(await page.locator('.phone-card video.primary-video').count(),1);
  await page.locator('#fullscreen-card').click();
  await page.waitForFunction(()=>document.fullscreenElement?.classList.contains('phone-screen'));
  assert.equal(await editor.isVisible(),true);
  assert.equal(await original.getAttribute('data-t52-original'),'still-owned',
    'entering fullscreen must not remount or swap the original player');
  assert.equal(await original.evaluate(node=>node.srcObject?.id),mockedStreamId,
    'fullscreen must keep the same synthetic MediaStream identity');
  await page.keyboard.press('Escape');
});

test('T52 original PhoneCard protects the video stop control while batch outcome is unknown',async t=>{
  const page=await setup(t,realCardFixture);
  const original=page.locator('.phone-card video.primary-video');
  await original.evaluate(node=>node.dataset.t52Original='untouched');
  assert.equal(await page.locator('.phone-card .adb-port').isEnabled(),true,
    'the simulated runner must allow initial screen toggle before batch protection');
  await page.evaluate(()=>window.phoneCardReview.setBatchPhase('unknown'));
  await page.waitForFunction(()=>document.querySelector('.phone-card .adb-port')?.disabled===true);
  assert.equal(await original.getAttribute('data-t52-original'),'untouched');
  assert.equal(await page.locator('.phone-card .batch-card-status').count(),1);
  await page.evaluate(()=>window.phoneCardReview.setBatchPhase('failed'));
  await page.waitForFunction(()=>document.querySelector('.phone-card .adb-port')?.disabled===false);
  assert.equal(await original.getAttribute('data-t52-original'),'untouched');
});

test('T52 actual PhoneCard shows failed and partial-failed batch outcomes instead of green success',async t=>{
  const page=await setup(t,realCardFixture);
  await page.locator('#open-panel').click();
  // A row-level device status can use the same text as the aggregate Tag.
  // Scope each assertion to the *aggregate* badge, not a global text search.
  const badge=page.locator('.phone-card .batch-head .p-tag');
  async function verifyOutcome(outcome,label,severity){
    await page.evaluate(value=>window.phoneCardReview.setRunOutcome(value),outcome);
    await page.waitForFunction(expected=>
      document.querySelector('.phone-card .batch-head .p-tag')?.textContent?.trim()===expected,
    label);
    const classes=await badge.getAttribute('class');
    assert.equal(classes?.includes('p-tag-'+severity),true,
      outcome+' must show aggregate '+severity+' severity');
    if(outcome!=='completed')assert.equal(classes?.includes('p-tag-success'),false,
      outcome+' must not appear green');
  }
  await verifyOutcome('failed','执行失败或准备失败','danger');
  await verifyOutcome('partial_failed','部分手机失败','warn');
  await verifyOutcome('completed','执行成功','success');
});

test('T53 real PhoneCard offers explicit draft save, formal publish and this-phone-only full run',async t=>{
  const page=await setup(t,realCardFixture);
  await page.locator('#open-panel').click();
  const card=page.locator('.phone-card');
  await card.getByRole('button',{name:'保存草稿'}).click();
  const attempts=()=>page.evaluate(()=>window.phoneCardReview.attempts());
  assert.deepEqual(await attempts(),[{type:'save',device:'phone-a'}],
    'manual save must stay bound to the original card identity');
  const formal=card.getByRole('button',{name:'发布正式版本'});
  assert.equal(await formal.isVisible(),true,'formal publication must be an explicit control');
  await formal.click();
  await page.getByRole('dialog',{name:'发布新的正式脚本'}).waitFor();
  await page.getByRole('button',{name:'取消'}).last().click();
  const local=card.getByRole('button',{name:'▶ 本机运行全流程'});
  assert.equal(await local.isEnabled(),true,
    'local full-script run should not require any workbench checkbox');
  await local.click();
  assert.deepEqual((await attempts()).at(-1),{type:'local',device:'phone-a',params:{}},
    'local run must submit exactly its own card, not the selected-device group');
  assert.equal(await card.getByRole('button',{name:'运行已选 0 台'}).isDisabled(),true,
    'multi-phone batch remains explicitly scoped to workbench checkboxes');
});

test('T53 saved server draft unblocks the reactive formal-publish / full-run gate',async t=>{
  const page=await setup(t,realCardFixture);
  const result=await page.evaluate(()=>window.phoneCardReview.verifySaveUnlock());
  assert.deepEqual(result,{initialized:true,before:false,after:true,phase:'synced'},
    'the exact UI computed guard must update after the async request clears, not stay stale');
});

test('T54 real PhoneCard shows manual permission separately from stopping a touch action',async t=>{
  const page=await setup(t,realCardFixture);
  const card=page.locator('.phone-card');
  await page.locator('#open-panel').click();
  // No live media/API in the browser fixture, so the permission control
  // must remain disabled instead of pretending to have manual authority.
  const permission=card.getByRole('button',{name:/手控不可用/});
  assert.equal(await permission.count(),1,
    'a persistent manual-mode button must reflect actual viewer permission');
  assert.equal(await permission.isDisabled(),true);
  assert.equal(await card.getByRole('button',{name:'停止当前手动操作'}).count(),1,
    'a touch-stop command is not the same operation as viewerOnly/resumeManual');
  assert.equal(await permission.getAttribute('aria-pressed'),'false');
  // Closing the console cannot close the original PhoneCard video or create
  // a manual session when none exists.
  const original=card.locator('video.primary-video');
  await original.evaluate(node=>node.dataset.t54Original='preserved');
  await card.getByRole('button',{name:'关闭自动化脚本窗口'}).click();
  assert.equal(await original.getAttribute('data-t54-original'),'preserved');
  assert.equal(await card.locator('video.primary-video').count(),1);
});
