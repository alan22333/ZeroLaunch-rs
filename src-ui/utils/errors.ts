/**
 * 将未知错误转为可读错误文案。
 * BridgeError 取 message；对象/异常取 message 字段；普通值取 String；
 * 避免 String({...}) 产出 "[object Object]" 之类的无意义文案。
 */
export function formatErrorMessage(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message
  if (typeof e === 'object' && e !== null && 'message' in e) {
    const msg = (e as { message: unknown }).message
    if (typeof msg === 'string' && msg) return msg
  }
  return typeof e === 'string' ? e : String(e)
}
