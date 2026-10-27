/**
 * T14 - read-only result observer for a single-step trial.
 * Never schedules or retries an Android action. All polling is GET /runs/{id}.
 */
import { computed, onBeforeUnmount, ref } from 'vue';
import type { AutomationRun } from '../types/automation';
import AutomationApi, { automationErrorMessage } from './automation-api';

import { summarizeCardRun, type CardRunView } from './automation-run-result.ts';
export { summarizeCardRun } from './automation-run-result.ts';
export type { CardRunPhase, CardRunView } from './automation-run-result.ts';
type RunReader = Pick<AutomationApi, 'getRun'>;

/**
 * A page-local observer. When the dialog hides, stop polling. Reopening may
 * explicitly resume GET-only observation of the same persisted run.
 */
export function useCardRunStatus(api: RunReader = new AutomationApi()) {
    const run = ref<AutomationRun | null>(null);
    const deviceId = ref('');
    const stepId = ref('');
    const error = ref('');
    const polling = ref(false);
    const observing = ref(false);
    const view = computed<CardRunView | null>(() => run.value
        ? summarizeCardRun(run.value,deviceId.value,stepId.value) : null);
    let owner = 0, generation = 0, timer: ReturnType<typeof setTimeout> | undefined;
    function stopTimer() { if(timer!==undefined)clearTimeout(timer);timer=undefined; }
    function stop() { ++generation; observing.value=false; polling.value=false; stopTimer(); }
    function reset() { stop();run.value=null;deviceId.value='';stepId.value='';owner=0;error.value=''; }
    async function refresh(): Promise<void> {
        const current=run.value;
        if (!current || !observing.value || polling.value) return;
        const mine=generation, account=owner, id=current.id;
        polling.value=true;
        try {
            const response=await api.getRun(account,id);
            if(mine!==generation || !observing.value || response.id!==id)return;
            // Do not display an unrelated device/step from an invalid response.
            if (!response.devices.some(item=>item.device_id===deviceId.value)) {
                error.value='运行记录没有目标手机，已停止查询。';
                stop();return;
            }
            run.value=response;error.value='';
        }catch(cause){
            if(mine!==generation || !observing.value)return;
            error.value='查询执行进度失败：'+automationErrorMessage(cause);
        }finally{
            if(mine===generation) {
                polling.value=false;stopTimer();
                // Unknown outcomes and terminal states are for the user to
                // inspect, not a reason to resubmit an action.
                const phase=view.value?.phase;
                if (observing.value && phase!=='unknown' &&
                    phase!=='succeeded' && phase!=='action_sent' && phase!=='failed' && phase!=='cancelled')
                    timer=setTimeout(()=>void refresh(),2000);
            }
        }
    }
    function observe(account:number,initial:AutomationRun,did:string,sid:string) {
        stop();owner=account;run.value=initial;deviceId.value=did;stepId.value=sid;
        error.value='';observing.value=true;
        if (!initial.devices.some(item=>item.device_id===did)) {
            error.value='返回的任务不包含选定的手机。';stop();return;
        }
        const phase=view.value?.phase;
        if (!['succeeded','action_sent','failed','cancelled','unknown'].includes(phase??'')) {
            timer=setTimeout(()=>void refresh(),1500);
        }
    }
    function resume() {
        if (!run.value || observing.value) return;
        observing.value=true;
        void refresh();
    }
    onBeforeUnmount(stop);
    return {run,view,error,polling,observing,observe,refresh,resume,stop,reset};
}
