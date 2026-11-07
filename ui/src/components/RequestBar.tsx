import { METHODS } from '../api'

interface Props {
  method: string
  url: string
  sending: boolean
  onMethod: (m: string) => void
  onUrl: (u: string) => void
  onSend: () => void
}

export function RequestBar({ method, url, sending, onMethod, onUrl, onSend }: Props) {
  return (
    <form
      className="url"
      onSubmit={(e) => {
        e.preventDefault()
        onSend()
      }}
    >
      <select className={`verb m-${method}`} value={method} onChange={(e) => onMethod(e.target.value)} aria-label="Method">
        {METHODS.map((m) => (
          <option key={m}>{m}</option>
        ))}
      </select>
      <input
        className="path"
        value={url}
        onChange={(e) => onUrl(e.target.value)}
        placeholder="localhost:8080/api or api.github.com/zen"
        aria-label="Address"
        spellCheck={false}
        autoFocus
      />
      <button className="send" disabled={sending}>
        {sending ? 'Sending…' : 'Send'} <kbd>Ctrl ⏎</kbd>
      </button>
    </form>
  )
}
