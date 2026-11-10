import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import App from '../src/App'
import * as preview from '../src/previewBackend'

afterEach(cleanup)

const strict = {
  contact: 'it@example.com',
  allowedHosts: ['*.example.com'],
  blockedHosts: [],
  allowSavingSecrets: false,
  allowInsecureTls: false,
  containers: false,
  keepHistory: false,
  checkUpdates: false,
  source: '/etc/crane/policy.toml',
  problem: null,
}

describe('organisation policy', () => {
  it('shows that the app is managed and hides what is turned off', async () => {
    preview.resetPolicy(strict)
    preview.reset({ active: 'local', environments: [{ id: 'local', name: 'Local', production: false, vars: [] }] })
    preview.resetCollection({ 'me.http': { name: 'Me', method: 'GET', url: 'api.example.com/me', headers: [], bodyMode: 'none', body: '' } })
    render(<App />)

    const badge = await screen.findByText('managed by your organisation')
    expect(badge.getAttribute('title')).toContain('/etc/crane/policy.toml')

    fireEvent.click(screen.getByRole('tab', { name: 'History' }))
    expect(screen.getByText(/Saving history is turned off by your organisation's policy. Ask it@example.com/)).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Environments' }))
    const dialog = screen.getByRole('dialog', { name: 'Environments' })
    fireEvent.click(within(dialog).getByRole('button', { name: /Network/ }))
    expect((within(dialog).getByLabelText(/Skip certificate checks/) as HTMLInputElement).disabled).toBe(true)
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    fireEvent.click(screen.getByRole('tab', { name: 'Saved' }))
    fireEvent.click(await screen.findByText('Me'))
    await waitFor(() => expect((screen.getByLabelText('Address') as HTMLInputElement).value).toBe('api.example.com/me'))
    fireEvent.click(screen.getByRole('tab', { name: /headers/i }))
    fireEvent.change(screen.getByLabelText('Headers name 1'), { target: { value: 'Authorization' } })
    fireEvent.change(screen.getByLabelText('Headers value 1'), { target: { value: 'Bearer abc123' } })
    fireEvent.keyDown(window, { key: 's', ctrlKey: true })
    const review = await screen.findByRole('alertdialog', { name: 'This would put a secret in the file' })
    expect(within(review).queryByRole('button', { name: 'Save with the secret' })).toBeNull()
  })

  it('says loudly when the policy file is broken', async () => {
    preview.resetPolicy({ ...strict, problem: 'The policy file has a mistake: expected a boolean' })
    render(<App />)
    const badge = await screen.findByText('policy error: locked down')
    expect(badge.getAttribute('title')).toContain('has a mistake')
  })
})
