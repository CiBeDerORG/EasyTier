<script setup lang="ts">
import { Button, Tag } from 'primevue';
import { useRouter } from 'vue-router';
import { useI18n } from 'vue-i18n';

const router = useRouter();
const { t } = useI18n({ useScope: 'local', messages: {
    cn: {
        title: '自动化中心', description: '集中查看手机画面、执行批量操作，制作可复用的脚本。',
        planned: '规划中', preparation: '准备接入主机', stage: '当前阶段 · 接入准备',
        emptyTitle: '从第一组手机开始',
        emptyDescription: '此入口已就绪。手机接入、实时画面和脚本执行尚未开放；当前可以先管理用于接入手机的主机。',
        manual: '由你点击开始，按需连接和操作。',
        screens: '设备画面', screensDescription: '规划：多屏预览、单机放大和分组选择，显示每台设备最近的画面状态。',
        batch: '批量操作', batchDescription: '规划：对选中的设备执行操作，逐台查看结果，失败后由你决定是否重试。',
        scripts: '脚本库', scriptsDescription: '规划：录制操作、编辑步骤与参数，保存版本并手动运行。',
        hosts: '接入主机', hostsDescription: '先在主机安装 Agent，再通过「应用与服务」验证连接。手机接入组件将在后续版本提供。',
        preparationTitle: '接入准备', stepOne: '准备主机', stepOneDescription: '首期方案以 Linux 或 fnOS 主机接入 Android 手机为起点。',
        stepTwo: '连接手机', stepTwoDescription: '准备可传输数据的 USB 连接，开启并授权 USB 调试。',
        stepThree: '等待手机接入能力', stepThreeDescription: '主机 Agent 的连接验证不代表手机已可控制。手机数量、接入方式与平台支持仍需确认。',
    },
    en: {
        title: 'Automation center', description: 'View phone screens, run batch actions and create reusable scripts.',
        planned: 'Planned', preparation: 'Prepare a host', stage: 'Current stage · Connection preparation',
        emptyTitle: 'Start with your first group of phones',
        emptyDescription: 'This entry is ready. Phone connections, live screens and script execution are not available yet. You can prepare and manage a host now.',
        manual: 'You choose when to connect and start each action.',
        screens: 'Device screens', screensDescription: 'Planned: a screen wall, focused device view and group selection, with the latest frame state for each phone.',
        batch: 'Batch actions', batchDescription: 'Planned: act on selected devices, inspect individual results and choose when to retry failures.',
        scripts: 'Script library', scriptsDescription: 'Planned: record actions, edit steps and parameters, save versions and run manually.',
        hosts: 'Connection hosts', hostsDescription: 'Install the Agent on a host, then verify it in Applications & services. The phone bridge will be available in a later version.',
        preparationTitle: 'Prepare for connection', stepOne: 'Prepare a host', stepOneDescription: 'The initial proposal uses a Linux or fnOS host with Android phones.',
        stepTwo: 'Connect phones', stepTwoDescription: 'Prepare USB data connections and enable and authorize USB debugging.',
        stepThree: 'Phone support comes next', stepThreeDescription: 'Verifying a host Agent does not enable phone control. Device counts, connection methods and supported platforms still need confirmation.',
    },
} });
const areas = [
    { key: 'screens', icon: 'pi-th-large' },
    { key: 'batch', icon: 'pi-check-square' },
    { key: 'scripts', icon: 'pi-code' },
    { key: 'hosts', icon: 'pi-server' },
];
</script>

<template>
    <section class="console-page automation-page" aria-labelledby="automation-title">
        <header class="page-heading">
            <div class="heading-identity">
                <span class="entity-icon page-icon"><i class="pi pi-mobile" aria-hidden="true"></i></span>
                <div><h1 id="automation-title">{{ t('title') }}</h1><p class="page-description">{{ t('description') }}</p></div>
            </div>
            <Tag :value="t('planned')" severity="secondary" />
        </header>
        <section class="console-panel introduction">
            <div class="intro-art" aria-hidden="true"><i class="pi pi-mobile"></i><i class="pi pi-mobile"></i><i class="pi pi-mobile"></i></div>
            <div class="intro-copy">
                <p class="stage">{{ t('stage') }}</p><h2>{{ t('emptyTitle') }}</h2><p class="description">{{ t('emptyDescription') }}</p>
                <Button :label="t('preparation')" icon="pi pi-arrow-right" iconPos="right" @click="router.push({ name: 'deployAgents' })" />
            </div>
        </section>
        <div class="capability-grid">
            <article v-for="area in areas" :key="area.key" class="console-panel capability">
                <span class="entity-icon"><i class="pi" :class="area.icon" aria-hidden="true"></i></span>
                <h2>{{ t(area.key) }}</h2><p>{{ t(`${area.key}Description`) }}</p>
            </article>
        </div>
        <section class="console-panel">
            <header class="section-heading"><h2>{{ t('preparationTitle') }}</h2></header>
            <ol class="preparation-list">
                <li v-for="(step, index) in ['stepOne', 'stepTwo', 'stepThree']" :key="step">
                    <span class="step-number" aria-hidden="true">{{ index + 1 }}</span><div><h3>{{ t(step) }}</h3><p>{{ t(`${step}Description`) }}</p></div>
                </li>
            </ol>
        </section>
        <p class="page-footnote"><i class="pi pi-bolt" aria-hidden="true"></i>{{ t('manual') }}</p>
    </section>
</template>

<style scoped>
.heading-identity { display: flex; align-items: center; gap: 14px; min-width: 0; }
.page-heading > .p-tag { flex-shrink: 0; white-space: nowrap; }
.page-icon { width: 48px; height: 48px; font-size: 20px; border-radius: 13px; }
.introduction { display: flex; align-items: center; gap: 32px; padding: 36px; background: linear-gradient(125deg, var(--console-accent-soft), var(--console-panel) 70%); }
.intro-art { display: flex; align-items: center; gap: 10px; color: var(--p-primary-color); padding: 20px 12px; }
.intro-art .pi { font-size: 48px; }
.intro-art .pi:nth-child(2) { font-size: 68px; }
.intro-copy { max-width: 680px; }
.stage { color: var(--console-muted); font-size: 12px; margin-bottom: 12px; }
.intro-copy h2 { font-size: 22px; font-weight: 600; }
.description { margin: 12px 0 20px; color: var(--console-muted); font-size: 14px; line-height: 1.8; }
.capability-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 20px; }
.capability { padding: 20px; }
.capability h2 { font-size: 14px; font-weight: 600; margin: 16px 0 8px; }
.capability p, .preparation-list p { font-size: 13px; color: var(--console-muted); line-height: 1.8; }
.preparation-list { list-style: none; padding: 20px; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 24px; }
.preparation-list li { display: flex; gap: 12px; }
.step-number { display: inline-flex; justify-content: center; align-items: center; flex-shrink: 0; width: 26px; height: 26px; border: 1px solid var(--console-border); border-radius: 50%; background: var(--console-panel-raised); font-size: 12px; color: var(--console-muted); }
.preparation-list h3 { font-size: 13px; font-weight: 550; margin-bottom: 6px; }
@media (max-width: 1100px) { .capability-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } .preparation-list { grid-template-columns: 1fr; } }
@media (max-width: 600px) { .page-heading { align-items: flex-start; } .heading-identity { align-items: flex-start; } .introduction { flex-direction: column; align-items: flex-start; gap: 12px; padding: 24px; } .intro-art { padding: 0; } .capability-grid { grid-template-columns: 1fr; } }
</style>
