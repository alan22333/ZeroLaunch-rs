import { useConfigStore } from '../stores/config-store'
import type { ComponentInfo } from '../bridge/contract'

export function useSearchEngineToggle(getEngines: () => ComponentInfo[]) {
  const configStore = useConfigStore()

  async function onToggle(componentId: string, val: boolean) {
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
  }

  return { onToggle }
}
