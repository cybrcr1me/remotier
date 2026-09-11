<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Field, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { COLORS, HOST_ICONS, catalogKey, catalogReference, colorSwatch } from '@/lib/appearance'
import { searchCatalog } from '@/lib/icon-catalog'
import { errorMessage } from '@/lib/ipc'
import { cn } from '@/lib/utils'
import { useIconsStore } from '@/stores/icons'
import { CheckIcon, SearchIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { computed, ref, watch } from 'vue'
import EntityIcon from './EntityIcon.vue'

const icon = defineModel<string>('icon', { required: true })
const color = defineModel<string>('color', { required: true })

defineProps<{ showIcon?: boolean }>()

const icons = useIconsStore()
const { catalog } = storeToRefs(icons)

/** The catalog icon currently chosen, or `null` when the icon is a built-in one. */
const reference = computed(() => catalogReference(icon.value))

const open = ref(false)
const query = ref('')
/**
 * The query results are computed from, trailing the input. Every result is an image
 * fetched on first sight, so each keystroke must not order a fresh page of them.
 */
const settled = ref('')
const catalogError = ref<string | null>(null)

watch(query, (value, _previous, onCleanup) => {
  const timer = setTimeout(() => {
    settled.value = value
  }, 200)
  onCleanup(() => clearTimeout(timer))
})

watch(open, async (isOpen) => {
  if (!isOpen) return
  catalogError.value = null
  try {
    await icons.loadCatalog()
  } catch (e) {
    catalogError.value = errorMessage(e)
  }
})

const results = computed(() => searchCatalog(catalog.value ?? [], settled.value))

function choose(chosen: string) {
  icon.value = catalogKey(chosen)
  open.value = false
}
</script>

<template>
  <Field v-if="showIcon !== false">
    <FieldLabel>Icon</FieldLabel>
    <div class="flex flex-wrap gap-1">
      <Button
        v-for="option in HOST_ICONS"
        :key="option.key"
        type="button"
        variant="outline"
        size="icon"
        :aria-label="option.label"
        :aria-pressed="icon === option.key"
        :class="cn('size-9', icon === option.key && 'border-ring bg-accent')"
        @click="icon = option.key"
      >
        <component :is="option.icon" />
      </Button>

      <!--
        App icons from the selfh.st catalog: a Portainer or Proxmox logo rather than a
        generic server. Rust fetches and caches them; the webview never goes online.
      -->
      <Popover v-model:open="open">
        <PopoverTrigger as-child>
          <Button
            type="button"
            variant="outline"
            :aria-pressed="reference !== null"
            :class="cn('h-9 max-w-48 font-normal', reference && 'border-ring bg-accent')"
          >
            <EntityIcon v-if="reference" :icon="icon" class="size-4" />
            <SearchIcon v-else />
            <span class="truncate">{{ reference ?? 'App icons' }}</span>
          </Button>
        </PopoverTrigger>

        <PopoverContent class="flex w-72 flex-col gap-2 p-2" align="start">
          <Input v-model="query" placeholder="Search app icons…" />

          <p v-if="catalogError" class="px-1 text-xs text-destructive">
            Catalog unavailable. {{ catalogError }}
          </p>
          <p v-else-if="!catalog" class="px-1 text-xs text-muted-foreground">
            Fetching the catalog…
          </p>
          <p v-else-if="!settled.trim()" class="px-1 text-xs text-muted-foreground">
            Search {{ catalog.length.toLocaleString() }} icons from selfh.st.
          </p>
          <p v-else-if="results.length === 0" class="px-1 text-xs text-muted-foreground">
            No icon matches.
          </p>
          <div v-else class="flex max-h-72 flex-col overflow-y-auto">
            <Button
              v-for="entry in results"
              :key="entry.reference"
              type="button"
              variant="ghost"
              :class="cn('justify-start font-normal', reference === entry.reference && 'bg-accent')"
              @click="choose(entry.reference)"
            >
              <EntityIcon :icon="catalogKey(entry.reference)" class="size-4 shrink-0" />
              <span class="truncate">{{ entry.name }}</span>
              <CheckIcon v-if="reference === entry.reference" class="ml-auto" />
            </Button>
          </div>
        </PopoverContent>
      </Popover>
    </div>
  </Field>

  <Field>
    <FieldLabel>Colour</FieldLabel>
    <div class="flex flex-wrap gap-1">
      <Button
        v-for="option in COLORS"
        :key="option.key"
        type="button"
        variant="outline"
        size="icon"
        :aria-label="option.label"
        :aria-pressed="color === option.key"
        :class="cn('size-9', color === option.key && 'border-ring')"
        @click="color = option.key"
      >
        <span :class="cn('flex size-4 items-center justify-center rounded-full', colorSwatch(option.key))">
          <CheckIcon v-if="color === option.key" class="size-3 text-background" />
        </span>
      </Button>
    </div>
  </Field>
</template>
