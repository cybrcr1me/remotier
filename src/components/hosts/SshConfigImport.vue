<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { errorMessage, ipc } from '@/lib/ipc'
import type { ConfigHost } from '@/lib/types'
import { useInventoryStore } from '@/stores/inventory'
import { ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })

const inventory = useInventoryStore()
const available = ref<ConfigHost[]>([])
const selected = ref(new Set<string>())
const busy = ref(false)

watch(open, async (isOpen) => {
  if (!isOpen) return
  busy.value = true
  try {
    available.value = await ipc.previewSshConfig()
    // Everything is pre-selected; unticking a few is less work than ticking many.
    selected.value = new Set(available.value.map(host => host.alias))
  } catch (e) {
    toast.error('Could not read ~/.ssh/config', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
})

function toggle(alias: string) {
  const next = new Set(selected.value)
  if (next.has(alias)) next.delete(alias)
  else next.add(alias)
  selected.value = next
}

async function runImport() {
  busy.value = true
  try {
    const count = await ipc.importSshConfig([...selected.value])
    await inventory.load()
    toast.success(
      count === 1 ? 'Imported 1 host' : `Imported ${count} hosts`,
      { description: count < selected.value.size ? 'Hosts that already existed were skipped.' : undefined },
    )
    open.value = false
  } catch (e) {
    toast.error('Import failed', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="max-w-2xl">
      <DialogHeader>
        <DialogTitle>Import from ~/.ssh/config</DialogTitle>
        <DialogDescription>
          Your config file is not modified. Wildcard and Match blocks are skipped, and hosts
          you already have are left alone.
        </DialogDescription>
      </DialogHeader>

      <ScrollArea class="max-h-96">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead class="w-10" />
              <TableHead>Alias</TableHead>
              <TableHead>Hostname</TableHead>
              <TableHead>User</TableHead>
              <TableHead>Port</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            <TableRow v-for="host in available" :key="host.alias">
              <TableCell>
                <Checkbox
                  :model-value="selected.has(host.alias)"
                  :aria-label="`Import ${host.alias}`"
                  @update:model-value="toggle(host.alias)"
                />
              </TableCell>
              <TableCell class="font-medium">{{ host.alias }}</TableCell>
              <TableCell class="text-muted-foreground">{{ host.hostname }}</TableCell>
              <TableCell class="text-muted-foreground">{{ host.user ?? '—' }}</TableCell>
              <TableCell class="text-muted-foreground">{{ host.port ?? '22' }}</TableCell>
            </TableRow>
          </TableBody>
        </Table>

        <p v-if="!available.length && !busy" class="p-6 text-center text-sm text-muted-foreground">
          No importable hosts found in ~/.ssh/config.
        </p>
      </ScrollArea>

      <DialogFooter>
        <Button variant="ghost" @click="open = false">Cancel</Button>
        <Button :disabled="busy || selected.size === 0" @click="runImport">
          Import {{ selected.size }} host{{ selected.size === 1 ? '' : 's' }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
