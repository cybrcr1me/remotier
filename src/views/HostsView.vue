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
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import GroupEditor from '@/components/hosts/GroupEditor.vue'
import HostEditor from '@/components/hosts/HostEditor.vue'
import HostTree from '@/components/hosts/HostTree.vue'
import SshConfigImport from '@/components/hosts/SshConfigImport.vue'
import { errorMessage } from '@/lib/ipc'
import { buildTree, filterTree, groupIds, type TreeNode } from '@/lib/tree'
import type { Group, Host } from '@/lib/types'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { DownloadIcon, FolderPlusIcon, PlusIcon, ServerIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import { toast } from 'vue-sonner'

const inventory = useInventoryStore()
const sessions = useSessionsStore()
const router = useRouter()
const { groups, hosts } = storeToRefs(inventory)

const search = ref('')
const expanded = ref(new Set<string>())
const selectedId = ref<string | null>(null)

const hostEditorOpen = ref(false)
const groupEditorOpen = ref(false)
const importOpen = ref(false)
const editingHost = ref<Host | null>(null)
const editingGroup = ref<Group | null>(null)
const pendingParentId = ref<string | null>(null)
const pendingDelete = ref<TreeNode | null>(null)

const tree = computed(() => buildTree(groups.value, hosts.value))
const visible = computed(() => {
  const filtered = filterTree(tree.value, search.value)
  // Searching is useless if the matches stay collapsed, so expand everything shown.
  if (search.value.trim()) {
    expanded.value = new Set(groupIds(filtered))
  }
  return filtered
})

function toggle(groupId: string) {
  const next = new Set(expanded.value)
  if (next.has(groupId)) next.delete(groupId)
  else next.add(groupId)
  expanded.value = next
}

function newHost(groupId: string | null = null) {
  editingHost.value = null
  pendingParentId.value = groupId
  hostEditorOpen.value = true
}

function newGroup(parentId: string | null = null) {
  editingGroup.value = null
  pendingParentId.value = parentId
  groupEditorOpen.value = true
}

function edit(node: TreeNode) {
  if (node.kind === 'host') {
    editingHost.value = node.host
    pendingParentId.value = node.host.groupId
    hostEditorOpen.value = true
  } else {
    editingGroup.value = node.group
    pendingParentId.value = node.group.parentId
    groupEditorOpen.value = true
  }
}

function connect(hostId: string) {
  const host = inventory.hostById.get(hostId)
  const tab = sessions.openTab(host?.label ?? 'Session')
  sessions.setPaneHost(tab.id, tab.activePaneId, hostId)
  void router.push('/terminals')
}

async function confirmDelete() {
  const node = pendingDelete.value
  if (!node) return
  pendingDelete.value = null

  try {
    if (node.kind === 'host') await inventory.deleteHost(node.host.id)
    else await inventory.deleteGroup(node.group.id)
  } catch (e) {
    toast.error('Could not delete', { description: errorMessage(e) })
  }
}
</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <div class="flex shrink-0 items-center gap-2 border-b p-3">
      <Input v-model="search" placeholder="Search hosts, groups, tags…" class="max-w-sm" />
      <div class="ml-auto flex gap-2">
        <Button variant="ghost" size="sm" @click="importOpen = true">
          <DownloadIcon data-icon="inline-start" />
          Import ~/.ssh/config
        </Button>
        <Button variant="secondary" size="sm" @click="newGroup()">
          <FolderPlusIcon data-icon="inline-start" />
          Group
        </Button>
        <Button size="sm" @click="newHost()">
          <PlusIcon data-icon="inline-start" />
          Host
        </Button>
      </div>
    </div>

    <ScrollArea v-if="visible.length" class="min-h-0 flex-1">
      <div class="p-2">
        <HostTree
          :nodes="visible"
          :expanded="expanded"
          :selected-id="selectedId"
          @toggle="toggle"
          @select="node => (selectedId = node.id)"
          @connect="connect"
          @edit="edit"
          @remove="node => (pendingDelete = node)"
          @add-host="newHost"
          @add-group="newGroup"
        />
      </div>
    </ScrollArea>

    <Empty v-else class="flex-1">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <ServerIcon />
        </EmptyMedia>
        <EmptyTitle>{{ search ? 'Nothing matches' : 'No hosts yet' }}</EmptyTitle>
        <EmptyDescription>
          {{ search ? 'Try a different search.' : 'Add a host, or import your ~/.ssh/config.' }}
        </EmptyDescription>
      </EmptyHeader>
      <Button v-if="!search" @click="newHost()">Add a host</Button>
    </Empty>

    <HostEditor
      v-model:open="hostEditorOpen"
      :host="editingHost"
      :default-group-id="pendingParentId"
    />
    <GroupEditor
      v-model:open="groupEditorOpen"
      :group="editingGroup"
      :default-parent-id="pendingParentId"
    />
    <SshConfigImport v-model:open="importOpen" />

    <AlertDialog :open="pendingDelete !== null">
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            Delete {{ pendingDelete?.kind === 'host' ? 'host' : 'group' }}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            <template v-if="pendingDelete?.kind === 'host'">
              “{{ pendingDelete.host.label }}” will be removed. This cannot be undone.
            </template>
            <template v-else-if="pendingDelete">
              “{{ pendingDelete.name }}” and its subgroups will be removed. Hosts inside it
              are kept and become ungrouped.
            </template>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel @click="pendingDelete = null">Cancel</AlertDialogCancel>
          <AlertDialogAction variant="destructive" @click="confirmDelete">
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
