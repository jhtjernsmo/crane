// Small helpers for showing a response. Pure functions, tested in format.test.ts.
import type { Header } from './api'

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
  return `${(n / 1024 / 1024).toFixed(1)} MB`
}

export function formatMs(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(2)} s`
}

export type Tone = 'ok' | 'redirect' | 'client' | 'server'

export function statusTone(status: number): Tone {
  if (status >= 500) return 'server'
  if (status >= 400) return 'client'
  if (status >= 300) return 'redirect'
  return 'ok'
}

export function headerValue(headers: Header[], name: string): string | undefined {
  return headers.find((h) => h.name.toLowerCase() === name.toLowerCase())?.value
}

/** Indents JSON so it can be read. Anything else comes back as it was. */
export function prettyBody(body: string, contentType = ''): { text: string; json: boolean } {
  const looksJson = /json/i.test(contentType) || /^\s*[[{]/.test(body)
  if (!looksJson) return { text: body, json: false }
  try {
    return { text: JSON.stringify(JSON.parse(body), null, 2), json: true }
  } catch {
    return { text: body, json: false }
  }
}

export type TokenKind = 'key' | 'string' | 'number' | 'literal' | 'plain'

/** Splits indented JSON into coloured pieces. Joined back, they are the input. */
export function jsonTokens(text: string): { kind: TokenKind; text: string }[] {
  const re = /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false|null)\b/g
  const out: { kind: TokenKind; text: string }[] = []
  let last = 0
  for (const m of text.matchAll(re)) {
    if (m.index > last) out.push({ kind: 'plain', text: text.slice(last, m.index) })
    if (m[1]) {
      out.push({ kind: m[2] ? 'key' : 'string', text: m[1] })
      if (m[2]) out.push({ kind: 'plain', text: m[2] })
    } else if (m[3]) out.push({ kind: 'number', text: m[3] })
    else out.push({ kind: 'literal', text: m[4] })
    last = m.index + m[0].length
  }
  if (last < text.length) out.push({ kind: 'plain', text: text.slice(last) })
  return out
}
