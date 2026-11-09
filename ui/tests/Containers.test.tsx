import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import App from '../src/App'
import { hostPortFor, nameFor } from '../src/components/ContainersView'
import * as preview from '../src/previewBackend'

afterEach(cleanup)

const openContainers = () => fireEvent.click(screen.getByRole('tab', { name: 'Containers' }))

describe('containers', () => {
  it('suggests names and ports', () => {
    expect(nameFor('postgres:16-alpine')).toBe('postgres')
    expect(nameFor('ghcr.io/org/api:1.2')).toBe('api')
    expect(nameFor('traefik/whoami@sha256:abc')).toBe('whoami')
    expect(hostPortFor(80)).toBe(8080)
    expect(hostPortFor(5432)).toBe(5432)
  })

  it('pulls an image, runs it and hands its address to a request', async () => {
    render(<App />)
    openContainers()
    fireEvent.change(await screen.findByLabelText('Image to pull'), { target: { value: 'postgres:16-alpine' } })
    fireEvent.click(screen.getByRole('button', { name: 'Pull' }))
    fireEvent.click(await screen.findByTitle('Run postgres:16-alpine'))

    const form = screen.getByRole('form', { name: 'Run a container' })
    expect((within(form).getByLabelText('Container name') as HTMLInputElement).value).toBe('postgres')
    expect((within(form).getByLabelText('Ports name 1') as HTMLInputElement).value).toBe('5432')
    fireEvent.change(within(form).getByLabelText('Container env name 1'), { target: { value: 'POSTGRES_PASSWORD' } })
    fireEvent.change(within(form).getByLabelText('Container env value 1'), { target: { value: 'local-only' } })
    fireEvent.click(within(form).getByRole('button', { name: 'Run' }))

    const list = await screen.findByRole('list', { name: 'Containers' })
    expect(within(list).getByText('running')).toBeTruthy()
    await waitFor(() => expect(screen.getByLabelText('Container log').textContent).toContain('ready to accept connections'))

    fireEvent.click(within(list).getByRole('button', { name: 'localhost:5432' }))
    expect((screen.getByLabelText('Address') as HTMLInputElement).value).toBe('localhost:5432')
  })

  it('refuses bad ports and taken names', async () => {
    preview.resetBoxes([{ name: 'redis:7', size: 1, exposedPorts: [6379] }])
    render(<App />)
    openContainers()
    fireEvent.click(await screen.findByTitle('Run redis:7'))
    const form = screen.getByRole('form', { name: 'Run a container' })
    fireEvent.change(within(form).getByLabelText('Ports name 1'), { target: { value: '99999' } })
    expect(within(form).getByText('Ports are numbers from 1 to 65535.')).toBeTruthy()
    expect((within(form).getByRole('button', { name: 'Run' }) as HTMLButtonElement).disabled).toBe(true)
    fireEvent.change(within(form).getByLabelText('Ports name 1'), { target: { value: '6379' } })
    fireEvent.click(within(form).getByRole('button', { name: 'Run' }))
    await screen.findByRole('list', { name: 'Containers' })

    fireEvent.click(screen.getByTitle('Run redis:7'))
    fireEvent.click(within(screen.getByRole('form', { name: 'Run a container' })).getByRole('button', { name: 'Run' }))
    expect((await screen.findByRole('alert')).textContent).toContain('already a container called redis')
  })

  it('stops and removes after asking, and keeps used images', async () => {
    preview.resetBoxes([{ name: 'redis:7', size: 1, exposedPorts: [6379] }])
    preview.boxRun({ name: 'cache', image: 'redis:7', ports: [], env: [], args: null })
    render(<App />)
    openContainers()
    const list = await screen.findByRole('list', { name: 'Containers' })
    fireEvent.click(within(list).getByRole('button', { name: 'Stop' }))
    await waitFor(() => expect(within(list).getByText(/stopped/)).toBeTruthy())

    fireEvent.click(screen.getByRole('button', { name: 'Remove redis:7' }))
    fireEvent.click(within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Remove' }))
    expect((await screen.findByRole('alert')).textContent).toContain('used by a container')

    fireEvent.click(within(list).getByRole('button', { name: 'Remove' }))
    fireEvent.click(within(screen.getByRole('alertdialog', { name: 'Remove cache?' })).getByRole('button', { name: 'Remove' }))
    await waitFor(() => expect(screen.queryByText('cache')).toBeNull())
  })
})

describe('stacks', () => {
  const local = { id: 'local', name: 'Local', production: false, vars: [{ name: 'db_password', value: '', secret: true }] }

  it('starts and stops the open folder’s stack with the picked environment', async () => {
    preview.reset({ active: 'local', environments: [local] })
    preview.resetCollection({ 'health.http': { name: 'Health', method: 'GET', url: '/health', headers: [], bodyMode: 'none', body: '' } })
    preview.resetStack(preview.SAMPLE_STACK_INFO)
    render(<App />)
    await screen.findByRole('option', { name: 'Local' })
    openContainers()

    const panel = await screen.findByLabelText('Stack')
    expect(within(panel).getByText('Stack · api-tests')).toBeTruthy()
    expect(within(panel).getByText('after db')).toBeTruthy()
    expect(within(panel).getByText('Local')).toBeTruthy()
    fireEvent.click(within(panel).getByRole('button', { name: 'Start stack' }))

    const list = await screen.findByRole('list', { name: 'Containers' })
    await waitFor(() => expect(within(list).getAllByText('api-tests')).toHaveLength(2))
    expect(within(list).queryByRole('button', { name: 'Remove' })).toBeNull()
    fireEvent.click(within(screen.getByLabelText('Stack')).getByRole('button', { name: 'Stop stack' }))
    await waitFor(() => expect(screen.queryByRole('list', { name: 'Containers' })?.textContent ?? '').not.toContain('api-tests'))
  })

  it('needs an environment when the stack uses variables', async () => {
    preview.resetCollection({})
    preview.resetStack(preview.SAMPLE_STACK_INFO)
    render(<App />)
    openContainers()
    fireEvent.click(within(await screen.findByLabelText('Stack')).getByRole('button', { name: 'Start stack' }))
    expect((await screen.findByRole('alert')).textContent).toContain('pick an environment first')
  })

  it('creates a crane.toml to start from', async () => {
    preview.resetCollection({})
    render(<App />)
    openContainers()
    fireEvent.click(await screen.findByRole('button', { name: 'Create crane.toml in the open folder' }))
    expect(within(await screen.findByLabelText('Stack')).getByText('postgres:16-alpine')).toBeTruthy()
  })
})
