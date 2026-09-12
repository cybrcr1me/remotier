/**
 * The connect handshake, including the host key prompt.
 *
 * Kept apart from the terminal component so the trust decisions can be tested without a
 * DOM or a live server.
 *
 * The first attempt is always strict. An unknown host comes back as an error carrying the
 * fingerprint, the user is asked, and only then is the connection retried with a policy
 * that accepts it. A *changed* key is never offered as a prompt - it is the one case that
 * should stop the user rather than ask them.
 */

import { asRemotierError } from '@/lib/ipc'
import type { ConnectRequest, HostKeyPolicy } from '@/lib/types'

export interface HostKeyPrompt {
  host: string
  fingerprint: string
}

export interface PasswordPrompt {
  username: string
  host: string
}

/** What the user chose when shown an unknown host key. */
export type HostKeyDecision = 'reject' | 'once' | 'save'

export interface ConnectFlowOptions {
  request: ConnectRequest
  /** Performs the actual connect; injected so tests need no IPC. */
  connect: (request: ConnectRequest) => Promise<string>
  /** Shown when the host is not in `known_hosts`. */
  askAboutHostKey: (prompt: HostKeyPrompt) => Promise<HostKeyDecision>
  /**
   * Shown when a `{{placeholder}}` has no value. Resolves to the values the user typed,
   * or `null` if they cancelled.
   */
  askAboutVariables?: (names: string[]) => Promise<Record<string, string> | null>
  /** Persists the answers before retrying. */
  saveVariables?: (values: Record<string, string>) => Promise<void>
  /**
   * Shown when password authentication is configured but nothing is stored. Resolves to
   * the password, or `null` if the user cancelled.
   */
  askForPassword?: (prompt: PasswordPrompt) => Promise<string | null>
  /** Called when a security key refuses to sign without its PIN. */
  askForPin?: () => Promise<string | null>
}

const POLICY_FOR: Record<Exclude<HostKeyDecision, 'reject'>, HostKeyPolicy> = {
  once: 'trustOnce',
  save: 'trustAndSave',
}

export class ConnectCancelled extends Error {
  constructor(reason: string) {
    super(reason)
    this.name = 'ConnectCancelled'
  }
}

export class HostKeyRejected extends Error {
  // Written out rather than a constructor parameter property, which `erasableSyntaxOnly`
  // rejects because it emits code.
  readonly fingerprint: string

  constructor(fingerprint: string) {
    super('Connection cancelled: the host key was not trusted.')
    this.name = 'HostKeyRejected'
    this.fingerprint = fingerprint
  }
}

/**
 * Connect, prompting once if the host is unknown.
 *
 * @returns the new session id.
 */
export async function connectWithHostKeyPrompt(options: ConnectFlowOptions): Promise<string> {
  const {
    request,
    connect,
    askAboutHostKey,
    askAboutVariables,
    saveVariables,
    askForPassword,
    askForPin,
  } = options

  // Unresolved variables and a missing password are both collected before the connection
  // is attempted, so they are handled ahead of the host key.
  let attempt: ConnectRequest = { ...request, policy: 'strict' }
  let askedForPassword = false
  let askedForPin = false
  let askedAboutHostKey = false

  /*
   * One round per thing that can be asked, plus the attempt that succeeds: variables, the
   * host key, a password and a PIN can all come up on the same connection, and each guard
   * below makes sure none of them can be asked twice.
   */
  for (let round = 0; round < 5; round += 1) {
    try {
      return await connect(attempt)
    } catch (error) {
      const tagged = asRemotierError(error)

      if (tagged?.kind === 'unresolvedVariables' && askAboutVariables && round === 0) {
        const names = (tagged as { variables: string[] }).variables
        const values = await askAboutVariables(names)
        if (!values) {
          throw new ConnectCancelled('Connection cancelled: variables were not filled in.')
        }
        await saveVariables?.(values)
        continue
      }

      if (tagged?.kind === 'passwordRequired' && askForPassword && !askedForPassword) {
        askedForPassword = true
        const { username, host } = tagged as PasswordPrompt & { kind: string }
        const password = await askForPassword({ username, host })
        if (password === null) {
          throw new ConnectCancelled('Connection cancelled: no password was given.')
        }
        // Carried on the request only; the backend never writes it down.
        attempt = { ...attempt, password }
        continue
      }

      // The token asked for its PIN. A key file's flags do not reliably say whether one
      // is set, so this is only knowable by having been refused once.
      if (tagged?.kind === 'pinRequired' && askForPin && !askedForPin) {
        askedForPin = true
        const pin = await askForPin()
        if (pin === null) {
          throw new ConnectCancelled('Connection cancelled: no PIN was given.')
        }
        attempt = { ...attempt, pin }
        continue
      }

      if (tagged?.kind !== 'unknownHostKey') {
        // Changed keys, auth failures and everything else propagate untouched.
        throw error
      }

      if (askedAboutHostKey) {
        throw error
      }
      askedAboutHostKey = true

      const { host, fingerprint } = tagged as HostKeyPrompt & { kind: string }
      const decision = await askAboutHostKey({ host, fingerprint })

      if (decision === 'reject') {
        throw new HostKeyRejected(fingerprint)
      }

      /*
       * Spread `attempt`, not `request`: anything already collected - a password, a
       * security key's PIN - has to survive being asked about the host key, or it is
       * thrown away and asked for again.
       *
       * And loop rather than connecting here: this attempt can raise a prompt of its own,
       * which only the loop knows how to answer. Returning straight from here is why a
       * hardware key on an unknown host failed with no PIN prompt at all.
       */
      attempt = { ...attempt, policy: POLICY_FOR[decision] }
      continue
    }
  }

  // Unreachable: the loop either returns or throws. Present to satisfy the return type.
  throw new ConnectCancelled('Connection cancelled.')
}
