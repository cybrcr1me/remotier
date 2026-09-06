<script setup lang="ts">
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import { useInventoryStore } from '@/stores/inventory'
import { ServerIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { computed } from 'vue'

const open = defineModel<boolean>('open', { required: true })
const emit = defineEmits<{ select: [hostId: string] }>()

const inventory = useInventoryStore()
const { hosts, groups } = storeToRefs(inventory)

const groupName = computed(() => new Map(groups.value.map(g => [g.id, g.name])))

function choose(hostId: string) {
  open.value = false
  emit('select', hostId)
}
</script>

<template>
  <CommandDialog v-model:open="open">
    <CommandInput placeholder="Search hosts…" />
    <CommandList>
      <CommandEmpty>No hosts yet. Add one from the Hosts screen.</CommandEmpty>
      <CommandGroup heading="Hosts">
        <CommandItem
          v-for="host in hosts"
          :key="host.id"
          :value="`${host.label} ${host.hostname} ${host.groupId ? groupName.get(host.groupId) ?? '' : ''}`"
          @select="choose(host.id)"
        >
          <ServerIcon />
          <span class="truncate">{{ host.label }}</span>
          <span class="ml-auto truncate text-xs text-muted-foreground">{{ host.hostname }}</span>
        </CommandItem>
      </CommandGroup>
    </CommandList>
  </CommandDialog>
</template>
