import { describe, expect, it } from 'vitest'
import type { Response } from './api'
import { check } from './checks'

const res: Response = {
  status: 201,
  statusText: 'Created',
  url: 'http://x/',
  headers: [{ name: 'content-type', value: 'application/json' }],
  body: '{"id":7,"name":"Ada","tags":["a","b"],"items":[{"sku":"c-1"},{"sku":"c-2"}]}',
  truncated: false,
  size: 10,
  timing: { dnsMs: 0, connectMs: 0, waitingMs: 80, downloadMs: 4, totalMs: 84 },
}

const ok = (line: string) => check(line, res).passed

describe('checks (preview copy of the Rust rules)', () => {
  it('agrees with the Rust tests', () => {
    expect(ok('status 201') && ok('status 2xx') && !ok('status 200')).toBe(true)
    expect(ok('time < 500ms') && ok('time < 1s') && !ok('time < 50')).toBe(true)
    expect(ok('header Content-Type contains json') && !ok('header x-nope exists')).toBe(true)
    expect(ok('json $.id = 7') && ok('json $.name = Ada') && ok('json $.items[1].sku = c-2')).toBe(true)
    expect(ok('json items length 2') && ok('json $.tags contains b') && !ok('json $.items[5] exists')).toBe(true)
    expect(check('json $.name = Grace', res).detail).toBe('got Ada')
    expect(check('speed < 3', res).detail).toContain('should start with')
  })
})
