import { describe, expect, it } from 'vitest'
import { looksLikeSecret } from './secretScan'

describe('looksLikeSecret', () => {
  it('spots well-known key formats', () => {
    for (const v of [
      // Built at run time, so secret scanners do not mistake the test for a leak.
      ['sk', 'live', '51HxYz2eZvKYlo2C8w9a7b6c'].join('_'),
      'gh' + 'p_1234567890abcdefghijABCDEFGHIJ123456',
      ['xoxb', '123456789012', '1234567890123', 'AbCdEf'].join('-'),
      'AKIA' + 'IOSFODNN7EXAMPLE',
      'eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.c2lnbmF0dXJl',
      'Bearer 9f8e7d6c5b4a39281706f5e4d3c2b1a0',
    ]) {
      expect(looksLikeSecret(v), v).toBe(true)
    }
  })

  it('spots long random-looking strings', () => {
    expect(looksLikeSecret('Zx9QpL2mN8vR4tY7wK3jH6gF')).toBe(true)
  })

  it('leaves ordinary values alone', () => {
    for (const v of [
      '',
      'http://localhost:8080',
      'https://api.example.com/v1/users',
      'application/json',
      'Hello world, this is a long sentence',
      '{{token}}',
      'Bearer {{token}}',
      '12345',
      '2026-11-09T12:00:00Z',
      'ada.lovelace@example.com',
      'c0a8016e-6f4b-4e7a-9a3d-2f1b3c4d5e6f',
    ]) {
      expect(looksLikeSecret(v), v).toBe(false)
    }
  })
})
