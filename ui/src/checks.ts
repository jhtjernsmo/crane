// The check language for the browser preview and tests. The app runs the
// Rust version (crates/crane-core/src/checks.rs); keep the two in step.
import type { CheckResult, Response } from './api'

type Found = { value: unknown } | { missing: true } | { error: string }

function walk(value: unknown, path: string): unknown {
  const steps = path.replace(/^\$/, '').match(/[^.[\]]+|\[\d+\]/g) ?? []
  return steps.reduce<unknown>((cur, step) => {
    if (cur === undefined || cur === null) return undefined
    const key = step.startsWith('[') ? Number(step.slice(1, -1)) : step.replace(/^["']|["']$/g, '')
    return (cur as Record<string | number, unknown>)[key]
  }, value)
}

function read(source: string[], res: Response): Found {
  const [kind, arg] = source
  switch (kind) {
    case 'status':
      return { value: res.status }
    case 'time':
      return { value: res.timing.totalMs }
    case 'body':
      return res.body === null ? { missing: true } : { value: res.body }
    case 'header': {
      const h = res.headers.find((x) => x.name.toLowerCase() === arg?.toLowerCase())
      return h ? { value: h.value } : { missing: true }
    }
    case 'json': {
      try {
        const v = walk(JSON.parse(res.body ?? ''), arg ?? '')
        return v === undefined ? { missing: true } : { value: v }
      } catch {
        return { error: 'The answer is not JSON.' }
      }
    }
    default:
      return { error: `"${source.join(' ')}" should start with status, time, header, body or json.` }
  }
}

const text = (v: unknown) => (typeof v === 'string' ? v : JSON.stringify(v))
const unquote = (s: string) => s.trim().replace(/^(["'])(.*)\1$/, '$2')
const millis = (s: string) => (s.endsWith('ms') ? Number(s.slice(0, -2)) : s.endsWith('s') ? Number(s.slice(0, -1)) * 1000 : Number(s))

export function check(line: string, res: Response): CheckResult {
  const words = line.trim().split(/\s+/)
  const takes = words[0] === 'header' || words[0] === 'json' ? 2 : 1
  const found = read(words.slice(0, takes), res)
  const [op, ...rest] = words.slice(takes)
  const arg = unquote(rest.join(' '))
  const result = (passed: boolean, detail: string | null) => ({ check: line, passed, detail: passed ? null : detail })
  if ('error' in found) return result(false, found.error)
  if ('missing' in found) return result(op === '!=', 'it is not there')
  const v = found.value
  const got = `got ${text(v)}`
  if (words[0] === 'status' && op && !rest.length) {
    return op.endsWith('xx') ? result(Math.floor(res.status / 100) === Number(op[0]), got) : result(String(res.status) === op, got)
  }
  switch (op) {
    case 'exists':
      return result(true, null)
    case '=':
    case '==':
      return result(text(v) === arg || JSON.stringify(v) === arg, got)
    case '!=':
      return result(text(v) !== arg, got)
    case 'contains':
      return result(Array.isArray(v) ? v.some((x) => text(x) === arg) : text(v).includes(arg), got)
    case '<':
      return result(typeof v === 'number' && v < millis(arg), got)
    case '>':
      return result(typeof v === 'number' && v > millis(arg), got)
    case 'length': {
      const len = Array.isArray(v) ? v.length : typeof v === 'string' ? v.length : v && typeof v === 'object' ? Object.keys(v).length : null
      return result(len === Number(arg), `length is ${len ?? 'not countable'}`)
    }
    default:
      return result(false, `"${line}" is not a check Crane knows. Try: status 200, time < 500ms, json $.id exists.`)
  }
}
