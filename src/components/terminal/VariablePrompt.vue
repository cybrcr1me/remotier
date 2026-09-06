<script setup lang="ts">
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Field, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { reactive, watch } from 'vue'

const props = defineProps<{ names: string[] }>()
const emit = defineEmits<{ submit: [values: Record<string, string>], cancel: [] }>()

const values = reactive<Record<string, string>>({})

watch(
  () => props.names,
  (names) => {
    for (const name of names) values[name] = values[name] ?? ''
  },
  { immediate: true },
)

function submit() {
  emit('submit', { ...values })
}
</script>

<template>
  <Dialog :open="props.names.length > 0">
    <DialogContent>
      <DialogHeader>
        <DialogTitle>Fill in connection details</DialogTitle>
        <DialogDescription>
          This host uses placeholders that have no value yet. What you enter is saved on
          this machine only and is never shared.
        </DialogDescription>
      </DialogHeader>

      <FieldGroup>
        <Field v-for="name in props.names" :key="name">
          <FieldLabel :for="`prompt-${name}`">
            {{ name }}
          </FieldLabel>
          <Input :id="`prompt-${name}`" v-model="values[name]" autocomplete="off" />
        </Field>
      </FieldGroup>

      <DialogFooter>
        <Button variant="ghost" @click="emit('cancel')">Cancel</Button>
        <Button @click="submit">Connect</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
