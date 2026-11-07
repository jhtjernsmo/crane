import { describe, expect, it } from 'vitest'
import { checkJson } from './jsonCheck'

describe('checkJson', () => {
  it('accepts valid JSON and empty text', () => {
    for (const ok of ['{"a": [1, 2.5e3, -0, true, null, "x\\n\\u00e6"]}', '[]', '  "just text" ', '   ']) {
      expect(checkJson(ok), ok).toBeNull()
    }
  })

  it('points at the line and column of the first mistake', () => {
    expect(checkJson('{\n  "a": 1,\n  "b": \n}')).toEqual({ message: 'Expected a value, but found "}".', line: 4, column: 1 })
  })

  it('explains the usual mistakes in plain words', () => {
    expect(checkJson('{"a": 1,}')?.message).toBe('A comma is not allowed before the closing bracket.')
    expect(checkJson("{'a': 1}")?.message).toBe('Expected a name in double quotes, but found "\'".')
    expect(checkJson('["a\']')?.message).toMatch(/not closed/)
    expect(checkJson('{"a" 1}')?.message).toBe('Expected a colon after the name, but found "1".')
    expect(checkJson('{"a": 1')?.message).toBe('Expected a comma or "}", but the text ended.')
    expect(checkJson('{} {}')?.message).toBe('Expected the end, but found "{".')
    expect(checkJson('[01]')).not.toBeNull()
  })

  it('agrees with JSON.parse', () => {
    const samples = ['{"a":1}', '{"a":1,}', '[1,2', 'nul', 'true false', '"\t"', '{"x":[{"y":-1.5E+2}]}', '-', '1.', '[,]']
    for (const s of samples) {
      let valid = true
      try {
        JSON.parse(s)
      } catch {
        valid = false
      }
      expect(checkJson(s) === null, s).toBe(valid)
    }
  })
})
