import type { NLocale } from 'naive-ui'
import { zhCN, zhTW, enUS } from 'naive-ui'

/**
 * 界面语言 → Naive UI 语言包映射。
 * zh-Hant 映射繁体包（zhTW），避免落到简体语言包。
 * 供 App.vue / SettingsApp.vue 的 n-config-provider locale 使用。
 */
export function naiveLocaleFor(lang: string): NLocale {
  if (lang === 'en') return enUS
  if (lang === 'zh-Hant') return zhTW
  return zhCN
}
