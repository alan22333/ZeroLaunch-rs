<template>
  <div class="plugin-panel-host" data-no-drag>
    <component
      :is="panelComponent"
      v-if="panelComponent"
      v-bind="panelProps"
    />
    <div v-else class="fallback-panel">
      <n-text depth="3">{{ $t('pluginPanel.unavailable.title') }}</n-text>
      <n-text depth="2" class="fallback-desc">{{ $t('pluginPanel.unavailable.desc') }}</n-text>
      <n-text depth="3" class="fallback-type">{{ $t('pluginPanel.unavailable.type', { type: panelType }) }}</n-text>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { NText } from 'naive-ui'
import { usePluginStore } from '@/stores/plugin-store'
import { useSearchStore } from '@/stores/search-store'

const props = defineProps<{
  panelType: string
  panelData: unknown
}>()

const pluginStore = usePluginStore()
const searchStore = useSearchStore()

const panelComponent = computed(() =>
  pluginStore.getPanelComponent(props.panelType),
)

const panelProps = computed(() => ({
  data: props.panelData,
  actions: searchStore.panelActions,
}))
</script>

<style scoped>
.plugin-panel-host {
  flex: 1;
  min-height: 0;
  max-height: 420px; /* 默认最大高度 */
  overflow-y: auto;
  padding: 0;
}

.fallback-panel {
  padding: 16px;
  margin: 8px 16px;
  border-radius: var(--radius-sm);
  background: var(--bg-secondary);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.fallback-type {
  font-size: var(--font-size-sm);
  color: var(--text-secondary);
}
</style>
