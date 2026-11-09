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
  /** Looking up the address; 0 for IP addresses and in the browser preview. */
  dnsMs: number
  /** TCP and TLS, after the lookup. */
  connectMs: number
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
      dnsMs: 0,
      connectMs: 0,
      waitingMs: Math.round(headersAt - started),
      downloadMs: Math.round(done - headersAt),
      totalMs: Math.round(done - started),
    },
  })
}

// ---- Collections: folders of .http files ---------------------------------

export interface SavedHeader {
  name: string
  value: string
  enabled: boolean
}

export interface SavedRequest {
  name: string
  method: string
  url: string
  headers: SavedHeader[]
  bodyMode: string
  body: string
}

export interface ReadRequest {
  request: SavedRequest
  /** Why Crane will not write the file back, if it will not. */
  readOnly: string | null
}

export type TreeNode =
  | { kind: 'folder'; name: string; path: string; children: TreeNode[] }
  | { kind: 'request'; name: string; path: string; method: string }

export interface CollectionView {
  name: string
  root: string
  tree: TreeNode[]
}

export interface Finding {
  place: string
  header: string | null
  value: string
  suggestedName: string
  preview: string
}

export interface SaveResult {
  saved: string | null
  findings: Finding[]
}

/** Opens the system folder picker from Rust. `null` when cancelled. */
export const pickCollection = () => call<CollectionView | null>('pick_collection', {}, async () => preview.pickCollection())
export const collectionTree = () => call<CollectionView | null>('collection_tree', {}, async () => preview.collectionTree())
export const closeCollection = () => call<void>('close_collection', {}, async () => preview.closeCollection())
export const readRequest = (path: string) => call<ReadRequest>('read_request', { path }, async () => preview.readRequest(path))
/** Rust refuses literal secrets unless `allowSecrets`; the findings come back instead. */
export const saveRequest = (path: string, request: SavedRequest, allowSecrets = false) =>
  call<SaveResult>('save_request', { path, request, allowSecrets }, async () => preview.saveRequest(path, request, allowSecrets))
export const newRequestPath = (folder: string, name: string) => call<string>('new_request_path', { folder, name }, async () => preview.newRequestPath(folder, name))
export const createFolder = (parent: string, name: string) => call<string>('create_folder', { parent, name }, async () => preview.createFolder(parent, name))
export const deleteRequest = (path: string) => call<void>('delete_request', { path }, async () => preview.deleteRequest(path))

// ---- History -------------------------------------------------------------

/** What is kept of a send between runs: never the body of the answer. */
export interface HistoryEntry {
  at: number
  envName: string | null
  request: SavedRequest
  status: number | null
  totalMs: number | null
  size: number | null
  error: string | null
}

/** Oldest first. */
export const historyList = () => call<HistoryEntry[]>('history_list', {}, async () => preview.historyList())
/** Rust replaces anything that looks like a secret before writing; the stored entry comes back. */
export const historyAdd = (entry: HistoryEntry) => call<HistoryEntry>('history_add', { entry }, async () => preview.historyAdd(entry))
export const historyClear = () => call<void>('history_clear', {}, async () => preview.historyClear())
