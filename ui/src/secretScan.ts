// Spots values that look like keys or tokens, so Crane can warn before one
// lands in a plain variable or a saved file. A warning, never a block: some
// look-alikes are harmless, and the user knows best.

const KNOWN = [
  /\b(sk|pk|rk)_(live|test)_[A-Za-z0-9]{10,}/, // Stripe
  /\bgh[pousr]_[A-Za-z0-9]{30,}/, // GitHub
  /\bgithub_pat_[A-Za-z0-9_]{30,}/,
  /\bxox[abprs]-[A-Za-z0-9-]{10,}/, // Slack
  /\bAKIA[0-9A-Z]{16}\b/, // AWS access key id
  /\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]+/, // JWT
  /\bAIza[0-9A-Za-z_-]{35}\b/, // Google API key
  /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
]

const MIN_RANDOM_LEN = 20
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/** A long word that mixes letters and digits, without spaces or URL parts. */
function looksRandom(word: string): boolean {
  if (word.length < MIN_RANDOM_LEN || UUID.test(word) || /[/:@.{}]/.test(word)) return false
  const kinds = [/[a-z]/, /[A-Z]/, /[0-9]/].filter((re) => re.test(word)).length
  return kinds >= 2 && /[0-9]/.test(word) && /^[A-Za-z0-9_+=-]+$/.test(word)
}

export function looksLikeSecret(value: string): boolean {
  const text = value.trim()
  if (!text) return false
  if (KNOWN.some((re) => re.test(text))) return true
  const withoutScheme = text.replace(/^(Bearer|Basic|Token)\s+/i, '')
  return withoutScheme.split(/\s+/).some(looksRandom)
}
