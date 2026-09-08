<script setup lang="ts">
import { ref } from 'vue'
import { CheckIcon, CopyIcon } from '@lucide/vue'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { toast } from 'vue-sonner'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { useSyncStore } from '@/stores/sync'

const sync = useSyncStore()

const copied = ref(false)
const acknowledged = ref(false)

async function copy() {
  if (!sync.recoveryCode) return
  try {
    await writeText(sync.recoveryCode)
    copied.value = true
  } catch {
    toast.error('Could not copy. Write it down instead.')
  }
}

function dismiss() {
  copied.value = false
  acknowledged.value = false
  sync.dismissRecoveryCode()
}
</script>

<template>
  <!--
    Deliberately not dismissible by clicking away or pressing Escape. This code is shown
    once and is the only way back into the account; closing it by accident is the failure
    mode the whole dialog exists to prevent.
  -->
  <Dialog :open="sync.recoveryCode !== null">
    <DialogContent
      class="sm:max-w-md"
      @escape-key-down.prevent
      @pointer-down-outside.prevent
      @interact-outside.prevent
    >
      <DialogHeader>
        <DialogTitle>Recovery code</DialogTitle>
        <DialogDescription>
          Store this somewhere safe. If you forget your password, this is the only way
          back in — the server holds no key and cannot reset it for you.
        </DialogDescription>
      </DialogHeader>

      <p class="rounded-md border bg-muted/40 p-4 text-center font-mono text-lg tracking-widest select-all">
        {{ sync.recoveryCode }}
      </p>

      <Button variant="outline" class="w-full" @click="copy">
        <CheckIcon v-if="copied" class="size-4" />
        <CopyIcon v-else class="size-4" />
        {{ copied ? 'Copied' : 'Copy' }}
      </Button>

      <div class="flex items-start gap-2">
        <Checkbox id="recovery-ack" v-model="acknowledged" class="mt-0.5" />
        <Label for="recovery-ack" class="text-sm font-normal leading-snug">
          I have saved it. I understand it will not be shown again.
        </Label>
      </div>

      <DialogFooter>
        <Button :disabled="!acknowledged" @click="dismiss">Done</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
</template>
