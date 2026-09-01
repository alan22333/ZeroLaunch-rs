import { reactive, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { useI18n } from 'vue-i18n'

/**
 * 调试工具查询样板：封装「输入框 + 执行 + loading + 结果」状态与失败提示，
 * 供调试页各查询工具复用，消除重复的 loading/error 样板代码。
 * @param runner 接收去空格后的输入并返回结果；输入为空时不执行。
 */
export function useQueryTool<T>(runner: (input: string) => Promise<T>) {
  const message = useMessage()
  const { t } = useI18n()
  const input = ref('')
  const loading = ref(false)
  const result = ref<T | null>(null)

  async function run(): Promise<void> {
    // loading 守卫：请求在途时忽略新的触发（Enter 连击/重复点击），
    // 防止慢响应并发覆盖新结果；完成后 loading 复位即可再次触发。
    if (loading.value) return
    const query = input.value.trim()
    if (!query) return
    loading.value = true
    try {
      result.value = await runner(query)
    } catch {
      message.error(t('debug.queryFailed'))
    } finally {
      loading.value = false
    }
  }

  // reactive 包装使模板可直接访问解包后的 input/loading/result
  return reactive({ input, loading, result, run })
}
