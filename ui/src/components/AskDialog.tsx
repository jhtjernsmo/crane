import { useEffect, useRef } from 'react'

export interface Ask {
  title: string
  text: string
  yes: string
  danger?: boolean
  onYes: () => void
}

/** A yes/no question. The safe answer has focus, so a stray Enter does nothing harmful. */
export function AskDialog({ ask, onClose }: { ask: Ask; onClose: () => void }) {
  const no = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    no.current?.focus()
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])
  return (
    <div className="scrim">
      <div className={`dialog confirm${ask.danger ? '' : ' calm'}`} role="alertdialog" aria-modal="true" aria-labelledby="ask-title" aria-describedby="ask-text">
        <h2 id="ask-title">{ask.title}</h2>
        <p id="ask-text">{ask.text}</p>
        <div className="dialog-foot">
          <button type="button" className="btn-quiet" onClick={onClose} ref={no}>Cancel</button>
          <button type="button" className={`btn-main${ask.danger ? ' danger' : ''}`} onClick={() => { ask.onYes(); onClose() }}>{ask.yes}</button>
        </div>
      </div>
    </div>
  )
}
