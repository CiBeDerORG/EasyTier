import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { mkdir } from 'node:fs/promises';
import { chromium } from 'playwright';
const base='http://127.0.0.1:5203';let browser,server;
before(async()=>{server=spawn(process.execPath,['node_modules/vite/bin/vite.js','preview','--host','127.0.0.1','--port','5203','--strictPort'],{stdio:'pipe'});for(let i=0;i<100;i++){if(await fetch(base).then(r=>r.ok).catch(()=>false))break;await delay(100);}browser=await chromium.launch({executablePath:'/usr/bin/chromium',headless:true});});
after(async()=>{await browser?.close();server?.kill();});
const id=n=>`00000000-0000-4000-8000-${String(n).padStart(12,'0')}`;
async function open(t,mobile=false){
 const context=await browser.newContext({viewport:mobile?{width:390,height:844}:{width:1440,height:1000}});t.after(()=>context.close());await context.addInitScript(()=>localStorage.setItem('lang','cn'));const page=await context.newPage();page.setDefaultTimeout(10000);await page.clock.install();const requests=[],errors=[];page.on('pageerror',e=>errors.push(e.message));t.after(()=>assert.deepEqual(errors,[]));
 const binding={state:'current',reason:null,instance_id:id(5),snapshot:{display_name:'Home',ipv4:'10.0.0.1',rules:[]},current:{disabled:false},can_edit_address:true};
 const gateway={id:id(1),owner_user_id:1,machine_id:id(2),gateway_group_id:id(3),device_type_id:id(4),name:'Home OpenWrt',overlay_ip:'10.0.0.1',lan_ip:null,is_gateway_self:true,semantic_kind:'openwrt',address_mode:'gateway_self',binding,warnings:[],created_at:'2026-10-24T00:00:00Z'};
 const types=[{id:id(4),owner_user_id:1,name:'Router',semantic_kind:'openwrt'}];
 const row={row_id:id(11),ipv4:'192.168.1.20',input:{hostname:'Phone A',mac:'02:11:22:33:44:55',ipv6:['fd00::20'],observation:'cached'},matched:[],candidates:[],blocked_reason:null,suggested_action:'create',addresses:{overlay_ip:'10.60.1.20'},applied:null};
 let op={operation_id:id(10),preview_id:id(10),preview_revision:2,revision:2,kind:'devices',status:'recorded',expires_at:'2026-10-24T12:10:00Z',rows:[row],events:[],snapshot:{status:'partial',truncated:true,scanned_at:1792843200,errors:[{source:'arp',code:'unavailable'}]},result:{state:'preview'}};
 const state={saved:undefined,applied:undefined};
 await page.route('**/api/v1/**',async route=>{
  const req=route.request(),path=new URL(req.url()).pathname.replace('/api/v1',''),method=req.method(),body=req.postDataJSON();requests.push({path,method,body});let result={};
  if(path==='/console-info')result={username:'tester',webhook_auth:false,config_server_protocol:'tcp',config_server_port:7008};
  else if(path==='/admin/users/context')result={available:true,can_manage_users:false};
  else if(path==='/assets/context')result={available:true,can_manage_others:false,actor:{id:1,username:'tester'}};
  else if(path==='/assets/nodes')result={items:[{owner_user_id:1,machine_id:id(2),hostname:'Home node',alias:'',device_count:1}]};
  else if(path.includes('/device-types'))result={items:types};
  else if(path==='/assets/gateway-groups')result={items:[{id:id(3),name:'Home group',subnets:['192.168.1.0/24'],binding}]};
  else if(path==='/assets/devices')result={items:[gateway],total:1,page:1,page_size:20};
  else if(path===`/assets/devices/${id(1)}`)result=gateway;
  else if(path.endsWith('/access')){if(method==='PUT')state.saved=body;result={endpoint_url:'https://10.0.0.1/ubus',ca_pem:'',has_secret:!!state.saved,config_revision:state.saved?1:0};}
  else if(path.endsWith('/devices/scan'))result=op;
  else if(path.endsWith('/adb/scan')){op={...op,kind:'adb',rows:[],result:{rows:[{row_id:id(11),ip:'192.168.1.20',state:'choose_endpoint',endpoints:[{endpoint_id:'first',port:37001},{endpoint_id:'second',port:37002}]}]}};result=op;}
  else if(path.endsWith('/devices/apply')){state.applied=body;row.applied={state:'saved',device_id:id(12)};result={operation_id:id(10),rows:[row.applied]};}
  else if(path.endsWith('/resolve')){assert.equal(body.choices[0].selected_endpoint_id,'second');op.result.rows=[{row_id:id(11),ip:'192.168.1.20',port:37002,state:'saved'}];result={state:'recorded'};}
  else if(path.endsWith('/operations'))result={items:[]};
  else if(path.includes('/operations/'))result=op;
  else if(path.endsWith('/events')){await route.abort();return;}
  else if(path==='/networks')result={networks:[]};else if(path==='/blocked-devices')result={blocked:[]};
  await route.fulfill({json:result});
 });
 await page.goto(`${base}/#/h/node-assets?owner=1&node=${id(2)}`);await page.getByRole('button',{name:'立即同步',exact:true}).waitFor();return {page,requests,state};
}
test('explicit scan, reviewed apply and saved endpoint selection with no idle polling',async t=>{
 const {page,requests,state}=await open(t);await delay(1500);const before=requests.length;await page.clock.fastForward(120000);await delay(100);assert.equal(requests.length,before,JSON.stringify(requests.slice(before)));assert.equal(requests.filter(r=>r.path.endsWith('/scan')).length,0);
 await page.getByRole('button',{name:'立即同步',exact:true}).click();const dialog=page.getByRole('dialog',{name:'OpenWrt 发现 · Home OpenWrt'});await dialog.getByText('Phone A',{exact:false}).waitFor();assert.equal(requests.filter(r=>r.path.endsWith('/devices/scan')).length,1);assert.equal(requests.filter(r=>r.path.endsWith('/devices/apply')).length,0);
 await dialog.getByText('cached',{exact:false}).waitFor();await dialog.getByText('结果已截断',{exact:false}).waitFor();const idle=requests.length;await page.clock.fastForward(60000);await delay(100);assert.equal(requests.length,idle);
 await dialog.getByRole('button',{name:'应用已选处理方式'}).click();await dialog.getByText('已保存',{exact:true}).waitFor();assert.equal(state.applied.rows[0].action,'create');assert.equal(state.applied.preview_id,id(10));
 await dialog.getByRole('button',{name:'ADB 发现',exact:true}).click();await dialog.locator('.adb-results select').selectOption('second');await dialog.getByRole('button',{name:'确认保存的连接端点'}).click();await dialog.getByText('192.168.1.20:37002',{exact:true}).waitFor();assert.equal(requests.filter(r=>r.path.endsWith('/adb/scan')).length,1);
 await mkdir('/tmp/ods-screenshots',{recursive:true});await page.screenshot({path:'/tmp/ods-screenshots/discovery-desktop.png',fullPage:true});
});
test('mobile access settings preserve exact secret and never scan on save',async t=>{
 const {page,requests,state}=await open(t,true);await page.locator('button[title="编辑设备"]').click();
 await page.getByRole('button',{name:'OpenWrt 发现接入设置'}).click();const dialog=page.getByRole('dialog',{name:'OpenWrt 发现 · Home OpenWrt'});await dialog.getByText('HTTPS /ubus 地址',{exact:true}).waitFor();
 await dialog.getByLabel('授权密码',{exact:true}).selectOption('replace');const secret='  ! $ ` 中文 password  ';await dialog.getByLabel('新密码（保留原始字符）').fill(secret);await dialog.getByRole('button',{name:'保存接入设置'}).click();await dialog.getByText('接入设置已保存。').waitFor();assert.equal(state.saved.secret,secret);assert.equal(requests.filter(r=>r.path.endsWith('/scan')).length,0);
 assert.ok(await dialog.evaluate(e=>e.getBoundingClientRect().width<=window.innerWidth));await mkdir('/tmp/ods-screenshots',{recursive:true});await page.screenshot({path:'/tmp/ods-screenshots/discovery-mobile.png',fullPage:true});
});
test('endpoint events restore only the still-watched screen and never replay input',async t=>{
 const {page}=await open(t);const calls=[];
 await page.addInitScript(()=>{
  const sources=[];
  window.EventSource=class {listeners={};constructor(){sources.push(this);}addEventListener(name,callback){this.listeners[name]=callback;}close(){this.closed=true;}};
  window.emitEndpoint=value=>sources.filter(s=>!s.closed).forEach(s=>s.listeners.endpoint?.({data:JSON.stringify(value)}));
  window.WebSocket=class {static OPEN=1;readyState=1;constructor(){queueMicrotask(()=>this.onmessage?.({data:JSON.stringify({type:'ready'})}));}close(){this.readyState=3;}send(){}};
  window.RTCPeerConnection=class {iceGatheringState='complete';addTransceiver(){return {setCodecPreferences(){}};}async createOffer(){return {type:'offer',sdp:'mock video'};}async setLocalDescription(v){this.localDescription=v;}async setRemoteDescription(){}close(){}};
  window.RTCRtpReceiver.getCapabilities=()=>({codecs:[{mimeType:'video/H264'}]});
 });
 const phone={id:id(20),owner_user_id:1,machine_id:id(2),name:'Phone A',node_name:'Home node',gateway_group_id:id(3),gateway_name:'Home',device_type_id:id(4),type_name:'手机',overlay_ip:'10.60.1.20',lan_ip:'192.168.1.20',binding_state:'current',connection:{enabled:true,adb_port:37001,connection_mode:'wireless'},availability_reason:null,active_session:null};
 await page.route('**/api/v1/automation/**',async route=>{
  const request=route.request(),path=new URL(request.url()).pathname,body=request.postDataJSON();calls.push({path,method:request.method(),body});let result={};
  if(path.endsWith('/context'))result={available:true,runner_configured:true,can_manage_others:false,actor:{id:1,username:'tester'},protocol_version:1};
  else if(path.endsWith('/devices'))result={items:[phone],issues:[]};
  else if(path.endsWith('/sessions'))result={id:id(100+calls.length),device_id:phone.id,state:'active',ice_servers:[]};
  else if(path.endsWith('/offer'))result={type:'answer',sdp:'mock video',width:1080,height:1920};
  else if(path.endsWith('/close'))result={cleanup_pending:false,state:'closed'};
  else if(path.endsWith('/scripts')||path.endsWith('/runs'))result={items:[]};
  await route.fulfill({json:result});
 });
 await page.goto(`${base}/?automation-test#/h/automation`);const card=page.locator('.phone-card');await card.getByRole('button',{name:'打开画面',exact:true}).waitFor();
 const opens=()=>calls.filter(c=>c.path.endsWith('/sessions')&&c.method==='POST').length;
 await card.getByRole('button',{name:'打开画面',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.phone-card .phase')?.textContent.includes('正在建立画面'));assert.equal(opens(),1);
 const event=revision=>({event_id:id(200+revision),owner_user_id:1,device_id:phone.id,endpoint_revision:revision,cleanup:{state:'pending'},connection:{state:'pending'}});
 await page.evaluate(e=>window.emitEndpoint(e),event(2));await delay(150);
 await page.evaluate(e=>window.emitEndpoint(e),{...event(2),cleanup:{state:'confirmed'},connection:{state:'connected'}});await delay(250);assert.equal(opens(),2);
 // Duplicate and late events must not reopen the same endpoint.
 await page.evaluate(e=>window.emitEndpoint(e),{...event(2),cleanup:{state:'confirmed'},connection:{state:'connected'}});await delay(100);assert.equal(opens(),2);
 await card.locator('.main-orb').click();await card.getByRole('button',{name:'关闭画面',exact:true}).click();
 await page.evaluate(e=>window.emitEndpoint(e),{...event(3),cleanup:{state:'confirmed'},connection:{state:'connected'}});await delay(200);assert.equal(opens(),2);
 await card.getByRole('button',{name:'打开画面',exact:true}).click();await delay(200);assert.equal(opens(),3);
 await page.getByRole('button',{name:'脚本库',exact:true}).click();
 await page.evaluate(e=>window.emitEndpoint(e),{...event(4),cleanup:{state:'confirmed'},connection:{state:'connected'}});await delay(200);assert.equal(opens(),3);
 assert.equal(calls.filter(c=>c.path.endsWith('/commands')||c.path.endsWith('/runs')&&c.method==='POST').length,0);
});
