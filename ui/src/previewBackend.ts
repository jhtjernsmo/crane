// Stand-in for the Rust side when the UI runs in a plain browser (npm run dev)
// or in tests. Everything lives in memory and is gone on reload; the real app
// keeps plain values in a file and secrets in the system keyring.
import type { EnvFile, EnvList, RequestSpec, Response } from './api'
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
