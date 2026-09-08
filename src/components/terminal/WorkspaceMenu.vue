<script setup lang="ts">
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Field, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { errorMessage, ipc } from '@/lib/ipc'
import { lastSyncLabel } from '@/lib/sync-status'
import type { DeviceLayout, Workspace } from '@/lib/types'
import { useSessionsStore } from '@/stores/sessions'
import { useSyncStore } from '@/stores/sync'
import { LaptopIcon, LayoutGridIcon, SaveIcon, TrashIcon } from '@lucide/vue'
import { onMounted, ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const sessions = useSessionsStore()
const sync = useSyncStore()

const workspaces = ref<Workspace[]>([])
const devices = ref<DeviceLayout[]>([])
const saveOpen = ref(false)
const name = ref('')
const busy = ref(false)

async function load() {
  try {
    workspaces.value = await ipc.listWorkspaces()
  } catch (e) {
    toast.error('Could not load workspaces', { description: errorMessage(e) })
  }

  // Signed out, or nothing else has synced a layout: the section simply does not appear.
  try {
    devices.value = await sync.devices()
  } catch (e) {
    console.warn('could not list devices', errorMessage(e))
    devices.value = []
  }
}

/**
 * Open another machine's tabs here.
 *
 * Never automatic. A layout arriving in the background and replacing what is on screen
 * is the reason `apply` stores these rather than applying them.
 */
async function openDevice(device: DeviceLayout) {
  try {
    const layout = await sync.deviceLayout(device.deviceId)
    await sessions.applyWorkspace(layout)
    toast.success(`Opened tabs from ${device.name}`, {
      description: 'Panes are ready to connect.',
    })
  } catch (e) {
    toast.error(`Could not open the layout from ${device.name}`, {
      description: errorMessage(e),
    })
  }
}

async function save() {
  if (!name.value.trim()) return
  busy.value = true
  try {
    await ipc.saveWorkspace(name.value.trim(), sessions.currentLayoutJson())
    await load()
    saveOpen.value = false
    name.value = ''
    toast.success('Workspace saved')
  } catch (e) {
    toast.error('Could not save the workspace', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}

async function apply(workspace: Workspace) {
  try {
    await sessions.applyWorkspace(workspace.layoutJson)
    toast.success(`Opened ${workspace.name}`, {
      description: 'Panes are ready to connect.',
    })
  } catch (e) {
    toast.error('Could not open the workspace', { description: errorMessage(e) })
  }
}

async function remove(workspace: Workspace) {
  try {
    await ipc.deleteWorkspace(workspace.id)
    await load()
  } catch (e) {
    toast.error('Could not delete the workspace', { description: errorMessage(e) })
  }
}

onMounted(load)

// The menu can mount before bootstrap has read the sync status, and a device list built
// while signed-out would then stay empty until a reload. Watching the two things that
// change it - signing in, and a cycle that actually wrote something - covers both.
watch(
  () => [sync.status.signedIn, sync.status.lastSyncAt] as const,
  () => void load(),
)
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger as-child>
      <Button variant="ghost" size="icon" class="size-7 self-center" aria-label="Workspaces">
        <LayoutGridIcon />
      </Button>
    </DropdownMenuTrigger>

    <DropdownMenuContent align="end" class="w-56">
      <DropdownMenuLabel>Workspaces</DropdownMenuLabel>
      <DropdownMenuGroup>
        <DropdownMenuItem
          v-for="workspace in workspaces"
          :key="workspace.id"
          @select="apply(workspace)"
        >
          <span class="truncate">{{ workspace.name }}</span>
          <Button
            variant="ghost"
            size="icon"
            class="ml-auto size-5"
            :aria-label="`Delete ${workspace.name}`"
            @click.stop="remove(workspace)"
          >
            <TrashIcon />
          </Button>
        </DropdownMenuItem>
        <DropdownMenuItem v-if="!workspaces.length" disabled>
          No saved workspaces
        </DropdownMenuItem>
      </DropdownMenuGroup>

      <template v-if="devices.length">
        <DropdownMenuSeparator />
        <DropdownMenuLabel>Other devices</DropdownMenuLabel>
        <DropdownMenuGroup>
          <DropdownMenuItem
            v-for="device in devices"
            :key="device.deviceId"
            @select="openDevice(device)"
          >
            <LaptopIcon />
            <span class="truncate">{{ device.name }}</span>
            <span class="ml-auto shrink-0 text-xs text-muted-foreground">
              {{ lastSyncLabel(device.updatedAt) }}
            </span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </template>

      <DropdownMenuSeparator />
      <DropdownMenuGroup>
        <DropdownMenuItem :disabled="!sessions.tabs.length" @select="saveOpen = true">
          <SaveIcon />
          Save current tabs…
        </DropdownMenuItem>
      </DropdownMenuGroup>
    </DropdownMenuContent>
  </DropdownMenu>

  <Dialog v-model:open="saveOpen">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Save workspace</DialogTitle>
        <DialogDescription>
          Stores the current tabs, splits and hosts. Saving over an existing name replaces it.
        </DialogDescription>
      </DialogHeader>

      <Field>
        <FieldLabel for="workspace-name">Name</FieldLabel>
        <Input id="workspace-name" v-model="name" placeholder="Morning checks" @keyup.enter="save" />
      </Field>

      <DialogFooter>
        <Button variant="ghost" @click="saveOpen = false">Cancel</Button>
        <Button :disabled="!name.trim() || busy" @click="save">Save</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
