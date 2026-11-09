import { useState } from 'react'
import type { CheckResult, Response } from '../api'
import { formatBytes, formatMs, headerValue, statusTone } from '../format'
import { BodyView } from './BodyView'

const PHASES = [
  { key: 'dnsMs', label: 'lookup', cls: 'dns' },
  { key: 'connectMs', label: 'connect + TLS', cls: 'conn' },
  { key: 'waitingMs', label: 'waiting for the server', cls: 'wait' },
  { key: 'downloadMs', label: 'download', cls: 'down' },
] as const

/** Where the time went. A fresh connection is opened for every send, so lookup and connect are always measured. */
function TimingRuler(timing: Response['timing']) {
  const total = Math.max(timing.totalMs, 1)
  const shown = PHASES.filter((p) => p.key === 'waitingMs' || p.key === 'downloadMs' || timing[p.key] > 0)
  return (
    <div className="ruler">
      <div className="lbl">Timing</div>
      <div className="bar" aria-hidden="true">
        {shown.map((p) => (
          <b key={p.key} className={p.cls} style={{ width: `${(timing[p.key] / total) * 100}%` }} />
        ))}
      </div>
      <ul className="ticks" aria-label="Timing">
        {shown.map((p) => (
          <li key={p.key}>
            <i className={p.cls} aria-hidden="true" />
            {p.label} {formatMs(timing[p.key])}
          </li>
        ))}
      </ul>
    </div>
  )
}

export function Checks({ checks }: { checks: CheckResult[] }) {
  const failed = checks.filter((c) => !c.passed).length
  return (
    <div className={`checks ${failed ? 'has-failed' : ''}`}>
      <div className="lbl">Checks · {checks.length - failed} of {checks.length} passed</div>
      <ul aria-label="Checks">
        {checks.map((c, i) => (
          <li key={i} className={c.passed ? 'pass' : 'fail'}>
            <span className="mark" aria-hidden="true">{c.passed ? 'ok' : 'no'}</span>
            <span className="mono">{c.check}</span>
            {c.detail && <span className="small quiet">{c.detail}</span>}
          </li>
        ))}
      </ul>
    </div>
  )
}

export function ResponsePane({ res }: { res: Response }) {
  const [tab, setTab] = useState<'body' | 'headers'>('body')
  const contentType = headerValue(res.headers, 'content-type') ?? ''
  return (
    <>
      <div className="stat">
        <span className={`code-num tone-${statusTone(res.status)}`}>{res.status}</span>
        <span>{res.statusText}</span>
        <span className="mono push">{formatMs(res.timing.totalMs)}</span>
        <span className="mono">{formatBytes(res.size)}</span>
      </div>
      {res.checks && res.checks.length > 0 && <Checks checks={res.checks} />}
      <TimingRuler {...res.timing} />
      <div className="tabs" role="tablist" aria-label="Response parts">
        <button role="tab" aria-selected={tab === 'body'} onClick={() => setTab('body')}>Body</button>
        <button role="tab" aria-selected={tab === 'headers'} onClick={() => setTab('headers')}>
          Headers <sup>{res.headers.length}</sup>
        </button>
        <span className="quiet mono push small" title={res.url}>{contentType.split(';')[0]}</span>
      </div>
      {tab === 'body' ? (
        res.body === null ? (
          <p className="quiet">This is not text ({contentType || 'unknown type'}, {formatBytes(res.size)}). Saving it to a file comes later.</p>
        ) : (
          <>
            {res.truncated && <p className="help">This body is big, so only the first part is shown. The size above is the whole body.</p>}
            <BodyView body={res.body} contentType={contentType} />
          </>
        )
      ) : (
        <table className="headers">
          <tbody>
            {res.headers.map((h, i) => (
              <tr key={i}>
                <th>{h.name}</th>
                <td>{h.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  )
}
