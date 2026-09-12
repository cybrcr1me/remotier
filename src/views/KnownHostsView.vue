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
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import PageBody from '@/components/layout/PageBody.vue'
import ViewToolbar from '@/components/layout/ViewToolbar.vue'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { errorMessage, ipc } from '@/lib/ipc'
import type { KnownHostEntry } from '@/lib/types'
import { ShieldCheck, TrashIcon } from '@lucide/vue'
import { computed, onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'

const entries = ref<KnownHostEntry[]>([])
const search = ref('')
const pendingRevoke = ref<KnownHostEntry | null>(null)

const visible = computed(() => {
  const query = search.value.trim().toLowerCase()
  if (!query) return entries.value
  return entries.value.filter(entry =>
    `${entry.host ?? ''} ${entry.fingerprint} ${entry.algorithm}`.toLowerCase().includes(query),
  )
})

async function load() {
  try {
    entries.value = await ipc.listKnownHosts()
  } catch (e) {
    toast.error('Could not read known_hosts', { description: errorMessage(e) })
  }
}

async function revoke() {
  const entry = pendingRevoke.value
  if (!entry) return
  pendingRevoke.value = null

  try {
    await ipc.revokeKnownHost(entry.line)
    await load()
    toast.success('Host key removed', {
      description: 'You will be asked to verify it again on the next connection.',
    })
  } catch (e) {
    toast.error('Could not remove the entry', { description: errorMessage(e) })
  }
}

onMounted(load)
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <ViewToolbar>
      <Input v-model="search" placeholder="Search hosts or fingerprints…" class="max-w-sm" />
      <p class="ml-auto text-xs text-muted-foreground">
        Shared with OpenSSH: ~/.ssh/known_hosts
      </p>
    </ViewToolbar>

    <PageBody v-if="visible.length">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Host</TableHead>
            <TableHead>Algorithm</TableHead>
            <TableHead>Fingerprint</TableHead>
            <TableHead class="w-16" />
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-for="entry in visible" :key="entry.line">
            <TableCell class="font-medium">
              <span v-if="entry.host">{{ entry.host }}</span>
              <Badge v-else variant="secondary">hashed</Badge>
            </TableCell>
            <TableCell class="text-muted-foreground">{{ entry.algorithm }}</TableCell>
            <TableCell class="font-mono text-xs break-all">{{ entry.fingerprint }}</TableCell>
            <TableCell>
              <Button
                variant="ghost"
                size="icon"
                :aria-label="`Remove entry on line ${entry.line}`"
                @click="pendingRevoke = entry"
              >
                <TrashIcon />
              </Button>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </PageBody>

    <Empty v-else class="flex-1">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <ShieldCheck />
        </EmptyMedia>
        <EmptyTitle>{{ search ? 'Nothing matches' : 'No known hosts' }}</EmptyTitle>
        <EmptyDescription>
          Host keys are recorded here the first time you trust a server.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>

    <AlertDialog :open="pendingRevoke !== null">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove this host key?</AlertDialogTitle>
          <AlertDialogDescription>
            This edits ~/.ssh/known_hosts, which the ssh command also uses. You will be asked
            to verify {{ pendingRevoke?.host ?? 'this host' }} again on the next connection.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <p class="rounded-md bg-muted px-3 py-2 font-mono text-xs break-all">
          {{ pendingRevoke?.fingerprint }}
        </p>
        <AlertDialogFooter>
          <AlertDialogCancel @click="pendingRevoke = null">Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" @click="revoke">Remove</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
