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
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbLink,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from '@/components/ui/breadcrumb'
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
import {
  breadcrumb,
  buildTree,
  directHosts,
  filterTree,
  flattenHosts,
  groupIds,
  groupNodes,
  nodesAt,
  type TreeNode,
} from '@/lib/tree'
import type { Group, Host } from '@/lib/types'
import { useInventoryStore } from '@/stores/inventory'
import { useSessionsStore } from '@/stores/sessions'
import { DownloadIcon, FolderPlusIcon, HouseIcon, LayoutGridIcon, ListIcon, PlusIcon, ServerIcon } from '@lucide/vue'
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

/**
 * The group ids the grid has been opened into, outermost first.
 *
 * Grid only: the list view nests, so it shows the whole structure at once and has nothing
 * to navigate into.
 */
const folder = ref<string[]>([])

/*
 * Fall back to the root if the open folder stops existing - deleted here, or on another
 * machine once sync arrives. Showing an empty level that cannot be named would leave the
 * user stuck with no way back but the breadcrumb they can no longer read.
 */
watch([tree, folder], ([nodes, path]) => {
  if (path.length > 0 && nodesAt(nodes, path) === null) folder.value = []
}, { immediate: true })

const trail = computed(() => breadcrumb(tree.value, folder.value))
const inFolder = computed(() => trail.value.length > 0)

/** Where a new host or group should land: the folder currently open. */
const currentGroupId = computed(() => folder.value[folder.value.length - 1] ?? null)

/** The level being browsed, before any search is applied. */
const level = computed(() => nodesAt(tree.value, folder.value) ?? tree.value)

const searching = computed(() => search.value.trim().length > 0)

/**
 * What the grid shows.
 *
 * Browsing shows this level only, folders included. Searching looks through everything
 * below the open folder and reports the hosts flat, with the path that says where each one
 * came from - a match three groups down is still a match, and hiding it because the user
 * happens to be standing one level up would make search useless.
 */
const gridGroups = computed(() =>
  searching.value ? groupNodes(filterTree(level.value, search.value)) : groupNodes(level.value),
)

const gridHosts = computed(() =>
  searching.value ? flattenHosts(filterTree(level.value, search.value)) : directHosts(level.value),
)

/** The list view keeps searching the whole tree; it has no notion of an open folder. */
const visible = computed(() => {
  const filtered = filterTree(tree.value, search.value)
  // Searching is useless if the matches stay collapsed, so expand everything shown.
  if (searching.value) {
    expanded.value = new Set(groupIds(filtered))
  }
  return filtered
})

/** Whether the grid has anything to draw, which is not the same as the tree having it. */
const gridEmpty = computed(() => gridGroups.value.length === 0 && gridHosts.value.length === 0)
const nothingToShow = computed(() =>
  layout.value === 'grid' ? gridEmpty.value : visible.value.length === 0,
)

const emptyTitle = computed(() => {
  if (searching.value) return 'Nothing matches'
  return inFolder.value ? 'This group is empty' : 'No hosts yet'
})

const emptyDescription = computed(() => {
  if (searching.value) {
    return inFolder.value
      ? `Nothing in ${trail.value[trail.value.length - 1]?.name} matches. Clear the search to look everywhere.`
      : 'Try a different search.'
  }
  return inFolder.value
    ? 'Add a host here, or go back up.'
    : 'Add a host, or import your ~/.ssh/config.'
})

function openGroup(groupId: string) {
  folder.value = [...folder.value, groupId]
  search.value = ''
}

/** Jump to a point in the trail; `-1` is the root. */
function goUpTo(index: number) {
  folder.value = folder.value.slice(0, index + 1)
  search.value = ''
}

function editGroupById(groupId: string) {
  const node = level.value.find(child => child.kind === 'group' && child.id === groupId)
  if (node) edit(node)
}

function removeGroupById(groupId: string) {
  const node = level.value.find(child => child.kind === 'group' && child.id === groupId)
  if (node) pendingDelete.value = node
}

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
      <Input
        v-model="search"
        :placeholder="inFolder ? `Search in ${trail[trail.length - 1]?.name}…` : 'Search hosts, groups, tags…'"
        class="max-w-sm"
      />

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
        <Button variant="secondary" size="sm" @click="newGroup(currentGroupId)">
          <FolderPlusIcon data-icon="inline-start" />
          Group
        </Button>
        <Button size="sm" @click="newHost(currentGroupId)">
          <PlusIcon data-icon="inline-start" />
          Host
        </Button>
      </div>
    </ViewToolbar>

    <!--
      Grid only: the list view nests, so it already shows where everything sits and has
      nothing to navigate into.

      Shown at the root too, rather than only inside a group. A bar that appears and
      disappears shifts everything under it by its own height, and the way out of a group
      should be in the same place it was before you went in.
    -->
    <div v-if="layout === 'grid'" class="flex h-9 shrink-0 items-center border-b px-4">
      <Breadcrumb>
        <BreadcrumbList>
          <BreadcrumbItem>
            <BreadcrumbPage v-if="!inFolder" class="flex items-center gap-1.5">
              <HouseIcon class="size-3.5" />
              All hosts
            </BreadcrumbPage>
            <BreadcrumbLink
              v-else
              as="button"
              type="button"
              class="flex cursor-pointer items-center gap-1.5"
              @click="goUpTo(-1)"
            >
              <HouseIcon class="size-3.5" />
              All hosts
            </BreadcrumbLink>
          </BreadcrumbItem>

          <template v-for="(node, index) in trail" :key="node.id">
            <BreadcrumbSeparator />
            <BreadcrumbItem>
              <BreadcrumbPage v-if="index === trail.length - 1">{{ node.name }}</BreadcrumbPage>
              <BreadcrumbLink
                v-else
                as="button"
                type="button"
                class="cursor-pointer"
                @click="goUpTo(index)"
              >
                {{ node.name }}
              </BreadcrumbLink>
            </BreadcrumbItem>
          </template>
        </BreadcrumbList>
      </Breadcrumb>
    </div>

    <PageBody v-if="!nothingToShow">
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
        :groups="gridGroups"
        :hosts="gridHosts"
        @open="openGroup"
        @connect="connect"
        @edit="id => editHostById(id)"
        @remove="id => removeHostById(id)"
        @edit-group="editGroupById"
        @remove-group="removeGroupById"
      />
    </PageBody>

    <Empty v-else class="flex-1">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <ServerIcon />
        </EmptyMedia>
        <EmptyTitle>{{ emptyTitle }}</EmptyTitle>
        <EmptyDescription>{{ emptyDescription }}</EmptyDescription>
      </EmptyHeader>
      <Button v-if="!searching" @click="newHost(currentGroupId)">Add a host</Button>
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
