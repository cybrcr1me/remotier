<script setup lang="ts">
/**
 * Every `{{placeholder}}` declared by a group, and this machine's answer to it.
 *
 * The declaration is shared - it travels with the group - and the answer is local and never
 * synced. That split is what lets one warpgate host be shared by a team while each person
 * connects as themselves, but it also means a user has no single place to see what they
 * have been asked for. This is that place.
 *
 * Declarations are made in the group editor. Only the answers are editable here.
 */
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from '@/components/ui/empty'
import { Input } from '@/components/ui/input'
import type { VarScope } from '@/lib/types'
import { errorMessage } from '@/lib/ipc'
import { placeholderExample } from '@/lib/placeholder'
import { blockingCount, GLOBAL_SCOPE_ID, groupPlaceholders } from '@/lib/placeholder-list'
import { cn } from '@/lib/utils'
import { useInventoryStore } from '@/stores/inventory'
import { useVarsStore } from '@/stores/vars'
import { Checkbox } from '@/components/ui/checkbox'
import { BracesIcon, PlusIcon, Trash2Icon, XIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { computed, reactive, ref } from 'vue'
import { toast } from 'vue-sonner'

const vars = useVarsStore()
const inventory = useInventoryStore()
const { defs } = storeToRefs(vars)
const { groups } = storeToRefs(inventory)

const buckets = computed(() =>
  groupPlaceholders(defs.value, groups.value, (scope, scopeId, name) =>
    vars.valueFor(scope, scopeId, name),
  ),
)

const blocking = computed(() => blockingCount(buckets.value))

const draft = reactive({ name: '', label: '', defaultValue: '', required: false })
const busy = ref(false)

const nameTaken = computed(() =>
  vars.defs.some(d => d.scope === 'global' && d.name === draft.name.trim()),
)
const draftInvalid = computed(() => draft.name.trim().length === 0 || nameTaken.value)

/**
 * Declare an account-wide placeholder.
 *
 * Only this scope is declared here. A group's declarations travel with the group and are
 * shared with whoever else has it, so they are made where that is evident - in the group
 * editor - rather than on a page about this machine.
 */
async function declare() {
  if (draftInvalid.value) return

  busy.value = true
  try {
    await vars.upsertDef({
      scope: 'global',
      scopeId: GLOBAL_SCOPE_ID,
      name: draft.name.trim(),
      label: draft.label.trim() || null,
      defaultValue: draft.defaultValue.trim() || null,
      required: draft.required,
    })
    Object.assign(draft, { name: '', label: '', defaultValue: '', required: false })
  } catch (e) {
    toast.error('Could not declare the placeholder', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}

async function remove(id: string) {
  try {
    await vars.deleteDef(id)
  } catch (e) {
    toast.error('Could not delete the placeholder', { description: errorMessage(e) })
  }
}

async function save(scope: VarScope, scopeId: string, name: string, value: string) {
  try {
    if (value.trim()) await vars.setValue(scope, scopeId, name, value.trim())
    else await vars.clearValue(scope, scopeId, name)
  } catch (e) {
    toast.error('Could not save the value', { description: errorMessage(e) })
  }
}
</script>

<template>
  <div class="flex flex-col gap-6">
    <p class="text-sm text-muted-foreground">
      A hostname, port or username can contain
      <code class="font-mono text-xs">{{ placeholderExample('name') }}</code>. Declarations are
      shared; the values you give them stay on this machine and are never synced.
    </p>

    <section class="flex flex-col gap-3 rounded-md border p-3">
      <div>
        <h3 class="text-sm font-medium">New account placeholder</h3>
        <p class="text-xs text-muted-foreground">
          Every host can use it. A group or host declaring the same name overrides it.
        </p>
      </div>

      <div class="flex flex-wrap gap-2">
        <Input
          v-model="draft.name"
          placeholder="Name, e.g. wg_user"
          class="w-48 font-mono text-xs md:text-xs"
          aria-label="Placeholder name"
          @keydown.enter="declare"
        />
        <Input
          v-model="draft.label"
          placeholder="Description (optional)"
          class="min-w-48 flex-1"
          aria-label="Label"
          @keydown.enter="declare"
        />
        <Input
          v-model="draft.defaultValue"
          placeholder="Default (optional)"
          class="w-48 font-mono text-xs md:text-xs"
          aria-label="Default value"
          @keydown.enter="declare"
        />
      </div>

      <div class="flex items-center gap-2">
        <Checkbox id="new-account-var-required" v-model="draft.required" />
        <label for="new-account-var-required" class="text-xs text-muted-foreground">
          Required before connecting
        </label>
        <Button size="sm" class="ml-auto" :disabled="draftInvalid || busy" @click="declare">
          <PlusIcon data-icon="inline-start" />
          Declare
        </Button>
      </div>

      <p v-if="nameTaken" class="text-xs text-destructive">
        An account placeholder called {{ placeholderExample(draft.name.trim()) }} already exists.
      </p>
    </section>

    <Empty v-if="buckets.length === 0" class="border">
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <BracesIcon />
        </EmptyMedia>
        <EmptyTitle>No placeholders declared</EmptyTitle>
        <EmptyDescription>
          Declare one above to reach every host, or declare it on a group under Variables to
          share it with whoever else has that group.
        </EmptyDescription>
      </EmptyHeader>
    </Empty>

    <template v-else>
      <p v-if="blocking > 0" class="text-sm text-destructive">
        {{ blocking }} required {{ blocking === 1 ? 'placeholder has' : 'placeholders have' }}
        no value. Connections that use {{ blocking === 1 ? 'it' : 'them' }} will stop and ask.
      </p>

      <section v-for="bucket in buckets" :key="bucket.name" class="flex flex-col gap-2">
        <div class="flex items-baseline gap-2">
          <h3 class="text-sm font-medium">{{ bucket.name }}</h3>
          <span class="text-xs text-muted-foreground">
            {{ bucket.global ? 'Applies everywhere. Any group or host can override it.' : 'Declared by this group.' }}
          </span>
        </div>

        <div class="flex flex-col divide-y rounded-md border">
          <div
            v-for="row in bucket.rows"
            :key="row.def.id"
            class="flex flex-wrap items-center gap-3 p-3"
          >
            <div class="flex min-w-52 flex-1 flex-col gap-0.5">
              <div class="flex items-center gap-2">
                <code class="font-mono text-xs">{{ placeholderExample(row.def.name) }}</code>
                <Badge v-if="row.def.required" variant="secondary">Required</Badge>
                <Badge v-if="row.blocking" variant="secondary" class="text-destructive">
                  Not set
                </Badge>
              </div>
              <p v-if="row.def.label" class="text-xs text-muted-foreground">
                {{ row.def.label }}
              </p>
              <!--
                Shown only when it is doing something: a default the user has not overridden
                is the value that will actually be used, and is worth saying so.
              -->
              <p v-if="!row.value && row.def.defaultValue" class="text-xs text-muted-foreground">
                Falls back to <span class="font-mono">{{ row.def.defaultValue }}</span>
              </p>
            </div>

            <Input
              :model-value="row.value ?? ''"
              :placeholder="row.def.defaultValue ?? 'No value'"
              :class="cn('max-w-64 flex-1 font-mono text-xs md:text-xs', row.blocking && 'border-destructive')"
              :aria-label="`Value for ${row.def.name}`"
              @change="(event: Event) => save(row.def.scope, row.def.scopeId, row.def.name, (event.target as HTMLInputElement).value)"
            />

            <Button
              variant="ghost"
              size="icon"
              class="size-8 shrink-0"
              :disabled="row.value === undefined"
              :aria-label="`Clear ${row.def.name}`"
              @click="save(row.def.scope, row.def.scopeId, row.def.name, '')"
            >
              <XIcon />
            </Button>

            <!-- Only account declarations are made here, so only those can be removed here. -->
            <Button
              v-if="bucket.global"
              variant="ghost"
              size="icon"
              class="size-8 shrink-0 text-muted-foreground hover:text-destructive"
              :aria-label="`Delete ${row.def.name}`"
              @click="remove(row.def.id)"
            >
              <Trash2Icon />
            </Button>
          </div>
        </div>
      </section>
    </template>
  </div>
</template>
