// Typed wrapper around the Rust command in src-tauri/src/lib.rs.
//
// Outside Tauri (plain `npm run dev` in a browser, or tests) there is no Rust
// side, so requests fall back to the browser's fetch. That works for sites
// that allow it (CORS); inside the app every address works.
import { invoke } from '@tauri-apps/api/core'
import * as preview from './previewBackend'

export interface Header {
  name: string
  value: string
}

export interface RequestSpec {
  method: string
  url: string
  headers: Header[]
  body: string | null
}

export interface Timing {
  waitingMs: number
  downloadMs: number
  totalMs: number
}

export interface Response {
  status: number
  statusText: string
  url: string
  headers: Header[]
  body: string | null
  truncated: boolean
  size: number
  timing: Timing
}

export interface Var {
  name: string
  /** Always empty for secrets: their value is only in the system keyring. */
  value: string
  secret: boolean
}

export interface Environment {
  id: string
  name: string
  production: boolean
  vars: Var[]
}

export interface EnvFile {
  active: string | null
  environments: Environment[]
}

export interface EnvList extends EnvFile {
  /** `"<env id>/<name>"` for each secret that has a value. */
  secretsSet: string[]
  keyringProblem: string | null
}

export const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const

export const IN_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

const asError = (e: unknown) => new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e))

async function call<T>(command: string, args: Record<string, unknown>, fallback: () => Promise<T>): Promise<T> {
  try {
    return IN_TAURI ? await invoke<T>(command, args) : await fallback()
  } catch (e) {
    throw asError(e)
  }
}

/** Variables are filled in by Rust, which also hides echoed secrets in the answer. */
export const sendRequest = (spec: RequestSpec, envId: string | null = null, confirmed = false) =>
  call<Response>('send_request', { spec, envId, confirmed }, async () => sendFromBrowser(preview.resolve(spec, envId, confirmed)))

export const listEnvironments = () => call<EnvList>('list_environments', {}, async () => preview.list())

export const saveEnvironments = (file: EnvFile) => call<EnvList>('save_environments', { file }, async () => preview.save(file))

/** Write-only: the value goes to the keyring and never comes back. Empty removes it. */
export const setSecret = (envId: string, name: string, value: string) =>
  call<EnvList>('set_secret', { envId, name, value }, async () => preview.setSecret(envId, name, value))

async function sendFromBrowser({ spec, secrets }: { spec: RequestSpec; secrets: [string, string][] }): Promise<Response> {
  const url = /^https?:\/\//i.test(spec.url.trim()) ? spec.url.trim() : `https://${spec.url.trim()}`
  const started = performance.now()
  let res: globalThis.Response
  try {
    res = await fetch(url, {
      method: spec.method,
      headers: spec.headers.filter((h) => h.name.trim()).map((h) => [h.name, h.value]),
      body: spec.body || undefined,
    })
  } catch {
    throw 'The browser preview could not reach that address (it may block other sites). Run the app with `npm run tauri dev` to send anywhere.'
  }
  const headersAt = performance.now()
  const buf = new Uint8Array(await res.arrayBuffer())
  const done = performance.now()
  let body: string | null
  try {
    body = new TextDecoder('utf-8', { fatal: true }).decode(buf)
  } catch {
    body = null
  }
  return preview.mask(secrets, {
    status: res.status,
    statusText: res.statusText,
    url: res.url,
    headers: [...res.headers].map(([name, value]) => ({ name, value })),
    body,
    truncated: false,
    size: buf.length,
    timing: {
      waitingMs: Math.round(headersAt - started),
      downloadMs: Math.round(done - headersAt),
      totalMs: Math.round(done - started),
    },
  })
}
