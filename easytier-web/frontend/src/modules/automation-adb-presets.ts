/** Device-scoped shell commands that can be selected without typing. */
export interface AdbCommandPreset {
    id: string;
    title: string;
    englishTitle: string;
    command: string;
}
export const builtinAdbCommands: readonly AdbCommandPreset[] = [
    { id:'home', title:'返回桌面', englishTitle:'Home', command:'adb shell input keyevent 3' },
    { id:'back', title:'返回上一页', englishTitle:'Back', command:'adb shell input keyevent 4' },
    { id:'recent', title:'最近任务', englishTitle:'Recent apps', command:'adb shell input keyevent 187' },
    { id:'wake', title:'唤醒屏幕', englishTitle:'Wake screen', command:'adb shell input keyevent 224' },
    { id:'screen-size', title:'查看屏幕分辨率', englishTitle:'Screen resolution', command:'adb shell wm size' },
    { id:'density', title:'查看屏幕密度', englishTitle:'Screen density', command:'adb shell wm density' },
    { id:'battery', title:'查看电池状态', englishTitle:'Battery information', command:'adb shell dumpsys battery' },
    { id:'model', title:'查看手机型号', englishTitle:'Phone model', command:'adb shell getprop ro.product.model' },
    { id:'version', title:'查看安卓版本', englishTitle:'Android version', command:'adb shell getprop ro.build.version.release' },
    { id:'packages', title:'查看已安装应用', englishTitle:'Installed packages', command:'adb shell pm list packages' },
    { id:'screen-timeout', title:'查看息屏时间', englishTitle:'Screen timeout', command:'adb shell settings get system screen_off_timeout' },
] as const;
