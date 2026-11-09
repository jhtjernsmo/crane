import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from '../src/App'
import * as preview from '../src/previewBackend'

const local = { id: 'local', name: 'Local', production: false, vars: [{ name: 'base', value: 'localhost:8080', secret: false }] }

beforeEach(() => {
  preview.reset({ active: 'local', environments: [local] })
  preview.resetCollection({
    'users/get-me.http': { name: 'Get me', method: 'GET', url: '{{base}}/me', headers: [], bodyMode: 'none', body: '' },
    'health.http': { name: 'Health', method: 'GET', url: '{{base}}/health', headers: [], bodyMode: 'none', body: '' },
  })
})
afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

const address = () => screen.getByLabelText('Address') as HTMLInputElement
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } })

describe('collections', () => {
  it('lists the folder and opens a request from it', async () => {
    render(<App />)
    const tree = await screen.findByRole('tree', { name: 'Requests' })
    expect(within(tree).getByText('users')).toBeTruthy()
    fireEvent.click(within(tree).getByText('Get me'))
    await waitFor(() => expect(address().value).toBe('{{base}}/me'))
    expect(screen.getByText('users/get-me')).toBeTruthy()
    expect(screen.queryByText('changed')).toBeNull()

    type('Address', '{{base}}/me?full=1')
    expect(screen.getByText('changed')).toBeTruthy()
    fireEvent.keyDown(window, { key: 's', ctrlKey: true })
    await waitFor(() => expect(screen.queryByText('changed')).toBeNull())
    expect(preview.readRequest('users/get-me.http').request.url).toBe('{{base}}/me?full=1')
  })

  it('asks before throwing away changes', async () => {
    render(<App />)
    fireEvent.click(await screen.findByText('Get me'))
    await waitFor(() => expect(address().value).toBe('{{base}}/me'))
    type('Address', 'edited')
    fireEvent.click(screen.getByText('Health'))
    const ask = screen.getByRole('alertdialog', { name: 'Discard your changes?' })
    fireEvent.click(within(ask).getByRole('button', { name: 'Cancel' }))
    expect(address().value).toBe('edited')

    fireEvent.click(screen.getByText('Health'))
    fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Discard' }))
    await waitFor(() => expect(address().value).toBe('{{base}}/health'))
  })

  it('names a new request and saves it into a folder', async () => {
    render(<App />)
    await screen.findByRole('tree', { name: 'Requests' })
    fireEvent.click(screen.getByRole('button', { name: 'New' }))
    type('Address', '{{base}}/orders')
    fireEvent.click(screen.getByRole('button', { name: /^Save/ }))
    fireEvent.change(await screen.findByLabelText('Request name'), { target: { value: 'List orders' } })
    fireEvent.change(screen.getByLabelText('Folder'), { target: { value: 'users' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(screen.getByText('users/list-orders')).toBeTruthy())
    expect(within(screen.getByRole('tree')).getByText('List orders')).toBeTruthy()
  })

  it('moves a secret to the keyring instead of writing it to the file', async () => {
    render(<App />)
    fireEvent.click(await screen.findByText('Get me'))
    await waitFor(() => expect(address().value).toBe('{{base}}/me'))
    fireEvent.click(screen.getByRole('tab', { name: /headers/i }))
    type('Headers name 1', 'Authorization')
    type('Headers value 1', 'Bearer tk_live_8f3k2Qx9')
    fireEvent.keyDown(window, { key: 's', ctrlKey: true })

    const review = await screen.findByRole('alertdialog', { name: 'This would put a secret in the file' })
    expect(within(review).getByText('header Authorization')).toBeTruthy()
    expect(within(review).getByText('tk_l… (16 characters)')).toBeTruthy()
    expect((within(review).getByLabelText('Variable name for header Authorization') as HTMLInputElement).value).toBe('token')
    expect(preview.readRequest('users/get-me.http').request.headers).toEqual([])

    fireEvent.click(within(review).getByRole('button', { name: 'Move to keyring and save' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())

    expect(preview.readRequest('users/get-me.http').request.headers).toEqual([{ name: 'Authorization', value: 'Bearer {{token}}', enabled: true }])
    expect(preview.list().environments[0].vars).toContainEqual({ name: 'token', value: '', secret: true })
    expect(preview.list().secretsSet).toEqual(['local/token'])
    expect((screen.getByLabelText('Headers value 1') as HTMLInputElement).value).toBe('Bearer {{token}}')
  })

  it('deletes a file only after asking', async () => {
    render(<App />)
    await screen.findByRole('tree', { name: 'Requests' })
    fireEvent.click(screen.getByRole('button', { name: 'Delete Health' }))
    fireEvent.click(within(screen.getByRole('alertdialog', { name: 'Delete Health?' })).getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(screen.queryByText('Health')).toBeNull())
    expect(() => preview.readRequest('health.http')).toThrow()
  })

  it('offers to open a folder when none is open', async () => {
    preview.resetCollection(null)
    render(<App />)
    fireEvent.click(screen.getByRole('tab', { name: 'Requests' }))
    fireEvent.click(screen.getByRole('button', { name: 'Open a folder' }))
    expect(await screen.findByRole('tree', { name: 'Requests' })).toBeTruthy()
  })
})
