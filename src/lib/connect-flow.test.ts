import { describe, expect, it, vi } from 'vitest'
import { ConnectCancelled, connectWithHostKeyPrompt, HostKeyRejected } from './connect-flow'
import type { ConnectRequest } from '@/lib/types'

const request: ConnectRequest = { hostId: 'host-1', cols: 80, rows: 24, attemptId: 'attempt-1' }

function unknownHostKey() {
  return {
    kind: 'unknownHostKey',
    message: 'host key for example.com:22 is not known',
    host: 'example.com:22',
    fingerprint: 'SHA256:abc',
  }
}

describe('connectWithHostKeyPrompt', () => {
  it('connects strictly and never prompts for a known host', async () => {
    const connect = vi.fn(async () => 'session-1')
    const askAboutHostKey = vi.fn(async () => 'save' as const)

    const sessionId = await connectWithHostKeyPrompt({ request, connect, askAboutHostKey })

    expect(sessionId).toBe('session-1')
    expect(connect).toHaveBeenCalledExactlyOnceWith({ ...request, policy: 'strict' })
    expect(askAboutHostKey).not.toHaveBeenCalled()
  })

  it('shows the fingerprint the server actually presented', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(unknownHostKey())
      .mockResolvedValueOnce('session-1')
    const askAboutHostKey = vi.fn(async () => 'save' as const)

    await connectWithHostKeyPrompt({ request, connect, askAboutHostKey })

    expect(askAboutHostKey).toHaveBeenCalledExactlyOnceWith({
      host: 'example.com:22',
      fingerprint: 'SHA256:abc',
    })
  })

  it('retries with trustAndSave when the user saves the key', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(unknownHostKey())
      .mockResolvedValueOnce('session-1')

    const sessionId = await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'save',
    })

    expect(sessionId).toBe('session-1')
    expect(connect).toHaveBeenLastCalledWith({ ...request, policy: 'trustAndSave' })
  })

  it('retries with trustOnce when the user accepts for this session only', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(unknownHostKey())
      .mockResolvedValueOnce('session-1')

    await connectWithHostKeyPrompt({ request, connect, askAboutHostKey: async () => 'once' })

    expect(connect).toHaveBeenLastCalledWith({ ...request, policy: 'trustOnce' })
  })

  it('does not connect at all when the user rejects the key', async () => {
    const connect = vi.fn().mockRejectedValueOnce(unknownHostKey())

    await expect(
      connectWithHostKeyPrompt({ request, connect, askAboutHostKey: async () => 'reject' }),
    ).rejects.toBeInstanceOf(HostKeyRejected)

    expect(connect).toHaveBeenCalledOnce()
  })

  it('never prompts for a changed host key', async () => {
    const changed = {
      kind: 'changedHostKey',
      message: 'HOST KEY CHANGED',
      host: 'example.com:22',
      fingerprint: 'SHA256:evil',
      line: 12,
    }
    const connect = vi.fn().mockRejectedValueOnce(changed)
    const askAboutHostKey = vi.fn(async () => 'save' as const)

    // A changed key is the MITM case; offering "trust this" would be the wrong UI.
    await expect(connectWithHostKeyPrompt({ request, connect, askAboutHostKey }))
      .rejects.toMatchObject({ kind: 'changedHostKey' })

    expect(askAboutHostKey).not.toHaveBeenCalled()
    expect(connect).toHaveBeenCalledOnce()
  })

  it('propagates an auth failure without prompting', async () => {
    const connect = vi.fn().mockRejectedValueOnce({ kind: 'auth', message: 'bad password' })
    const askAboutHostKey = vi.fn(async () => 'save' as const)

    await expect(connectWithHostKeyPrompt({ request, connect, askAboutHostKey }))
      .rejects.toMatchObject({ kind: 'auth' })

    expect(askAboutHostKey).not.toHaveBeenCalled()
  })

  it('propagates a failure from the retry itself', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(unknownHostKey())
      .mockRejectedValueOnce({ kind: 'auth', message: 'bad password' })

    await expect(
      connectWithHostKeyPrompt({ request, connect, askAboutHostKey: async () => 'save' }),
    ).rejects.toMatchObject({ kind: 'auth' })
  })
})

describe('unresolved variables', () => {
  function unresolved(...variables: string[]) {
    return { kind: 'unresolvedVariables', message: 'unresolved variables', variables }
  }

  it('asks for every missing variable at once', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(unresolved('wg_user', 'target'))
      .mockResolvedValueOnce('session-1')
    const askAboutVariables = vi.fn(async () => ({ wg_user: 'flex', target: 'web-01' }))

    const sessionId = await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'reject',
      askAboutVariables,
      saveVariables: async () => {},
    })

    expect(sessionId).toBe('session-1')
    expect(askAboutVariables).toHaveBeenCalledExactlyOnceWith(['wg_user', 'target'])
  })

  it('saves the answers before retrying', async () => {
    const order: string[] = []
    const connect = vi.fn(async () => {
      order.push('connect')
      if (order.filter(o => o === 'connect').length === 1) throw unresolved('wg_user')
      return 'session-1'
    })
    const saveVariables = vi.fn(async () => {
      order.push('save')
    })

    await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'reject',
      askAboutVariables: async () => ({ wg_user: 'flex' }),
      saveVariables,
    })

    // Retrying before the values are stored would just fail again.
    expect(order).toEqual(['connect', 'save', 'connect'])
  })

  it('gives up when the user cancels the prompt', async () => {
    const connect = vi.fn().mockRejectedValueOnce(unresolved('wg_user'))

    await expect(
      connectWithHostKeyPrompt({
        request,
        connect,
        askAboutHostKey: async () => 'reject',
        askAboutVariables: async () => null,
      }),
    ).rejects.toBeInstanceOf(ConnectCancelled)

    expect(connect).toHaveBeenCalledOnce()
  })

  it('asks once and then reports the real error rather than looping', async () => {
    const connect = vi.fn().mockRejectedValue(unresolved('wg_user'))

    // Surfacing the original error names the variable that is still missing, which is
    // more useful than a generic "cancelled".
    await expect(
      connectWithHostKeyPrompt({
        request,
        connect,
        askAboutHostKey: async () => 'reject',
        askAboutVariables: async () => ({ wg_user: '' }),
        saveVariables: async () => {},
      }),
    ).rejects.toMatchObject({ kind: 'unresolvedVariables' })

    expect(connect).toHaveBeenCalledTimes(2)
  })

  it('handles a host that needs variables and is also unknown', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(unresolved('wg_user'))
      .mockRejectedValueOnce(unknownHostKey())
      .mockResolvedValueOnce('session-1')

    const sessionId = await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'save',
      askAboutVariables: async () => ({ wg_user: 'flex' }),
      saveVariables: async () => {},
    })

    expect(sessionId).toBe('session-1')
    expect(connect).toHaveBeenLastCalledWith({ ...request, policy: 'trustAndSave' })
  })

  it('propagates unresolved variables when no prompt is wired up', async () => {
    const connect = vi.fn().mockRejectedValueOnce(unresolved('wg_user'))

    await expect(
      connectWithHostKeyPrompt({ request, connect, askAboutHostKey: async () => 'reject' }),
    ).rejects.toMatchObject({ kind: 'unresolvedVariables' })
  })
})

describe('password prompt', () => {
  function passwordRequired() {
    return {
      kind: 'passwordRequired',
      message: 'a password is required',
      username: 'root',
      host: 'example.com:22',
    }
  }

  it('asks for a password and retries with it', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(passwordRequired())
      .mockResolvedValueOnce('session-1')
    const askForPassword = vi.fn(async () => 'hunter2')

    const sessionId = await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'reject',
      askForPassword,
    })

    expect(sessionId).toBe('session-1')
    expect(askForPassword).toHaveBeenCalledExactlyOnceWith({
      username: 'root',
      host: 'example.com:22',
    })
    expect(connect).toHaveBeenLastCalledWith(expect.objectContaining({ password: 'hunter2' }))
  })

  it('gives up when the prompt is cancelled', async () => {
    const connect = vi.fn().mockRejectedValueOnce(passwordRequired())

    await expect(
      connectWithHostKeyPrompt({
        request,
        connect,
        askAboutHostKey: async () => 'reject',
        askForPassword: async () => null,
      }),
    ).rejects.toBeInstanceOf(ConnectCancelled)

    expect(connect).toHaveBeenCalledOnce()
  })

  it('accepts an empty password rather than treating it as a cancel', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(passwordRequired())
      .mockResolvedValueOnce('session-1')

    // Some servers accept an empty password; only null means cancelled.
    await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'reject',
      askForPassword: async () => '',
    })

    expect(connect).toHaveBeenLastCalledWith(expect.objectContaining({ password: '' }))
  })

  it('asks only once, then surfaces the failure', async () => {
    const connect = vi.fn().mockRejectedValue(passwordRequired())
    const askForPassword = vi.fn(async () => 'wrong')

    await expect(
      connectWithHostKeyPrompt({
        request,
        connect,
        askAboutHostKey: async () => 'reject',
        askForPassword,
      }),
    ).rejects.toMatchObject({ kind: 'passwordRequired' })

    expect(askForPassword).toHaveBeenCalledOnce()
  })

  it('handles a host needing a password and then trust', async () => {
    const connect = vi.fn()
      .mockRejectedValueOnce(passwordRequired())
      .mockRejectedValueOnce(unknownHostKey())
      .mockResolvedValueOnce('session-1')

    const sessionId = await connectWithHostKeyPrompt({
      request,
      connect,
      askAboutHostKey: async () => 'save',
      askForPassword: async () => 'hunter2',
    })

    expect(sessionId).toBe('session-1')
  })

  it('propagates the request when no prompt is wired up', async () => {
    const connect = vi.fn().mockRejectedValueOnce(passwordRequired())

    await expect(
      connectWithHostKeyPrompt({ request, connect, askAboutHostKey: async () => 'reject' }),
    ).rejects.toMatchObject({ kind: 'passwordRequired' })
  })
})
