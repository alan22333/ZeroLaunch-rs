<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'
import type { BridgeQueryResponse, ResultAction } from '@/bridge/contract'
import { bridgeConfirm, bridgeQuery } from '@/bridge/commands'
import { useSearchStore } from '@/stores/search-store'
import { i18n } from '@/i18n'

/**
 * 第三方插件面板宿主：插件 UI 资源通过宿主协议动态 import 到宿主 document 的
 * Shadow DOM 容器内执行——键盘事件冒泡到宿主窗口（bindings「声明即接管」生效）、
 * i18n 同步直查、数据/动作直连 IPC。Shadow DOM 保留样式隔离。
 */
const props = defineProps<{
  pluginId: string
  panelEntryUrl: string
  data: unknown
  actions: ResultAction[]
}>()

const searchStore = useSearchStore()

const containerRef = ref<HTMLDivElement | null>(null)

type PanelHost = {
  pluginId: string
  onDataUpdate: (cb: (data: unknown, actions: ResultAction[]) => void) => void
  t: (key: string, params?: Record<string, unknown>) => string
  executeAction: (action: string, args: unknown) => Promise<void>
  query: (rawQuery: string) => Promise<BridgeQueryResponse>
  exit: () => void
}

let mountFn: ((rootEl: HTMLElement, host: PanelHost) => void) | null = null
let dataUpdateCb: ((data: unknown, actions: ResultAction[]) => void) | null = null

/// 面板数据通道：bridge_query 显式指定当前面板插件（QueryChannel::Panel——
/// 只读辅助路径，不经触发词路由、不改写 GUI 会话），响应为 BridgeQueryResponse
/// （mode 词表 + 图标 data URL 已解析）。
async function query(rawQuery: string): Promise<BridgeQueryResponse> {
  return bridgeQuery(rawQuery, false, props.pluginId)
}

async function executeAction(action: string, args: unknown): Promise<void> {
  await bridgeConfirm({
    kind: 'pluginAction',
    pluginId: props.pluginId,
    action,
    args: args ?? {},
    generation: searchStore.currentGeneration,
  })
}

function buildHost(): PanelHost {
  return {
    pluginId: props.pluginId,
    onDataUpdate(cb) {
      dataUpdateCb = cb
      // 注册后立即重放当前载荷（watch immediate 触发时回调尚未注册，初值已错过）
      cb(props.data, props.actions)
    },
    /// 面板翻译：自动补插件 id 前缀（与 Rust 侧 t_key 同键格式），key-or-literal；
    /// 支持 vue-i18n 命名插值参数（如 {count}）。
    t(key, params) {
      const fullKey = `plugin.${props.pluginId}.${key}`
      return i18n.global.te(fullKey) ? i18n.global.t(fullKey, params ?? {}) : key
    },
    executeAction,
    query,
    exit: () => searchStore.hideWindow(),
  }
}

async function loadPanel() {
	if (!containerRef.value || !props.panelEntryUrl) return
	const shadow = containerRef.value.attachShadow({ mode: 'open' })
	// fallback 样式：CSS 变量经宿主继承进 Shadow DOM（inline style 不支持 var()）
	const style = document.createElement('style')
	style.textContent =
		'.fallback { padding: 24px; color: var(--text-error); font-size: var(--font-size-sm); }'
	shadow.appendChild(style)
  const mountEl = document.createElement('div')
  mountEl.style.display = 'flex'
  mountEl.style.flex = '1'
  mountEl.style.flexDirection = 'column'
  mountEl.style.minWidth = '0'
  mountEl.style.minHeight = '0'
  shadow.appendChild(mountEl)
  try {
    // 动态加载插件资源（CSP 已允许 zlplugin.localhost 源）。
    const mod = await import(/* @vite-ignore */ props.panelEntryUrl)
		mountFn = mod.default
		mountFn?.(mountEl, buildHost())
	} catch (err) {
		shadow.innerHTML = ''
		const fallback = document.createElement('div')
		fallback.className = 'fallback'
		fallback.textContent = i18n.global.t('pluginPanel.loadError', {
			message: (err as Error)?.message ?? '',
		})
		shadow.appendChild(fallback)
		console.error('[ThirdPartyPanelHost] 面板加载失败:', err)
	}
}

onMounted(loadPanel)

onUnmounted(() => {
  mountFn = null
  dataUpdateCb = null
})

// 面板数据更新（会话推送 data-update 语义：直接调用面板注册的回调）
watch(
  () => [props.data, props.actions],
  () => {
    dataUpdateCb?.(props.data, props.actions)
  },
  { deep: true, immediate: true },
)
</script>

<template>
  <div ref="containerRef" class="third-party-panel-host" data-no-drag />
</template>

<style scoped>
.third-party-panel-host {
  display: flex;
  width: 100%;
  height: 100%;
  min-height: 0;
}
</style>
