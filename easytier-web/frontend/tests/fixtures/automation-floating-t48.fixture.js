/** Browser-only test host that mounts production floating components.
 * No AutomationApi, websocket, media transport, ADB or real phone is used.
 */
import { createApp, h, ref, onMounted, onBeforeUnmount, nextTick } from 'vue';
import AutomationFloatingPanel from '../../src/components/automation/AutomationFloatingPanel.vue';
import AutomationFloatingDialog from '../../src/components/automation/AutomationFloatingDialog.vue';

const Fixture={
  setup(){
    const screen=ref(null), fullscreen=ref(false);
    const main=ref(true), editor=ref(false), detail=ref(false);
    const dirty=ref(false), parameter=ref(''), closeLog=ref([]);
    function syncFullscreen(){ fullscreen.value=document.fullscreenElement===screen.value; }
    async function toggleFullscreen(){
      if(!screen.value)return;
      if(document.fullscreenElement===screen.value)await document.exitFullscreen();
      else await screen.value.requestFullscreen();
      syncFullscreen();
    }
    onMounted(()=>{
      document.addEventListener('fullscreenchange',syncFullscreen);
      window.t48Probe=()=>({
        fullscreen:fullscreen.value, main:main.value,
        editor:editor.value,detail:detail.value,dirty:dirty.value,
        parameter:parameter.value,closes:[...closeLog.value],
      });
    });
    onBeforeUnmount(()=>document.removeEventListener('fullscreenchange',syncFullscreen));
    function editorClose(reason){
      closeLog.value.push('editor:'+reason);
      editor.value=false; if(reason==='discard')parameter.value='';
      dirty.value=false;
    }
    function detailClose(reason){closeLog.value.push('detail:'+reason);detail.value=false;}
    const button=(id,label,action)=>h('button',{id,onClick:action,type:'button',
      style:'margin:3px;padding:5px'},label);
    return ()=>h('div',{style:'padding:12px'},[
      h('p',{id:'description'},'Production automation floating components in one simulated original card'),
      h('div',{id:'screen-host',ref:screen,class:'phone-screen',style:
        'position:relative;width:760px;max-width:95vw;height:520px;max-height:92vh;border:2px solid #aaa;background:#263342;overflow:hidden'},[
        h('div',{id:'test-toolbar',style:'position:absolute;top:2px;right:3px;z-index:150;display:flex;flex-wrap:wrap;max-width:360px'},[
          button('editor-open','编辑步骤',()=>{editor.value=true;}),
          button('detail-open','打开第二子窗',()=>{detail.value=true;}),
          button('dirty-toggle','设置脏编辑',()=>{dirty.value=!dirty.value;}),
          button('fullscreen-toggle','切换全屏',()=>{void toggleFullscreen();}),
          button('main-toggle','主窗开关',()=>{main.value=!main.value;}),
        ]),
        h('div',{id:'primary-video-sentinel',style:'position:absolute;inset:0;pointer-events:none',
          'data-session-id':'one-original-session'},'原视频容器（模拟，不创建video/stream）'),
        h(AutomationFloatingPanel,{visible:main.value,fullscreen:fullscreen.value,
          title:'自动化脚本',deviceName:'Virtual A',onClose:()=>{main.value=false;}},{
            default:()=>h('span',{id:'main-content'},'主浮窗内容与原画面同宿主'),
          }),
        h(AutomationFloatingDialog,{
          visible:editor.value,portalTarget:screen.value,fullscreen:fullscreen.value,
          title:'步骤编辑',dirty:dirty.value,
          onClose:editorClose,
        },{
          default:()=>h('label',{for:'parameter-input'},[
            '待确认参数',
            h('input',{id:'parameter-input',value:parameter.value,placeholder:'type here',
              onInput:e=>{parameter.value=e.target.value;},style:'margin-left:8px'}),
          ]),
        }),
        h(AutomationFloatingDialog,{
          visible:detail.value,portalTarget:screen.value,fullscreen:fullscreen.value,
          title:'条件子窗',dirty:false,onClose:detailClose,
        },{
          default:()=>h('p',{id:'detail-body'},'另一个可移动子窗'),
        }),
      ]),
      h('div',{id:'status',style:'margin-top:8px'},'无真实手机动作/网络访问'),
    ]);
  },
};
createApp(Fixture).mount('#app');
