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
import PageBody from '@/components/layout/PageBody.vue'
import ViewToolbar from '@/components/layout/ViewToolbar.vue'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import IdentityEditor from '@/components/keys/IdentityEditor.vue'
import { errorMessage } from '@/lib/ipc'
import type { Identity } from '@/lib/types'
import { useCredentialsStore } from '@/stores/credentials'
import { MonitorCog, PlusIcon, TrashIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { ref } from 'vue'
import { toast } from 'vue-sonner'

const credentials = useCredentialsStore()
const { identities, keys } = storeToRefs(credentials)

const editorOpen = ref(false)
const editing = ref<Identity | null>(null)
const pendingDelete = ref<Identity | null>(null)

const AUTH_LABEL: Record<string, string> = {
  password: 'Password',
  key: 'Key',
  agent: 'ssh-agent',
  interactive: 'Keyboard interactive',
}

function create() {
  editing.value = null
  editorOpen.value = true
}

function edit(identity: Identity) {
  editing.value = identity
  editorOpen.value = true
}

async function confirmDelete() {
  const identity = pendingDelete.value
  if (!identity) return
  pendingDelete.value = null
  try {
    await credentials.deleteIdentity(identity.id)
  } catch (e) {
    toast.error('Could not delete the identity', { description: errorMessage(e) })
  }
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <ViewToolbar>
      <p class="text-sm text-muted-foreground">
        Reusable username and credential pairs. Hosts and groups point at these.
      </p>
      <Button size="sm" class="ml-auto" @click="create">
        <PlusIcon data-icon="inline-start" />
        Identity
      </Button>
    </ViewToolbar>

    <PageBody v-if="identities.length">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Username</TableHead>
            <TableHead>Authentication</TableHead>
            <TableHead>Secret</TableHead>
            <TableHead class="w-16" />
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow
            v-for="identity in identities"
            :key="identity.id"
            class="cursor-default"
            @click="edit(identity)"
          >
            <TableCell class="font-medium">{{ identity.label }}</TableCell>
            <TableCell class="font-mono text-xs">{{ identity.username }}</TableCell>
            <TableCell class="text-muted-foreground">
              {{ AUTH_LABEL[identity.authKind] ?? identity.authKind }}
            </TableCell>
            <TableCell>
              <Badge v-if="identity.hasPassword" variant="secondary">Password stored</Badge>
              <Badge v-else-if="identity.keyId" variant="secondary">
                {{ keys.find(k => k.id === identity.keyId)?.label ?? 'Key' }}
              </Badge>
              <span v-else class="text-xs text-muted-foreground">—</span>
            </TableCell>
            <TableCell>
              <Button
                variant="ghost"
                size="icon"
                :aria-label="`Delete ${identity.label}`"
                @click.stop="pendingDelete = identity"
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
          <MonitorCog />
        </EmptyMedia>
        <EmptyTitle>No identities</EmptyTitle>
        <EmptyDescription>
          An identity holds a username and how to authenticate with it.
        </EmptyDescription>
      </EmptyHeader>
      <Button @click="create">Create an identity</Button>
    </Empty>

    <IdentityEditor v-model:open="editorOpen" :identity="editing" />

    <AlertDialog :open="pendingDelete !== null">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete identity?</AlertDialogTitle>
          <AlertDialogDescription>
            “{{ pendingDelete?.label }}” and any stored password will be removed. Hosts using
            it will fall back to inheriting or the ssh-agent.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel @click="pendingDelete = null">Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" @click="confirmDelete">Delete</AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
