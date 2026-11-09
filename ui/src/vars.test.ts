import { describe, expect, it } from 'vitest'
import { fillVars, findVars, listVars } from './vars'

describe('vars', () => {
  it('finds each name once, across texts', () => {
    expect(findVars('{{base}}/u/{{ id }}', null, 'Bearer {{token}} {{base}}')).toEqual(['base', 'id', 'token'])
    expect(findVars('{{ not a name }} {{2fa}} {"a":{"b":1}}')).toEqual([])
  })

  it('fills in and lists what is missing', () => {
    expect(fillVars('{{a}}-{{b}}-{{a}}', (n) => (n === 'a' ? 'x' : undefined))).toEqual({ text: 'x-{{b}}-x', missing: ['b'] })
  })

  it('lists names like the Rust messages', () => {
    expect(listVars(['a'])).toBe('{{a}}')
    expect(listVars(['a', 'b'])).toBe('{{a}} or {{b}}')
    expect(listVars(['a', 'b', 'c'])).toBe('{{a}}, {{b}} or {{c}}')
  })
})
