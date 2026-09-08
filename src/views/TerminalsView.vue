<script setup lang="ts">
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import QuickConnect from '@/components/terminal/QuickConnect.vue'
import SplitView from '@/components/terminal/SplitView.vue'
import PasswordPrompt from '@/components/terminal/PasswordPrompt.vue'
import PinPrompt from '@/components/terminal/PinPrompt.vue'
import VariablePrompt from '@/components/terminal/VariablePrompt.vue'
import TabBar from '@/components/terminal/TabBar.vue'
import type {
  HostKeyDecision,
  HostKeyPrompt,
  PasswordPrompt as PasswordPromptData,
} from '@/lib/connect-flow'
import type { DropZone } from '@/lib/dnd'
import { dragging, endDrag } from '@/lib/drag'
import { markLost, releaseMissing } from '@/lib/terminal-registry'
import { useTerminalShortcuts } from '@/lib/shortcuts'
import type { SessionEvent } from '@/lib/types'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { listen } from '@tauri-apps/api/event'
import { TerminalIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const sessions = useSessionsStore()
const inventory = useInventoryStore()
const { activeTab, activeTabId, tabs } = storeToRefs(sessions)

const quickConnectOpen = ref(false)
const hostKeyPrompt = ref<HostKeyPrompt | null>(null)
let hostKeyDecide: ((choice: HostKeyDecision) => void) | null = null

function askAboutHostKey(prompt: HostKeyPrompt, decide: (choice: HostKeyDecision) => void) {
  hostKeyPrompt.value = prompt
  hostKeyDecide = decide
}

type PasswordAnswer = { password: string, remember: boolean } | null

const passwordPrompt = ref<PasswordPromptData | null>(null)
let passwordDecide: ((answer: PasswordAnswer) => void) | null = null

function askForPassword(
  prompt: PasswordPromptData,
  decide: (answer: PasswordAnswer) => void,
) {
  passwordPrompt.value = prompt
  passwordDecide = decide
}

function answerPassword(answer: PasswordAnswer) {
  passwordPrompt.value = null
  passwordDecide?.(answer)
  passwordDecide = null
}

const pinOpen = ref(false)
let pinDecide: ((pin: string | null) => void) | null = null

function askForPin(decide: (pin: string | null) => void) {
  pinOpen.value = true
  pinDecide = decide
}

function answerPin(pin: string | null) {
  pinOpen.value = false
  pinDecide?.(pin)
  pinDecide = null
}

const variableNames = ref<string[]>([])
let variablesDecide: ((values: Record<string, string> | null) => void) | null = null

function askAboutVariables(
  names: string[],
  decide: (values: Record<string, string> | null) => void,
) {
  variableNames.value = names
  variablesDecide = decide
}

function answerVariables(values: Record<string, string> | null) {
  variableNames.value = []
  variablesDecide?.(values)
  variablesDecide = null
}

function answerHostKey(choice: HostKeyDecision) {
  hostKeyPrompt.value = null
  hostKeyDecide?.(choice)
  hostKeyDecide = null
}

function openHost(hostId: string) {
  const host = inventory.hostById.get(hostId)
  const tab = sessions.activeTab ?? sessions.openTab(host?.label ?? 'Session')

  const pane = sessions.activePane
  if (!pane) return

  // No renaming: a tab's title is derived from the panes it holds, so naming it after the
  // first host would only leave a stale label behind once that pane moved or changed host.
  sessions.setPaneHost(tab.id, pane.id, hostId)
}

function newTab() {
  sessions.openTab()
  quickConnectOpen.value = true
}

/**
 * Apply a drop onto a pane.
 *
 * A centre drop means "put it here", which for a tab is a plain reorder to the end of the
 * bar rather than a split - splitting a pane with something dropped in its middle is the
 * one arrangement nobody means.
 */
function onDropOnPane(targetTabId: string, targetPaneId: string, zone: DropZone) {
  const payload = dragging.value
  endDrag()
  if (!payload) return

  if (payload.kind === 'tab') {
    if (zone === 'center') sessions.focusTab(payload.tabId)
    else sessions.mergeTabInto(payload.tabId, targetTabId, targetPaneId, zone)
    return
  }

  if (zone !== 'center') sessions.movePane(payload.paneId, targetPaneId, zone)
}

/*
 * Dispose the terminals of panes that no longer exist anywhere.
 *
 * A pane's component unmounts both when it is closed and when it is dragged elsewhere, so
 * the component cannot tell those apart - it deliberately disposes nothing. The layout is
 * the only thing that knows, so cleanup is driven from here.
 */
watch(
  // Joined, because `allPaneIds` builds a fresh array each time and would otherwise
  // report a change on every keystroke that touches the layout.
  () => sessions.allPaneIds().join('\u0000'),
  () => releaseMissing(sessions.allPaneIds()),
  { flush: 'post' },
)

useTerminalShortcuts({
  newTab,
  quickConnect: () => {
    quickConnectOpen.value = true
  },
  splitRow: () => sessions.splitActivePane('row'),
  splitCol: () => sessions.splitActivePane('col'),
  closePane: () => {
    const tab = sessions.activeTab
    const pane = sessions.activePane
    if (tab && pane) void sessions.closePane(tab.id, pane.id)
  },
  nextPane: () => sessions.focusRelativePane(1),
  previousPane: () => sessions.focusRelativePane(-1),
  moveTabLeft: () => sessions.moveActiveTab(-1),
  moveTabRight: () => sessions.moveActiveTab(1),
})

let unlisten: (() => void) | null = null

onMounted(async () => {
  unlisten = await listen<SessionEvent>('ssh://session', ({ payload }) => {
    // A lost connection is the backend's watchdog reporting that a round trip failed, so
    // the pane says so and offers to reconnect rather than quietly going blank. Marked
    // before detaching: detaching is what clears the pane's session.
    if (payload.kind === 'lost') {
      const paneId = sessions.paneIdForSession(payload.sessionId)
      if (paneId) markLost(paneId, payload.message)
    }

    sessions.detachSession(payload.sessionId)

    if (payload.kind === 'failed') {
      toast.error('Session ended', { description: payload.message })
    }
  })
})

onBeforeUnmount(() => unlisten?.())
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <TabBar @new-tab="newTab" />

    <!--
      Every tab stays mounted and is merely hidden. Tearing one down would dispose its
      terminal, which drops the IPC channel and ends the SSH session behind it - so
      switching tabs would silently disconnect you.
    -->
    <div
      v-for="tab in tabs"
      v-show="tab.id === activeTabId"
      :key="tab.id"
      class="min-h-0 flex-1"
    >
      <SplitView
        :node="tab.layout"
        :tab-id="tab.id"
        :active-pane-id="tab.activePaneId"
        :tab-active="tab.id === activeTabId"
        @focus-pane="sessions.focusPane"
        @drop-on-pane="(paneId, zone) => onDropOnPane(tab.id, paneId, zone)"
        @resize="(splitId, sizes) => sessions.setSizes(tab.id, splitId, sizes)"
        @host-key="askAboutHostKey"
        @variables="askAboutVariables"
        @password="askForPassword"
        @pin="askForPin"
      />
    </div>

    <Empty v-if="!activeTab" class="flex-1">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <TerminalIcon />
        </EmptyMedia>
        <EmptyTitle>No open sessions</EmptyTitle>
        <EmptyDescription>Press ⌘K to connect to a host.</EmptyDescription>
      </EmptyHeader>
      <Button variant="secondary" @click="newTab">
        New session
      </Button>
    </Empty>

    <QuickConnect v-model:open="quickConnectOpen" @select="openHost" />

    <VariablePrompt
      :names="variableNames"
      @submit="answerVariables"
      @cancel="answerVariables(null)"
    />

    <PinPrompt :open="pinOpen" @submit="answerPin" @cancel="answerPin(null)" />

    <PasswordPrompt
      :prompt="passwordPrompt"
      @submit="(password, remember) => answerPassword({ password, remember })"
      @cancel="answerPassword(null)"
    />

    <AlertDialog :open="hostKeyPrompt !== null">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Unrecognised host key</AlertDialogTitle>
          <AlertDialogDescription>
            {{ hostKeyPrompt?.host }} presented a key that is not in your known_hosts file.
            Check the fingerprint matches the server before continuing.
          </AlertDialogDescription>
        </AlertDialogHeader>

        <p class="rounded-md bg-muted px-3 py-2 font-mono text-xs break-all">
          {{ hostKeyPrompt?.fingerprint }}
        </p>

        <AlertDialogFooter>
          <AlertDialogCancel @click="answerHostKey('reject')">
            Cancel
          </AlertDialogCancel>
          <Button variant="secondary" @click="answerHostKey('once')">
            Connect once
          </Button>
          <AlertDialogAction @click="answerHostKey('save')">
            Trust and save
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
