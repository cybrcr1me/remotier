<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import { useSessionsStore } from '@/stores/sessions'
import WorkspaceMenu from './WorkspaceMenu.vue'
import { PlusIcon, XIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'

const sessions = useSessionsStore()
const { tabs, activeTabId } = storeToRefs(sessions)

const emit = defineEmits<{ newTab: [] }>()
</script>

<template>
  <div class="flex h-9 shrink-0 items-stretch gap-1 border-b px-1">
    <div class="flex min-w-0 flex-1 items-stretch gap-1 overflow-x-auto">
      <div
        v-for="tab in tabs"
        :key="tab.id"
        :class="cn(
          'group flex min-w-32 max-w-52 shrink-0 cursor-default items-center gap-1 rounded-md px-2 text-sm',
          tab.id === activeTabId
            ? 'bg-accent text-accent-foreground'
            : 'text-muted-foreground hover:bg-accent/50',
        )"
        @mousedown="sessions.focusTab(tab.id)"
      >
        <span class="truncate">{{ tab.name }}</span>
        <Button
          variant="ghost"
          size="icon"
          class="ml-auto size-5 opacity-0 group-hover:opacity-100"
          :aria-label="`Close ${tab.name}`"
          @click.stop="sessions.closeTab(tab.id)"
        >
          <XIcon />
        </Button>
      </div>
    </div>

    <Button
      variant="ghost"
      size="icon"
      class="size-7 self-center"
      aria-label="New tab"
      @click="emit('newTab')"
    >
      <PlusIcon />
    </Button>

    <WorkspaceMenu />
  </div>
</template>
