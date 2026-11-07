// Finds where JSON goes wrong and says it in plain words. JSON.parse only says
// *that* it failed, and each browser engine words it differently.

export interface JsonProblem {
  message: string
  line: number
  column: number
}

class Stop extends Error {
  readonly at: number
  constructor(at: number, message: string) {
    super(message)
    this.at = at
  }
}

const NUMBER = /-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/y
const STRING = /"(?:[^"\\\u0000-\u001f]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"/y
const LITERAL = /true|false|null/y

function scan(text: string): void {
  let i = 0
  const skip = () => {
    while (i < text.length && ' \t\n\r'.includes(text[i])) i++
  }
  const what = () => (i >= text.length ? 'the text ended' : `found ${JSON.stringify(text[i])}`)
  const match = (re: RegExp) => {
    re.lastIndex = i
    const m = re.exec(text)
    if (m) i += m[0].length
    return m !== null
  }
  const expect = (ch: string, wanted: string) => {
    skip()
    if (text[i] !== ch) throw new Stop(i, `Expected ${wanted}, but ${what()}.`)
    i++
  }
  const list = (close: string, item: () => void) => {
    i++
    skip()
    if (text[i] === close) return void i++
    for (;;) {
      item()
      skip()
      if (text[i] === close) return void i++
      if (text[i] !== ',') throw new Stop(i, `Expected a comma or ${JSON.stringify(close)}, but ${what()}.`)
      const comma = i++
      skip()
      if (text[i] === close) throw new Stop(comma, 'A comma is not allowed before the closing bracket.')
    }
  }
  const value = (): void => {
    skip()
    const c = text[i]
    if (c === '{') {
      return list('}', () => {
        skip()
        if (text[i] !== '"' || !match(STRING)) throw new Stop(i, `Expected a name in double quotes, but ${what()}.`)
        expect(':', 'a colon after the name')
        value()
      })
    }
    if (c === '[') return list(']', value)
    if (c === '"') {
      if (!match(STRING)) throw new Stop(i, 'This text is not closed, or has a character JSON does not allow.')
      return
    }
    if (match(NUMBER) || match(LITERAL)) return
    if (c === "'") throw new Stop(i, 'JSON needs double quotes, not single quotes.')
    throw new Stop(i, `Expected a value, but ${what()}.`)
  }

  value()
  skip()
  if (i < text.length) throw new Stop(i, `Expected the end, but ${what()}.`)
}

/** `null` when the text is empty or valid JSON. */
export function checkJson(text: string): JsonProblem | null {
  if (!text.trim()) return null
  try {
    scan(text)
    return null
  } catch (e) {
    if (!(e instanceof Stop)) throw e
    const before = text.slice(0, e.at).split('\n')
    return { message: e.message, line: before.length, column: before[before.length - 1].length + 1 }
  }
}
