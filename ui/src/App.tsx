import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { type Response, sendRequest } from './api'
import { type Ask, AskDialog } from './components/AskDialog'
import { CollectionTree } from './components/CollectionTree'
import { ConfirmSend } from './components/ConfirmSend'
import { EnvEditor } from './components/EnvEditor'
import { EnvPicker } from './components/EnvPicker'
import { FileBar } from './components/FileBar'
import { Logo } from './components/Logo'
import { RequestBar } from './components/RequestBar'
import { RequestEditor } from './components/RequestEditor'
import { ResponsePane } from './components/ResponsePane'
import { SecretReview } from './components/SecretReview'
import { StartHint } from './components/StartHint'
import { VarLine } from './components/VarLine'
import { formatMs } from './format'
import { type Draft, buildSpec, emptyDraft, forHistory, reopenDraft } from './request'
import { THEMES, type Theme, applyTheme, loadTheme } from './theme'
import { useCollection } from './useCollection'
import { useEnvironments } from './useEnvironments'
import { findVars } from './vars'

/** One send this session, with secret values already removed. Saved collections come in a later step. */
interface Sent {
  id: number
  at: Date
  draft: Draft
  /** The environment it went to, if any. */
  envName: string | null
  res?: Response
  error?: string
}

const HISTORY_LIMIT = 50
/** Methods that change data, so a production environment asks first. */
const WRITE_METHODS = ['POST', 'PUT', 'PATCH', 'DELETE']

export default function App() {
  const [draft, setDraft] = useState<Draft>(emptyDraft)
  const [sending, setSending] = useState(false)
  const [current, setCurrent] = useState<Sent | null>(null)
  const [history, setHistory] = useState<Sent[]>([])
  const [theme, setTheme] = useState<Theme>(loadTheme)
  const nextId = useRef(1)
  const envs = useEnvironments()
  const [editingEnvs, setEditingEnvs] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const col = useCollection(draft, setDraft, envs)
  // Until a tab is picked: the requests when a folder is open, else the history.
  const [pickedSide, setSide] = useState<'requests' | 'history' | null>(null)
  const side = pickedSide ?? (col.view ? 'requests' : 'history')
  const [naming, setNaming] = useState(false)
  const [ask, setAsk] = useState<Ask | null>(null)
  const closeAsk = useCallback(() => setAsk(null), [])
  const closeEnvs = useCallback(() => setEditingEnvs(false), [])
  const cancelConfirm = useCallback(() => setConfirming(false), [])
  const spec = useMemo(() => buildSpec(draft), [draft])
  const varNames = useMemo(() => findVars(spec.url, spec.body, ...spec.headers.flatMap((h) => [h.name, h.value])), [spec])

  useEffect(() => applyTheme(theme), [theme])

  const send = useCallback(async (confirmed = false) => {
    if (sending) return
    const env = envs.active
    if (env?.production && !confirmed && WRITE_METHODS.includes(spec.method)) {
      setConfirming(true)
      return
    }
    setConfirming(false)
    setSending(true)
    const base = { id: nextId.current++, at: new Date(), draft: forHistory({ ...draft, url: draft.url.trim() }), envName: env?.name ?? null }
    let sent: Sent
    try {
      sent = { ...base, res: await sendRequest(spec, env?.id ?? null, confirmed) }
    } catch (e) {
      sent = { ...base, error: e instanceof Error ? e.message : String(e) }
    }
    setCurrent(sent)
    setHistory((h) => [sent, ...h].slice(0, HISTORY_LIMIT))
    setSending(false)
  }, [draft, spec, sending, envs.active])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const busy = editingEnvs || confirming || ask !== null || col.review !== null
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && !busy) {
        e.preventDefault()
        void send()
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 's' && !busy && col.view) {
        e.preventDefault()
        void col.save().then((done) => !done && setNaming(true))
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [send, editingEnvs, confirming, ask, col])

  /** Opens a file, asking first if that would throw away unsaved changes. */
  const openFile = (path: string) => {
    if (col.open?.path === path) return
    if (col.dirty && col.open) {
      setAsk({ title: 'Discard your changes?', text: `${col.open.saved.name} has changes that are not saved.`, yes: 'Discard', danger: true, onYes: () => void col.load(path) })
      return
    }
    void col.load(path)
  }

  const askDelete = (path: string, name: string) =>
    setAsk({ title: `Delete ${name}?`, text: `${path} is removed from the folder. If it is in git, you can get it back from there.`, yes: 'Delete', danger: true, onYes: () => void col.remove(path) })

  const reopen = (s: Sent) => {
    setDraft((now) => reopenDraft(s.draft, now))
    setCurrent(s)
  }

  return (
    <div className="app">
      <header>
        <div className="brand"><Logo />CRANE</div>
        <EnvPicker envs={envs} onEdit={() => setEditingEnvs(true)} />
        <div className="seg push" role="radiogroup" aria-label="Theme">
          {THEMES.map((t) => (
            <button key={t} role="radio" aria-checked={theme === t} onClick={() => setTheme(t)}>{t}</button>
          ))}
        </div>
      </header>

      <aside>
        <div className="side-tabs" role="tablist" aria-label="Sidebar">
          <button role="tab" aria-selected={side === 'requests'} onClick={() => setSide('requests')} className="lbl">Requests</button>
          <button role="tab" aria-selected={side === 'history'} onClick={() => setSide('history')} className="lbl">History</button>
        </div>
        {side === 'requests' && <CollectionTree col={col} onOpen={openFile} onDelete={askDelete} />}
        {side === 'requests' && col.problem && !col.review && <p className="problem small side-problem" role="alert">{col.problem}</p>}
        {side === 'history' && history.length === 0 && <p className="quiet small pad">What you send shows up here.</p>}
        {side === 'history' && <ul className="history">
          {history.map((s) => (
            <li key={s.id}>
              <button className={current?.id === s.id ? 'sel' : ''} onClick={() => reopen(s)} title={s.draft.url}>
                <span className={`m m-${s.draft.method}`}>{s.draft.method}</span>
                <span className="u">{s.draft.url.replace(/^https?:\/\//, '')}</span>
                <span className="meta">
                  {s.at.toTimeString().slice(0, 5)}{s.envName && ` · ${s.envName}`} · {s.res ? `${s.res.status} · ${formatMs(s.res.timing.totalMs)}` : 'failed'}
                </span>
              </button>
            </li>
          ))}
        </ul>}
      </aside>

      <main>
        <section>
          <div className="lbl"><span className="num">01</span>Request</div>
          <FileBar col={col} naming={naming} onNaming={setNaming} />
          <RequestBar
            method={draft.method}
            url={draft.url}
            sending={sending}
            onMethod={(method) => setDraft((d) => ({ ...d, method }))}
            onUrl={(url) => setDraft((d) => ({ ...d, url }))}
            onSend={() => void send()}
          />
          <VarLine names={varNames} envs={envs} onEdit={() => setEditingEnvs(true)} />
          {envs.problem && <p className="problem small" role="alert">{envs.problem}</p>}
          <RequestEditor draft={draft} onChange={setDraft} />
          {history.length === 0 && <StartHint onPick={(method, url) => setDraft((d) => ({ ...d, method, url }))} />}
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
      {ask && <AskDialog ask={ask} onClose={closeAsk} />}
      {col.review && <SecretReview key={col.review.path + col.review.findings.length} col={col} envs={envs} />}
      {editingEnvs && <EnvEditor envs={envs} onClose={closeEnvs} />}
      {confirming && envs.active && (
        <ConfirmSend env={envs.active.name} method={spec.method} url={spec.url} onSend={() => void send(true)} onCancel={cancelConfirm} />
      )}
    </div>
  )
}
