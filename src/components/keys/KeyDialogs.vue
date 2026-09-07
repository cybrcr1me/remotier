<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { ButtonGroup } from '@/components/ui/button-group'
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
import { Textarea } from '@/components/ui/textarea'
import { errorMessage } from '@/lib/ipc'
import type { KeyAlgorithm } from '@/lib/types'
import { useCredentialsStore } from '@/stores/credentials'
import { computed, reactive, ref, watch } from 'vue'
import { toast } from 'vue-sonner'

const generateOpen = defineModel<boolean>('generateOpen', { required: true })
const importOpen = defineModel<boolean>('importOpen', { required: true })

const ALGORITHMS: { value: KeyAlgorithm, label: string }[] = [
  { value: 'ed25519', label: 'Ed25519' },
  { value: 'rsa', label: 'RSA 4096' },
]

const credentials = useCredentialsStore()
const busy = ref(false)

const generate = reactive({
  label: '',
  algorithm: 'ed25519' as KeyAlgorithm,
  comment: '',
  passphrase: '',
})

const imported = reactive({ label: '', pem: '', passphrase: '' })

const generateInvalid = computed(() => generate.label.trim().length === 0)
const importInvalid = computed(
  () => imported.label.trim().length === 0 || imported.pem.trim().length === 0,
)

watch(generateOpen, (open) => {
  if (open) Object.assign(generate, { label: '', algorithm: 'ed25519', comment: '', passphrase: '' })
})

watch(importOpen, (open) => {
  if (open) Object.assign(imported, { label: '', pem: '', passphrase: '' })
})

async function runGenerate() {
  if (generateInvalid.value) return
  busy.value = true
  try {
    await credentials.generateKey(
      generate.label.trim(),
      generate.algorithm,
      generate.comment.trim() || undefined,
      generate.passphrase || undefined,
    )
    generateOpen.value = false
    toast.success('Key generated', { description: 'The private half is sealed in the vault.' })
  } catch (e) {
    toast.error('Could not generate the key', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}

async function runImport() {
  if (importInvalid.value) return
  busy.value = true
  try {
    await credentials.importKey(
      imported.label.trim(),
      imported.pem,
      imported.passphrase || undefined,
    )
    importOpen.value = false
    toast.success('Key imported')
  } catch (e) {
    // A wrong passphrase lands here, which is the common case worth reading.
    toast.error('Could not import the key', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <Dialog v-model:open="generateOpen">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Generate a key</DialogTitle>
        <DialogDescription>
          The private key is encrypted with the vault key before it is written to disk.
        </DialogDescription>
      </DialogHeader>

      <FieldGroup>
        <Field :data-invalid="generateInvalid ? '' : undefined">
          <FieldLabel for="generate-label">Name</FieldLabel>
          <Input
            id="generate-label"
            v-model="generate.label"
            placeholder="Work laptop"
            :aria-invalid="generateInvalid ? true : undefined"
          />
        </Field>

        <Field>
          <FieldLabel>Algorithm</FieldLabel>
          <!--
            Both buttons keep `variant="outline"` and the selected one is marked by its
            fill alone. Switching the selected button to another variant would change its
            border colour too, which breaks the seam the group draws between them.
          -->
          <ButtonGroup orientation="horizontal">
            <Button
              v-for="option in ALGORITHMS"
              :key="option.value"
              type="button"
              variant="outline"
              :aria-pressed="generate.algorithm === option.value"
              :class="generate.algorithm === option.value && 'bg-secondary dark:bg-secondary text-foreground'"
              @click="generate.algorithm = option.value"
            >
              {{ option.label }}
            </Button>
          </ButtonGroup>
          <FieldDescription>
            Ed25519 is smaller and faster. Choose RSA only for servers that require it.
          </FieldDescription>
        </Field>

        <Field>
          <FieldLabel for="generate-comment">Comment</FieldLabel>
          <Input id="generate-comment" v-model="generate.comment" placeholder="you@laptop" />
        </Field>

        <Field>
          <FieldLabel for="generate-passphrase">Passphrase</FieldLabel>
          <Input
            id="generate-passphrase"
            v-model="generate.passphrase"
            type="password"
            placeholder="Optional"
          />
          <FieldDescription>
            Optional. The vault already encrypts the key at rest; a passphrase adds a second
            layer if the key is ever exported.
          </FieldDescription>
        </Field>
      </FieldGroup>

      <DialogFooter>
        <Button variant="ghost" @click="generateOpen = false">Cancel</Button>
        <Button :disabled="generateInvalid || busy" @click="runGenerate">Generate</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>

  <Dialog v-model:open="importOpen">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Import a key</DialogTitle>
        <DialogDescription>
          Paste an OpenSSH private key. It is parsed before anything is saved, so a bad key
          or wrong passphrase is caught here.
        </DialogDescription>
      </DialogHeader>

      <FieldGroup>
        <Field :data-invalid="imported.label.trim() ? undefined : ''">
          <FieldLabel for="import-label">Name</FieldLabel>
          <Input id="import-label" v-model="imported.label" placeholder="Deploy key" />
        </Field>

        <Field :data-invalid="imported.pem.trim() ? undefined : ''">
          <FieldLabel for="import-pem">Private key</FieldLabel>
          <Textarea
            id="import-pem"
            v-model="imported.pem"
            rows="8"
            class="font-mono text-xs"
            placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
          />
        </Field>

        <Field>
          <FieldLabel for="import-passphrase">Passphrase</FieldLabel>
          <Input
            id="import-passphrase"
            v-model="imported.passphrase"
            type="password"
            placeholder="Only if the key is encrypted"
          />
          <FieldDescription>Only needed if the key is encrypted.</FieldDescription>
        </Field>
      </FieldGroup>

      <DialogFooter>
        <Button variant="ghost" @click="importOpen = false">Cancel</Button>
        <Button :disabled="importInvalid || busy" @click="runImport">Import</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
