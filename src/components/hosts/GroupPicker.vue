<script setup lang="ts">
/**
 * Choosing a group, with its colour, its nesting and a search box.
 *
 * A plain `Select` listed every group flat and unlabelled, which stops being usable at the
 * point the groups are worth having: two groups called "Staging" under different parents
 * were indistinguishable, and there was no way to find one without reading the whole list.
 */
import { Button } from '@/components/ui/button'
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from '@/components/ui/command'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { colorSwatch, hasColor } from '@/lib/appearance'
import EntityIcon from './EntityIcon.vue'
import { buildTree, descendantIds, flattenGroups } from '@/lib/tree'
import type { Group } from '@/lib/types'
import { cn } from '@/lib/utils'
import { CheckIcon, ChevronsUpDownIcon } from '@lucide/vue'
import { computed, ref } from 'vue'

const props = withDefaults(defineProps<{
  groups: Group[]
  /** Excluded along with everything under it, so a group cannot be put inside itself. */
  exclude?: string | null
  id?: string
  /** Wording for "belongs to nothing", which differs between a host and a group. */
  noneLabel?: string
}>(), {
  exclude: null,
  noneLabel: 'No group',
})

/** `null` means the group at the top level. */
const selected = defineModel<string | null>({ required: true })

const open = ref(false)

/*
 * Depth as padding, capped: past four levels the indentation costs more room than the
 * structure it conveys, and the path shown beside each name says it anyway.
 */
const INDENT = ['', 'pl-3', 'pl-6', 'pl-9', 'pl-12'] as const

const options = computed(() => {
  const tree = buildTree(props.groups, [])
  const banned = new Set(props.exclude ? descendantIds(tree, props.exclude) : [])
  return flattenGroups(tree).filter(option => !banned.has(option.group.id))
})

const current = computed(() =>
  options.value.find(option => option.group.id === selected.value) ?? null,
)

function choose(groupId: string | null) {
  selected.value = groupId
  open.value = false
}
</script>

<template>
  <Popover v-model:open="open">
    <PopoverTrigger as-child>
      <Button
        :id="props.id"
        type="button"
        variant="outline"
        role="combobox"
        :aria-expanded="open"
        class="w-full justify-between font-normal"
      >
        <span class="flex min-w-0 items-center gap-2">
          <span
            v-if="current && hasColor(current.group.color)"
            :class="cn('size-2 shrink-0 rounded-full', colorSwatch(current.group.color))"
            aria-hidden="true"
          />
          <EntityIcon v-if="current" :icon="current.group.icon" kind="group" class="size-4 shrink-0" />
          <span class="truncate">{{ current ? current.group.name : props.noneLabel }}</span>
        </span>
        <ChevronsUpDownIcon class="size-4 shrink-0 opacity-50" />
      </Button>
    </PopoverTrigger>

    <PopoverContent class="w-(--reka-popper-anchor-width) p-0" align="start">
      <Command>
        <CommandInput placeholder="Search groups…" />
        <CommandList>
          <CommandEmpty>No group matches.</CommandEmpty>
          <CommandGroup>
            <CommandItem :value="props.noneLabel" @select="choose(null)">
              <CheckIcon :class="cn('size-4', selected === null ? 'opacity-100' : 'opacity-0')" />
              <span class="truncate text-muted-foreground">{{ props.noneLabel }}</span>
            </CommandItem>

            <!--
              The full path is part of the search value, so typing a parent's name finds
              everything under it - the reason nesting is worth showing at all.
            -->
            <CommandItem
              v-for="option in options"
              :key="option.group.id"
              :value="[...option.path, option.group.name].join(' ')"
              @select="choose(option.group.id)"
            >
              <CheckIcon
                :class="cn('size-4 shrink-0', selected === option.group.id ? 'opacity-100' : 'opacity-0')"
              />
              <span :class="cn('flex min-w-0 items-center gap-2', INDENT[Math.min(option.depth, 4)])">
                <span
                  v-if="hasColor(option.group.color)"
                  :class="cn('size-2 shrink-0 rounded-full', colorSwatch(option.group.color))"
                  aria-hidden="true"
                />
                <EntityIcon :icon="option.group.icon" kind="group" class="size-4 shrink-0" />
                <span class="truncate">{{ option.group.name }}</span>
              </span>
              <span v-if="option.path.length" class="ml-auto truncate pl-2 text-xs text-muted-foreground">
                {{ option.path.join(' / ') }}
              </span>
            </CommandItem>
          </CommandGroup>
        </CommandList>
      </Command>
    </PopoverContent>
  </Popover>
</template>
