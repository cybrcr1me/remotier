import { KeyRound, MonitorCog, ServerIcon, Settings, ShieldCheck, TerminalIcon } from '@lucide/vue'
import type { Component } from 'vue'

export interface NavEntry {
  to: string
  label: string
  icon: Component
}

export const navEntries: NavEntry[] = [
  { to: '/terminals', label: 'Terminals', icon: TerminalIcon },
  { to: '/hosts', label: 'Hosts', icon: ServerIcon },
  { to: '/keychain', label: 'Keychain', icon: KeyRound },
  { to: '/identities', label: 'Identities', icon: MonitorCog },
  { to: '/known-hosts', label: 'Known hosts', icon: ShieldCheck },
  { to: '/settings', label: 'Settings', icon: Settings },
]
