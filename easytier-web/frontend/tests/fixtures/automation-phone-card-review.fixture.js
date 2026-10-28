/**
 * Non-device Chromium integration: mount the actual production PhoneCard.vue
 * with the real floating panel/step editor and a fake static account/phone.
 * No WebRTC SDP, ADB or API request is made.
 */
import { createApp, h, computed, nextTick, reactive, ref } from 'vue';
import PrimeVue from 'primevue/config';
import { createI18n } from 'vue-i18n';
import PhoneCard from '../../src/components/automation/PhoneCard.vue';
import { emptyAutomationDraft } from '../../src/modules/automation-draft.ts';
import { createAutomationDraftSync } from '../../src/modules/automation-draft-sync.ts';

const owner=42,phone='phone-a';
const device={
  id:phone,name:'模拟原手机',owner_user_id:owner,model:'Virtual',
  machine_id:'virtual-node',node_name:'Test',gateway_group_id:null,gateway_name:null,
  device_type_id:'virtual-phone',type_name:'phone',overlay_ip:'192.0.2.88',
  lan_ip:null,binding_state:'bound',target_fingerprint:'mock-fingerprint',
  availability_reason:null,run_occupied:false,active_session:null,
  connection:{enabled:true,adb_port:5555,connection_mode:'wireless'},
};
const App={
  setup(){
    const card=ref(null),editing=ref(null);
    const attempts=ref([]);
    const draft=reactive(emptyAutomationDraft(owner));
    draft.name='浏览器集成测试';
    draft.steps=[{id:'step-one',type:'key',key:'HOME'}];
    const batch=reactive({
      owner,phase:'idle',error:'',requestId:'',selected:[],
      devices:{},run:null,pendingReceipt:null,
    });
    async function verifySaveUnlock(){
      // A complete and deterministic fake remote draft POST: the computed
      // publication/run gate must turn TRUE after the async save finishes.
      const work=reactive(emptyAutomationDraft(47));
      let settle;
      const gate=new Promise(resolve=>{settle=resolve;});
      const fakeApi={
        listDrafts:async()=>({items:[]}),
        saveDraft:async()=>{await gate;return {id:'browser-fake-draft',revision:7};},
      };
      const sync=createAutomationDraftSync(fakeApi,work);
      await new Promise(resolve=>setTimeout(resolve,0));
      const eligible=computed(()=>sync.canLeave());
      const initialized=sync.state.initialized;
      work.name='已确认步骤';
      work.steps=[{id:'save-step',type:'key',key:'HOME'}];
      await nextTick();
      const saving=sync.flush();
      const before=eligible.value;
      settle();
      await saving;
      await nextTick();
      const after=eligible.value;
      const phase=sync.state.phase;
      sync.dispose();
      return {initialized,before,after,phase};
    }
    window.phoneCardReview={
      verifySaveUnlock,
      open:()=>card.value?.openAutomationPanel(),
      attempts:()=>[...attempts.value],
      close:()=>card.value?.closeAutomationPanel(),
      setBatchPhase:phase=>{
        batch.phase=phase;
        batch.selected=phase==='idle'?[]:[phone];
        batch.devices=phase==='idle'?{}:{[phone]:{
          name:device.name,phase:phase==='failed'?'manual_restore_failed':phase,
          message:'浏览器模拟状态',sessionId:'mock-existing-session',
        }};
      },
      setForeignRun:()=>{
        // The shared batch state intentionally belongs to another phone.
        // The current PhoneCard must not render/control its actions or errors.
        batch.owner=owner;
        batch.phase='running';
        batch.error='模拟其他设备错误';
        batch.requestId='foreign-request-uuid';
        batch.selected=['phone-b'];
        batch.devices={'phone-b':{
          name:'远端模拟手机',phase:'running',message:'远端任务运行中',
          sessionId:'foreign-media-session',runState:'running',
        }};
        batch.run={
          id:'foreign-run-id',state:'running',request_id:'foreign-request-uuid',
          devices:[{device_id:'phone-b',name:'远端模拟手机',state:'running',steps:[],current_step:null}],
          script_snapshot:{steps:[...draft.steps]},
        };
      },
      setRunOutcome:outcome=>{
        batch.phase='completed';batch.selected=[phone];
        batch.run={id:'simulated-run',state:outcome,request_id:'simulated-request',
          devices:[{device_id:phone,name:device.name,state:outcome,steps:[],current_step:null}],
          script_snapshot:{steps:[...draft.steps]}};
        batch.devices={[phone]:{name:device.name,phase:outcome,
          message:'服务端执行结果模拟',runState:outcome}};
      },
      edit:()=>{editing.value='step-one';card.value?.openAutomationPanel();},
      closeEdit:()=>{editing.value=null;},
      state:()=>({phase:batch.phase,sessionId:card.value?.getSessionId()}),
    };
    const control=(id,label,fn)=>h('button',{id,type:'button',onClick:fn,
      style:'padding:6px;margin:4px'},label);
    return ()=>h('div',{style:'padding:16px'},[
      h('div',{id:'fixture-controls'},[
        control('open-panel','打开真实脚本浮窗',()=>card.value?.openAutomationPanel()),
        control('open-step','打开真实步骤子窗',()=>{editing.value='step-one';card.value?.openAutomationPanel();}),
        control('fullscreen-card','真实卡片画面全屏',()=>document.querySelector('.phone-screen')?.requestFullscreen()),
      ]),
      h(PhoneCard,{
        ref:card,device,ownerId:owner,selected:false,runnerAvailable:true,
        syncActive:false,recording:false,draft,scriptGuard:'clean',
        scripts:[],batchRunState:batch,
        draftSyncLabel:'工作草稿已同步 · 修订 7',
        batchDraftReady:true,editingStepId:editing.value,
        onStepEdit:(_device,id)=>{editing.value=id;},
        onDraftSave:id=>{attempts.value.push({type:'save',device:id});},
        onBatchStartLocal:(id,params)=>{attempts.value.push({type:'local',device:id,params});},
        onStepClose:()=>{editing.value=null;},
        onStepDirty:()=>{},
      }),
    ]);
  },
};
const app=createApp(App);
app.use(PrimeVue);
app.use(createI18n({legacy:false,locale:'zh',fallbackLocale:'zh',messages:{zh:{}}}));
app.mount('#app');
