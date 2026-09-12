<script setup lang="ts">
import { computed, onMounted } from 'vue'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Field, FieldDescription, FieldLabel } from '@/components/ui/field'
import { Switch } from '@/components/ui/switch'
import { errorMessage } from '@/lib/ipc'
// Relative time, written for the sync panel and not specific to it. A second
// implementation here would be one that could drift.
import { lastSyncLabel } from '@/lib/sync-status'
import { describeProgress, downloadFraction } from '@/lib/updates'
import { useSettingsStore } from '@/stores/settings'
import { useUpdatesStore } from '@/stores/updates'
import { toast } from 'vue-sonner'

const settings = useSettingsStore()
const updates = useUpdatesStore()

onMounted(() => void updates.loadVersion())

const checked = computed(() =>
  updates.lastCheckedAt === null ? 'Not checked yet' : `Checked ${lastSyncLabel(updates.lastCheckedAt)}`,
)

/** The bar's width. Null while the size is unknown, where a bar would be a guess. */
const fraction = computed(() => downloadFraction(updates.progress))

async function setAutoCheck(value: boolean) {
  try {
    await settings.set('updates.autoCheck', String(value))
  } catch (e) {
    toast.error('Could not save the setting', { description: errorMessage(e) })
  }
}

/** A check the user asked for reports its failure; one on a timer stays quiet. */
async function checkNow() {
  const found = await updates.check()
  if (updates.error) {
    toast.error('Could not check for updates', { description: updates.error })
    return
  }
  if (!found) toast.success('Remotier is up to date')
}
</script>

<template>
  <div class="flex flex-col gap-4">
    <Card>
      <CardHeader>
        <CardTitle>Version</CardTitle>
        <CardDescription>
          Updates are downloaded from the project's GitHub releases and checked against a
          signature before anything is replaced.
        </CardDescription>
      </CardHeader>
      <CardContent class="flex flex-col gap-4">
        <div class="flex items-center gap-3">
          <span class="size-1.5 shrink-0" :class="updates.available ? 'bg-[var(--rm-lime)]' : 'bg-[var(--rm-idle)]'" />
          <div class="flex min-w-0 flex-col">
            <span class="truncate font-mono text-sm">{{ updates.version || 'unknown' }}</span>
            <span class="truncate text-xs text-muted-foreground">{{ checked }}</span>
          </div>
          <Button
            variant="outline"
            size="sm"
            class="ml-auto"
            :disabled="updates.busy"
            @click="checkNow"
          >
            {{ updates.state === 'checking' ? 'Checking…' : 'Check now' }}
          </Button>
        </div>

        <Field orientation="horizontal">
          <FieldLabel for="auto-check">Check automatically</FieldLabel>
          <Switch
            id="auto-check"
            :model-value="settings.get('updates.autoCheck') === 'true'"
            @update:model-value="value => setAutoCheck(Boolean(value))"
          />
          <FieldDescription>
            On launch and every few hours. Nothing is ever installed without you saying so:
            an update restarts the app, which ends every open session.
          </FieldDescription>
        </Field>

        <p v-if="updates.error" class="text-sm text-destructive">{{ updates.error }}</p>
      </CardContent>
    </Card>

    <Card v-if="updates.available">
      <CardHeader>
        <CardTitle>{{ updates.available.version }} is available</CardTitle>
        <CardDescription>
          Installing replaces this build and restarts. Every open SSH session ends.
        </CardDescription>
      </CardHeader>
      <CardContent class="flex flex-col gap-4">
        <p
          v-if="updates.available.notes"
          class="text-sm whitespace-pre-line text-muted-foreground"
        >{{ updates.available.notes }}</p>

        <template v-if="updates.state === 'downloading'">
          <div class="h-px w-full bg-border" aria-hidden="true">
            <div
              v-if="fraction !== null"
              class="h-px bg-primary"
              :style="{ width: `${Math.round(fraction * 100)}%` }"
            />
          </div>
          <p class="font-mono text-xs text-muted-foreground">
            Downloading {{ describeProgress(updates.progress) }}
          </p>
        </template>

        <div v-else class="flex items-center gap-2">
          <Button size="sm" @click="updates.install()">Install and restart</Button>
          <span class="text-xs text-muted-foreground">
            {{ updates.available.currentVersion }} → {{ updates.available.version }}
          </span>
        </div>
      </CardContent>
    </Card>
  </div>
</template>
