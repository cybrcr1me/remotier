<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { LoaderIcon, PencilIcon } from '@lucide/vue'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { errorMessage } from '@/lib/ipc'
import { FORMAT_VERSION, instanceProblem, normaliseInstanceUrl } from '@/lib/sync-status'
import { DEFAULT_INSTANCE, useSyncStore } from '@/stores/sync'

const open = defineModel<boolean>('open', { required: true })

const sync = useSyncStore()

type Mode = 'signIn' | 'create' | 'recover'
const mode = ref<Mode>('signIn')

const email = ref('')
const password = ref('')
const recoveryCode = ref('')

const instanceUrl = ref(DEFAULT_INSTANCE)
const editingInstance = ref(false)
const probing = ref(false)
/** What the instance said about itself, once it answered. */
const instanceName = ref<string | null>(null)
const instanceError = ref<string | null>(null)
const registrationOpen = ref(true)

const error = ref<string | null>(null)

const normalised = computed(() => normaliseInstanceUrl(instanceUrl.value))
const canSubmit = computed(() => {
  if (sync.busy || probing.value || !normalised.value || instanceError.value) return false
  if (!email.value.includes('@')) return false
  return mode.value === 'recover' ? recoveryCode.value.length > 0 : password.value.length > 0
})

/**
 * Probe before anything is sent. A typo then reads as "no Remotier instance there"
 * rather than as a failed login against a stranger's server.
 */
async function probe() {
  const url = normalised.value
  instanceName.value = null
  instanceError.value = null
  if (!url) {
    instanceError.value = 'That is not a valid address.'
    return
  }

  probing.value = true
  try {
    const info = await sync.probe(url)
    const problem = instanceProblem(info, FORMAT_VERSION)
    if (problem) {
      instanceError.value = problem
      return
    }
    instanceName.value = info.name
    registrationOpen.value = info.registrationOpen
  } catch (e) {
    instanceError.value = errorMessage(e)
  } finally {
    probing.value = false
  }
}

async function submit() {
  const url = normalised.value
  if (!url) return
  error.value = null

  try {
    if (mode.value === 'create') {
      await sync.register(url, email.value, password.value)
    } else if (mode.value === 'recover') {
      await sync.recover(url, email.value, recoveryCode.value)
    } else {
      await sync.login(url, email.value, password.value)
    }
    password.value = ''
    recoveryCode.value = ''
    open.value = false
  } catch (e) {
    error.value = errorMessage(e)
  }
}

// Probe on open, and whenever the address changes, so the state on screen is never a
// claim about a server nobody has spoken to.
watch(open, isOpen => {
  if (isOpen) {
    error.value = null
    void probe()
  }
})
</script>

<template>
  <Dialog v-model:open="open">
    <DialogContent class="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>Sync</DialogTitle>
        <DialogDescription>
          Hosts, groups, identities and placeholders travel between your devices. Keys and
          passwords never leave this machine.
        </DialogDescription>
      </DialogHeader>

      <Tabs v-model="mode" class="gap-4">
        <TabsList class="w-full">
          <TabsTrigger value="signIn" class="flex-1">Sign in</TabsTrigger>
          <TabsTrigger value="create" class="flex-1">Create</TabsTrigger>
          <TabsTrigger value="recover" class="flex-1">Recover</TabsTrigger>
        </TabsList>

        <FieldGroup>
          <Field>
            <FieldLabel for="sync-instance">Instance</FieldLabel>
            <div v-if="!editingInstance" class="flex items-center gap-2">
              <span class="font-mono text-sm text-muted-foreground truncate">
                {{ normalised ?? instanceUrl }}
              </span>
              <Button
                variant="ghost"
                size="sm"
                class="ml-auto shrink-0"
                @click="editingInstance = true"
              >
                <PencilIcon class="size-3.5" />
                Change
              </Button>
            </div>
            <Input
              v-else
              id="sync-instance"
              v-model="instanceUrl"
              class="font-mono"
              placeholder="sync.example.com"
              @blur="probe"
              @keydown.enter.prevent="probe"
            />
            <FieldDescription v-if="probing" class="flex items-center gap-1.5">
              <LoaderIcon class="size-3 animate-spin" />
              Checking.
            </FieldDescription>
            <FieldDescription v-else-if="instanceError" class="text-destructive">
              {{ instanceError }}
            </FieldDescription>
            <FieldDescription v-else-if="instanceName">
              {{ instanceName }}<template v-if="mode === 'create' && !registrationOpen">
                — registration is closed here.</template>
            </FieldDescription>
          </Field>

          <Field>
            <FieldLabel for="sync-email">Email</FieldLabel>
            <Input
              id="sync-email"
              v-model="email"
              type="email"
              autocomplete="username"
              placeholder="you@example.com"
            />
          </Field>

          <TabsContent value="recover" class="m-0">
            <Field>
              <FieldLabel for="sync-recovery">Recovery code</FieldLabel>
              <Input
                id="sync-recovery"
                v-model="recoveryCode"
                class="font-mono"
                placeholder="ABCDE-FGHJK-MNPQR-STVWX"
              />
              <FieldDescription>
                The code shown when the account was created. Set a new password after.
              </FieldDescription>
            </Field>
          </TabsContent>

          <Field v-if="mode !== 'recover'">
            <FieldLabel for="sync-password">Password</FieldLabel>
            <Input
              id="sync-password"
              v-model="password"
              type="password"
              :autocomplete="mode === 'create' ? 'new-password' : 'current-password'"
              :placeholder="mode === 'create' ? 'At least 12 characters' : 'Your sync password'"
              @keydown.enter.prevent="canSubmit && submit()"
            />
            <FieldDescription v-if="mode === 'create'">
              This password encrypts your config before it leaves the machine. The server
              never sees it, and cannot reset it. You get one recovery code.
            </FieldDescription>
          </Field>
        </FieldGroup>
      </Tabs>

      <p v-if="error" class="text-sm text-destructive">{{ error }}</p>

      <DialogFooter>
        <Button variant="ghost" @click="open = false">Cancel</Button>
        <Button :disabled="!canSubmit" @click="submit">
          <LoaderIcon v-if="sync.busy" class="size-4 animate-spin" />
          {{ mode === 'create' ? 'Create account' : mode === 'recover' ? 'Recover' : 'Sign in' }}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
