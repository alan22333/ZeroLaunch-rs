<template>
  <div class="field-input-row">
    <n-select
      :value="modelValue as string"
      :options="options"
      :loading="loading"
      :disabled="field.readOnly"
      @update:value="emit('update:modelValue', $event)"
    />
    <ConfigActionButton
      v-if="field.action"
      :component-id="componentId"
      :field-action="field.action"
      :field-key="field.key"
      :editable="!field.readOnly"
      @update:model-value="onActionResult"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NSelect } from 'naive-ui'
import ConfigActionButton from '../ConfigActionButton.vue'
import { getSchemaEnumOptions } from '../../../utils/schemaTypes'
import type { FieldConfig } from '../../../utils/schemaTypes'
import type { DataActionBinding } from '../../../bridge/contract'
import { useConfigStore } from '../../../stores/config-store'

const props = defineProps<{
  field: FieldConfig
  componentId: string
  modelValue: unknown
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: unknown): void
}>()

const configStore = useConfigStore()
const loading = ref(false)
const dynamicOptions = ref<{ label: string; value: string }[]>([])

/** 字段声明的 data action（返回数组映射为下拉选项）；effect action 仍走按钮副作用。 */
const dataAction = computed<DataActionBinding | null>(() =>
  props.field.action?.kind === 'data' ? props.field.action.binding : null,
)

/** 下拉选项：声明 data action 时由 action 运行时提供，否则用 schema enum。 */
const options = computed(() =>
  dataAction.value
    ? dynamicOptions.value
    : getSchemaEnumOptions(props.field.schema),
)

/** 把 action 返回的对象数组按 labelField/valueField 映射为选项。 */
function mapOptions(result: unknown): { label: string; value: string }[] {
  const arr = Array.isArray(result) ? result : []
  const binding = dataAction.value
  if (!binding) return []
  return (arr as Record<string, unknown>[])
    .map((item) => ({
      label: String(item[binding.labelField] ?? ''),
      value: String(item[binding.valueField] ?? ''),
    }))
    .filter((opt) => opt.value.length > 0)
}

/** 挂载时若有 data action，调用 action 拉取选项。 */
onMounted(async () => {
  const binding = dataAction.value
  if (!binding) return
  loading.value = true
  try {
    const result = await configStore.executeAction(
      binding.component ?? props.componentId,
      binding.action,
    )
    dynamicOptions.value = mapOptions(result)
  } catch {
    dynamicOptions.value = []
  } finally {
    loading.value = false
  }
})

/** 按钮刷新：data action 返回的数组映射为选项；effect action 返回值仍写入字段。 */
function onActionResult(value: unknown): void {
  if (dataAction.value) {
    dynamicOptions.value = mapOptions(value)
  } else {
    emit('update:modelValue', value)
  }
}
</script>

<style scoped>
.field-input-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.field-input-row > :first-child {
  flex: 1;
}
</style>
