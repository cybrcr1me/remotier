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
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import GroupEditor from '@/components/hosts/GroupEditor.vue'
import HostEditor from '@/components/hosts/HostEditor.vue'
import HostGrid from '@/components/hosts/HostGrid.vue'
import HostTree from '@/components/hosts/HostTree.vue'
import SshConfigImport from '@/components/hosts/SshConfigImport.vue'
import PageBody from '@/components/layout/PageBody.vue'
import ViewToolbar from '@/components/layout/ViewToolbar.vue'
import { errorMessage } from '@/lib/ipc'
import { buildTree, filterTree, flattenHosts, groupIds, type TreeNode } from '@/lib/tree'
import type { Group, Host } from '@/lib/types'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { DownloadIcon, FolderPlusIcon, LayoutGridIcon, ListIcon, PlusIcon, ServerIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { toast } from 'vue-sonner'

const inventory = useInventoryStore()
const sessions = useSessionsStore()
const router = useRouter()
const { groups, hosts } = storeToRefs(inventory)

const search = ref('')
// Remembered per machine; a browsing preference is not worth a database round trip.
const layout = ref<'list' | 'grid'>(
  (localStorage.getItem('hosts.layout') as 'list' | 'grid' | null) ?? 'grid',
)
watch(layout, (value) => {
  // Ignore a storage failure: losing the preference is not worth an error.
  try {
    localStorage.setItem('hosts.layout', value)
  } catch {}
})
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
const gridHosts = computed(() => flattenHosts(visible.value))
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

function editHostById(hostId: string) {
  const host = inventory.hostById.get(hostId)
  if (host) edit({ kind: 'host', id: host.id, host })
}

function removeHostById(hostId: string) {
  const host = inventory.hostById.get(hostId)
  if (host) pendingDelete.value = { kind: 'host', id: host.id, host }
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
    <ViewToolbar>
      <Input v-model="search" placeholder="Search hosts, groups, tags…" class="max-w-sm" />

      <ToggleGroup
        v-model="layout"
        type="single"
        variant="outline"
        size="sm"
        orientation="horizontal"
      >
        <Tooltip>
          <TooltipTrigger as-child>
            <ToggleGroupItem value="list" aria-label="List view">
              <ListIcon />
            </ToggleGroupItem>
          </TooltipTrigger>
          <TooltipContent>List</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger as-child>
            <ToggleGroupItem value="grid" aria-label="Grid view">
              <LayoutGridIcon />
            </ToggleGroupItem>
          </TooltipTrigger>
          <TooltipContent>Grid</TooltipContent>
        </Tooltip>
      </ToggleGroup>

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
    </ViewToolbar>

    <PageBody v-if="visible.length">
      <HostTree
        v-if="layout === 'list'"
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

      <HostGrid
        v-else
        :hosts="gridHosts"
        @connect="connect"
        @edit="id => editHostById(id)"
        @remove="id => removeHostById(id)"
      />
    </PageBody>

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
