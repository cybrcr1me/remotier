/** Mirrors `src-tauri/src/db/models.rs`. Keep the two in sync. */

export type AuthKind = 'password' | 'key' | 'agent' | 'interactive'
export type KeySource = 'managed' | 'system_path' | 'agent'
/** `global` is the whole account and has an empty `scopeId`; there is only one. */
export type VarScope = 'global' | 'group' | 'host'

/** One icon in the selfh.st catalog. Mirrors `CatalogEntry` in `src-tauri/src/icons.rs`. */
export interface CatalogIcon {
  name: string
  reference: string
  tags: string[]
}

export interface Group {
  id: string
  parentId: string | null
  name: string
  sort: number
  defaultPort: number | null
  defaultIdentityId: string | null
  defaultJumpHostId: string | null
  /** Opaque key resolved by `lib/appearance`. */
  icon: string | null
  color: string | null
  createdAt: number
  updatedAt: number
}

export interface Host {
  id: string
  groupId: string | null
  label: string
  hostname: string
  /** `null` means inherit from the group chain, it is not the same as 22. */
  port: number | null
  identityId: string | null
  jumpHostId: string | null
  color: string | null
  /** Opaque key resolved by `lib/appearance`. */
  icon: string | null
  tags: string[]
  sort: number
  /** Set when the host carries its own username instead of using an identity's. */
  username: string | null
  /** `null` means "use the inherited identity"; otherwise these credentials win. */
  authKind: AuthKind | null
  /** Whether a password is stored on the host. The password itself never crosses IPC. */
  hasPassword: boolean
  keyId: string | null
  createdAt: number
  updatedAt: number
}

export interface Identity {
  id: string
  label: string
  /** May contain `{{placeholders}}`, resolved at connect time by the Rust side. */
  username: string
  authKind: AuthKind
  /** Whether a password is stored. The password itself never crosses the IPC boundary. */
  hasPassword: boolean
  keyId: string | null
  createdAt: number
  updatedAt: number
}

export interface SshKey {
  id: string
  label: string
  algorithm: string
  source: KeySource
  publicKey: string
  fingerprint: string
  path: string | null
  comment: string | null
  hasPassphrase: boolean
  createdAt: number
  updatedAt: number
}

/** A placeholder a group or host declares. Shared, and syncable later. */
export interface VarDef {
  id: string
  scope: VarScope
  scopeId: string
  name: string
  label: string | null
  defaultValue: string | null
  required: boolean
  createdAt: number
  updatedAt: number
}

/** This machine's answer to a `VarDef`. Local only, never synced. */
export interface VarValue {
  scope: VarScope
  scopeId: string
  name: string
  value: string
}

export interface GroupInput {
  name: string
  parentId?: string | null
  sort?: number | null
  defaultPort?: number | null
  defaultIdentityId?: string | null
  defaultJumpHostId?: string | null
  icon?: string | null
  color?: string | null
}

export interface HostInput {
  label: string
  hostname: string
  groupId?: string | null
  port?: number | null
  identityId?: string | null
  jumpHostId?: string | null
  color?: string | null
  icon?: string | null
  tags?: string[]
  sort?: number | null
  /** Credentials on the host itself. `authKind` of null keeps using the identity. */
  username?: string | null
  authKind?: AuthKind | null
  /** Omit to keep the stored password, `''` to clear it. */
  password?: string | null
  keyId?: string | null
}

export interface IdentityInput {
  label: string
  username: string
  authKind: AuthKind
  /** Omit to keep the stored password, `''` to clear it. */
  password?: string | null
  keyId?: string | null
}

export interface KeyMetaInput {
  label: string
  /** Omit to keep the stored passphrase, `''` to clear it. */
  passphrase?: string | null
}

export interface VarDefInput {
  scope: VarScope
  scopeId: string
  name: string
  label?: string | null
  defaultValue?: string | null
  required?: boolean
}

export interface VaultStatus {
  available: boolean
  error: string | null
  /** True when the dev key file is in use instead of the OS keychain. */
  developmentKeyStore: boolean
}

export type HostKeyPolicy = 'strict' | 'trustOnce' | 'trustAndSave'
export type KeyAlgorithm = 'ed25519' | 'rsa'

/** Resolved connection details, secrets excluded. */
export interface TargetPreview {
  hostId: string
  label: string
  hostname: string
  port: number
  username: string
  authKind: AuthKind
  identityLabel: string | null
  keyLabel: string | null
  /** Non-empty means connecting will fail until these are filled in. */
  missingVariables: string[]
}

export interface ConnectRequest {
  hostId: string
  cols: number
  rows: number
  policy?: HostKeyPolicy
  term?: string
  /** Used for this connection only; never stored. */
  password?: string
  /** A security key's PIN, for this connection only; never stored. */
  pin?: string
  /** Correlates `ssh://progress` events with the pane that started this attempt. */
  attemptId: string
}

export interface PublicKeyInfo {
  algorithm: string
  fingerprint: string
  comment: string
  openssh: string
  /** A FIDO key. The secret lives on a token, so it only works through the ssh-agent. */
  hardwareBacked: boolean
}

/** A key pair discovered in `~/.ssh`, left where it is. */
export interface DiscoveredKey extends PublicKeyInfo {
  path: string
  publicPath: string
  privateKeyPresent: boolean
  encrypted: boolean
}

export interface AgentKey {
  fingerprint: string
  algorithm: string
  comment: string
  openssh: string
}

/** Emitted on `ssh://session` when a session ends. */
export type SessionEvent =
  /** `exitStatus` is set only when the shell exited on its own, which closes its pane. */
  | { kind: 'closed', sessionId: string, exitStatus: number | null }
  | { kind: 'failed', sessionId: string, message: string }
  /** The connection died under us. The backend's watchdog found it, not the user. */
  | { kind: 'lost', sessionId: string, message: string }

/** Rust command errors arrive as tagged objects, never bare strings. */
export type RemotierError =
  | { kind: 'unknownHostKey', message: string, host: string, fingerprint: string }
  | { kind: 'changedHostKey', message: string, host: string, fingerprint: string, line: number }
  | { kind: 'unresolvedVariables', message: string, variables: string[] }
  | { kind: 'passwordRequired', message: string, username: string, host: string }
  | { kind: 'pinRequired', message: string }
  | { kind: string, message: string }

/** A `Host` block read from `~/.ssh/config`. */
export interface ConfigHost {
  alias: string
  hostname: string
  user: string | null
  port: number | null
  identityFile: string | null
  proxyJump: string | null
}

/** An entry in `~/.ssh/known_hosts`. */
export interface KnownHostEntry {
  /** 1-based file line, used to revoke it. */
  line: number
  /** `null` for hashed entries, whose hostnames cannot be recovered. */
  host: string | null
  algorithm: string
  fingerprint: string
  hashed: boolean
}

/** A named, saved tab layout. */
export interface Workspace {
  id: string
  name: string
  /** Opaque to the backend: a serialised session snapshot. */
  layoutJson: string
  createdAt: number
  updatedAt: number
}

/** What the sync engine is doing, as the settings panel and sidebar show it. */
export interface SyncStatus {
  signedIn: boolean
  instanceUrl: string | null
  email: string | null
  /** This machine's id. Breaks last-write-wins ties and names its stored layout. */
  deviceId: string
  lastSyncAt: number | null
  /** Records waiting to be pushed. */
  pending: number
  syncing: boolean
  /**
   * Local rows written or deleted by the last cycle. The engine writes SQLite directly,
   * so the stores hold a stale copy until they reload; this is what tells them to.
   */
  applied: number
  /**
   * The last failure, cleared by the next success. Sync failing is a condition that
   * usually fixes itself, not an error the user has to dismiss.
   */
  error: string | null
}

/**
 * One record a cycle sent or received. Local-only and capped in the database, so the
 * panel shows recent activity rather than an audit trail.
 */
export interface SyncHistoryEntry {
  at: number
  /** 'push' this machine sent it, 'pull' it arrived from another. */
  direction: 'push' | 'pull'
  action: 'written' | 'deleted'
  /** 'host', 'group', 'identity', 'var_def', 'workspace', 'setting', 'device_layout'. */
  kind: string
  recordId: string
  /**
   * The record's name when it went past. Null for a tombstone this machine pushed — the
   * row was already gone before the cycle started.
   */
  label: string | null
}

/** Another machine with a saved tab layout. */
export interface DeviceLayout {
  deviceId: string
  name: string
  updatedAt: number
}

/** One person a group is shared with, or one share you are a member of. */
export interface Share {
  groupId: string
  /** Whose group it is. Only the owner can share it further or revoke. */
  ownerId: string
  userId: string
  /** The other party: the member when you own the group, the owner when you do not. */
  email: string
  wrappedGroupKey: string
  createdAt: number
}

/** What `GET /v1/instance` answers. Probed before anything is sent to an address. */
export interface InstanceInfo {
  name: string
  version: string
  /** Envelope formats the instance accepts. A build not listed says so up front. */
  formatVersions: number[]
  registrationOpen: boolean
}

/** A step reported while a connection is being established. */
export type ConnectStage =
  | { stage: 'connecting', host: string, port: number }
  | { stage: 'hostKeyAccepted', fingerprint: string }
  | { stage: 'authenticating', method: string, username: string }
  | { stage: 'touchRequired' }
  | { stage: 'authenticated', method: string }
  | { stage: 'openingShell', term: string }
  | { stage: 'ready' }

/** Emitted on `ssh://progress`, correlated by the attempt id the caller supplied. */
export type ConnectProgress = ConnectStage & { attemptId: string }
