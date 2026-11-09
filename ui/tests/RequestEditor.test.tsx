import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import * as api from '../src/api'
import App from '../src/App'

const ok: api.Response = {
  status: 200,
  statusText: 'OK',
  url: 'https://x.dev/',
  headers: [],
  body: '{}',
  truncated: false,
  size: 2,
  timing: { dnsMs: 0, connectMs: 0, waitingMs: 1, downloadMs: 1, totalMs: 2 },
}

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

const address = () => screen.getByLabelText('Address') as HTMLInputElement
const tab = (name: RegExp) => fireEvent.click(screen.getByRole('tab', { name }))
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } })

describe('request editor', () => {
  it('keeps the query table and the address in step', () => {
    render(<App />)
    type('Address', 'x.dev/search?q=red')
    expect((screen.getByLabelText('Query value 1') as HTMLInputElement).value).toBe('red')

    type('Query value 1', 'red car')
    expect(address().value).toBe('x.dev/search?q=red%20car')

    type('Query name 2', 'page')
    type('Query value 2', '2')
    expect(address().value).toBe('x.dev/search?q=red%20car&page=2')

    fireEvent.click(screen.getByRole('button', { name: 'Remove q' }))
    expect(address().value).toBe('x.dev/search?page=2')
  })

  it('sends headers and a JSON body with its content type', async () => {
    const send = vi.spyOn(api, 'sendRequest').mockResolvedValue(ok)
    render(<App />)
    fireEvent.change(screen.getByLabelText('Method'), { target: { value: 'POST' } })
    type('Address', 'x.dev/orders')
    tab(/headers/i)
    type('Headers name 1', 'Accept')
    type('Headers value 1', 'application/json')
    type('Headers name 2', 'X-Debug')
    type('Headers value 2', '1')
    fireEvent.click(screen.getByLabelText('Send X-Debug'))
    tab(/body/i)
    fireEvent.click(screen.getByRole('radio', { name: 'JSON' }))
    type('Body', '{"qty": 2}')

    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    await screen.findByText('200')

    expect(send).toHaveBeenCalledWith({
      method: 'POST',
      url: 'x.dev/orders',
      headers: [
        { name: 'Accept', value: 'application/json' },
        { name: 'Content-Type', value: 'application/json' },
      ],
      body: '{"qty": 2}',
    }, null, false, [])
  })

  it('points at broken JSON and tidies valid JSON', () => {
    render(<App />)
    tab(/body/i)
    fireEvent.click(screen.getByRole('radio', { name: 'JSON' }))
    type('Body', '{"a": 1,}')
    expect(screen.getByRole('status').textContent).toContain('Line 1, column 8')

    type('Body', '{"a":1}')
    fireEvent.click(screen.getByRole('button', { name: 'Tidy up' }))
    expect((screen.getByLabelText('Body') as HTMLTextAreaElement).value).toBe('{\n  "a": 1\n}')
  })

  it('hides secret values and keeps them out of history', async () => {
    vi.spyOn(api, 'sendRequest').mockResolvedValue(ok)
    render(<App />)
    type('Address', 'x.dev/me')
    tab(/headers/i)
    type('Headers name 1', 'Authorization')
    type('Headers value 1', 'Bearer s3cret')
    const value = screen.getByLabelText('Headers value 1') as HTMLInputElement
    expect(value.type).toBe('password')
    fireEvent.click(screen.getByRole('button', { name: 'show' }))
    expect(value.type).toBe('text')

    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    await screen.findByText('200')

    // Clear the header, then reopen the old request: the secret is not brought back.
    type('Headers value 1', '')
    fireEvent.click(screen.getByTitle('x.dev/me'))
    expect((screen.getByLabelText('Headers value 1') as HTMLInputElement).value).toBe('')
    expect(document.body.innerHTML).not.toContain('s3cret')
  })

  it('warns that GET bodies are often ignored', () => {
    render(<App />)
    tab(/body/i)
    fireEvent.click(screen.getByRole('radio', { name: 'Text' }))
    expect(screen.getByText(/ignore a body on GET/)).toBeTruthy()
  })
})
