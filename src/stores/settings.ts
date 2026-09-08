import { defineStore } from 'pinia'
import { ref } from 'vue'
import { ipc } from '@/lib/ipc'

export const DEFAULT_SETTINGS = {
  'terminal.fontSize': '13',
  'terminal.term': 'xterm-256color',
  'ssh.defaultPort': '22',
  // Blank means "whatever SSH_AUTH_SOCK points at", which is right on most machines.
  'ssh.agentSocket': '',
  'session.autoReconnect': 'false',
  'appearance.theme': 'dark',
} as const

export type SettingKey = keyof typeof DEFAULT_SETTINGS

export const useSettingsStore = defineStore('settings', () => {
  const values = ref<Record<string, string>>({ ...DEFAULT_SETTINGS })

  function get(key: SettingKey) {
    return values.value[key] ?? DEFAULT_SETTINGS[key]
  }

  async function load() {
    const stored = await ipc.getSettings()
    values.value = { ...DEFAULT_SETTINGS, ...stored }
  }

  async function set(key: SettingKey, value: string) {
    await ipc.setSetting(key, value)
    values.value[key] = value
  }

  return { values, get, load, set }
})
