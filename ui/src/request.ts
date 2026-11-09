// The request being edited, and the plain functions that turn it into what the
// Rust side sends. Kept free of React so every rule here has a test.
import type { Header, RequestSpec } from './api'

import { findVars } from './vars'

export { checkJson } from './jsonCheck'

/** One line in a name/value table. `on: false` keeps it on screen but unsent. */
export interface Row {
  id: number
  name: string
  value: string
  on: boolean
}

export const BODY_MODES = ['none', 'json', 'text', 'form'] as const
export type BodyMode = (typeof BODY_MODES)[number]

export interface Draft {
  method: string
  url: string
  headers: Row[]
  bodyMode: BodyMode
  body: string
  form: Row[]
}

let nextRowId = 1
export const row = (name = '', value = ''): Row => ({ id: nextRowId++, name, value, on: true })

export const emptyDraft = (): Draft => ({ method: 'GET', url: '', headers: [], bodyMode: 'none', body: '', form: [] })

// ---- Query string -------------------------------------------------------

export interface Pair {
  name: string
  value: string
}

function splitUrl(url: string): { base: string; query: string; hash: string } {
  const hashAt = url.indexOf('#')
  const hash = hashAt < 0 ? '' : url.slice(hashAt)
  const rest = hashAt < 0 ? url : url.slice(0, hashAt)
  const queryAt = rest.indexOf('?')
  return queryAt < 0 ? { base: rest, query: '', hash } : { base: rest.slice(0, queryAt), query: rest.slice(queryAt + 1), hash }
}

function decode(part: string): string {
  try {
    return decodeURIComponent(part.replace(/\+/g, ' '))
  } catch {
    return part
  }
}

/** Only what would break the query is escaped, so the address stays readable. */
const encode = (part: string) => part.replace(/[%&=#+\s]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase().padStart(2, '0')}`)

export function parseQuery(url: string): Pair[] {
  const { query } = splitUrl(url)
  if (!query) return []
  return query
    .split('&')
    .filter(Boolean)
    .map((part) => {
      const eq = part.indexOf('=')
      return eq < 0 ? { name: decode(part), value: '' } : { name: decode(part.slice(0, eq)), value: decode(part.slice(eq + 1)) }
    })
}

export function withQuery(url: string, pairs: Pair[]): string {
  const { base, hash } = splitUrl(url)
  const query = pairs.map((p) => (p.value === '' && p.name !== '' ? encode(p.name) : `${encode(p.name)}=${encode(p.value)}`)).join('&')
  return `${base}${query ? `?${query}` : ''}${hash}`
}

// ---- Secrets ------------------------------------------------------------

const SECRET_EXACT = new Set(['authorization', 'proxy-authorization', 'cookie', 'set-cookie'])
const SECRET_PART = /(token|secret|passw(or)?d|api[-_]?key|session|credential|private)/i

/** Names whose values are hidden on screen and never kept in history. */
export function isSecretName(name: string): boolean {
  const n = name.trim().toLowerCase()
  return SECRET_EXACT.has(n) || SECRET_PART.test(n)
}

// ---- Body ---------------------------------------------------------------

const MODE_TYPES: Record<BodyMode, string | null> = {
  none: null,
  json: 'application/json',
  text: 'text/plain',
  form: 'application/x-www-form-urlencoded',
}

const sendable = (rows: Row[]) => rows.filter((r) => r.on && r.name.trim() !== '')

/** The Content-Type Crane adds for the body, unless one is set by hand. */
export function autoContentType(d: Draft): string | null {
  const own = sendable(d.headers).some((h) => h.name.trim().toLowerCase() === 'content-type')
  return own ? null : MODE_TYPES[d.bodyMode]
}

function bodyOf(d: Draft): string | null {
  switch (d.bodyMode) {
    case 'none':
      return null
    case 'form':
      return new URLSearchParams(sendable(d.form).map((r) => [r.name, r.value])).toString()
    default:
      return d.body
  }
}

// ---- Sending ------------------------------------------------------------

export function buildSpec(d: Draft): RequestSpec {
  const headers: Header[] = sendable(d.headers).map((h) => ({ name: h.name.trim(), value: h.value }))
  const type = autoContentType(d)
  return {
    method: d.method,
    url: d.url.trim(),
    headers: type ? [...headers, { name: 'Content-Type', value: type }] : headers,
    body: bodyOf(d),
  }
}

// ---- History ------------------------------------------------------------

/** `Bearer {{token}}` only points at the keyring, so it is safe to keep; `Bearer abc` is not. */
const pointsAtVariable = (value: string) => findVars(value).length > 0

const blankSecrets = (rows: Row[]) => rows.map((r) => (isSecretName(r.name) && !pointsAtVariable(r.value) ? { ...r, value: '' } : r))

/** A copy safe to keep in the history list: secret values are dropped. */
export const forHistory = (d: Draft): Draft => ({ ...d, headers: blankSecrets(d.headers), form: blankSecrets(d.form) })

/** Brings an old request back, taking secret values from the screen as it is now. */
export function reopenDraft(old: Draft, now: Draft): Draft {
  const refill = (rows: Row[], current: Row[]) =>
    rows.map((r) => {
      if (!isSecretName(r.name) || r.value) return { ...r }
      const match = current.find((c) => c.name.trim().toLowerCase() === r.name.trim().toLowerCase())
      return match ? { ...r, value: match.value } : { ...r }
    })
  return { ...old, headers: refill(old.headers, now.headers), form: refill(old.form, now.form) }
}
