import { useEffect, useRef } from 'react'

/** The yes Crane asks for before changing data in a production environment. */
export function ConfirmSend({ env, method, url, onSend, onCancel }: {
  env: string
  method: string
  url: string
  onSend: () => void
  onCancel: () => void
}) {
  const cancel = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    // The safe choice has focus, so a stray Enter does not send.
    cancel.current?.focus()
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel()
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) e.stopImmediatePropagation()
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onCancel])
  return (
    <div className="scrim">
      <div className="dialog confirm" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" aria-describedby="confirm-text">
        <h2 id="confirm-title">Send {method} to {env}?</h2>
        <p id="confirm-text">{env} is marked as production, and this request can change real data.</p>
        <p className="mono small target"><span className={`m m-${method}`}>{method}</span> {url}</p>
        <div className="dialog-foot">
          <button type="button" className="btn-quiet" onClick={onCancel} ref={cancel}>Cancel</button>
          <button type="button" className="btn-main danger" onClick={onSend}>Send to {env}</button>
        </div>
      </div>
    </div>
  )
}
