<script setup lang="ts">
/**
 * Asks for a security key's PIN, after the token has already refused without one.
 *
 * Shown on refusal rather than up front: a key file's `verify-required` flag does not
 * reliably say whether a PIN is set, so the only honest way to find out is to be told.
 */
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Field, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { ref, watch } from 'vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ submit: [pin: string], cancel: [] }>()

const pin = ref('')

watch(() => props.open, () => {
  pin.value = ''
})
</script>

<template>
  <Dialog :open="props.open">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Security key PIN</DialogTitle>
        <DialogDescription>
          The token will not sign without its PIN. It is used for this connection only and
          is never stored. Touch the key when it starts blinking.
        </DialogDescription>
      </DialogHeader>

      <Field>
        <FieldLabel for="security-key-pin">PIN</FieldLabel>
        <Input
          id="security-key-pin"
          v-model="pin"
          type="password"
          placeholder="Token PIN"
          autocomplete="off"
          @keydown.enter="emit('submit', pin)"
        />
      </Field>

      <DialogFooter>
        <Button variant="ghost" @click="emit('cancel')">
          Cancel
        </Button>
        <Button :disabled="pin.length === 0" @click="emit('submit', pin)">
          Continue
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
