import { useState } from 'react'
import type { Response } from '../api'
import { formatBytes, formatMs, headerValue, statusTone } from '../format'
import { BodyView } from './BodyView'

function TimingRuler({ waitingMs, downloadMs, totalMs }: Response['timing']) {
  const total = Math.max(totalMs, 1)
  return (
    <div className="ruler">
      <div className="lbl">Timing</div>
      <div className="bar">
        <b className="wait" style={{ width: `${(waitingMs / total) * 100}%` }} />
        <b className="down" style={{ width: `${(downloadMs / total) * 100}%` }} />
      </div>
      <div className="ticks">
        <span>waiting for the server {formatMs(waitingMs)}</span>
        <span>download {formatMs(downloadMs)}</span>
      </div>
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
      <TimingRuler {...res.timing} />
      <div className="tabs" role="tablist">
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
