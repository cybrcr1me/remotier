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
  const { request, connect, askAboutHostKey, askAboutVariables, saveVariables } = options

  // Unresolved variables are collected first: they fail before the connection is even
  // attempted, so asking for the host key first would be the wrong order.
  let attempt: ConnectRequest = { ...request, policy: 'strict' }

  for (let round = 0; round < 2; round += 1) {
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

      if (tagged?.kind !== 'unknownHostKey') {
        // Changed keys, auth failures and everything else propagate untouched.
        throw error
      }

      const { host, fingerprint } = tagged as HostKeyPrompt & { kind: string }
      const decision = await askAboutHostKey({ host, fingerprint })

      if (decision === 'reject') {
        throw new HostKeyRejected(fingerprint)
      }

      attempt = { ...request, policy: POLICY_FOR[decision] }
      return await connect(attempt)
    }
  }

  // Unreachable: the loop either returns or throws. Present to satisfy the return type.
  throw new ConnectCancelled('Connection cancelled.')
}
