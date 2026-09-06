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
import { ScrollArea } from '@/components/ui/scroll-area'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import KeyDialogs from '@/components/keys/KeyDialogs.vue'
import { errorMessage, ipc } from '@/lib/ipc'
import type { AgentKey, DiscoveredKey, SshKey } from '@/lib/types'
import { useCredentialsStore } from '@/stores/credentials'
import { KeyRound, PlusIcon, TrashIcon, UploadIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'

const credentials = useCredentialsStore()
const { keys } = storeToRefs(credentials)

const systemKeys = ref<DiscoveredKey[]>([])
const agentKeys = ref<AgentKey[]>([])
const agentError = ref<string | null>(null)
const pendingDelete = ref<SshKey | null>(null)
const generateOpen = ref(false)
const importOpen = ref(false)

const SOURCE_LABEL: Record<string, string> = {
  managed: 'In vault',
  system_path: 'On disk',
  agent: 'Agent',
}

async function loadExternal() {
  try {
    systemKeys.value = await ipc.scanSystemKeys()
  } catch (e) {
    toast.error('Could not scan ~/.ssh', { description: errorMessage(e) })
  }

  try {
    agentKeys.value = await ipc.listAgentKeys()
    agentError.value = null
  } catch (e) {
    // No agent running is ordinary, not a failure worth a toast.
    agentError.value = errorMessage(e)
    agentKeys.value = []
  }
}

async function adopt(key: DiscoveredKey) {
  try {
    await ipc.registerSystemKey({
      label: key.path.split('/').pop() ?? key.path,
      path: key.path,
      publicKey: key.openssh,
      algorithm: key.algorithm,
      fingerprint: key.fingerprint,
      comment: key.comment || null,
    })
    await credentials.load()
    toast.success('Key added', { description: 'The private key stays where it is on disk.' })
  } catch (e) {
    toast.error('Could not add the key', { description: errorMessage(e) })
  }
}

async function confirmDelete() {
  const key = pendingDelete.value
  if (!key) return
  pendingDelete.value = null
  try {
    await credentials.deleteKey(key.id)
  } catch (e) {
    toast.error('Could not delete the key', { description: errorMessage(e) })
  }
}

onMounted(loadExternal)
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 items-center gap-2 border-b p-3">
      <p class="text-sm text-muted-foreground">
        Keys Remotier holds, plus the ones already on this machine.
      </p>
      <div class="ml-auto flex gap-2">
        <Button variant="secondary" size="sm" @click="importOpen = true">
          <UploadIcon data-icon="inline-start" />
          Import
        </Button>
        <Button size="sm" @click="generateOpen = true">
          <PlusIcon data-icon="inline-start" />
          Generate
        </Button>
      </div>
    </div>

    <Tabs default-value="repository" class="min-h-0 flex-1">
      <TabsList class="mx-3 mt-3">
        <TabsTrigger value="repository">Repository</TabsTrigger>
        <TabsTrigger value="system">On this machine</TabsTrigger>
        <TabsTrigger value="agent">ssh-agent</TabsTrigger>
      </TabsList>

      <TabsContent value="repository" class="min-h-0">
        <ScrollArea v-if="keys.length" class="h-full">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Algorithm</TableHead>
                <TableHead>Stored</TableHead>
                <TableHead>Fingerprint</TableHead>
                <TableHead class="w-16" />
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-for="key in keys" :key="key.id">
                <TableCell class="font-medium">{{ key.label }}</TableCell>
                <TableCell class="text-muted-foreground">{{ key.algorithm }}</TableCell>
                <TableCell>
                  <Badge variant="secondary">{{ SOURCE_LABEL[key.source] ?? key.source }}</Badge>
                  <Badge v-if="key.hasPassphrase" variant="secondary">Passphrase</Badge>
                </TableCell>
                <TableCell class="font-mono text-xs break-all">{{ key.fingerprint }}</TableCell>
                <TableCell>
                  <Button
                    variant="ghost"
                    size="icon"
                    :aria-label="`Delete ${key.label}`"
                    @click="pendingDelete = key"
                  >
                    <TrashIcon />
                  </Button>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </ScrollArea>

        <Empty v-else class="h-full">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <KeyRound />
            </EmptyMedia>
            <EmptyTitle>No keys yet</EmptyTitle>
            <EmptyDescription>
              Generate one, import an existing key, or adopt a key from this machine.
            </EmptyDescription>
          </EmptyHeader>
          <Button @click="generateOpen = true">Generate a key</Button>
        </Empty>
      </TabsContent>

      <TabsContent value="system" class="min-h-0">
        <ScrollArea class="h-full">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Path</TableHead>
                <TableHead>Algorithm</TableHead>
                <TableHead>Fingerprint</TableHead>
                <TableHead class="w-24" />
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-for="key in systemKeys" :key="key.path">
                <TableCell class="font-mono text-xs">{{ key.path }}</TableCell>
                <TableCell class="text-muted-foreground">
                  {{ key.algorithm }}
                  <Badge v-if="key.encrypted" variant="secondary">Encrypted</Badge>
                  <Badge v-if="!key.privateKeyPresent" variant="secondary">Public only</Badge>
                </TableCell>
                <TableCell class="font-mono text-xs break-all">{{ key.fingerprint }}</TableCell>
                <TableCell>
                  <Button
                    size="sm"
                    variant="secondary"
                    :disabled="!key.privateKeyPresent || keys.some(k => k.fingerprint === key.fingerprint)"
                    @click="adopt(key)"
                  >
                    {{ keys.some(k => k.fingerprint === key.fingerprint) ? 'Added' : 'Use this' }}
                  </Button>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
          <p v-if="!systemKeys.length" class="p-6 text-center text-sm text-muted-foreground">
            No key pairs found in ~/.ssh.
          </p>
        </ScrollArea>
      </TabsContent>

      <TabsContent value="agent" class="min-h-0">
        <ScrollArea class="h-full">
          <Table v-if="agentKeys.length">
            <TableHeader>
              <TableRow>
                <TableHead>Comment</TableHead>
                <TableHead>Algorithm</TableHead>
                <TableHead>Fingerprint</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-for="key in agentKeys" :key="key.fingerprint">
                <TableCell class="font-medium">{{ key.comment || '—' }}</TableCell>
                <TableCell class="text-muted-foreground">{{ key.algorithm }}</TableCell>
                <TableCell class="font-mono text-xs break-all">{{ key.fingerprint }}</TableCell>
              </TableRow>
            </TableBody>
          </Table>
          <p v-else class="p-6 text-center text-sm text-muted-foreground">
            {{ agentError ?? 'The ssh-agent is not holding any keys.' }}
          </p>
        </ScrollArea>
      </TabsContent>
    </Tabs>

    <KeyDialogs
      v-model:generate-open="generateOpen"
      v-model:import-open="importOpen"
    />

    <AlertDialog :open="pendingDelete !== null">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete key?</AlertDialogTitle>
          <AlertDialogDescription>
            <template v-if="pendingDelete?.source === 'managed'">
              “{{ pendingDelete.label }}” is stored in the vault and will be permanently lost.
            </template>
            <template v-else>
              “{{ pendingDelete?.label }}” is only removed from Remotier. The file on disk is
              left untouched.
            </template>
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
