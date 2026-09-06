<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Separator } from '@/components/ui/separator'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { errorMessage } from '@/lib/ipc'
import AppearancePicker from './AppearancePicker.vue'
import { INHERIT, resolveInherited } from '@/lib/inherit'
import { WG_USER_EXAMPLE } from '@/lib/placeholder'
import type { AuthKind, Host, HostInput } from '@/lib/types'
import { useCredentialsStore } from '@/stores/credentials'
import { useInventoryStore } from '@/stores/inventory'
import { storeToRefs } from 'pinia'
import { computed, reactive, ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })

const props = defineProps<{
  host: Host | null
  defaultGroupId: string | null
}>()

const emit = defineEmits<{ saved: [host: Host] }>()

const inventory = useInventoryStore()
const credentials = useCredentialsStore()
const { groups } = storeToRefs(inventory)
const { identities, keys } = storeToRefs(credentials)

const form = reactive({
  label: '',
  hostname: '',
  groupId: INHERIT,
  port: '',
  identityId: INHERIT,
  tags: '',
  /** 'identity' uses the identity chain; 'host' stores credentials on the host. */
  credentials: 'identity' as 'identity' | 'host',
  username: '',
  authKind: 'password' as AuthKind,
  password: '',
  keyId: INHERIT,
  icon: 'server',
  color: 'default',
})

// Only send a password the user actually typed, so an untouched field keeps the stored one.
const passwordTouched = ref(false)

const saving = reactive({ busy: false })

/** What an inherited field will actually resolve to, shown as help text. */
const inheritedPort = computed(() => {
  const chain = inventory.groupChain(resolveInherited(form.groupId))
  const found = chain.find(group => group.defaultPort !== null)
  return found ? `${found.defaultPort} (from ${found.name})` : '22 (default)'
})

const inheritedIdentity = computed(() => {
  const chain = inventory.groupChain(resolveInherited(form.groupId))
  const found = chain.find(group => group.defaultIdentityId !== null)
  if (!found) return 'the ssh-agent'
  const identity = credentials.identityById.get(found.defaultIdentityId!)
  return identity ? `${identity.label} (from ${found.name})` : `inherited from ${found.name}`
})

watch(
  () => [open.value, props.host] as const,
  ([isOpen]) => {
    if (!isOpen) return
    form.label = props.host?.label ?? ''
    form.hostname = props.host?.hostname ?? ''
    form.groupId = props.host?.groupId ?? props.defaultGroupId ?? INHERIT
    form.port = props.host?.port?.toString() ?? ''
    form.identityId = props.host?.identityId ?? INHERIT
    form.tags = props.host?.tags.join(', ') ?? ''

    form.credentials = props.host?.authKind ? 'host' : 'identity'
    form.username = props.host?.username ?? ''
    form.authKind = props.host?.authKind ?? 'password'
    form.keyId = props.host?.keyId ?? INHERIT
    form.password = ''
    passwordTouched.value = false
    form.icon = props.host?.icon ?? 'server'
    form.color = props.host?.color ?? 'default'
  },
  { immediate: true },
)

const hostnameInvalid = computed(() => form.hostname.trim().length === 0)
const usernameInvalid = computed(
  () => form.credentials === 'host' && form.username.trim().length === 0,
)
const needsPassword = computed(
  () => form.credentials === 'host'
    && (form.authKind === 'password' || form.authKind === 'interactive'),
)
const needsKey = computed(
  () => form.credentials === 'host' && (form.authKind === 'key' || form.authKind === 'agent'),
)
const portInvalid = computed(() => {
  if (form.port.trim() === '') return false
  const port = Number(form.port)
  return !Number.isInteger(port) || port < 1 || port > 65535
})

async function save() {
  if (hostnameInvalid.value || portInvalid.value || usernameInvalid.value) return

  const usingHostCredentials = form.credentials === 'host'

  const input: HostInput = {
    label: form.label.trim() || form.hostname.trim(),
    hostname: form.hostname.trim(),
    groupId: resolveInherited(form.groupId),
    // An empty port means inherit, which is not the same as 22.
    port: form.port.trim() === '' ? null : Number(form.port),
    // The two modes are exclusive: sending both would leave it ambiguous which wins.
    identityId: usingHostCredentials ? null : resolveInherited(form.identityId),
    username: usingHostCredentials ? form.username.trim() : null,
    authKind: usingHostCredentials ? form.authKind : null,
    keyId: usingHostCredentials ? resolveInherited(form.keyId) : null,
    password: usingHostCredentials && passwordTouched.value ? form.password : undefined,
    tags: form.tags.split(',').map(tag => tag.trim()).filter(Boolean),
    icon: form.icon,
    // 'default' means no colour, stored as NULL rather than a magic string.
    color: form.color === 'default' ? null : form.color,
  }

  saving.busy = true
  try {
    const saved = props.host
      ? await inventory.updateHost(props.host.id, input)
      : await inventory.createHost(input)
    emit('saved', saved)
    open.value = false
  } catch (e) {
    toast.error('Could not save the host', { description: errorMessage(e) })
  } finally {
    saving.busy = false
  }
}
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent class="flex w-full flex-col sm:max-w-lg">
      <SheetHeader>
        <SheetTitle>{{ props.host ? 'Edit host' : 'New host' }}</SheetTitle>
        <SheetDescription>
          Fields left as “Inherit” follow the group chain. Placeholders like
          <code class="font-mono">{{ WG_USER_EXAMPLE }}</code> are filled in per user.
        </SheetDescription>
      </SheetHeader>

      <div class="min-h-0 flex-1 overflow-y-auto px-4">
        <FieldGroup>
          <Field :data-invalid="hostnameInvalid ? '' : undefined">
            <FieldLabel for="host-hostname">Hostname</FieldLabel>
            <Input
              id="host-hostname"
              v-model="form.hostname"
              placeholder="web-01.example.com"
              :aria-invalid="hostnameInvalid ? true : undefined"
            />
            <FieldDescription v-if="hostnameInvalid">
              A hostname is required.
            </FieldDescription>
          </Field>

          <Field>
            <FieldLabel for="host-label">Name</FieldLabel>
            <Input id="host-label" v-model="form.label" placeholder="Defaults to the hostname" />
          </Field>

          <Field>
            <FieldLabel for="host-group">Group</FieldLabel>
            <Select v-model="form.groupId">
              <SelectTrigger id="host-group">
                <SelectValue placeholder="No group" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem :value="INHERIT">No group</SelectItem>
                  <SelectItem v-for="group in groups" :key="group.id" :value="group.id">
                    {{ group.name }}
                  </SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>

          <Field :data-invalid="portInvalid ? '' : undefined">
            <FieldLabel for="host-port">Port</FieldLabel>
            <Input
              id="host-port"
              v-model="form.port"
              inputmode="numeric"
              placeholder="Inherit"
              :aria-invalid="portInvalid ? true : undefined"
            />
            <FieldDescription>
              {{ portInvalid ? 'Enter a port between 1 and 65535.' : `Inherits ${inheritedPort}.` }}
            </FieldDescription>
          </Field>

          <Field>
            <FieldLabel>Credentials</FieldLabel>
            <ToggleGroup
              v-model="form.credentials"
              type="single"
              variant="outline"
              orientation="horizontal"
            >
              <ToggleGroupItem value="identity">Use an identity</ToggleGroupItem>
              <ToggleGroupItem value="host">Set on this host</ToggleGroupItem>
            </ToggleGroup>
            <FieldDescription>
              Set them here for a one-off server; use an identity when several hosts share
              the same login.
            </FieldDescription>
          </Field>

          <Field v-if="form.credentials === 'identity'">
            <FieldLabel for="host-identity">Identity</FieldLabel>
            <Select v-model="form.identityId">
              <SelectTrigger id="host-identity">
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
            <FieldDescription>Inherits {{ inheritedIdentity }}.</FieldDescription>
          </Field>

          <template v-else>
            <Field :data-invalid="usernameInvalid ? '' : undefined">
              <FieldLabel for="host-username">Username</FieldLabel>
              <Input
                id="host-username"
                v-model="form.username"
                placeholder="root"
                :aria-invalid="usernameInvalid ? true : undefined"
              />
              <FieldDescription>
                May contain placeholders such as <code class="font-mono">{{ WG_USER_EXAMPLE }}</code>.
              </FieldDescription>
            </Field>

            <Field>
              <FieldLabel for="host-auth">Authentication</FieldLabel>
              <Select v-model="form.authKind">
                <SelectTrigger id="host-auth">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem value="password">Password</SelectItem>
                    <SelectItem value="key">Key</SelectItem>
                    <SelectItem value="agent">ssh-agent</SelectItem>
                    <SelectItem value="interactive">Keyboard interactive</SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>

            <Field v-if="needsPassword">
              <FieldLabel for="host-password">Password</FieldLabel>
              <Input
                id="host-password"
                v-model="form.password"
                type="password"
                :placeholder="props.host?.hasPassword ? 'Unchanged' : 'Stored in the vault'"
                @input="passwordTouched = true"
              />
              <FieldDescription>
                <template v-if="props.host?.hasPassword">
                  Leave blank to keep the stored password, or clear it and save to remove it.
                </template>
                <template v-else>
                  Encrypted with the vault key before it touches disk.
                </template>
              </FieldDescription>
            </Field>

            <Field v-if="needsKey">
              <FieldLabel for="host-key">Key</FieldLabel>
              <Select v-model="form.keyId">
                <SelectTrigger id="host-key">
                  <SelectValue placeholder="Any key" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    <SelectItem :value="INHERIT">
                      {{ form.authKind === 'agent' ? 'Any key the agent holds' : 'No key selected' }}
                    </SelectItem>
                    <SelectItem v-for="key in keys" :key="key.id" :value="key.id">
                      {{ key.label }} — {{ key.algorithm }}
                    </SelectItem>
                  </SelectGroup>
                </SelectContent>
              </Select>
            </Field>
          </template>

          <Separator />

          <AppearancePicker v-model:icon="form.icon" v-model:color="form.color" />

          <Field>
            <FieldLabel for="host-tags">Tags</FieldLabel>
            <Input id="host-tags" v-model="form.tags" placeholder="eu, edge" />
            <FieldDescription>Comma separated. Searchable from the host list.</FieldDescription>
          </Field>
        </FieldGroup>
      </div>

      <SheetFooter>
        <Button variant="ghost" @click="open = false">Cancel</Button>
        <Button
          :disabled="hostnameInvalid || portInvalid || usernameInvalid || saving.busy"
          @click="save"
        >
          {{ props.host ? 'Save changes' : 'Create host' }}
        </Button>
      </SheetFooter>
    </SheetContent>
  </Sheet>
</template>
