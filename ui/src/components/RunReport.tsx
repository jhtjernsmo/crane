import type { Report } from '../api'
import { formatMs } from '../format'

/** The outcome of Run all: one line per request, failed checks under it. */
export function RunReport({ report, onOpen, onClose }: { report: Report; onOpen: (path: string) => void; onClose: () => void }) {
  const failed = report.outcomes.filter((o) => !o.passed).length
  return (
    <div className="run-report" aria-label="Run report">
      <div className="row-gap">
        <span className={`code-num ${failed ? 'tone-server' : 'tone-ok'}`}>{report.outcomes.length - failed}/{report.outcomes.length}</span>
        <span>{failed ? `${failed} failed` : 'all passed'}</span>
        <span className="mono push small quiet">{formatMs(report.elapsedMs)}</span>
        <button type="button" className="link" onClick={onClose}>Close</button>
      </div>
      <ul>
        {report.outcomes.map((o) => (
          <li key={o.path} className={o.passed ? 'pass' : 'fail'}>
            <button type="button" onClick={() => onOpen(o.path)} title={`Open ${o.path}`}>
              <span className="mark" aria-hidden="true">{o.passed ? 'ok' : 'no'}</span>
              <span className={`m m-${o.method}`}>{o.method}</span>
              <span className="mono">{o.path.replace(/\.http$/, '')}</span>
              <span className="mono small quiet push">{o.status ?? '---'}{o.totalMs !== null && ` · ${formatMs(o.totalMs)}`}</span>
            </button>
            {o.error && <p className="small bad-text">{o.error}</p>}
            {o.checks.filter((c) => !c.passed).map((c, i) => (
              <p key={i} className="small"><span className="mono">{c.check}</span>{c.detail && <span className="quiet">: {c.detail}</span>}</p>
            ))}
          </li>
        ))}
      </ul>
      <p className="small quiet">The same run in CI: <span className="mono">crane-cli run &lt;folder&gt; --env &lt;name&gt;</span></p>
    </div>
  )
}
