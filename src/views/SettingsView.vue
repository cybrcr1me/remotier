<script setup lang="ts">
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import PlaceholderSettings from '@/components/settings/PlaceholderSettings.vue'
import SyncSettings from '@/components/settings/SyncSettings.vue'
import { Field, FieldDescription, FieldGroup, FieldLabel } from '@/components/ui/field'
import { Input } from '@/components/ui/input'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Switch } from '@/components/ui/switch'
import { errorMessage, ipc } from '@/lib/ipc'
import type { SettingKey } from '@/stores/settings'
import type { VaultStatus } from '@/lib/types'
import { useSettingsStore } from '@/stores/settings'
import { settingsRoute, settingsTabFrom } from '@/lib/settings-tabs'
import { TriangleAlertIcon } from '@lucide/vue'
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { toast } from 'vue-sonner'

const settings = useSettingsStore()
const vault = ref<VaultStatus | null>(null)

const route = useRoute()
const router = useRouter()

/** Read from the route, not held here - see `lib/settings-tabs.ts` for why. */
const tab = computed(() => settingsTabFrom(route.query.tab))

function openTab(value: string | number) {
  // Replace, not push: flicking between tabs is not somewhere to go back to.
  router.replace(settingsRoute(settingsTabFrom(String(value))))
}

async function update(key: SettingKey, value: string) {
  try {
    await settings.set(key, value)
  } catch (e) {
    toast.error('Could not save the setting', { description: errorMessage(e) })
  }
}

onMounted(async () => {
  try {
    vault.value = await ipc.vaultStatus()
  } catch (e) {
    console.warn(`could not read the vault status: ${errorMessage(e)}`)
  }
})
</script>

<template>
  <ScrollArea class="h-full">
    <div class="mx-auto flex max-w-2xl flex-col gap-4 p-6">
      <Alert v-if="vault && !vault.available" variant="destructive">
        <TriangleAlertIcon />
        <AlertTitle>Secrets are unavailable</AlertTitle>
        <AlertDescription>
          {{ vault.error }} Hosts and groups still work, but passwords and key passphrases
          cannot be saved or read.
        </AlertDescription>
      </Alert>

      <Alert v-else-if="vault?.developmentKeyStore">
        <TriangleAlertIcon />
        <AlertTitle>Development key store</AlertTitle>
        <AlertDescription>
          This build keeps its vault key in a file next to the database rather than in the
          system keychain, and uses a separate development database.
        </AlertDescription>
      </Alert>

      <Tabs :model-value="tab" class="gap-4" @update:model-value="openTab">
        <TabsList>
          <TabsTrigger value="general">General</TabsTrigger>
          <TabsTrigger value="placeholders">Placeholders</TabsTrigger>
          <TabsTrigger value="sync">Sync</TabsTrigger>
        </TabsList>

        <TabsContent value="general" class="flex flex-col gap-4">
      <Card>
        <CardHeader>
          <CardTitle>Terminal</CardTitle>
          <CardDescription>Applies to terminals opened from now on.</CardDescription>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field>
              <FieldLabel for="font-size">Font size</FieldLabel>
              <Input
                id="font-size"
                :model-value="settings.get('terminal.fontSize')"
                inputmode="numeric"
                placeholder="13"
                class="max-w-24"
                @update:model-value="value => update('terminal.fontSize', String(value))"
              />
            </Field>

            <Field>
              <FieldLabel for="term">TERM</FieldLabel>
              <Input
                id="term"
                :model-value="settings.get('terminal.term')"
                placeholder="xterm-256color"
                class="max-w-64"
                @update:model-value="value => update('terminal.term', String(value))"
              />
              <FieldDescription>
                Sent to the server when the shell opens. Leave this alone unless a host
                needs something specific.
              </FieldDescription>
            </Field>
          </FieldGroup>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Sessions</CardTitle>
          <CardDescription>What happens to your tabs between launches.</CardDescription>
        </CardHeader>
        <CardContent>
          <FieldGroup>
            <Field orientation="horizontal">
              <FieldLabel for="auto-reconnect">Reconnect on launch</FieldLabel>
              <Switch
                id="auto-reconnect"
                :model-value="settings.get('session.autoReconnect') === 'true'"
                @update:model-value="value => update('session.autoReconnect', String(value))"
              />
              <FieldDescription>
                Off by default: restored tabs wait for you rather than opening every
                connection at startup.
              </FieldDescription>
            </Field>

            <Field>
              <FieldLabel for="default-port">Default SSH port</FieldLabel>
              <Input
                id="default-port"
                :model-value="settings.get('ssh.defaultPort')"
                inputmode="numeric"
                placeholder="22"
                class="max-w-24"
                @update:model-value="value => update('ssh.defaultPort', String(value))"
              />
              <FieldDescription>
                Used when neither the host nor any of its groups sets one.
              </FieldDescription>
            </Field>

            <Field>
              <FieldLabel for="agent-socket">ssh-agent socket</FieldLabel>
              <Input
                id="agent-socket"
                :model-value="settings.get('ssh.agentSocket')"
                placeholder="$SSH_AUTH_SOCK"
                class="font-mono text-xs"
                @update:model-value="value => update('ssh.agentSocket', String(value))"
              />
              <FieldDescription>
                Blank uses whichever agent the environment points at. Set this to reach a
                different one — on macOS every app is handed launchd's agent, and Apple's
                build ships no <code class="font-mono">ssh-sk-helper</code>, so it cannot
                sign for a hardware key however the key got in.
              </FieldDescription>
            </Field>
          </FieldGroup>
        </CardContent>
      </Card>
        </TabsContent>

        <TabsContent value="placeholders">
          <PlaceholderSettings />
        </TabsContent>

        <TabsContent value="sync">
          <SyncSettings />
        </TabsContent>
      </Tabs>
    </div>
  </ScrollArea>
</template>
