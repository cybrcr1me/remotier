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
import type { Workspace } from '@/lib/types'
import { useSessionsStore } from '@/stores/sessions'
import { LayoutGridIcon, SaveIcon, TrashIcon } from '@lucide/vue'
import { onMounted, ref } from 'vue'
import { toast } from 'vue-sonner'

const sessions = useSessionsStore()

const workspaces = ref<Workspace[]>([])
const saveOpen = ref(false)
const name = ref('')
const busy = ref(false)

async function load() {
  try {
    workspaces.value = await ipc.listWorkspaces()
  } catch (e) {
    toast.error('Could not load workspaces', { description: errorMessage(e) })
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
