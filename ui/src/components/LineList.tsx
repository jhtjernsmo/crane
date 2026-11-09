import { useState } from 'react'

/** One text line per row, with an empty row at the end for the next one. */
export function LineList({ label, lines, onChange, placeholder }: { label: string; lines: string[]; onChange: (lines: string[]) => void; placeholder: string }) {
  // Stable keys, so typing in a row keeps its focus.
  const [keys, setKeys] = useState(() => lines.map((_, i) => i))
  const [next, setNext] = useState(lines.length)
  const ids = keys.length === lines.length ? keys : lines.map((_, i) => i)
  const set = (i: number, value: string) => {
    if (i === lines.length) {
      setKeys([...ids, next])
      setNext(next + 1)
      onChange([...lines, value])
      return
    }
    onChange(lines.map((l, j) => (j === i ? value : l)))
  }
  const remove = (i: number) => {
    setKeys(ids.filter((_, j) => j !== i))
    onChange(lines.filter((_, j) => j !== i))
  }
  return (
    <ul className="lines" aria-label={label}>
      {[...lines, ''].map((line, i) => (
        <li key={i < lines.length ? ids[i] : `new-${next}`}>
          <input value={line} onChange={(e) => set(i, e.target.value)} placeholder={i === lines.length ? placeholder : ''} aria-label={`${label} ${i + 1}`} spellCheck={false} />
          {i < lines.length && (
            <button type="button" onClick={() => remove(i)} aria-label={`Remove ${label.toLowerCase()} ${i + 1}`} title="Remove">
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.5" /></svg>
            </button>
          )}
        </li>
      ))}
    </ul>
  )
}
