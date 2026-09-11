/** Typed wrappers over the Rust command surface. Nothing else should call `invoke`. */

import { Channel, invoke } from '@tauri-apps/api/core'
import type {
  AgentKey,
  CatalogIcon,
  ConfigHost,
  ConnectRequest,
  DeviceLayout,
  DiscoveredKey,
  Group,
  GroupInput,
  Host,
  HostInput,
  Identity,
  IdentityInput,
  InstanceInfo,
  KeyMetaInput,
  Share,
  SshKey,
  SyncStatus,
  VarDef,
  VarDefInput,
  VarScope,
  VarValue,
  Workspace,
  KeyAlgorithm,
  KnownHostEntry,
  RemotierError,
  TargetPreview,
  VaultStatus,
} from '@/lib/types'

export const ipc = {
  listGroups: () => invoke<Group[]>('list_groups'),
  createGroup: (input: GroupInput) => invoke<Group>('create_group', { input }),
  updateGroup: (id: string, input: GroupInput) => invoke<Group>('update_group', { id, input }),
  deleteGroup: (id: string) => invoke<void>('delete_group', { id }),

  listHosts: () => invoke<Host[]>('list_hosts'),
  createHost: (input: HostInput) => invoke<Host>('create_host', { input }),
  updateHost: (id: string, input: HostInput) => invoke<Host>('update_host', { id, input }),
  deleteHost: (id: string) => invoke<void>('delete_host', { id }),
  moveHosts: (ids: string[], groupId: string | null) =>
    invoke<void>('move_hosts', { ids, groupId }),

  listIdentities: () => invoke<Identity[]>('list_identities'),
  createIdentity: (input: IdentityInput) => invoke<Identity>('create_identity', { input }),
  updateIdentity: (id: string, input: IdentityInput) =>
    invoke<Identity>('update_identity', { id, input }),
  deleteIdentity: (id: string) => invoke<void>('delete_identity', { id }),

  listKeys: () => invoke<SshKey[]>('list_keys'),
  generateKey: (label: string, algorithm: KeyAlgorithm, comment?: string, passphrase?: string) =>
    invoke<SshKey>('generate_key', { label, algorithm, comment, passphrase }),
  importKey: (label: string, pem: string, passphrase?: string) =>
    invoke<SshKey>('import_key', { label, pem, passphrase }),
  scanSystemKeys: () => invoke<DiscoveredKey[]>('scan_system_keys'),
  /** Records a key that stays on disk; the private half is never copied. */
  registerSystemKey: (args: {
    label: string
    path: string
    publicKey: string
    algorithm: string
    fingerprint: string
    comment: string | null
  }) => invoke<SshKey>('register_system_key', args),
  listAgentKeys: () => invoke<AgentKey[]>('list_agent_keys'),
  updateKey: (id: string, input: KeyMetaInput) => invoke<SshKey>('update_key', { id, input }),
  deleteKey: (id: string) => invoke<void>('delete_key', { id }),

  vaultStatus: () => invoke<VaultStatus>('vault_status'),

  /** The selfh.st icon catalog, fetched by Rust and cached on disk. */
  iconCatalog: () => invoke<CatalogIcon[]>('icon_catalog'),
  /** A catalog icon as a `data:` URL, downloaded the first time and served from cache after. */
  iconImage: (reference: string) => invoke<string>('icon_image', { reference }),

  resolveHost: (hostId: string) => invoke<TargetPreview>('resolve_host', { hostId }),
  /**
   * Opens a session. Terminal output arrives on `onData` as raw ArrayBuffers rather than
   * Tauri events - JSON-encoding every burst of output is the usual bottleneck here.
   */
  sshConnect: (request: ConnectRequest, onData: Channel<ArrayBuffer>) =>
    invoke<string>('ssh_connect', { request, onData }),
  sshWrite: (sessionId: string, data: Uint8Array) =>
    invoke<void>('ssh_write', { sessionId, data: Array.from(data) }),
  sshResize: (sessionId: string, cols: number, rows: number) =>
    invoke<void>('ssh_resize', { sessionId, cols, rows }),
  sshDisconnect: (sessionId: string) => invoke<void>('ssh_disconnect', { sessionId }),
  sshSessions: () => invoke<string[]>('ssh_sessions'),

  listKnownHosts: () => invoke<KnownHostEntry[]>('list_known_hosts'),
  /** Edits the user's real `~/.ssh/known_hosts`, so confirm before calling. */
  revokeKnownHost: (line: number) => invoke<void>('revoke_known_host', { line }),
  previewSshConfig: () => invoke<ConfigHost[]>('preview_ssh_config'),
  importSshConfig: (aliases: string[]) => invoke<number>('import_ssh_config', { aliases }),

  getSessionState: () => invoke<string | null>('get_session_state'),
  setSessionState: (payload: string) => invoke<void>('set_session_state', { payload }),

  listWorkspaces: () => invoke<Workspace[]>('list_workspaces'),
  saveWorkspace: (name: string, layoutJson: string) =>
    invoke<Workspace>('save_workspace', { name, layoutJson }),
  deleteWorkspace: (id: string) => invoke<void>('delete_workspace', { id }),

  getSettings: () => invoke<Record<string, string>>('get_settings'),
  setSetting: (key: string, value: string) => invoke<void>('set_setting', { key, value }),

  syncStatus: () => invoke<SyncStatus>('sync_status'),
  /** Ask an address whether it is a Remotier instance, before trusting it with anything. */
  syncProbeInstance: (url: string) => invoke<InstanceInfo>('sync_probe_instance', { url }),
  /** Returns the recovery code. It is shown once and never stored. */
  syncRegister: (url: string, email: string, password: string, deviceName: string) =>
    invoke<string>('sync_register', { url, email, password, deviceName }),
  syncLogin: (url: string, email: string, password: string, deviceName: string) =>
    invoke<SyncStatus>('sync_login', { url, email, password, deviceName }),
  syncRecover: (url: string, email: string, recoveryCode: string, deviceName: string) =>
    invoke<SyncStatus>('sync_recover', { url, email, recoveryCode, deviceName }),
  syncLogout: () => invoke<SyncStatus>('sync_logout'),
  syncNow: () => invoke<SyncStatus>('sync_now'),

  /** Machines with a saved layout, this one excluded. */
  syncDevices: () => invoke<DeviceLayout[]>('sync_devices'),
  /**
   * One device's saved tabs. Fetched on request and never applied by the engine:
   * replacing what is on screen is the user's decision, not a sync outcome.
   */
  syncDeviceLayout: (deviceId: string) => invoke<string>('sync_device_layout', { deviceId }),

  /**
   * Share a group and everything beneath it. The recipient gets hostnames, ports and
   * usernames in that branch — never a password, passphrase or private key.
   */
  syncShareGroup: (groupId: string, email: string) =>
    invoke<Share[]>('sync_share_group', { groupId, email }),
  syncListShares: (groupId: string) => invoke<Share[]>('sync_list_shares', { groupId }),
  /** Revokes and rotates the group key, so the removed member's copy stops working. */
  syncUnshareGroup: (groupId: string, userId: string) =>
    invoke<Share[]>('sync_unshare_group', { groupId, userId }),

  listVarDefs: () => invoke<VarDef[]>('list_var_defs'),
  upsertVarDef: (input: VarDefInput) => invoke<VarDef>('upsert_var_def', { input }),
  deleteVarDef: (id: string) => invoke<void>('delete_var_def', { id }),
  listVarValues: () => invoke<VarValue[]>('list_var_values'),
  setVarValue: (scope: VarScope, scopeId: string, name: string, value: string) =>
    invoke<void>('set_var_value', { scope, scopeId, name, value }),
  clearVarValue: (scope: VarScope, scopeId: string, name: string) =>
    invoke<void>('clear_var_value', { scope, scopeId, name }),
}

/** Narrow an unknown catch value to a tagged Rust error. */
export function asRemotierError(error: unknown): RemotierError | null {
  if (typeof error === 'object' && error !== null && 'kind' in error && 'message' in error) {
    return error as RemotierError
  }
  return null
}

export function errorMessage(error: unknown): string {
  const tagged = asRemotierError(error)
  if (tagged) return tagged.message
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return String(error)
}
