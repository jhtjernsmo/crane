import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import App from '../src/App'
import * as preview from '../src/previewBackend'

/** An API answering {"id":7,"name":"Ada"} for /users and 404 elsewhere. */
function api() {
  const fetchMock = vi.fn(async (url: string) => {
    const found = url.includes('/users')
    const body = new TextEncoder().encode(found ? '{"id":7,"name":"Ada"}' : '{}')
    return { status: found ? 200 : 404, statusText: found ? 'OK' : 'Not Found', url, headers: new Headers({ 'content-type': 'application/json' }), arrayBuffer: async () => body.buffer }
  })
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const local = { id: 'local', name: 'Local', production: false, vars: [{ name: 'base', value: 'api.test', secret: false }] }
const prod = { ...local, id: 'prod', name: 'Prod', production: true }

describe('checks', () => {
  it('runs the checks of a request and saves them in its file', async () => {
    api()
    preview.reset({ active: 'local', environments: [local] })
    preview.resetCollection({ 'users.http': { name: 'Users', method: 'GET', url: '{{base}}/users/7', headers: [], bodyMode: 'none', body: '' } })
    render(<App />)
    fireEvent.click(await screen.findByText('Users'))
    await waitFor(() => expect((screen.getByLabelText('Address') as HTMLInputElement).value).toBe('{{base}}/users/7'))
    fireEvent.click(screen.getByRole('tab', { name: /Checks/ }))
    fireEvent.change(screen.getByLabelText('Check 1'), { target: { value: 'status 200' } })
    fireEvent.change(screen.getByLabelText('Check 2'), { target: { value: 'json $.name = Grace' } })
    fireEvent.click(screen.getByRole('button', { name: /send/i }))

    const list = await screen.findByRole('list', { name: 'Checks' })
    expect(screen.getByText('Checks · 1 of 2 passed')).toBeTruthy()
    expect(within(list).getByText('got Ada')).toBeTruthy()

    fireEvent.keyDown(window, { key: 's', ctrlKey: true })
    await waitFor(() => expect(preview.readRequest('users.http').request.expect).toEqual(['status 200', 'json $.name = Grace']))
  })

  it('runs every request and reports what failed', async () => {
    api()
    preview.reset({ active: 'local', environments: [local] })
    preview.resetCollection({
      '01-users.http': { name: 'Users', method: 'GET', url: '{{base}}/users/7', headers: [], bodyMode: 'none', body: '', expect: ['json $.id = 7'] },
      '02-missing.http': { name: 'Missing', method: 'GET', url: '{{base}}/nope', headers: [], bodyMode: 'none', body: '' },
    })
    render(<App />)
    await screen.findByRole('option', { name: 'Local' })
    fireEvent.click(await screen.findByRole('button', { name: 'Run all' }))
    const report = await screen.findByLabelText('Run report')
    expect(within(report).getByText('1/2')).toBeTruthy()
    expect(within(report).getByText('1 failed')).toBeTruthy()
    fireEvent.click(within(report).getByTitle('Open 02-missing.http'))
    await waitFor(() => expect((screen.getByLabelText('Address') as HTMLInputElement).value).toBe('{{base}}/nope'))
  })

  it('asks before running against production', async () => {
    const fetchMock = api()
    preview.reset({ active: 'prod', environments: [prod] })
    preview.resetCollection({ 'del.http': { name: 'Delete', method: 'DELETE', url: '{{base}}/users/7', headers: [], bodyMode: 'none', body: '' } })
    render(<App />)
    await screen.findByText('production')
    fireEvent.click(await screen.findByRole('button', { name: 'Run all' }))
    const ask = screen.getByRole('alertdialog', { name: 'Run every request against Prod?' })
    fireEvent.click(within(ask).getByRole('button', { name: 'Cancel' }))
    expect(fetchMock).not.toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Run all' }))
    fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Run against Prod' }))
    await screen.findByLabelText('Run report')
    expect(fetchMock).toHaveBeenCalledTimes(1)
  })
})
