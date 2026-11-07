import { useState } from 'react'
import { type Row, isSecretName, row } from '../request'

interface Props {
  label: string
  rows: Row[]
  onChange: (rows: Row[]) => void
  /** Show a checkbox per row to keep it without sending it. */
  toggles?: boolean
  /** id of a <datalist> with name suggestions. */
  suggest?: string
}

/** Name/value lines with one blank line at the end that becomes a row when typed in. */
export function KeyValueTable({ label, rows, onChange, toggles = true, suggest }: Props) {
  const [blank, setBlank] = useState(() => row())
  const [shown, setShown] = useState<ReadonlySet<number>>(new Set())

  const update = (id: number, patch: Partial<Row>) => {
    if (id === blank.id) {
      onChange([...rows, { ...blank, ...patch }])
      setBlank(row())
      return
    }
    onChange(rows.map((r) => (r.id === id ? { ...r, ...patch } : r)))
  }
  const remove = (id: number) => onChange(rows.filter((r) => r.id !== id))
  const toggleShown = (id: number) =>
    setShown((s) => {
      const next = new Set(s)
      if (!next.delete(id)) next.add(id)
      return next
    })

  return (
    <table className="kv" aria-label={label}>
      <tbody>
        {[...rows, blank].map((r, i) => {
          const isBlank = r.id === blank.id
          const secret = isSecretName(r.name)
          const hidden = secret && !shown.has(r.id)
          return (
            <tr key={r.id} className={r.on ? '' : 'off'}>
              {toggles && (
                <td className="tick">
                  {!isBlank && (
                    <input type="checkbox" checked={r.on} onChange={(e) => update(r.id, { on: e.target.checked })} aria-label={`Send ${r.name || 'this row'}`} />
                  )}
                </td>
              )}
              <td>
                <input
                  value={r.name}
                  onChange={(e) => update(r.id, { name: e.target.value })}
                  placeholder={isBlank ? 'name' : ''}
                  aria-label={`${label} name ${i + 1}`}
                  list={suggest}
                  spellCheck={false}
                />
              </td>
              <td className="val">
                <input
                  type={hidden ? 'password' : 'text'}
                  value={r.value}
                  onChange={(e) => update(r.id, { value: e.target.value })}
                  placeholder={isBlank ? 'value' : ''}
                  aria-label={`${label} value ${i + 1}`}
                  autoComplete="off"
                  spellCheck={false}
                />
                {secret && !isBlank && (
                  <button type="button" className="eye" onClick={() => toggleShown(r.id)} title={hidden ? 'Show the value' : 'Hide the value'}>
                    {hidden ? 'show' : 'hide'}
                  </button>
                )}
              </td>
              <td className="del">
                {!isBlank && (
                  <button type="button" onClick={() => remove(r.id)} aria-label={`Remove ${r.name || 'row'}`} title="Remove">
                    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.5" /></svg>
                  </button>
                )}
              </td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
