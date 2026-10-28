/**
 * Non-device Chromium integration: mount the actual production PhoneCard.vue
 * with the real floating panel/step editor and a fake static account/phone.
 * No WebRTC SDP, ADB or API request is made.
 */
import { createApp, h, reactive, ref } from 'vue';
import PrimeVue from 'primevue/config';
import { createI18n } from 'vue-i18n';
import PhoneCard from '../../src/components/automation/PhoneCard.vue';
import { emptyAutomationDraft } from '../../src/modules/automation-draft.ts';

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
    const draft=reactive(emptyAutomationDraft(owner));
    draft.name='浏览器集成测试';
    draft.steps=[{id:'step-one',type:'key',key:'HOME'}];
    const batch=reactive({
      owner,phase:'idle',error:'',requestId:'',selected:[],
      devices:{},run:null,pendingReceipt:null,
    });
    window.phoneCardReview={
      open:()=>card.value?.openAutomationPanel(),
      close:()=>card.value?.closeAutomationPanel(),
      setBatchPhase:phase=>{
        batch.phase=phase;
        batch.selected=phase==='idle'?[]:[phone];
        batch.devices=phase==='idle'?{}:{[phone]:{
          name:device.name,phase:phase==='failed'?'manual_restore_failed':phase,
          message:'浏览器模拟状态',sessionId:'mock-existing-session',
        }};
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
        ref:card,device,ownerId:owner,selected:true,runnerAvailable:true,
        syncActive:false,recording:false,draft,scriptGuard:'clean',
        scripts:[],selectedDeviceCount:1,batchRunState:batch,
        batchDraftReady:true,editingStepId:editing.value,
        onStepEdit:(_device,id)=>{editing.value=id;},
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
