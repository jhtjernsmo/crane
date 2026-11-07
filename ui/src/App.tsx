import { useCallback, useEffect, useRef, useState } from 'react'
import { type Response, sendRequest } from './api'
import { Logo } from './components/Logo'
import { RequestBar } from './components/RequestBar'
import { ResponsePane } from './components/ResponsePane'
import { StartHint } from './components/StartHint'
import { formatMs } from './format'
import { THEMES, type Theme, applyTheme, loadTheme } from './theme'

/** One send this session. Saved collections come in a later step. */
interface Sent {
  id: number
  at: Date
  method: string
  url: string
  res?: Response
  error?: string
}

const HISTORY_LIMIT = 50

export default function App() {
  const [method, setMethod] = useState('GET')
  const [url, setUrl] = useState('')
  const [sending, setSending] = useState(false)
  const [current, setCurrent] = useState<Sent | null>(null)
  const [history, setHistory] = useState<Sent[]>([])
  const [theme, setTheme] = useState<Theme>(loadTheme)
  const nextId = useRef(1)

  useEffect(() => applyTheme(theme), [theme])

  const send = useCallback(async () => {
    if (sending) return
    setSending(true)
    const base = { id: nextId.current++, at: new Date(), method, url: url.trim() }
    let sent: Sent
    try {
      sent = { ...base, res: await sendRequest({ method, url, headers: [], body: null }) }
    } catch (e) {
      sent = { ...base, error: e instanceof Error ? e.message : String(e) }
    }
    setCurrent(sent)
    setHistory((h) => [sent, ...h].slice(0, HISTORY_LIMIT))
    setSending(false)
  }, [method, url, sending])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault()
        void send()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [send])

  const reopen = (s: Sent) => {
    setMethod(s.method)
    setUrl(s.url)
    setCurrent(s)
  }

  return (
    <div className="app">
      <header>
        <div className="brand"><Logo />CRANE</div>
        <div className="seg push" role="radiogroup" aria-label="Theme">
          {THEMES.map((t) => (
            <button key={t} role="radio" aria-checked={theme === t} onClick={() => setTheme(t)}>{t}</button>
          ))}
        </div>
      </header>

      <aside>
        <h4 className="lbl">History</h4>
        {history.length === 0 && <p className="quiet small pad">What you send shows up here.</p>}
        <ul className="history">
          {history.map((s) => (
            <li key={s.id}>
              <button className={current?.id === s.id ? 'sel' : ''} onClick={() => reopen(s)} title={s.url}>
                <span className={`m m-${s.method}`}>{s.method}</span>
                <span className="u">{s.url.replace(/^https?:\/\//, '')}</span>
                <span className="meta">
                  {s.at.toTimeString().slice(0, 5)} · {s.res ? `${s.res.status} · ${formatMs(s.res.timing.totalMs)}` : 'failed'}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <main>
        <section>
          <div className="lbl"><span className="num">01</span>Request</div>
          <RequestBar method={method} url={url} sending={sending} onMethod={setMethod} onUrl={setUrl} onSend={() => void send()} />
          {history.length === 0 ? (
            <StartHint onPick={(m, u) => { setMethod(m); setUrl(u) }} />
          ) : (
            <p className="quiet small">Headers, body and saved requests come in the next step.</p>
          )}
        </section>
        <section>
          <div className="lbl"><span className="num">02</span>Response</div>
          {sending && <p className="quiet">Waiting for an answer…</p>}
          {!sending && current?.error && (
            <div className="problem" role="alert">
              <b>Not sent.</b> {current.error}
            </div>
          )}
          {!sending && current?.res && <ResponsePane key={current.id} res={current.res} />}
          {!sending && !current && <p className="quiet">The answer shows up here.</p>}
        </section>
      </main>
    </div>
  )
}
