import { describe, expect, it } from 'vitest'
import { formatBytes, formatMs, headerValue, jsonTokens, prettyBody, statusTone } from './format'

describe('formatBytes', () => {
  it('uses B, KB and MB', () => {
    expect(formatBytes(412)).toBe('412 B')
    expect(formatBytes(2048)).toBe('2.0 KB')
    expect(formatBytes(3 * 1024 * 1024)).toBe('3.0 MB')
  })
})

describe('formatMs', () => {
  it('switches to seconds from 1000 ms', () => {
    expect(formatMs(84)).toBe('84 ms')
    expect(formatMs(1500)).toBe('1.50 s')
  })
})

describe('statusTone', () => {
  it('groups status codes', () => {
    expect([200, 201, 302, 404, 422, 500, 503].map(statusTone)).toEqual(['ok', 'ok', 'redirect', 'client', 'client', 'server', 'server'])
  })
})

describe('headerValue', () => {
  it('ignores case', () => {
    expect(headerValue([{ name: 'content-type', value: 'application/json' }], 'Content-Type')).toBe('application/json')
    expect(headerValue([], 'x')).toBeUndefined()
  })
})

describe('prettyBody', () => {
  it('indents JSON', () => {
    expect(prettyBody('{"a":1}', 'application/json')).toEqual({ text: '{\n  "a": 1\n}', json: true })
  })
  it('spots JSON without a content type', () => {
    expect(prettyBody('[1,2]').json).toBe(true)
  })
  it('leaves other text and broken JSON alone', () => {
    expect(prettyBody('<h1>hi</h1>', 'text/html')).toEqual({ text: '<h1>hi</h1>', json: false })
    expect(prettyBody('{"a":', 'application/json')).toEqual({ text: '{"a":', json: false })
  })
})

describe('jsonTokens', () => {
  const text = '{\n  "id": "ord_7",\n  "total": -3.5e2,\n  "ok": true,\n  "note": "say \\"hi\\"",\n  "x": null\n}'
  const tokens = jsonTokens(text)
  it('joins back into the input', () => {
    expect(tokens.map((t) => t.text).join('')).toBe(text)
  })
  it('tells keys, strings, numbers and literals apart', () => {
    const kinds = tokens.filter((t) => t.kind !== 'plain').map((t) => `${t.kind}:${t.text}`)
    expect(kinds).toEqual([
      'key:"id"', 'string:"ord_7"', 'key:"total"', 'number:-3.5e2', 'key:"ok"', 'literal:true',
      'key:"note"', 'string:"say \\"hi\\""', 'key:"x"', 'literal:null',
    ])
  })
})
