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

/**
 * Prefix of an icon key that names an app icon in the selfh.st catalog rather than a
 * built-in one. The key syncs like any other and each device fetches the image itself; a
 * build that predates the catalog does not know the prefix and shows its default icon.
 */
export const CATALOG_PREFIX = 'selfhst:'

/** The catalog reference a key names, or `null` for a built-in or unset icon. */
export function catalogReference(key: string | null | undefined): string | null {
  if (!key?.startsWith(CATALOG_PREFIX)) return null
  return key.slice(CATALOG_PREFIX.length) || null
}

export function catalogKey(reference: string): string {
  return `${CATALOG_PREFIX}${reference}`
}

/**
 * Falls back to the server icon for an unset or unrecognised key - a catalog key included,
 * which `EntityIcon` shows this for until the image arrives.
 */
export function hostIcon(key: string | null | undefined): Component {
  return (key && HOST_ICON_MAP.get(key)) || ServerIcon
}

export function groupIcon(key: string | null | undefined): Component {
  return (key && HOST_ICON_MAP.get(key)) || FolderIcon
}

export interface ColorOption {
  key: string
  label: string
  /** Border colour, for a marker along an edge. */
  border: string
  /** A filled swatch for the picker. */
  swatch: string
  /** A faint fill with a matching border, for a selected surface such as the active tab. */
  tint: string
  /** Foreground colour, for an icon. */
  text: string
}

/**
 * Markers only: an edge, an icon, or a faint tint behind something selected. A solid fill
 * would fight the terminal beneath it and the semantic tokens everywhere else.
 */
export const COLORS: ColorOption[] = [
  {
    key: 'default',
    label: 'None',
    border: 'border-border',
    swatch: 'bg-muted-foreground/40',
    tint: 'border-border bg-accent',
    text: 'text-muted-foreground',
  },
  {
    key: 'red',
    label: 'Red',
    border: 'border-red-500/70',
    swatch: 'bg-red-500',
    tint: 'border-red-500/40 bg-red-500/15',
    text: 'text-red-400',
  },
  {
    key: 'amber',
    label: 'Amber',
    border: 'border-amber-500/70',
    swatch: 'bg-amber-500',
    tint: 'border-amber-500/40 bg-amber-500/15',
    text: 'text-amber-400',
  },
  {
    key: 'green',
    label: 'Green',
    border: 'border-emerald-500/70',
    swatch: 'bg-emerald-500',
    tint: 'border-emerald-500/40 bg-emerald-500/15',
    text: 'text-emerald-400',
  },
  {
    key: 'teal',
    label: 'Teal',
    border: 'border-teal-500/70',
    swatch: 'bg-teal-500',
    tint: 'border-teal-500/40 bg-teal-500/15',
    text: 'text-teal-400',
  },
  {
    key: 'blue',
    label: 'Blue',
    border: 'border-blue-500/70',
    swatch: 'bg-blue-500',
    tint: 'border-blue-500/40 bg-blue-500/15',
    text: 'text-blue-400',
  },
  {
    key: 'violet',
    label: 'Violet',
    border: 'border-violet-500/70',
    swatch: 'bg-violet-500',
    tint: 'border-violet-500/40 bg-violet-500/15',
    text: 'text-violet-400',
  },
  {
    key: 'pink',
    label: 'Pink',
    border: 'border-pink-500/70',
    swatch: 'bg-pink-500',
    tint: 'border-pink-500/40 bg-pink-500/15',
    text: 'text-pink-400',
  },
]

const COLOR_MAP = new Map(COLORS.map(option => [option.key, option]))

/** Border class for a stored colour key. Unknown keys fall back to the default border. */
export function colorBorder(key: string | null | undefined): string {
  return (key && COLOR_MAP.get(key)?.border) || 'border-border'
}

/** Fill and border for a selected surface. Unknown keys get the neutral surface. */
export function colorTint(key: string | null | undefined): string {
  return (key && COLOR_MAP.get(key)?.tint) || 'border-border bg-accent'
}

/** Icon colour for a stored colour key. Unknown keys get the muted foreground. */
export function colorText(key: string | null | undefined): string {
  return (key && COLOR_MAP.get(key)?.text) || 'text-muted-foreground'
}

export function colorSwatch(key: string | null | undefined): string {
  return (key && COLOR_MAP.get(key)?.swatch) || 'bg-muted-foreground/40'
}

/** True when the host or group has been given a colour of its own. */
export function hasColor(key: string | null | undefined): boolean {
  return Boolean(key) && key !== 'default' && COLOR_MAP.has(key as string)
}
