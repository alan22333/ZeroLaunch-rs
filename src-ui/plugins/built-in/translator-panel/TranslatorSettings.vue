<script setup lang="ts">
import { computed, reactive, ref, watch, onMounted } from 'vue'
import { configExecuteAction, configGetSchema } from '@/bridge/commands'
import { NButton, NInputNumber, NSelect } from 'naive-ui'
import { useI18n } from 'vue-i18n'
import FormSection from '@/components/settings/FormSection.vue'

const { t } = useI18n()

type TranslatorLocalSettings = {
  translate_mode: 'live' | 'on_enter'
  default_target: string
  request_timeout_ms: number
  live_debounce_secs: number
  model_id: string
}

const props = defineProps<{
  currentSettings: unknown
}>()

const emit = defineEmits<{
  (e: 'save', settings: TranslatorLocalSettings): void
}>()

const saving = ref(false)

function modeFromRaw(raw: unknown): 'live' | 'on_enter' {
  // get_settings 当前直接返回代码值（如 "live"），不包含标签
  if (raw === 'on_enter') return 'on_enter'
  return 'live'
}

function defaults(): TranslatorLocalSettings {
  return {
    translate_mode: 'live',
    default_target: 'zh',
    request_timeout_ms: 15000,
    live_debounce_secs: 0.5,
    model_id: '',
  }
}

function fromProps(raw: unknown): TranslatorLocalSettings {
  const base = defaults()
  if (!raw || typeof raw !== 'object') return base
  const o = raw as Record<string, unknown>
  return {
    translate_mode: modeFromRaw(o.translate_mode),
    default_target:
      typeof o.default_target === 'string'
        ? o.default_target.trim()
        : base.default_target,
    request_timeout_ms:
      typeof o.request_timeout_ms === 'number'
        ? o.request_timeout_ms
        : base.request_timeout_ms,
    live_debounce_secs:
      typeof o.live_debounce_secs === 'number'
        ? o.live_debounce_secs
        : base.live_debounce_secs,
    model_id: typeof o.model_id === 'string' ? o.model_id : base.model_id,
  }
}

const local = reactive(fromProps(props.currentSettings))

watch(
  () => props.currentSettings,
  (v) => {
    Object.assign(local, fromProps(v))
  },
)

/** 由后端 schema default_target 的 enumLabels 填充。 */
const languageOptions = ref<{ label: string; value: string }[]>([])

onMounted(async () => {
  try {
    const schema = await configGetSchema('translator')
    const field = schema.contribution.properties?.['default_target']
    if (field?.type === 'string' && field.enum) {
      languageOptions.value = field.enum.map((v, i) => ({
        label: field.enumLabels?.[i] ?? v,
        value: v,
      }))
    }
  } catch {
    // schema 加载失败时语言选项为空列表，由 watch 回退到默认值
  }
  await loadHostModels()
})

// 保存的 default_target 不在语言选项内时仅提示，不静默改写已保存值；
// 用户下次保存（或修改该字段）时再校正，避免就绪即丢失用户配置。
const defaultTargetInvalid = ref(false)

watch(
  languageOptions,
  (opts) => {
    if (opts.length > 0) {
      defaultTargetInvalid.value = !opts.some((o) => o.value === local.default_target)
    }
  },
  { immediate: true },
)

/** 语言选项下拉：值不在选项内时提示重新选择（配合 defaultTargetInvalid 警告）。 */
function onDefaultTargetChange(val: string) {
  local.default_target = val
  defaultTargetInvalid.value = !languageOptions.value.some((o) => o.value === val)
}

// 选项数组用 computed：随界面语言切换（t 响应 locale）重新求值。
const translateModeOptions = computed(() => [
  { label: t('translator.modeLive'), value: 'live' },
  { label: t('translator.modeOnEnter'), value: 'on_enter' },
])

// ===== 宿主模型列表 =====

/** 宿主模型清单中的 chat 模型（经插件 config action 拉取，保持插件能力边界）。 */
type HostChatModel = { modelId: string; name: string; provider: string }
const hostModels = ref<HostChatModel[]>([])
const modelsLoading = ref(false)
const modelsLoadFailed = ref(false)

async function loadHostModels(): Promise<void> {
  modelsLoading.value = true
  modelsLoadFailed.value = false
  try {
    // 经 translator 插件自身的 config action 访问宿主模型服务，
    // 与第三方插件一致，不直接调用宿主内部命令。
    const result = (await configExecuteAction('translator', 'list_models')) as unknown
    const arr = Array.isArray(result) ? result : []
    hostModels.value = (arr as Record<string, unknown>[])
      .filter((m) => (m as { kind?: string }).kind === 'chat')
      .map((m) => ({
        modelId: String((m as { modelId?: unknown }).modelId ?? ''),
        name: String((m as { name?: unknown }).name ?? ''),
        provider: String((m as { provider?: unknown }).provider ?? ''),
      }))
      .filter((m) => m.modelId.length > 0)
  } catch {
    modelsLoadFailed.value = true
  } finally {
    modelsLoading.value = false
  }
}

const modelOptions = computed(() =>
  hostModels.value.map((m) => ({
    label: m.name || m.modelId,
    value: m.modelId,
  })),
)

async function onSave() {
  saving.value = true
  try {
    emit('save', {
      translate_mode: local.translate_mode,
      default_target: local.default_target,
      request_timeout_ms: local.request_timeout_ms,
      live_debounce_secs: local.live_debounce_secs,
      model_id: local.model_id.trim(),
    })
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <div class="translator-settings">
    <div class="form-groups">
      <FormSection :title="$t('translator.sectionBasic')" :collapsible="true">
        <div class="form-field">
          <label class="field-label">{{ $t('translator.translateTrigger') }}</label>
          <div class="field-control">
            <n-select
              v-model:value="local.translate_mode"
              :options="translateModeOptions"
              class="control-full"
            />
            <p class="field-hint">{{ $t('translator.triggerHint') }}</p>
          </div>
        </div>
        <div class="form-field">
          <label class="field-label">{{ $t('translator.defaultTargetLanguage') }}</label>
          <div class="field-control">
            <n-select
              v-model:value="local.default_target"
              :options="languageOptions"
              filterable
              class="control-full"
              @update:value="onDefaultTargetChange"
            />
            <p v-if="defaultTargetInvalid" class="field-hint field-hint-warn">
              {{ $t('translator.defaultTargetInvalid') }}
            </p>
          </div>
        </div>
        <div class="form-field">
          <label class="field-label">{{ $t('translator.timeoutMs') }}</label>
          <div class="field-control">
            <n-input-number
              v-model:value="local.request_timeout_ms"
              :min="1000"
              :max="60000"
              :step="500"
              class="control-full"
            />
          </div>
        </div>
        <div class="form-field">
          <label class="field-label">{{ $t('translator.liveDebounceSecs') }}</label>
          <div class="field-control">
            <n-input-number
              v-model:value="local.live_debounce_secs"
              :min="0.1"
              :max="5.0"
              :step="0.1"
              class="control-full"
            />
            <p class="field-hint">{{ $t('translator.liveDebounceHint') }}</p>
          </div>
        </div>
      </FormSection>

      <FormSection :title="$t('translator.sectionEngine')" :collapsible="true">
        <div class="form-field">
          <label class="field-label">{{ $t('translator.modelId') }}</label>
          <div class="field-control">
            <n-select
              v-model:value="local.model_id"
              :options="modelOptions"
              :loading="modelsLoading"
              :disabled="modelsLoadFailed"
              :placeholder="$t('translator.modelIdPlaceholder')"
              filterable
              clearable
              class="control-full"
            />
            <div v-if="modelsLoadFailed" class="model-load-error">
              <span class="field-hint">{{ $t('translator.modelLoadFailed') }}</span>
              <n-button size="tiny" quaternary type="primary" @click="loadHostModels">
                {{ $t('translator.modelLoadRetry') }}
              </n-button>
            </div>
            <p class="field-hint">{{ $t('translator.modelIdHint') }}</p>
          </div>
        </div>
      </FormSection>
    </div>

    <div class="form-actions">
      <n-button type="primary" :loading="saving" @click="onSave">{{ $t('translator.apply') }}</n-button>
    </div>
  </div>
</template>

<style scoped>
.translator-settings {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1 1 auto;
  padding: 16px 24px 0;
}

.form-groups {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-bottom: 16px;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field-label {
  font-size: var(--font-size-sm);
  color: var(--text-primary);
}

.field-control {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.control-full {
  width: 100%;
}

.field-hint {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
  line-height: 1.5;
}
.field-hint-warn {
  color: var(--text-error);
}

.model-load-error {
  display: flex;
  align-items: center;
  gap: 8px;
}

.form-actions {
  display: flex;
  gap: 8px;
  padding: 12px 0 16px;
  border-top: 1px solid var(--border-color);
  background-color: var(--bg-color);
  flex-shrink: 0;
}
</style>
