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
import VariablePrompt from '@/components/terminal/VariablePrompt.vue'
import TabBar from '@/components/terminal/TabBar.vue'
import type {
  HostKeyDecision,
  HostKeyPrompt,
  PasswordPrompt as PasswordPromptData,
} from '@/lib/connect-flow'
import { useTerminalShortcuts } from '@/lib/shortcuts'
import type { SessionEvent } from '@/lib/types'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { listen } from '@tauri-apps/api/event'
import { TerminalIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'

const sessions = useSessionsStore()
const inventory = useInventoryStore()
const { activeTab } = storeToRefs(sessions)

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

  sessions.setPaneHost(tab.id, pane.id, hostId)
  if (host && tab.name === 'New tab') sessions.renameTab(tab.id, host.label)
}

function newTab() {
  sessions.openTab()
  quickConnectOpen.value = true
}

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
})

let unlisten: (() => void) | null = null

onMounted(async () => {
  unlisten = await listen<SessionEvent>('ssh://session', ({ payload }) => {
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

    <div v-if="activeTab" class="min-h-0 flex-1">
      <SplitView
        :key="activeTab.id"
        :node="activeTab.layout"
        :tab-id="activeTab.id"
        :active-pane-id="activeTab.activePaneId"
        @focus-pane="sessions.focusPane"
        @resize="(splitId, sizes) => sessions.setSizes(activeTab!.id, splitId, sizes)"
        @host-key="askAboutHostKey"
        @variables="askAboutVariables"
        @password="askForPassword"
      />
    </div>

    <Empty v-else class="flex-1">
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
