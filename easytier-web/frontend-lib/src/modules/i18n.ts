import { createI18n } from 'vue-i18n'
import type { Locale } from 'vue-i18n'

import EnLocale from '../locales/en.yaml'
import CnLocale from '../locales/cn.yaml'
import NodeConsoleLocale from '../locales/node-console.yaml'

// Import i18n resources
// https://vitejs.dev/guide/features.html#glob-import
export const i18n = createI18n({
  legacy: false,
  locale: '',
  fallbackLocale: '',
  messages: {},
})

// Keep the shared page title in one place so sidebar, breadcrumb and heading
// change together. Merge before lazy loading; switching language keeps the copy.
function withNodeConsoleMessages(
  base: Record<string, any>,
  copy: { page: Record<string, string>; device: Record<string, string> },
) {
  return {
    ...base,
    assets: { ...base.assets, page: { ...base.assets.page, ...copy.page } },
    web: { ...base.web, device: { ...base.web.device, ...copy.device } },
  }
}

const localesMap = {
  "en": withNodeConsoleMessages(EnLocale, NodeConsoleLocale.en),
  "cn": withNodeConsoleMessages(CnLocale, NodeConsoleLocale.cn),
} as Record<string, any>

export const availableLocales = Object.keys(localesMap)

const loadedLanguages: string[] = []

export function toggleLanguage() {
  const currentLang = localStorage.getItem('lang') || 'en'
  const newLang = currentLang === 'en' ? 'cn' : 'en'
  loadLanguageAsync(newLang)
}

export function getCurrentLanguage() {
  return localStorage.getItem('lang') || 'en'
}

function setI18nLanguage(lang: Locale) {
  i18n.global.locale.value = lang as any
  localStorage.setItem('lang', lang)
  return lang
}

export async function loadLanguageAsync(lang: string): Promise<Locale> {
  // If the same language
  if (i18n.global.locale.value === lang)
    return setI18nLanguage(lang)

  // If the language was already loaded
  if (loadedLanguages.includes(lang))
    return setI18nLanguage(lang)

  // If the language hasn't been loaded yet
  let messages

  try {
    messages = localesMap[lang]
  }
  catch {
    messages = localesMap.en
  }

  i18n.global.setLocaleMessage(lang, messages)
  loadedLanguages.push(lang)
  return setI18nLanguage(lang)
}

export default {
  i18n,
  localesMap,
  loadLanguageAsync,
  toggleLanguage,
  getCurrentLanguage,
}
