<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Field, FieldDescription, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import type { PasswordPrompt } from '@/lib/connect-flow'
import { ref, watch } from 'vue'

const props = defineProps<{ prompt: PasswordPrompt | null }>()
const emit = defineEmits<{
  submit: [password: string, remember: boolean]
  cancel: []
}>()

const password = ref('')
const remember = ref(false)

watch(() => props.prompt, () => {
  password.value = ''
  remember.value = false
})
</script>

<template>
  <Dialog :open="props.prompt !== null">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Password required</DialogTitle>
        <DialogDescription>
          {{ props.prompt?.username }}@{{ props.prompt?.host }} needs a password. It is used
          for this connection only unless you choose to save it.
        </DialogDescription>
      </DialogHeader>

      <Field>
        <FieldLabel for="connect-password">Password</FieldLabel>
        <Input
          id="connect-password"
          v-model="password"
          type="password"
          placeholder="Password"
          autocomplete="off"
          @keyup.enter="emit('submit', password, remember)"
        />
      </Field>

      <Field orientation="horizontal">
        <Checkbox id="remember-password" v-model="remember" />
        <FieldLabel for="remember-password" class="font-normal">
          Save it for next time
        </FieldLabel>
        <FieldDescription>
          Stored encrypted with the vault key.
        </FieldDescription>
      </Field>

      <DialogFooter>
        <Button variant="ghost" @click="emit('cancel')">Cancel</Button>
        <Button @click="emit('submit', password, remember)">Connect</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
