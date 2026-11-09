import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as api from '../src/api'
import App from '../src/App'

const ok: api.Response = {
  status: 201,
  statusText: 'Created',
  url: 'http://localhost:8080/api/orders',
  headers: [{ name: 'content-type', value: 'application/json' }],
  body: '{"id":"ord_7f3k2","status":"pending"}',
  truncated: false,
  size: 412,
  timing: { waitingMs: 60, downloadMs: 24, totalMs: 84 },
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('App', () => {
  it('fills the address from an example and shows the answer', async () => {
    const send = vi.spyOn(api, 'sendRequest').mockResolvedValue(ok)
    render(<App />)
    fireEvent.click(screen.getByText('localhost:8080', { selector: '.mono' }))
    expect(screen.getByLabelText('Address')).toHaveProperty('value', 'localhost:8080')

    fireEvent.click(screen.getByRole('button', { name: /send/i }))

    expect(await screen.findByText('201')).toBeTruthy()
    expect(send).toHaveBeenCalledWith({ method: 'GET', url: 'localhost:8080', headers: [], body: null }, null, false)
    expect(screen.getByText('Created')).toBeTruthy()
    expect(screen.getByText('412 B')).toBeTruthy()
    expect(screen.getByText('"ord_7f3k2"').className).toBe('t-string')
    expect(screen.getByText(/201 · 84 ms/)).toBeTruthy()
  })

  it('sends with Ctrl+Enter and explains failures in words', async () => {
    vi.spyOn(api, 'sendRequest').mockRejectedValue(new Error('Nothing answered at localhost:9. Is the server running, and on that port?'))
    render(<App />)
    fireEvent.change(screen.getByLabelText('Address'), { target: { value: 'localhost:9' } })
    fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true })

    expect((await screen.findByRole('alert')).textContent).toContain('Is the server running')
    expect(screen.getByText(/failed/)).toBeTruthy()
  })

  it('shows the headers tab', async () => {
    vi.spyOn(api, 'sendRequest').mockResolvedValue(ok)
    render(<App />)
    fireEvent.change(screen.getByLabelText('Address'), { target: { value: 'x.dev' } })
    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    const tabs = await screen.findByRole('tablist', { name: 'Response parts' })
    fireEvent.click(within(tabs).getByRole('tab', { name: /headers/i }))
    expect(screen.getByText('content-type')).toBeTruthy()
  })
})
