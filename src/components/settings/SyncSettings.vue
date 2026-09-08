<script setup lang="ts">
import { computed, ref } from 'vue'
import { LoaderIcon, RefreshCwIcon } from '@lucide/vue'

import SignInDialog from '@/components/sync/SignInDialog.vue'
import RecoveryCode from '@/components/sync/RecoveryCode.vue'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { errorMessage } from '@/lib/ipc'
import { instanceLabel, summarise } from '@/lib/sync-status'
import { useSyncStore } from '@/stores/sync'
import { toast } from 'vue-sonner'

const sync = useSyncStore()
const signInOpen = ref(false)

const status = computed(() => sync.status)

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
