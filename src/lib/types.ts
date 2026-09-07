/** Mirrors `src-tauri/src/db/models.rs`. Keep the two in sync. */

export type AuthKind = 'password' | 'key' | 'agent' | 'interactive'
export type KeySource = 'managed' | 'system_path' | 'agent'
export type VarScope = 'group' | 'host'

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
  /** Correlates `ssh://progress` events with the pane that started this attempt. */
  attemptId: string
}

export interface PublicKeyInfo {
  algorithm: string
  fingerprint: string
  comment: string
  openssh: string
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

/** A step reported while a connection is being established. */
export type ConnectStage =
  | { stage: 'connecting', host: string, port: number }
  | { stage: 'hostKeyAccepted', fingerprint: string }
  | { stage: 'authenticating', method: string, username: string }
  | { stage: 'authenticated', method: string }
  | { stage: 'openingShell', term: string }
  | { stage: 'ready' }

/** Emitted on `ssh://progress`, correlated by the attempt id the caller supplied. */
export type ConnectProgress = ConnectStage & { attemptId: string }
