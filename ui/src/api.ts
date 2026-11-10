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

export interface CheckResult {
  check: string
  passed: boolean
  detail: string | null
}

export interface Response {
  /** How the request's `@expect` checks went; empty without checks. */
  checks?: CheckResult[]
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

export interface Network {
  /** PEM file with CA certificates to trust on top of the system's. */
  caFile: string
  /** PEM file with a client certificate and its unencrypted key. */
  clientCertFile: string
  /** Skip certificate checks; refused for production environments. */
  insecure: boolean
  /** '' = system settings, 'none' = direct, else a proxy URL. */
  proxy: string
  noProxy: string
}

export const NO_NETWORK: Network = { caFile: '', clientCertFile: '', insecure: false, proxy: '', noProxy: '' }

export interface Environment {
  id: string
  name: string
  production: boolean
  vars: Var[]
  network?: Network
  /** Sent with every request that does not set the same header itself. */
  headers?: Header[]
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
export const sendRequest = (spec: RequestSpec, envId: string | null = null, confirmed = false, expect: string[] = []) =>
  call<Response>('send_request', { spec, envId, confirmed, expect }, async () => {
    const res = await sendFromBrowser(preview.resolve(spec, envId, confirmed))
    return expect.length ? { ...res, checks: preview.checkAll(expect, res) } : res
  })

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
  expect?: string[]
  save?: string[]
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

/** Opens the system file picker from Rust; only the chosen path comes back. */
export const pickCertFile = () => call<string | null>('pick_cert_file', {}, async () => preview.pickCertFile())

// ---- Containers (Crane's own engine) -------------------------------------

export interface PortMap {
  host: number
  container: number
}

export interface ImageView {
  name: string
  size: number
  exposedPorts: number[]
}

export interface ContainerView {
  name: string
  /** The stack it belongs to; those start and stop together. */
  stack: string | null
  image: string
  running: boolean
  exitCode: number | null
  ports: PortMap[]
}

export interface BoxStatus {
  available: boolean
  problem: string | null
  images: ImageView[]
  containers: ContainerView[]
}

export interface PullProgress {
  step: string
  done: number
  total: number
}

export interface RunRequest {
  name: string
  image: string
  ports: PortMap[]
  env: [string, string][]
  args: string[] | null
}

export const boxStatus = () => call<BoxStatus>('box_status', {}, async () => preview.boxStatus())
export const boxPull = (name: string) => call<ImageView>('box_pull', { name }, async () => preview.boxPull(name))
export const boxRemoveImage = (name: string) => call<void>('box_remove_image', { name }, async () => preview.boxRemoveImage(name))
export const boxRun = (req: RunRequest) => call<void>('box_run', { req }, async () => preview.boxRun(req))
export const boxStop = (name: string) => call<void>('box_stop', { name }, async () => preview.boxStop(name))
export const boxRemove = (name: string) => call<void>('box_remove', { name }, async () => preview.boxRemove(name))
export const boxLogs = (name: string) => call<string>('box_logs', { name }, async () => preview.boxLogs(name))

/** Pull progress from Rust; returns a function that stops listening. */
export async function onPullProgress(handler: (p: PullProgress) => void): Promise<() => void> {
  if (!IN_TAURI) return preview.onPullProgress(handler)
  const { listen } = await import('@tauri-apps/api/event')
  return listen<PullProgress>('box-pull', (e) => handler(e.payload))
}

// ---- Stacks: crane.toml in the open folder ----------------------------------

export interface StackService {
  name: string
  image: string
  ports: string[]
  volumes: string[]
  dependsOn: string[]
}

export interface StackInfo {
  exists: boolean
  name: string
  services: StackService[]
  problem: string | null
  running: boolean
}

export interface StackEvent {
  service: string
  step: string
}

export const stackInfo = () => call<StackInfo>('stack_info', {}, async () => preview.stackInfo())
export const stackCreate = () => call<void>('stack_create', {}, async () => preview.stackCreate())
/** Starts every service; `{{variables}}` come from this environment. */
export const stackUp = (envId: string | null) => call<void>('stack_up', { envId }, async () => preview.stackUp(envId))
export const stackDown = () => call<void>('stack_down', {}, async () => preview.stackDown())

export async function onStackProgress(handler: (e: StackEvent) => void): Promise<() => void> {
  if (!IN_TAURI) return preview.onStackProgress(handler)
  const { listen } = await import('@tauri-apps/api/event')
  return listen<StackEvent>('stack-progress', (e) => handler(e.payload))
}

// ---- Runs -------------------------------------------------------------------

export interface Outcome {
  path: string
  name: string
  method: string
  status: number | null
  totalMs: number | null
  error: string | null
  checks: CheckResult[]
  saved: string[]
  passed: boolean
}

export interface Report {
  outcomes: Outcome[]
  elapsedMs: number
}

/** Runs every request in the open folder, in path order, and checks the answers. */
export const runCollection = (envId: string | null, confirmed = false) =>
  call<Report>('run_collection', { envId, paths: null, confirmed }, async () => preview.runCollection(envId, confirmed, sendFromBrowser))

// ---- Organisation policy ----------------------------------------------------

/** Rules an organisation sets in /etc/crane/policy.toml; enforced in Rust. */
export interface Policy {
  contact: string | null
  allowedHosts: string[] | null
  blockedHosts: string[]
  allowSavingSecrets: boolean
  allowInsecureTls: boolean
  containers: boolean
  keepHistory: boolean
  checkUpdates: boolean
  /** The file the rules come from; null when there is none. */
  source: string | null
  /** Set when the file is broken and the strictest rules apply. */
  problem: string | null
}

export const OPEN_POLICY: Policy = {
  contact: null,
  allowedHosts: null,
  blockedHosts: [],
  allowSavingSecrets: true,
  allowInsecureTls: true,
  containers: true,
  keepHistory: true,
  checkUpdates: true,
  source: null,
  problem: null,
}

export const getPolicy = () => call<Policy>('get_policy', {}, async () => preview.getPolicy())
