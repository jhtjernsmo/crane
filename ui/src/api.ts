// Typed wrapper around the Rust command in src-tauri/src/lib.rs.
//
// Outside Tauri (plain `npm run dev` in a browser, or tests) there is no Rust
// side, so requests fall back to the browser's fetch. That works for sites
// that allow it (CORS); inside the app every address works.
import { invoke } from '@tauri-apps/api/core'

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

export const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS'] as const

export const IN_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export async function sendRequest(spec: RequestSpec): Promise<Response> {
  try {
    if (IN_TAURI) return await invoke<Response>('send_request', { spec })
    return await sendFromBrowser(spec)
  } catch (e) {
    throw new Error(typeof e === 'string' ? e : e instanceof Error ? e.message : String(e))
  }
}

async function sendFromBrowser(spec: RequestSpec): Promise<Response> {
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
  return {
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
  }
}
