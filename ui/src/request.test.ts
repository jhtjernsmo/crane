import { describe, expect, it } from 'vitest'
import {
  type Draft,
  autoContentType,
  buildSpec,
  emptyDraft,
  forHistory,
  isSecretName,
  parseQuery,
  reopenDraft,
  row,
  withQuery,
} from './request'

const draft = (over: Partial<Draft>): Draft => ({ ...emptyDraft(), ...over })

describe('parseQuery', () => {
  it('reads pairs, decodes them and ignores the fragment', () => {
    expect(parseQuery('api.dev/users?page=2&q=red%20car&flag#top')).toEqual([
      { name: 'page', value: '2' },
      { name: 'q', value: 'red car' },
      { name: 'flag', value: '' },
    ])
  })

  it('keeps text it cannot decode as written', () => {
    expect(parseQuery('x.dev?off=100%')).toEqual([{ name: 'off', value: '100%' }])
  })

  it('returns nothing when there is no query', () => {
    expect(parseQuery('localhost:8080/api')).toEqual([])
  })
})

describe('withQuery', () => {
  it('replaces the query and keeps the path and fragment', () => {
    expect(withQuery('x.dev/a?old=1#f', [{ name: 'q', value: 'a b&c' }])).toBe('x.dev/a?q=a%20b%26c#f')
  })

  it('removes the question mark when no pairs are left', () => {
    expect(withQuery('x.dev/a?old=1', [])).toBe('x.dev/a')
  })

  it('leaves {{variables}} readable', () => {
    expect(withQuery('x.dev', [{ name: 'key', value: '{{token}}' }])).toBe('x.dev?key={{token}}')
  })

  it('round-trips with parseQuery', () => {
    const pairs = [{ name: 'a', value: '1=2' }, { name: 'b', value: '50% + tax' }]
    expect(parseQuery(withQuery('x.dev', pairs))).toEqual(pairs)
  })
})

describe('isSecretName', () => {
  it('spots names that usually hold secrets', () => {
    for (const n of ['Authorization', 'cookie', 'X-API-Key', 'x-auth-token', 'client_secret', 'password', 'apikey']) {
      expect(isSecretName(n), n).toBe(true)
    }
  })

  it('leaves ordinary names alone', () => {
    for (const n of ['Accept', 'Content-Type', 'page', 'keyword', 'User-Agent']) {
      expect(isSecretName(n), n).toBe(false)
    }
  })
})

describe('autoContentType', () => {
  it('follows the body mode unless a Content-Type is set', () => {
    expect(autoContentType(draft({ bodyMode: 'json', body: '{}' }))).toBe('application/json')
    expect(autoContentType(draft({ bodyMode: 'form', form: [row('a', '1')] }))).toBe('application/x-www-form-urlencoded')
    expect(autoContentType(draft({ bodyMode: 'none' }))).toBeNull()
    expect(autoContentType(draft({ bodyMode: 'json', body: '{}', headers: [row('content-type', 'application/vnd.api+json')] }))).toBeNull()
  })

  it('ignores a switched-off Content-Type', () => {
    const off = { ...row('Content-Type', 'text/xml'), on: false }
    expect(autoContentType(draft({ bodyMode: 'text', body: 'hi', headers: [off] }))).toBe('text/plain')
  })
})

describe('buildSpec', () => {
  it('sends only headers that are on and named', () => {
    const spec = buildSpec(
      draft({ url: 'x.dev', headers: [row('Accept', 'text/html'), { ...row('X-Debug', '1'), on: false }, row('  ', 'stray')] }),
    )
    expect(spec).toEqual({ method: 'GET', url: 'x.dev', headers: [{ name: 'Accept', value: 'text/html' }], body: null })
  })

  it('adds the content type and the body for JSON', () => {
    const spec = buildSpec(draft({ method: 'POST', url: 'x.dev', bodyMode: 'json', body: '{"qty":2}' }))
    expect(spec.headers).toEqual([{ name: 'Content-Type', value: 'application/json' }])
    expect(spec.body).toBe('{"qty":2}')
  })

  it('encodes form rows that are on', () => {
    const spec = buildSpec(draft({ bodyMode: 'form', form: [row('name', 'Ada L'), { ...row('skip', 'x'), on: false }, row('a&b', '=')] }))
    expect(spec.body).toBe('name=Ada+L&a%26b=%3D')
  })

  it('sends no body when the mode is none, even if text is left over', () => {
    expect(buildSpec(draft({ bodyMode: 'none', body: 'left over' })).body).toBeNull()
  })
})

describe('history', () => {
  it('never keeps secret header values', () => {
    const kept = forHistory(draft({ headers: [row('Authorization', 'Bearer abc'), row('Accept', 'json')] }))
    expect(kept.headers.map((h) => h.value)).toEqual(['', 'json'])
  })

  it('never keeps secret form values', () => {
    const kept = forHistory(draft({ form: [row('password', 'hunter2'), row('user', 'ada')] }))
    expect(kept.form.map((h) => h.value)).toEqual(['', 'ada'])
  })

  it('reopening refills secrets from what is on screen now', () => {
    const now = draft({ headers: [row('authorization', 'Bearer new')] })
    const old = forHistory(draft({ url: 'x.dev/old', headers: [row('Authorization', 'Bearer old'), row('Accept', 'json')] }))
    const back = reopenDraft(old, now)
    expect(back.url).toBe('x.dev/old')
    expect(back.headers.map((h) => [h.name, h.value])).toEqual([['Authorization', 'Bearer new'], ['Accept', 'json']])
  })
})
