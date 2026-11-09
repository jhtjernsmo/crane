// `{{name}}` variables, the same rules as crates/crane-core/src/vars.rs.
// The UI only needs to find them; Rust fills them in.

const VAR = /\{\{\s*([A-Za-z_][A-Za-z0-9_.-]*)\s*\}\}/g

export const isVarName = (name: string) => /^[A-Za-z_][A-Za-z0-9_.-]*$/.test(name)

/** Names used in the texts, in order, without repeats. */
export function findVars(...texts: (string | null | undefined)[]): string[] {
  const names: string[] = []
  for (const t of texts) {
    for (const m of (t ?? '').matchAll(VAR)) if (!names.includes(m[1])) names.push(m[1])
  }
  return names
}

export function fillVars(text: string, lookup: (name: string) => string | undefined): { text: string; missing: string[] } {
  const missing: string[] = []
  const filled = text.replace(VAR, (whole, name: string) => {
    const v = lookup(name)
    if (v !== undefined) return v
    if (!missing.includes(name)) missing.push(name)
    return whole
  })
  return { text: filled, missing }
}

/** `{{a}}`, `{{a}} or {{b}}`, `{{a}}, {{b}} or {{c}}`, like the Rust messages. */
export function listVars(names: string[]): string {
  const wrapped = names.map((n) => `{{${n}}}`)
  return wrapped.length < 2 ? (wrapped[0] ?? '') : `${wrapped.slice(0, -1).join(', ')} or ${wrapped[wrapped.length - 1]}`
}
