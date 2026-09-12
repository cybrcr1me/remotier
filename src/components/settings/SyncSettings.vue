<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  ArrowDownIcon,
  ArrowUpIcon,
  LoaderIcon,
  RefreshCwIcon,
  Trash2Icon,
} from '@lucide/vue'

import SignInDialog from '@/components/sync/SignInDialog.vue'
import RecoveryCode from '@/components/sync/RecoveryCode.vue'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { errorMessage } from '@/lib/ipc'
import {
  entryLabel,
  entryVerb,
  instanceLabel,
  kindLabel,
  lastSyncLabel,
  summarise,
} from '@/lib/sync-status'
import { useSyncStore } from '@/stores/sync'
import { toast } from 'vue-sonner'

const sync = useSyncStore()
const signInOpen = ref(false)

const status = computed(() => sync.status)

// The engine writes the log behind the panel, so the list is reloaded when a cycle
// finishes rather than polled. `lastSyncAt` moves on every completed cycle, including one
// that pushed and pulled nothing - which is also when the list is already correct, so the
// read is cheap and never wrong.
onMounted(() => void refreshHistory())
watch([() => status.value.lastSyncAt, () => status.value.signedIn], () => void refreshHistory())

async function refreshHistory() {
  try {
    await sync.loadHistory()
  } catch {
    // A panel with no activity list is worth strictly less than one that fails to render.
  }
}

/** Matches the connection-state convention: a 6px square, never a pill or a badge. */
const squareClass = computed(() => {
  switch (sync.indicator) {
    case 'synced':
      return 'bg-[var(--rm-lime)]'
    case 'syncing':
      return 'bg-[var(--rm-warn)]'
    case 'error':
      return 'bg-[var(--rm-error)]'
    default:
      return 'bg-[var(--rm-idle)]'
  }
})

async function signOut() {
  try {
    await sync.logout()
  } catch (e) {
    toast.error('Could not sign out', { description: errorMessage(e) })
  }
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <Card v-if="status.signedIn">
      <CardHeader>
        <CardTitle>Account</CardTitle>
        <CardDescription>
          Hosts, groups, identities, placeholders, workspaces and some preferences.
        </CardDescription>
      </CardHeader>
      <CardContent class="flex flex-col gap-4">
        <div class="flex items-center gap-3">
          <span class="size-1.5 shrink-0" :class="squareClass" />
          <div class="flex min-w-0 flex-col">
            <span class="truncate text-sm">{{ status.email }}</span>
            <span class="truncate font-mono text-xs text-muted-foreground">
              {{ instanceLabel(status.instanceUrl) }}
            </span>
          </div>
          <div class="ml-auto flex items-center gap-2">
            <Button variant="outline" size="sm" :disabled="sync.busy" @click="sync.syncNow()">
              <LoaderIcon v-if="status.syncing" class="size-3.5 animate-spin" />
              <RefreshCwIcon v-else class="size-3.5" />
              Sync now
            </Button>
            <Button variant="ghost" size="sm" :disabled="sync.busy" @click="signOut">
              Sign out
            </Button>
          </div>
        </div>

        <p class="text-sm" :class="status.error ? 'text-destructive' : 'text-muted-foreground'">
          {{ summarise(status) }}
        </p>

        <p class="text-xs text-muted-foreground">
          SSH keys, key passphrases and host passwords stay on this machine. Signing out
          keeps everything here.
        </p>
      </CardContent>
    </Card>

    <Card v-if="status.signedIn">
      <CardHeader>
        <CardTitle>Activity</CardTitle>
        <CardDescription>
          The last 30 records this device sent or received. Kept here only, and cleared
          when you sign out.
        </CardDescription>
      </CardHeader>
      <CardContent class="px-0">
        <p v-if="!sync.history.length" class="px-4 text-sm text-muted-foreground">
          Nothing synced yet. Sync now to push what is waiting.
        </p>
        <ul v-else class="flex flex-col">
          <li
            v-for="(entry, index) in sync.history"
            :key="`${entry.at}-${entry.kind}-${entry.recordId}-${index}`"
            class="flex h-9 items-center gap-3 border-t px-4"
          >
            <Trash2Icon
              v-if="entry.action === 'deleted'"
              class="size-3.5 shrink-0 text-muted-foreground"
            />
            <ArrowUpIcon
              v-else-if="entry.direction === 'push'"
              class="size-3.5 shrink-0 text-muted-foreground"
            />
            <ArrowDownIcon v-else class="size-3.5 shrink-0 text-muted-foreground" />

            <span class="w-20 shrink-0 truncate text-xs text-muted-foreground">
              {{ kindLabel(entry.kind) }}
            </span>
            <span class="min-w-0 flex-1 truncate font-mono text-sm">
              {{ entryLabel(entry) }}
            </span>
            <span class="shrink-0 text-xs text-muted-foreground">{{ entryVerb(entry) }}</span>
            <span class="w-16 shrink-0 text-right text-xs text-muted-foreground">
              {{ lastSyncLabel(entry.at) }}
            </span>
          </li>
        </ul>
      </CardContent>
    </Card>

    <Empty v-else class="border">
      <EmptyHeader>
        <EmptyTitle>Not signed in</EmptyTitle>
        <EmptyDescription>
          Sync keeps your hosts and groups the same on every machine. Keys and passwords
          are never uploaded.
        </EmptyDescription>
      </EmptyHeader>
      <Button @click="signInOpen = true">Sign in</Button>
    </Empty>

    <SignInDialog v-model:open="signInOpen" />
    <RecoveryCode />
  </div>
</template>
