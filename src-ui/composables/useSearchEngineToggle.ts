import { ref } from 'vue'
import { useConfigStore } from '../stores/config-store'
import type { ComponentInfo } from '../bridge/contract'

export function useSearchEngineToggle(getEngines: () => ComponentInfo[]) {
  const configStore = useConfigStore()

  // 互斥切换在途标志：多引擎逐个 setEnabled 非原子，期间禁用重复点击，
  // 防止并发切换产生互相覆盖（后端原子 set_exclusive 命令待后续，见下）。
  const toggling = ref(false)

  async function onToggle(componentId: string, val: boolean) {
    if (toggling.value) return
    toggling.value = true
    try {
      const engines = getEngines()

      if (!val) {
        // 允许全部禁用：无引擎时后端搜索管道候选零分透传，仅由增强器排序。
        try {
          await configStore.setEnabled(componentId, false)
        } catch (e) {
          console.error(e)
        }
        return
      }

      // 互斥语义：先顺序禁用其他已启用引擎，再启用当前引擎。
      // 注意：非原子（两次独立 IPC），切换期间若发生异常，可能停留在"全禁用"或"多启用"中间态。
      // 后端原子 set_exclusive 命令待后续实现，届时此处改为单次 IPC 调用。
      try {
        for (const engine of engines) {
          if (engine.componentId !== componentId && engine.enabled) {
            await configStore.setEnabled(engine.componentId, false)
          }
        }
        await configStore.setEnabled(componentId, true)
      } catch (e) {
        console.error(e)
      }
    } finally {
      toggling.value = false
    }
  }

  return { onToggle, toggling }
}
