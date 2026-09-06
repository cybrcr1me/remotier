<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import AppearancePicker from './AppearancePicker.vue'
import { INHERIT, resolveInherited } from '@/lib/inherit'
import { NAME_EXAMPLE, placeholderExample } from '@/lib/placeholder'
import { errorMessage } from '@/lib/ipc'
import type { Group, GroupInput } from '@/lib/types'
import { useCredentialsStore } from '@/stores/credentials'
import { useInventoryStore } from '@/stores/inventory'
import { useVarsStore } from '@/stores/vars'
import { PlusIcon, TrashIcon } from '@lucide/vue'
import { storeToRefs } from 'pinia'
import { computed, reactive, ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  group: Group | null
  defaultParentId: string | null
}>()

const inventory = useInventoryStore()
const credentials = useCredentialsStore()
const vars = useVarsStore()
const { groups } = storeToRefs(inventory)
const { identities } = storeToRefs(credentials)

const form = reactive({
  name: '',
  parentId: INHERIT,
  defaultPort: '',
  defaultIdentityId: INHERIT,
  icon: 'folder',
  color: 'default',
})

const newVariable = reactive({ name: '', label: '', defaultValue: '', required: false })
const busy = ref(false)

const nameInvalid = computed(() => form.name.trim().length === 0)

/** Groups that may be the parent, excluding this one to avoid an obvious cycle. */
const parentOptions = computed(() => groups.value.filter(g => g.id !== props.group?.id))

const declared = computed(() =>
  props.group ? vars.defsForScope('group', props.group.id) : [],
)

watch(
  () => [open.value, props.group] as const,
  ([isOpen]) => {
    if (!isOpen) return
    form.name = props.group?.name ?? ''
    form.parentId = props.group?.parentId ?? props.defaultParentId ?? INHERIT
    form.defaultPort = props.group?.defaultPort?.toString() ?? ''
    form.defaultIdentityId = props.group?.defaultIdentityId ?? INHERIT
    Object.assign(newVariable, { name: '', label: '', defaultValue: '', required: false })
    form.icon = props.group?.icon ?? 'folder'
    form.color = props.group?.color ?? 'default'
  },
  { immediate: true },
)

async function save() {
  if (nameInvalid.value) return

  const input: GroupInput = {
    name: form.name.trim(),
    parentId: resolveInherited(form.parentId),
    defaultPort: form.defaultPort.trim() === '' ? null : Number(form.defaultPort),
    defaultIdentityId: resolveInherited(form.defaultIdentityId),
    icon: form.icon,
    color: form.color === 'default' ? null : form.color,
  }

  busy.value = true
  try {
    if (props.group) await inventory.updateGroup(props.group.id, input)
    else await inventory.createGroup(input)
    open.value = false
  } catch (e) {
    toast.error('Could not save the group', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}

async function addVariable() {
  if (!props.group || !newVariable.name.trim()) return
  try {
    await vars.upsertDef({
      scope: 'group',
      scopeId: props.group.id,
      name: newVariable.name.trim(),
      label: newVariable.label.trim() || null,
      defaultValue: newVariable.defaultValue.trim() || null,
      required: newVariable.required,
    })
    Object.assign(newVariable, { name: '', label: '', defaultValue: '', required: false })
  } catch (e) {
    toast.error('Could not add the variable', { description: errorMessage(e) })
  }
}

async function setValue(name: string, value: string) {
  if (!props.group) return
  try {
    if (value.trim()) await vars.setValue('group', props.group.id, name, value.trim())
    else await vars.clearValue('group', props.group.id, name)
  } catch (e) {
    toast.error('Could not save the value', { description: errorMessage(e) })
  }
}
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent class="flex w-full flex-col sm:max-w-lg">
      <SheetHeader>
        <SheetTitle>{{ props.group ? 'Edit group' : 'New group' }}</SheetTitle>
        <SheetDescription>
          Hosts inherit these defaults. Variables declared here are shared; the values you
          type are stored only on this machine.
        </SheetDescription>
      </SheetHeader>

      <div class="min-h-0 flex-1 overflow-y-auto px-4">
        <FieldGroup>
          <Field :data-invalid="nameInvalid ? '' : undefined">
            <FieldLabel for="group-name">Name</FieldLabel>
            <Input
              id="group-name"
              v-model="form.name"
              placeholder="Production"
              :aria-invalid="nameInvalid ? true : undefined"
            />
          </Field>

          <Field>
            <FieldLabel for="group-parent">Parent group</FieldLabel>
            <Select v-model="form.parentId">
              <SelectTrigger id="group-parent">
                <SelectValue placeholder="Top level" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem :value="INHERIT">Top level</SelectItem>
                  <SelectItem v-for="group in parentOptions" :key="group.id" :value="group.id">
                    {{ group.name }}
                  </SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>

          <AppearancePicker v-model:icon="form.icon" v-model:color="form.color" />

          <Field>
            <FieldLabel for="group-port">Default port</FieldLabel>
            <Input id="group-port" v-model="form.defaultPort" inputmode="numeric" placeholder="Inherit" />
          </Field>

          <Field>
            <FieldLabel for="group-identity">Default identity</FieldLabel>
            <Select v-model="form.defaultIdentityId">
              <SelectTrigger id="group-identity">
                <SelectValue placeholder="Inherit" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem :value="INHERIT">Inherit</SelectItem>
                  <SelectItem v-for="identity in identities" :key="identity.id" :value="identity.id">
                    {{ identity.label }} — {{ identity.username }}
                  </SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
            <FieldDescription>
              Use <code class="font-mono">{{ NAME_EXAMPLE }}</code> in the identity username to
              have each user supply their own value.
            </FieldDescription>
          </Field>
        </FieldGroup>

        <template v-if="props.group">
          <Separator class="my-6" />

          <div class="flex flex-col gap-3">
            <div>
              <h3 class="text-sm font-medium">Variables</h3>
              <p class="text-xs text-muted-foreground">
                Shared declarations, local values. Your values are never synced.
              </p>
            </div>

            <div v-for="def in declared" :key="def.id" class="flex items-end gap-2">
              <Field class="flex-1">
                <FieldLabel :for="`var-${def.id}`">
                  {{ def.label || def.name }}
                  <span class="font-mono text-xs text-muted-foreground">
                    {{ placeholderExample(def.name) }}
                  </span>
                </FieldLabel>
                <Input
                  :id="`var-${def.id}`"
                  :model-value="vars.valueFor('group', props.group.id, def.name) ?? ''"
                  :placeholder="def.defaultValue || (def.required ? 'Required' : 'Optional')"
                  @update:model-value="value => setValue(def.name, String(value))"
                />
              </Field>
              <Button
                variant="ghost"
                size="icon"
                :aria-label="`Remove ${def.name}`"
                @click="vars.deleteDef(def.id)"
              >
                <TrashIcon />
              </Button>
            </div>

            <div class="flex flex-col gap-2 rounded-md border p-3">
              <div class="flex gap-2">
                <Input v-model="newVariable.name" placeholder="Name, e.g. wg_user" />
                <Input v-model="newVariable.defaultValue" placeholder="Default (optional)" />
              </div>
              <div class="flex items-center gap-2">
                <Checkbox id="var-required" v-model="newVariable.required" />
                <FieldLabel for="var-required" class="text-xs font-normal">
                  Required before connecting
                </FieldLabel>
                <Button
                  size="sm"
                  variant="secondary"
                  class="ml-auto"
                  :disabled="!newVariable.name.trim()"
                  @click="addVariable"
                >
                  <PlusIcon data-icon="inline-start" />
                  Add variable
                </Button>
              </div>
            </div>
          </div>
        </template>
      </div>

      <SheetFooter>
        <Button variant="ghost" @click="open = false">Cancel</Button>
        <Button :disabled="nameInvalid || busy" @click="save">
          {{ props.group ? 'Save changes' : 'Create group' }}
        </Button>
      </SheetFooter>
    </SheetContent>
  </Sheet>
</template>
