import { useCallback, useEffect, useMemo, useState } from 'react'
import { sendRequest } from './api'
import { type Ask, AskDialog } from './components/AskDialog'
import { CollectionTree } from './components/CollectionTree'
import { ConfirmSend } from './components/ConfirmSend'
import { ContainersView } from './components/ContainersView'
import { EnvEditor } from './components/EnvEditor'
import { EnvPicker } from './components/EnvPicker'
import { HistoryList } from './components/HistoryList'
import { FileBar } from './components/FileBar'
import { Logo } from './components/Logo'
import { RequestBar } from './components/RequestBar'
import { RequestEditor } from './components/RequestEditor'
import { ResponsePane } from './components/ResponsePane'
import { SecretReview } from './components/SecretReview'
import { StartHint } from './components/StartHint'
import { VarLine } from './components/VarLine'
import { type Draft, buildSpec, emptyDraft, reopenDraft } from './request'
import { THEMES, type Theme, applyTheme, loadTheme } from './theme'
import { useBoxes } from './useBoxes'
import { useCollection } from './useCollection'
import { useEnvironments } from './useEnvironments'
import { type Sent, useHistory } from './useHistory'
import { findVars } from './vars'

/** Methods that change data, so a production environment asks first. */
const WRITE_METHODS = ['POST', 'PUT', 'PATCH', 'DELETE']

export default function App() {
  const [draft, setDraft] = useState<Draft>(emptyDraft)
  const [sending, setSending] = useState(false)
  const history = useHistory()
  const current = history.current
  const [theme, setTheme] = useState<Theme>(loadTheme)
  const envs = useEnvironments()
  const [editingEnvs, setEditingEnvs] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const col = useCollection(draft, setDraft, envs)
  // Until a tab is picked: the requests when a folder is open, else the history.
  const [pickedSide, setSide] = useState<'requests' | 'history' | null>(null)
  const side = pickedSide ?? (col.view ? 'requests' : 'history')
  const [naming, setNaming] = useState(false)
  const [ask, setAsk] = useState<Ask | null>(null)
  const [view, setView] = useState<'requests' | 'containers'>('requests')
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
    try {
      history.record(draft, env?.name ?? null, { res: await sendRequest(spec, env?.id ?? null, confirmed) })
    } catch (e) {
      history.record(draft, env?.name ?? null, { error: e instanceof Error ? e.message : String(e) })
    }
    setSending(false)
  }, [draft, spec, sending, envs.active, history])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const busy = editingEnvs || confirming || ask !== null || col.review !== null || view !== 'requests'
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
  }, [send, editingEnvs, confirming, ask, col, view])

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
    history.setCurrent(s)
  }

  return (
    <div className="app">
      <header>
        <div className="brand"><Logo />CRANE</div>
        <div className="seg views" role="tablist" aria-label="View">
          <button role="tab" aria-selected={view === 'requests'} onClick={() => setView('requests')}>Requests</button>
          <button role="tab" aria-selected={view === 'containers'} onClick={() => setView('containers')}>Containers</button>
        </div>
        <EnvPicker envs={envs} onEdit={() => setEditingEnvs(true)} />
        <div className="seg push" role="radiogroup" aria-label="Theme">
          {THEMES.map((t) => (
            <button key={t} role="radio" aria-checked={theme === t} onClick={() => setTheme(t)}>{t}</button>
          ))}
        </div>
      </header>

      {view === 'containers' ? (
        <ContainersPage
          envId={envs.active?.id ?? null}
          envName={envs.active?.name ?? null}
          onAsk={setAsk}
          onUseAddress={(url) => {
            setDraft((d) => ({ ...d, url }))
            setView('requests')
          }}
        />
      ) : (
        <>
        <aside>
          <div className="side-tabs" role="tablist" aria-label="Sidebar">
            <button role="tab" aria-selected={side === 'requests'} onClick={() => setSide('requests')} className="lbl">Saved</button>
            <button role="tab" aria-selected={side === 'history'} onClick={() => setSide('history')} className="lbl">History</button>
          </div>
          {side === 'requests' && <CollectionTree col={col} onOpen={openFile} onDelete={askDelete} />}
          {side === 'requests' && col.problem && !col.review && <p className="problem small side-problem" role="alert">{col.problem}</p>}
          {side === 'history' && (
            <HistoryList
              history={history}
              onReopen={reopen}
              onClear={() => setAsk({ title: 'Clear the history?', text: 'Every saved send is removed from this computer.', yes: 'Clear', danger: true, onYes: () => void history.clear() })}
            />
          )}
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
            {history.items.length === 0 && <StartHint onPick={(method, url) => setDraft((d) => ({ ...d, method, url }))} />}
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
            {!sending && current && !current.res && !current.error && current.summary && (
              <p className="help">
                This was sent in an earlier session{current.summary.status ? ` and answered ${current.summary.status}` : ''}. Crane keeps only a summary, never the
                answer itself. Send it again to see the answer.
              </p>
            )}
            {!sending && !current && <p className="quiet">The answer shows up here.</p>}
          </section>
        </main>
        </>
      )}
      {ask && <AskDialog ask={ask} onClose={closeAsk} />}
      {col.review && <SecretReview key={col.review.path + col.review.findings.length} col={col} envs={envs} />}
      {editingEnvs && <EnvEditor envs={envs} onClose={closeEnvs} />}
      {confirming && envs.active && (
        <ConfirmSend env={envs.active.name} method={spec.method} url={spec.url} onSend={() => void send(true)} onCancel={cancelConfirm} />
      )}
    </div>
  )
}

/** Mounted only while the containers view is open, so status is only polled then. */
function ContainersPage(props: { onAsk: (a: Ask) => void; onUseAddress: (url: string) => void; envId: string | null; envName: string | null }) {
  const boxes = useBoxes()
  return <ContainersView boxes={boxes} {...props} />
}
