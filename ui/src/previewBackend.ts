// Stand-in for the Rust side when the UI runs in a plain browser (npm run dev)
// or in tests. Everything lives in memory and is gone on reload; the real app
// keeps plain values in a file and secrets in the system keyring.
import type { BoxStatus, CollectionView, EnvFile, EnvList, Finding, HistoryEntry, ImageView, PortMap, PullProgress, ReadRequest, RunRequest, RequestSpec, Response, SaveResult, SavedRequest, TreeNode } from './api'
import { looksLikeSecret } from './secretScan'
import { fillVars, findVars, listVars } from './vars'

const WRITE_METHODS = ['POST', 'PUT', 'PATCH', 'DELETE']
const MIN_MASKED_LEN = 4

let file: EnvFile = { active: null, environments: [] }
let secrets = new Map<string, string>()

const key = (envId: string, name: string) => `${envId}/${name}`

/** Starts over; used by tests. */
export function reset(start: EnvFile = { active: null, environments: [] }, startSecrets: Record<string, string> = {}) {
  file = structuredClone(start)
  secrets = new Map(Object.entries(startSecrets))
}

export function list(): EnvList {
  const secretsSet = file.environments.flatMap((e) => e.vars.filter((v) => v.secret && secrets.has(key(e.id, v.name))).map((v) => key(e.id, v.name)))
  return { ...structuredClone(file), secretsSet, keyringProblem: null }
}

export function save(next: EnvFile): EnvList {
  const names = next.environments.map((e) => e.name.trim().toLowerCase())
  if (names.some((n) => !n)) throw 'Every environment needs a name.'
  const twice = names.find((n, i) => names.indexOf(n) !== i)
  if (twice) throw `There are two environments called "${twice}".`
  const unsafe = next.environments.find((e) => e.production && e.network?.insecure)
  if (unsafe) throw `${unsafe.name} is a production environment, so it always checks certificates. Add the CA file instead.`
  file = structuredClone({
    ...next,
    environments: next.environments.map((e) => ({ ...e, vars: e.vars.map((v) => (v.secret ? { ...v, value: '' } : v)) })),
  })
  for (const k of [...secrets.keys()]) {
    const [envId, name] = k.split('/')
    if (!file.environments.some((e) => e.id === envId && e.vars.some((v) => v.secret && v.name === name))) secrets.delete(k)
  }
  return list()
}

export function setSecret(envId: string, name: string, value: string): EnvList {
  if (value) secrets.set(key(envId, name), value)
  else secrets.delete(key(envId, name))
  return list()
}

export function resolve(spec: RequestSpec, envId: string | null, confirmed: boolean): { spec: RequestSpec; secrets: [string, string][] } {
  const env = file.environments.find((e) => e.id === envId)
  if (env?.production && !confirmed && WRITE_METHODS.includes(spec.method.toUpperCase())) {
    throw `${env.name} is a production environment. Confirm before sending ${spec.method}.`
  }
  const names = findVars(spec.url, spec.body, ...spec.headers.flatMap((h) => [h.name, h.value]))
  if (names.length === 0) return { spec, secrets: [] }
  if (!env) throw `This request uses ${listVars(names)}, but no environment is picked. Pick one at the top, or add one under Environments.`
  const missing = names.filter((n) => !env.vars.some((v) => v.name === n))
  if (missing.length) {
    throw `${env.name} has no ${listVars(missing)}. Add ${missing.length === 1 ? 'it' : 'them'} under Environments, or pick another environment.`
  }
  const used: [string, string][] = []
  const lookup = (n: string) => {
    const v = env.vars.find((x) => x.name === n)
    if (!v?.secret) return v?.value
    const value = secrets.get(key(env.id, n))
    if (value === undefined) throw `The secret {{${n}}} in ${env.name} has no value yet. Set it under Environments.`
    if (!used.some(([u]) => u === n)) used.push([n, value])
    return value
  }
  const fill = (t: string) => fillVars(t, lookup).text
  return {
    spec: { ...spec, url: fill(spec.url), headers: spec.headers.map((h) => ({ name: fill(h.name), value: fill(h.value) })), body: spec.body === null ? null : fill(spec.body) },
    secrets: used,
  }
}

export function mask(used: [string, string][], res: Response): Response {
  const hide = (text: string) =>
    used
      .filter(([, v]) => v.length >= MIN_MASKED_LEN)
      .sort((a, b) => b[1].length - a[1].length)
      .reduce((t, [name, v]) => t.split(v).join(`‹${name} hidden›`).split(encodeURIComponent(v)).join(`‹${name} hidden›`), text)
  if (used.length === 0) return res
  return { ...res, url: hide(res.url), headers: res.headers.map((h) => ({ ...h, value: hide(h.value) })), body: res.body === null ? null : hide(res.body) }
}

// ---- Collections ----------------------------------------------------------
// A pretend folder with two requests. The real app reads and writes .http
// files in the folder you pick, and its secret check runs in Rust.

const SAMPLE: Record<string, SavedRequest> = {
  'users/get-me.http': {
    name: 'Get me',
    method: 'GET',
    url: '{{base}}/headers',
    headers: [{ name: 'Accept', value: 'application/json', enabled: true }],
    bodyMode: 'none',
    body: '',
  },
  'orders/create-order.http': {
    name: 'Create order',
    method: 'POST',
    url: '{{base}}/anything/orders',
    headers: [{ name: 'Accept', value: 'application/json', enabled: true }],
    bodyMode: 'json',
    body: '{\n  "item": "crane-001",\n  "qty": 2\n}',
  },
}

let files: Map<string, SavedRequest> | null = null
let folders = new Set<string>()

export function resetCollection(start: Record<string, SavedRequest> | null = null) {
  files = start ? new Map(Object.entries(structuredClone(start))) : null
  folders = new Set([...(files?.keys() ?? [])].filter((p) => p.includes('/')).map((p) => p.slice(0, p.lastIndexOf('/'))))
}

function treeOf(prefix: string): TreeNode[] {
  const all = files ?? new Map<string, SavedRequest>()
  const here = (p: string) => p.startsWith(prefix) && !p.slice(prefix.length).includes('/')
  const subfolders = [...folders].filter(here).sort()
  const requests = [...all.keys()].filter(here).sort()
  return [
    ...subfolders.map((f) => ({ kind: 'folder' as const, name: f.slice(prefix.length), path: f, children: treeOf(`${f}/`) })),
    ...requests.map((p) => ({ kind: 'request' as const, name: all.get(p)!.name, path: p, method: all.get(p)!.method })),
  ]
}

const view = (): CollectionView | null => (files ? { name: 'api-tests', root: '~/code/api-tests', tree: treeOf('') } : null)

export function pickCollection(): CollectionView | null {
  if (!files) resetCollection(SAMPLE)
  return view()
}
export const collectionTree = view
export function closeCollection() {
  files = null
}

function need(): Map<string, SavedRequest> {
  if (!files) throw 'Open a folder first.'
  return files
}

export function readRequest(path: string): ReadRequest {
  const r = need().get(path)
  if (!r) throw `"${path}" does not exist.`
  return { request: structuredClone(r), readOnly: null }
}

const SECRET_HEADER = /^(authorization|proxy-authorization|cookie)$|token|secret|passw(or)?d|api[-_]?key|session|credential|private/i

export function saveRequest(path: string, request: SavedRequest, allowSecrets: boolean): SaveResult {
  const all = need()
  if (path.split('/').some((p) => p === '..' || p === '') || !path.endsWith('.http')) throw `"${path}" is not a path inside the collection.`
  if (!allowSecrets) {
    const findings: Finding[] = []
    for (const h of request.headers) {
      const value = h.value.trim().replace(/^(bearer|basic|token|digest)\s+/i, '')
      if (value && !findVars(h.value).length && (SECRET_HEADER.test(h.name.trim()) || looksLikeSecret(h.value))) {
        const suggestedName = /^(proxy-)?authorization$/i.test(h.name.trim()) ? 'token' : h.name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_')
        findings.push({ place: `header ${h.name.trim()}`, header: h.name.trim(), value, suggestedName, preview: `${value.slice(0, 4)}… (${value.length} characters)` })
      }
    }
    if (findings.length) return { saved: null, findings }
  }
  all.set(path, structuredClone(request))
  return { saved: path, findings: [] }
}

export function newRequestPath(folder: string, name: string): string {
  const all = need()
  const slug = name.trim().toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/g, '') || 'request'
  const at = (n: number) => `${folder ? `${folder}/` : ''}${slug}${n > 1 ? `-${n}` : ''}.http`
  let n = 1
  while (all.has(at(n))) n++
  return at(n)
}

export function createFolder(parent: string, name: string): string {
  need()
  const path = parent ? `${parent}/${name.trim()}` : name.trim()
  folders.add(path)
  return path
}

export function deleteRequest(path: string) {
  need().delete(path)
}

// ---- History ---------------------------------------------------------------

let historyEntries: HistoryEntry[] = []

export function resetHistory(start: HistoryEntry[] = []) {
  historyEntries = structuredClone(start)
}
export const historyList = () => structuredClone(historyEntries)
export function historyAdd(entry: HistoryEntry): HistoryEntry {
  historyEntries.push(structuredClone(entry))
  return entry
}
export function historyClear() {
  historyEntries = []
}

export const pickCertFile = () => '/home/you/certs/company-ca.pem'

// ---- Containers -------------------------------------------------------------
// A pretend engine for the browser preview and tests. The real one pulls
// images and runs them in Linux namespaces (crates/crane-box).

interface FakeBox {
  name: string
  image: string
  running: boolean
  exitCode: number | null
  ports: PortMap[]
}

let images: ImageView[] = []
let boxes: FakeBox[] = []
let pullListeners: ((p: PullProgress) => void)[] = []

export function resetBoxes(start: ImageView[] = []) {
  images = structuredClone(start)
  boxes = []
}

export const boxStatus = (): BoxStatus => ({ available: true, problem: null, images: structuredClone(images), containers: structuredClone(boxes) })

export function onPullProgress(handler: (p: PullProgress) => void) {
  pullListeners.push(handler)
  return () => {
    pullListeners = pullListeners.filter((h) => h !== handler)
  }
}

export async function boxPull(name: string): Promise<ImageView> {
  const clean = name.trim().replace(/^docker\.io\/(library\/)?/, '')
  if (!/^[a-z0-9][a-z0-9._/-]*(:[\w.-]+)?$/.test(clean)) throw `"${name}" is not an image name. Try something like postgres:16 or ghcr.io/org/app:1.0.`
  const full = clean.includes(':') ? clean : `${clean}:latest`
  for (const done of [0, 40, 100]) pullListeners.forEach((h) => h({ step: done < 100 ? 'Downloading' : 'Unpacking', done, total: 100 }))
  const port = full.startsWith('postgres') ? [5432] : full.startsWith('redis') ? [6379] : [80]
  const image = { name: full, size: 41_000_000, exposedPorts: port }
  images = [...images.filter((i) => i.name !== full), image].sort((a, b) => a.name.localeCompare(b.name))
  return image
}

export function boxRemoveImage(name: string) {
  if (boxes.some((b) => b.image === name)) throw `${name} is used by a container. Remove the container first.`
  images = images.filter((i) => i.name !== name)
}

export function boxRun(req: RunRequest) {
  if (boxes.some((b) => b.name === req.name)) throw `There is already a container called ${req.name}. Remove it first.`
  if (!images.some((i) => i.name === req.image)) throw `${req.image} is not pulled yet.`
  const taken = boxes.flatMap((b) => b.ports.map((p) => p.host))
  const clash = req.ports.find((p) => taken.includes(p.host))
  if (clash) throw `Port ${clash.host} on this computer is taken or not allowed. Pick another in the ports list.`
  boxes = [...boxes, { name: req.name, image: req.image, running: true, exitCode: null, ports: req.ports }]
}

export function boxStop(name: string) {
  boxes = boxes.map((b) => (b.name === name ? { ...b, running: false, exitCode: 0 } : b))
}

export function boxRemove(name: string) {
  boxes = boxes.filter((b) => b.name !== name)
}

export const boxLogs = (name: string) => `${name} is a pretend container in the browser preview.\nready to accept connections\n`
