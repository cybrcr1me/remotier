<script setup lang="ts">
import { Button } from '@/components/ui/button'
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
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { INHERIT, resolveInherited } from '@/lib/inherit'
import { WG_USER_EXAMPLE } from '@/lib/placeholder'
import { errorMessage } from '@/lib/ipc'
import type { AuthKind, Identity } from '@/lib/types'
import { useCredentialsStore } from '@/stores/credentials'
import { storeToRefs } from 'pinia'
import { computed, reactive, ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const open = defineModel<boolean>('open', { required: true })
const props = defineProps<{ identity: Identity | null }>()

const credentials = useCredentialsStore()
const { keys } = storeToRefs(credentials)

const form = reactive({
  label: '',
  username: '',
  authKind: 'agent' as AuthKind,
  password: '',
  keyId: INHERIT,
})
const busy = ref(false)
/** Tracks whether the user typed in the password box at all. */
const passwordTouched = ref(false)

const usernameInvalid = computed(() => form.username.trim().length === 0)

watch(
  () => [open.value, props.identity] as const,
  ([isOpen]) => {
    if (!isOpen) return
    form.label = props.identity?.label ?? ''
    form.username = props.identity?.username ?? ''
    form.authKind = props.identity?.authKind ?? 'agent'
    form.keyId = props.identity?.keyId ?? INHERIT
    form.password = ''
    passwordTouched.value = false
  },
  { immediate: true },
)

async function save() {
  if (usernameInvalid.value) return

  busy.value = true
  try {
    const input = {
      label: form.label.trim() || form.username.trim(),
      username: form.username.trim(),
      authKind: form.authKind,
      keyId: resolveInherited(form.keyId),
      // Undefined keeps the stored password; only send a value the user actually typed.
      password: passwordTouched.value ? form.password : undefined,
    }

    if (props.identity) await credentials.updateIdentity(props.identity.id, input)
    else await credentials.createIdentity(input)
    open.value = false
  } catch (e) {
    toast.error('Could not save the identity', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Sheet v-model:open="open">
    <SheetContent class="flex w-full flex-col sm:max-w-lg">
      <SheetHeader>
        <SheetTitle>{{ props.identity ? 'Edit identity' : 'New identity' }}</SheetTitle>
        <SheetDescription>
          The username may contain placeholders such as
          <code class="font-mono">{{ WG_USER_EXAMPLE }}</code>.
        </SheetDescription>
      </SheetHeader>

      <div class="min-h-0 flex-1 overflow-y-auto px-4">
        <FieldGroup>
          <Field :data-invalid="usernameInvalid ? '' : undefined">
            <FieldLabel for="identity-username">Username</FieldLabel>
            <Input
              id="identity-username"
              v-model="form.username"
              :aria-invalid="usernameInvalid ? true : undefined"
              placeholder="deploy"
            />
          </Field>

          <Field>
            <FieldLabel for="identity-label">Name</FieldLabel>
            <Input id="identity-label" v-model="form.label" placeholder="Defaults to the username" />
          </Field>

          <Field>
            <FieldLabel for="identity-auth">Authentication</FieldLabel>
            <Select v-model="form.authKind">
              <SelectTrigger id="identity-auth">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="agent">ssh-agent</SelectItem>
                  <SelectItem value="key">Key</SelectItem>
                  <SelectItem value="password">Password</SelectItem>
                  <SelectItem value="interactive">Keyboard interactive</SelectItem>
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>

          <Field v-if="form.authKind === 'key' || form.authKind === 'agent'">
            <FieldLabel for="identity-key">Key</FieldLabel>
            <Select v-model="form.keyId">
              <SelectTrigger id="identity-key">
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

          <Field v-if="form.authKind === 'password' || form.authKind === 'interactive'">
            <FieldLabel for="identity-password">Password</FieldLabel>
            <Input
              id="identity-password"
              v-model="form.password"
              type="password"
              :placeholder="props.identity?.hasPassword ? 'Unchanged' : 'Stored in the vault'"
              @input="passwordTouched = true"
            />
            <FieldDescription>
              <template v-if="props.identity?.hasPassword">
                Leave blank to keep the stored password, or clear the field and save to remove it.
              </template>
              <template v-else>
                Encrypted with the vault key before it touches disk.
              </template>
            </FieldDescription>
          </Field>
        </FieldGroup>
      </div>

      <SheetFooter>
        <Button variant="ghost" @click="open = false">Cancel</Button>
        <Button :disabled="usernameInvalid || busy" @click="save">
          {{ props.identity ? 'Save changes' : 'Create identity' }}
        </Button>
      </SheetFooter>
    </SheetContent>
  </Sheet>
</template>
