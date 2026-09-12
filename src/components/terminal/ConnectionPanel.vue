<script setup lang="ts">
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyContent,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Spinner } from '@/components/ui/spinner'
import { formatTime, type LogEntry } from '@/lib/connection-log'
import { cn } from '@/lib/utils'
import { PlugZapIcon, TriangleAlertIcon } from '@lucide/vue'
import { computed } from 'vue'

const props = defineProps<{
  status: 'idle' | 'connecting' | 'connected' | 'checking' | 'lost' | 'error'
  entries: LogEntry[]
  error: string | null
  target: string | null
}>()

const emit = defineEmits<{ retry: [] }>()

const title = computed(() => {
  if (props.status === 'connecting') return 'Connecting…'
  if (props.status === 'checking') return 'Checking the connection…'
  if (props.status === 'lost') return 'Connection lost'
  if (props.status === 'error') return 'Could not connect'
  return 'Not connected'
})

const description = computed(() => {
  // The backend's watchdog says why: a suspend it can measure, or a server that stopped
  // answering. It knows which; this does not need to guess.
  if (props.status === 'lost') return props.error ?? 'The connection is gone.'
  if (props.status === 'error' && props.error) return props.error
  if (props.target) return props.target
  return 'This pane is not connected yet.'
})

/** Nothing to do but wait while a connection is being made or verified. */
const busy = computed(() => props.status === 'connecting' || props.status === 'checking')

/**
 * True for an action line that is still the last thing to have happened.
 *
 * Anything after it means the user already did the thing, so the spinner stops even though
 * the line stays in the log.
 */
function waitingAt(index: number): boolean {
  return (
    props.entries[index]?.level === 'action'
    && index === props.entries.length - 1
    && props.status !== 'error'
  )
}

const action = computed(() => {
  if (props.status === 'lost') return 'Reconnect'
  if (props.status === 'error') return 'Try again'
  return 'Connect'
})
</script>

<template>
  <!--
    z-10 keeps the panel above the terminal beneath it: xterm layers itself up to
    z-index 10 and would otherwise swallow every click.
  -->
  <div class="absolute inset-0 z-10 flex items-center justify-center overflow-y-auto bg-background/95 p-6">
    <div class="flex w-full max-w-md flex-col items-center gap-4">
      <Empty class="flex-none border-none p-0">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <Spinner v-if="busy" />
            <TriangleAlertIcon v-else-if="props.status === 'error' || props.status === 'lost'" />
            <PlugZapIcon v-else />
          </EmptyMedia>

          <EmptyTitle>{{ title }}</EmptyTitle>
          <EmptyDescription
            :class="cn(
              props.status === 'error' && 'text-destructive',
              props.status !== 'error' && props.status !== 'lost' && props.target && 'font-mono text-xs',
            )"
          >
            {{ description }}
          </EmptyDescription>
        </EmptyHeader>

        <EmptyContent v-if="!busy">
          <Button size="sm" variant="secondary" @click="emit('retry')">
            <PlugZapIcon data-icon="inline-start" />
            {{ action }}
          </Button>
        </EmptyContent>
      </Empty>

        <!--
        A definite height, not `max-h-*`: the viewport inside is `size-full`, so without one
        it has nothing to resolve against, never clips, and the log spills past the border
        instead of scrolling. A fixed box also stops the panel resizing on every progress
        line while a connection is being made.
      -->
    <ScrollArea v-if="props.entries.length" class="h-48 w-full rounded-md border bg-muted/40">
        <ul class="flex flex-col gap-1 p-3 text-left">
          <li
            v-for="(entry, index) in props.entries"
            :key="`${entry.at}-${index}`"
            class="flex gap-2 font-mono text-xs"
          >
            <span class="shrink-0 text-muted-foreground">{{ formatTime(entry.at) }}</span>
            <!--
              The spinner marks a step that is still waiting on the user. It is dropped
              once another line follows, because by then the wait is over and a spinner on
              a finished step is a lie.
            -->
            <Spinner v-if="waitingAt(index)" class="mt-0.5 size-3 shrink-0 text-primary" />
            <span
              :class="cn(
                'break-all',
                entry.level === 'error' && 'text-destructive',
                entry.level === 'action' && 'text-primary',
              )"
            >
              {{ entry.message }}
            </span>
          </li>
        </ul>
      </ScrollArea>
    </div>
  </div>
</template>
