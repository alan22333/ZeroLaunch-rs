<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import { i18n } from '@/i18n'

/**
 * 第三方插件设置面板宿主：插件设置 UI（zlplugin:// 资源）动态 import 到宿主
 * document 的 Shadow DOM 容器内执行。host API：
 * - onSettingsUpdate(cb)：宿主下发当前设置值（进入/变更时调用）
 * - save(settings)：设置值提交回宿主（emit save → 配置系统保存）
 * - t(key)：插件语言包翻译（自动补插件 id 前缀，key-or-literal）
 */
const props = defineProps<{
  pluginId: string
  settingsEntryUrl: string
  currentSettings: unknown
}>()

const emit = defineEmits<{
  (e: 'save', settings: unknown): void
}>()

const containerRef = ref<HTMLDivElement | null>(null)

type SettingsHost = {
  pluginId: string
  onSettingsUpdate: (cb: (settings: unknown) => void) => void
  save: (settings: unknown) => void
  t: (key: string, params?: Record<string, unknown>) => string
}

let mountFn: ((rootEl: HTMLElement, host: SettingsHost) => void) | null = null
let settingsUpdateCb: ((settings: unknown) => void) | null = null

async function loadSettings() {
  if (!containerRef.value || !props.settingsEntryUrl) return
  const shadow = containerRef.value.attachShadow({ mode: 'open' })
  // fallback 样式：CSS 变量经宿主继承进 Shadow DOM（inline style 不支持 var()）
  const style = document.createElement('style')
  style.textContent =
    '.fallback { padding: 24px; color: var(--text-error); font-size: var(--font-size-sm); }'
  shadow.appendChild(style)
  const mountEl = document.createElement('div')
  mountEl.style.height = '100%'
  shadow.appendChild(mountEl)
  try {
    const mod = await import(/* @vite-ignore */ props.settingsEntryUrl)
    mountFn = mod.default
    mountFn?.(mountEl, {
      pluginId: props.pluginId,
      onSettingsUpdate(cb) {
        settingsUpdateCb = cb
        // 注册后立即重放当前设置（watch immediate 触发时回调尚未注册，初值已错过）
        cb(props.currentSettings)
      },
      save: (settings) => emit('save', settings),
      t(key, params) {
        const fullKey = `plugin.${props.pluginId}.${key}`
        return i18n.global.te(fullKey) ? i18n.global.t(fullKey, params ?? {}) : key
      },
    })
  } catch (err) {
    shadow.innerHTML = ''
    const fallback = document.createElement('div')
    fallback.className = 'fallback'
    fallback.textContent = i18n.global.t('pluginSettings.loadError', {
      message: (err as Error)?.message ?? '',
    })
    shadow.appendChild(fallback)
    console.error('[ThirdPartySettingsHost] 设置面板加载失败:', err)
  }
}

onMounted(loadSettings)

onUnmounted(() => {
  mountFn = null
  settingsUpdateCb = null
})

watch(
  () => props.currentSettings,
  (newSettings) => {
    settingsUpdateCb?.(newSettings)
  },
  { immediate: true },
)
</script>

<template>
  <div ref="containerRef" class="third-party-settings-host" />
</template>

<style scoped>
.third-party-settings-host {
  width: 100%;
  height: 100%;
}
</style>
