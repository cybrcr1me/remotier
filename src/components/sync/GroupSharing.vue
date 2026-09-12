<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { LoaderIcon, XIcon } from '@lucide/vue'
import { toast } from 'vue-sonner'

import { Button } from '@/components/ui/button'
import { Field, FieldDescription, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { errorMessage, ipc } from '@/lib/ipc'
import type { Share } from '@/lib/types'
import { useSyncStore } from '@/stores/sync'

const props = defineProps<{
  /** `null` for a group that has not been saved yet — there is nothing to share. */
  groupId: string | null
}>()

const sync = useSyncStore()

const shares = ref<Share[]>([])
const email = ref('')
const busy = ref(false)
const loading = ref(false)

const canShare = computed(
  () => !busy.value && props.groupId !== null && email.value.includes('@'),
)

async function load() {
  if (!props.groupId || !sync.signedIn) {
    shares.value = []
    return
  }
  loading.value = true
  try {
    shares.value = await ipc.syncListShares(props.groupId)
  } catch (e) {
    // Not fatal: the group editor still works, sharing just cannot be shown.
    console.warn('could not list shares', errorMessage(e))
    shares.value = []
  } finally {
    loading.value = false
  }
}

async function share() {
  if (!props.groupId) return
  busy.value = true
  try {
    shares.value = await ipc.syncShareGroup(props.groupId, email.value.trim())
    email.value = ''
  } catch (e) {
    toast.error('Could not share the group', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}

async function revoke(member: Share) {
  if (!props.groupId) return
  busy.value = true
  try {
    shares.value = await ipc.syncUnshareGroup(props.groupId, member.userId)
    toast.success(`${member.email} removed`, {
      description: 'The group key was rotated. They keep whatever they already copied.',
    })
  } catch (e) {
    toast.error('Could not remove them', { description: errorMessage(e) })
  } finally {
    busy.value = false
  }
}

watch(() => props.groupId, load, { immediate: true })
</script>

<template>
  <div v-if="sync.signedIn" class="flex flex-col gap-3">
    <Field>
      <FieldLabel for="share-email">Shared with</FieldLabel>
      <div class="flex gap-2">
        <Input
          id="share-email"
          v-model="email"
          type="email"
          placeholder="colleague@example.com"
          :disabled="!groupId"
          @keydown.enter.prevent="canShare && share()"
        />
        <Button :disabled="!canShare" @click="share">
          <LoaderIcon v-if="busy" class="size-4 animate-spin" />
          Share
        </Button>
      </div>
      <FieldDescription v-if="!groupId">
        Save the group first.
      </FieldDescription>
      <FieldDescription v-else>
        They get the hosts in this group and everything under it — addresses, ports and
        usernames. Never a password, a passphrase or a key.
      </FieldDescription>
    </Field>

    <ul v-if="shares.length" class="flex flex-col">
      <li
        v-for="member in shares"
        :key="member.userId"
        class="flex h-9 items-center gap-2 border-b last:border-b-0"
      >
        <span class="size-1.5 shrink-0 bg-[var(--rm-ok)]" />
        <span class="truncate text-sm">{{ member.email }}</span>
        <Button
          variant="ghost"
          size="sm"
          class="ml-auto"
          :disabled="busy"
          :aria-label="`Remove ${member.email}`"
          @click="revoke(member)"
        >
          <XIcon class="size-3.5" />
        </Button>
      </li>
    </ul>
    <p v-else-if="!loading && groupId" class="text-sm text-muted-foreground">
      Not shared with anyone.
    </p>
  </div>
</template>
