<template>
  <div class="multiselect-field">
    <n-checkbox-group
      :value="selectedValues"
      @update:value="onUpdate"
    >
      <div class="option-list">
        <n-checkbox
          v-for="option in options"
          :key="option.value"
          :value="option.value"
          :disabled="field.readOnly"
        >
          {{ option.label }}
        </n-checkbox>
      </div>
    </n-checkbox-group>
    <ConfigActionButton
      v-if="field.action"
      :component-id="componentId"
      :field-action="field.action"
      :field-key="field.key"
      :editable="!field.readOnly"
      @update:model-value="emit('update:modelValue', $event)"
    />
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { NCheckbox, NCheckboxGroup } from 'naive-ui'
import ConfigActionButton from '../ConfigActionButton.vue'
import { getSchemaEnumOptions } from '../../../utils/schemaTypes'
import type { FieldConfig } from '../../../utils/schemaTypes'

const props = defineProps<{
  field: FieldConfig
  componentId: string
  modelValue: unknown
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', value: unknown): void
}>()

/** 从数组元素 schema 读取能力选项并解析本地化标签。 */
const options = computed(() => {
  if (props.field.schema.type !== 'array') return []
  return getSchemaEnumOptions(props.field.schema.items)
})

/** 将当前字段值收窄为复选框组件使用的字符串数组。 */
const selectedValues = computed<string[]>(() => {
  if (!Array.isArray(props.modelValue)) return []
  return props.modelValue.filter((value): value is string => typeof value === 'string')
})

/** 将复选框变更事件转换为设置字段的字符串数组。 */
function onUpdate(value: Array<string | number>) {
  emit('update:modelValue', value.map(String))
}
</script>

<style scoped>
.multiselect-field {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.option-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 16px;
}
</style>
