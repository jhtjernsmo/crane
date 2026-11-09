import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from '../src/App'
import * as preview from '../src/previewBackend'

const SECRET = 'tk_live_8f3k2Qx9'

/** A server that echoes what it got, like httpbin. */
function echoServer() {
  const fetchMock = vi.fn(async (url: string, init: RequestInit) => {
    const headers = Object.fromEntries((init.headers as [string, string][]) ?? [])
    const body = new TextEncoder().encode(JSON.stringify({ url, method: init.method, headers }))
    return {
      status: 200,
      statusText: 'OK',
      url,
      headers: new Headers({ 'content-type': 'application/json', 'x-echo': headers.Authorization ?? '' }),
      arrayBuffer: async () => body.buffer,
    }
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

const local = {
  id: 'local',
  name: 'Local',
  production: false,
  vars: [
    { name: 'base', value: 'localhost:8080', secret: false },
    { name: 'token', value: '', secret: true },
  ],
}
const prod = { id: 'prod', name: 'Prod', production: true, vars: [{ name: 'base', value: 'api.example.com', secret: false }] }

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } })

describe('environments', () => {
  it('creates an environment with a secret that never comes back to the screen', async () => {
    render(<App />)
    fireEvent.click(screen.getByRole('button', { name: 'Environments' }))
    const dialog = screen.getByRole('dialog', { name: 'Environments' })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Add environment' }))
    expect((within(dialog).getByLabelText('Environment name') as HTMLInputElement).value).toBe('Local')

    const names = () => within(dialog).getAllByLabelText('Variable name')
    fireEvent.change(names()[0], { target: { value: 'base' } })
    fireEvent.change(within(dialog).getByLabelText('Value for base'), { target: { value: 'localhost:8080' } })
    fireEvent.change(names()[1], { target: { value: 'token' } })
    fireEvent.change(within(dialog).getByLabelText('Value for token'), { target: { value: SECRET } })
    expect(within(dialog).getByText(/looks like a key or password/)).toBeTruthy()

    fireEvent.click(within(dialog).getByLabelText('token is secret'))
    const secretInput = within(dialog).getByLabelText('Secret value for token') as HTMLInputElement
    expect(secretInput.type).toBe('password')
    expect(secretInput.value).toBe('')
    fireEvent.change(secretInput, { target: { value: SECRET } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

    expect(preview.list().environments[0].vars[1]).toEqual({ name: 'token', value: '', secret: true })
    expect(preview.list().secretsSet).toEqual([`${preview.list().environments[0].id}/token`])

    // Reopening shows that a value is stored, not the value.
    fireEvent.click(screen.getByRole('button', { name: 'Environments' }))
    const again = screen.getByLabelText('Secret value for token') as HTMLInputElement
    expect(again.value).toBe('')
    expect(again.placeholder).toContain('stored in keyring')
    expect(document.body.innerHTML).not.toContain(SECRET)
  })

  it('shows what variables turn into, sends them filled in and hides the echoed secret', async () => {
    preview.reset({ active: 'local', environments: [local] }, { 'local/token': SECRET })
    const fetchMock = echoServer()
    render(<App />)
    type('Address', '{{base}}/me')
    fireEvent.click(screen.getByRole('tab', { name: /headers/i }))
    type('Headers name 1', 'Authorization')
    type('Headers value 1', 'Bearer {{token}}')

    const line = await screen.findByRole('list', { name: 'Variables in this request' })
    await waitFor(() => expect(line.textContent).toContain('{{base}} → localhost:8080'))
    expect(line.textContent).toContain('{{token}} → •••••• from keyring')

    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    await screen.findByText('200')

    expect(fetchMock).toHaveBeenCalledWith('https://localhost:8080/me', expect.objectContaining({ headers: [['Authorization', `Bearer ${SECRET}`]] }))
    expect(document.body.innerHTML).not.toContain(SECRET)
    expect(screen.getByText('"Bearer ‹token hidden›"')).toBeTruthy()
    expect(screen.getByText(/· Local ·/)).toBeTruthy()

    // History keeps the template, so the secret is not in it either.
    type('Headers value 1', '')
    fireEvent.click(screen.getByTitle('{{base}}/me'))
    expect((screen.getByLabelText('Headers value 1') as HTMLInputElement).value).toBe('Bearer {{token}}')
  })

  it('says which variables are missing before anything is sent', async () => {
    preview.reset({ active: 'local', environments: [local] })
    const fetchMock = echoServer()
    render(<App />)
    type('Address', '{{base}}/{{version}}/users')
    await waitFor(() => expect(screen.getByRole('list', { name: 'Variables in this request' }).textContent).toContain('not in Local'))
    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    expect((await screen.findByRole('alert')).textContent).toContain('Local has no {{version}}. Add it under Environments')
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('asks before changing data in production, and Cancel sends nothing', async () => {
    preview.reset({ active: 'prod', environments: [local, prod] })
    const fetchMock = echoServer()
    render(<App />)
    await screen.findByText('production')
    fireEvent.change(screen.getByLabelText('Method'), { target: { value: 'DELETE' } })
    type('Address', '{{base}}/users/7')

    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    const ask = screen.getByRole('alertdialog', { name: 'Send DELETE to Prod?' })
    expect(document.activeElement?.textContent).toBe('Cancel')
    fireEvent.click(within(ask).getByRole('button', { name: 'Cancel' }))
    expect(fetchMock).not.toHaveBeenCalled()

    fireEvent.keyDown(window, { key: 'Enter', ctrlKey: true })
    fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Send to Prod' }))
    await screen.findByText('200')
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(fetchMock.mock.calls[0][0]).toBe('https://api.example.com/users/7')
  })

  it('reads in production without asking', async () => {
    preview.reset({ active: 'prod', environments: [prod] })
    const fetchMock = echoServer()
    render(<App />)
    await screen.findByText('production')
    type('Address', '{{base}}/users')
    fireEvent.click(screen.getByRole('button', { name: /send/i }))
    await screen.findByText('200')
    expect(fetchMock).toHaveBeenCalledTimes(1)
    expect(screen.queryByRole('alertdialog')).toBeNull()
  })

  it('suggests a variable when a secret is typed straight into a header', () => {
    render(<App />)
    fireEvent.click(screen.getByRole('tab', { name: /headers/i }))
    type('Headers name 1', 'Authorization')
    type('Headers value 1', 'Bearer abc')
    expect(screen.getByText(/put this value in an environment as a secret/)).toBeTruthy()
  })
})

describe('network settings', () => {
  it('stores a CA file, a proxy and refuses skipped checks in production', async () => {
    preview.reset({ active: 'local', environments: [local] })
    render(<App />)
    await screen.findByRole('option', { name: 'Local' })
    fireEvent.click(screen.getByRole('button', { name: 'Environments' }))
    const dialog = screen.getByRole('dialog', { name: 'Environments' })
    fireEvent.click(within(dialog).getByRole('button', { name: /Network/ }))
    fireEvent.click(within(dialog).getAllByRole('button', { name: 'Browse' })[0])
    await waitFor(() => expect((within(dialog).getByLabelText('CA certificates') as HTMLInputElement).value).toBe('/home/you/certs/company-ca.pem'))
    fireEvent.click(within(dialog).getByRole('radio', { name: 'Custom' }))
    fireEvent.change(within(dialog).getByLabelText('Proxy address'), { target: { value: 'http://proxy.corp:3128' } })
    fireEvent.click(within(dialog).getByLabelText(/Skip certificate checks/))
    expect(within(dialog).getByText(/Certificates are not checked/)).toBeTruthy()

    // Marking it as production turns the skip off and locks it.
    fireEvent.click(within(dialog).getByLabelText(/Production: ask before/))
    const skip = within(dialog).getByLabelText(/Skip certificate checks/) as HTMLInputElement
    expect(skip.checked).toBe(false)
    expect(skip.disabled).toBe(true)

    fireEvent.click(within(dialog).getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(preview.list().environments[0].network).toEqual({
      caFile: '/home/you/certs/company-ca.pem',
      clientCertFile: '',
      insecure: false,
      proxy: 'http://proxy.corp:3128',
      noProxy: '',
    })
  })
})
