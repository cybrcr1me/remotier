/**
 * The small, curated sets of icons and colours a host or group can be given.
 *
 * Deliberately closed sets: the stored value is a key, so the palette can be restyled or
 * the icon set swapped without touching anyone's data, and an unknown key from a future
 * version degrades to the default rather than rendering nothing.
 */

import {
  BoxIcon,
  CloudIcon,
  ContainerIcon,
  CpuIcon,
  DatabaseIcon,
  FolderIcon,
  GlobeIcon,
  HardDriveIcon,
  LockIcon,
  MailIcon,
  NetworkIcon,
  ServerIcon,
  ShieldIcon,
  TerminalIcon,
  WrenchIcon,
  ZapIcon,
} from '@lucide/vue'
import type { Component } from 'vue'

export interface IconOption {
  key: string
  label: string
  icon: Component
}

/** Order matters: this is the order shown in the picker. */
export const HOST_ICONS: IconOption[] = [
  { key: 'server', label: 'Server', icon: ServerIcon },
  { key: 'database', label: 'Database', icon: DatabaseIcon },
  { key: 'cloud', label: 'Cloud', icon: CloudIcon },
  { key: 'container', label: 'Container', icon: ContainerIcon },
  { key: 'globe', label: 'Web', icon: GlobeIcon },
  { key: 'network', label: 'Network', icon: NetworkIcon },
  { key: 'shield', label: 'Security', icon: ShieldIcon },
  { key: 'lock', label: 'Bastion', icon: LockIcon },
  { key: 'cpu', label: 'Compute', icon: CpuIcon },
  { key: 'storage', label: 'Storage', icon: HardDriveIcon },
  { key: 'mail', label: 'Mail', icon: MailIcon },
  { key: 'build', label: 'Build', icon: WrenchIcon },
  { key: 'edge', label: 'Edge', icon: ZapIcon },
  { key: 'box', label: 'Box', icon: BoxIcon },
  { key: 'terminal', label: 'Terminal', icon: TerminalIcon },
]

const HOST_ICON_MAP = new Map(HOST_ICONS.map(option => [option.key, option.icon]))

/** Falls back to the server icon for an unset or unrecognised key. */
export function hostIcon(key: string | null | undefined): Component {
  return (key && HOST_ICON_MAP.get(key)) || ServerIcon
}

export function groupIcon(key: string | null | undefined): Component {
  return (key && HOST_ICON_MAP.get(key)) || FolderIcon
}

export interface ColorOption {
  key: string
  label: string
  /** Border colour, which is the only thing colour is used for. */
  border: string
  /** A filled swatch for the picker. */
  swatch: string
}

/**
 * Borders only. A full colour fill would fight the terminal beneath it and the semantic
 * tokens everywhere else, so colour is used purely as a marker.
 */
export const COLORS: ColorOption[] = [
  { key: 'default', label: 'None', border: 'border-border', swatch: 'bg-muted-foreground/40' },
  { key: 'red', label: 'Red', border: 'border-red-500/70', swatch: 'bg-red-500' },
  { key: 'amber', label: 'Amber', border: 'border-amber-500/70', swatch: 'bg-amber-500' },
  { key: 'green', label: 'Green', border: 'border-emerald-500/70', swatch: 'bg-emerald-500' },
  { key: 'teal', label: 'Teal', border: 'border-teal-500/70', swatch: 'bg-teal-500' },
  { key: 'blue', label: 'Blue', border: 'border-blue-500/70', swatch: 'bg-blue-500' },
  { key: 'violet', label: 'Violet', border: 'border-violet-500/70', swatch: 'bg-violet-500' },
  { key: 'pink', label: 'Pink', border: 'border-pink-500/70', swatch: 'bg-pink-500' },
]

const COLOR_MAP = new Map(COLORS.map(option => [option.key, option]))

/** Border class for a stored colour key. Unknown keys fall back to the default border. */
export function colorBorder(key: string | null | undefined): string {
  return (key && COLOR_MAP.get(key)?.border) || 'border-border'
}

export function colorSwatch(key: string | null | undefined): string {
  return (key && COLOR_MAP.get(key)?.swatch) || 'bg-muted-foreground/40'
}

/** True when the host or group has been given a colour of its own. */
export function hasColor(key: string | null | undefined): boolean {
  return Boolean(key) && key !== 'default' && COLOR_MAP.has(key as string)
}
