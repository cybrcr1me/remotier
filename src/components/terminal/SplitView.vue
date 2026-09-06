<script setup lang="ts">
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable'
import type { HostKeyPrompt } from '@/lib/connect-flow'
import { isPane, type LayoutNode } from '@/lib/layout'
import { cn } from '@/lib/utils'
import { useSessionsStore } from '@/stores/sessions'
import TerminalPane from './TerminalPane.vue'

// Recursive components need an explicit name to refer to themselves in their template.
defineOptions({ name: 'SplitView' })

const props = defineProps<{
  node: LayoutNode
  tabId: string
  activePaneId: string
  /** False while this tab is hidden behind another. */
  tabActive: boolean
}>()

const sessions = useSessionsStore()

const emit = defineEmits<{
  focusPane: [paneId: string]
  resize: [splitId: string, sizes: number[]]
  hostKey: [prompt: HostKeyPrompt, decide: (choice: 'reject' | 'once' | 'save') => void]
  variables: [names: string[], decide: (values: Record<string, string> | null) => void]
  password: [
    prompt: { username: string, host: string },
    decide: (answer: { password: string, remember: boolean } | null) => void,
  ]
}>()
</script>

<template>
  <TerminalPane
    v-if="isPane(props.node)"
    :key="props.node.id"
    :tab-id="props.tabId"
    :pane-id="props.node.id"
    :host-id="props.node.hostId"
    :session-id="props.node.sessionId"
    :active="props.node.id === props.activePaneId"
    :tab-active="props.tabActive"
    :auto-connect="sessions.shouldAutoConnect(props.node.id)"
    :class="cn(
      'h-full',
      props.node.id === props.activePaneId && 'ring-1 ring-ring',
    )"
    @focus="emit('focusPane', props.node.id)"
    @host-key="(prompt, decide) => emit('hostKey', prompt, decide)"
    @variables="(names, decide) => emit('variables', names, decide)"
    @password="(prompt, decide) => emit('password', prompt, decide)"
  />

  <ResizablePanelGroup
    v-else
    :direction="props.node.dir === 'row' ? 'horizontal' : 'vertical'"
    @layout="sizes => emit('resize', props.node.id, sizes)"
  >
    <template v-for="(child, index) in props.node.children" :key="child.id">
      <ResizableHandle v-if="index > 0" with-handle />
      <ResizablePanel :default-size="props.node.sizes[index]">
        <SplitView
          :node="child"
          :tab-id="props.tabId"
          :active-pane-id="props.activePaneId"
          :tab-active="props.tabActive"
          @focus-pane="paneId => emit('focusPane', paneId)"
          @resize="(splitId, sizes) => emit('resize', splitId, sizes)"
          @host-key="(prompt, decide) => emit('hostKey', prompt, decide)"
          @variables="(names, decide) => emit('variables', names, decide)"
          @password="(prompt, decide) => emit('password', prompt, decide)"
        />
      </ResizablePanel>
    </template>
  </ResizablePanelGroup>
</template>
