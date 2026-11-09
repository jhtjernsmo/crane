import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as api from '../src/api'
import App from '../src/App'
import * as preview from '../src/previewBackend'

const ok: api.Response = {
  status: 200,
  statusText: 'OK',
  url: 'https://x.dev/',
  headers: [],
  body: '{}',
  truncated: false,
  size: 2,
  timing: { dnsMs: 58, connectMs: 68, waitingMs: 26, downloadMs: 2, totalMs: 154 },
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('history and timing', () => {
  it('shows where the time went', async () => {
    vi.spyOn(api, 'sendRequest').mockResolvedValue(ok)
    render(<App />)
    fireEvent.change(screen.getByLabelText('Address'), { target: { value: 'x.dev' } })
    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    const ticks = await screen.findByRole('list', { name: 'Timing' })
    expect(ticks.textContent).toContain('lookup 58 ms')
    expect(ticks.textContent).toContain('connect + TLS 68 ms')
    expect(ticks.textContent).toContain('waiting for the server 26 ms')
  })

  it('saves a summary of each send without secrets or the answer', async () => {
    vi.spyOn(api, 'sendRequest').mockResolvedValue(ok)
    render(<App />)
    fireEvent.change(screen.getByLabelText('Address'), { target: { value: 'x.dev/me' } })
    fireEvent.click(screen.getByRole('tab', { name: /headers/i }))
    fireEvent.change(screen.getByLabelText('Headers name 1'), { target: { value: 'Authorization' } })
    fireEvent.change(screen.getByLabelText('Headers value 1'), { target: { value: 'Bearer s3cret' } })
    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    await screen.findByText('200')

    const saved = preview.historyList()
    expect(saved).toHaveLength(1)
    expect(saved[0]).toMatchObject({ status: 200, totalMs: 154, size: 2, envName: null, error: null })
    expect(saved[0].request.url).toBe('x.dev/me')
    expect(JSON.stringify(saved)).not.toContain('s3cret')
    expect(JSON.stringify(saved)).not.toContain('"body":"{}"')
  })

  it('brings back earlier sessions and explains why the answer is missing', async () => {
    preview.resetHistory([
      {
        at: Date.parse('2026-11-01T10:00:00Z'),
        envName: 'Local',
        request: { name: '', method: 'POST', url: '{{base}}/orders', headers: [], bodyMode: 'json', body: '{"qty":2}' },
        status: 201,
        totalMs: 84,
        size: 120,
        error: null,
      },
    ])
    render(<App />)
    const item = await screen.findByTitle('{{base}}/orders')
    expect(item.textContent).toContain('Local · 201 · 84 ms')
    fireEvent.click(item)
    expect((screen.getByLabelText('Address') as HTMLInputElement).value).toBe('{{base}}/orders')
    expect(screen.getByText(/earlier session and answered 201/)).toBeTruthy()
  })

  it('clears the history after asking', async () => {
    preview.resetHistory([
      { at: Date.now(), envName: null, request: { name: '', method: 'GET', url: 'x.dev', headers: [], bodyMode: 'none', body: '' }, status: 200, totalMs: 5, size: 1, error: null },
    ])
    render(<App />)
    await screen.findByTitle('x.dev')
    fireEvent.click(screen.getByRole('button', { name: 'Clear history' }))
    fireEvent.click(within(screen.getByRole('alertdialog', { name: 'Clear the history?' })).getByRole('button', { name: 'Clear' }))
    await waitFor(() => expect(screen.queryByTitle('x.dev')).toBeNull())
    expect(preview.historyList()).toEqual([])
  })
})
