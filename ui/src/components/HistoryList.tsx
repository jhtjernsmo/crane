import { formatMs } from '../format'
import type { History, Sent } from '../useHistory'

function outcome(s: Sent): string {
  if (s.res) return `${s.res.status} · ${formatMs(s.res.timing.totalMs)}`
  if (s.summary?.status) return `${s.summary.status}${s.summary.totalMs !== null ? ` · ${formatMs(s.summary.totalMs)}` : ''}`
  return 'failed'
}

function when(at: Date): string {
  const today = new Date().toDateString() === at.toDateString()
  return today ? at.toTimeString().slice(0, 5) : at.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
}

export function HistoryList({ history, onReopen, onClear }: { history: History; onReopen: (s: Sent) => void; onClear: () => void }) {
  if (history.items.length === 0) {
    return <p className="quiet small pad">What you send shows up here. Answers are kept until you close Crane; only a summary is saved.</p>
  }
  return (
    <>
      <ul className="history">
        {history.items.map((s) => (
          <li key={s.id}>
            <button className={history.current?.id === s.id ? 'sel' : ''} onClick={() => onReopen(s)} title={s.draft.url}>
              <span className={`m m-${s.draft.method}`}>{s.draft.method}</span>
              <span className="u">{s.draft.url.replace(/^https?:\/\//, '')}</span>
              <span className="meta">
                {when(s.at)}{s.envName && ` · ${s.envName}`} · {outcome(s)}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {history.problem && <p className="problem small side-problem" role="alert">{history.problem}</p>}
      <button type="button" className="link quiet-link pad clear-history" onClick={onClear}>Clear history</button>
    </>
  )
}
