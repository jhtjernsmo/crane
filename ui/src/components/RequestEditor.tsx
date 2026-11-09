import { useState } from 'react'
import { BODY_MODES, type BodyMode, type Draft, type Row, autoContentType, checkJson, isSecretName, parseQuery, row, withQuery } from '../request'
import { findVars } from '../vars'
import { KeyValueTable } from './KeyValueTable'
import { LineList } from './LineList'

type Tab = 'params' | 'headers' | 'body' | 'checks'

const MODE_LABELS: Record<BodyMode, string> = { none: 'None', json: 'JSON', text: 'Text', form: 'Form' }
const NO_BODY_METHODS = new Set(['GET', 'HEAD'])
const COMMON_HEADERS = [
  'Accept', 'Accept-Language', 'Authorization', 'Cache-Control', 'Content-Type', 'Cookie',
  'If-None-Match', 'Origin', 'Referer', 'User-Agent', 'X-Api-Key', 'X-Request-Id',
]

const rowsFromUrl = (url: string) => parseQuery(url).map((p) => row(p.name, p.value))

/** The query table and the address stay in step: editing one rewrites the other. */
function ParamsEditor({ url, onUrl }: { url: string; onUrl: (u: string) => void }) {
  const [rows, setRows] = useState(() => rowsFromUrl(url))
  const [seenUrl, setSeenUrl] = useState(url)
  if (url !== seenUrl) {
    setSeenUrl(url)
    setRows(rowsFromUrl(url))
  }
  const change = (next: Row[]) => {
    const kept = next.filter((r) => r.name !== '' || r.value !== '')
    const nextUrl = withQuery(url, kept)
    setRows(kept)
    setSeenUrl(nextUrl)
    onUrl(nextUrl)
  }
  return <KeyValueTable label="Query" rows={rows} onChange={change} toggles={false} />
}

function BodyEditor({ draft, onChange }: { draft: Draft; onChange: (d: Draft) => void }) {
  const problem = draft.bodyMode === 'json' ? checkJson(draft.body) : null
  const format = () => onChange({ ...draft, body: JSON.stringify(JSON.parse(draft.body), null, 2) })
  return (
    <>
      <div className="row-gap">
        <div className="seg" role="radiogroup" aria-label="Body type">
          {BODY_MODES.map((m) => (
            <button key={m} type="button" role="radio" aria-checked={draft.bodyMode === m} onClick={() => onChange({ ...draft, bodyMode: m })}>
              {MODE_LABELS[m]}
            </button>
          ))}
        </div>
        {draft.bodyMode === 'json' && draft.body.trim() && !problem && (
          <button type="button" className="link push" onClick={format}>Tidy up</button>
        )}
      </div>
      {draft.bodyMode !== 'none' && NO_BODY_METHODS.has(draft.method) && (
        <p className="help small">Many servers ignore a body on {draft.method}. Pick POST, PUT or PATCH to be sure it is read.</p>
      )}
      {(draft.bodyMode === 'json' || draft.bodyMode === 'text') && (
        <textarea
          className="code-edit"
          value={draft.body}
          onChange={(e) => onChange({ ...draft, body: e.target.value })}
          placeholder={draft.bodyMode === 'json' ? '{\n  "name": "Ada"\n}' : 'Plain text'}
          aria-label="Body"
          spellCheck={false}
          rows={10}
        />
      )}
      {problem && (
        <p className="problem small" role="status">
          <b>Line {problem.line}, column {problem.column}:</b> {problem.message} It will still be sent as written.
        </p>
      )}
      {draft.bodyMode === 'form' && <KeyValueTable label="Form" rows={draft.form} onChange={(form) => onChange({ ...draft, form })} />}
      {draft.bodyMode === 'none' && <p className="quiet small">No body is sent.</p>}
    </>
  )
}

export function RequestEditor({ draft, onChange }: { draft: Draft; onChange: (d: Draft) => void }) {
  const [tab, setTab] = useState<Tab>('params')
  const paramCount = parseQuery(draft.url).length
  const headerCount = draft.headers.filter((h) => h.on && h.name.trim()).length
  const auto = autoContentType(draft)

  const tabButton = (t: Tab, label: string, extra: string | number) => (
    <button type="button" role="tab" aria-selected={tab === t} onClick={() => setTab(t)}>
      {label} {extra !== 0 && extra !== '' && <sup>{extra}</sup>}
    </button>
  )

  return (
    <>
      <div className="tabs" role="tablist" aria-label="Request parts">
        {tabButton('params', 'Query', paramCount)}
        {tabButton('headers', 'Headers', headerCount + (auto ? 1 : 0))}
        {tabButton('body', 'Body', draft.bodyMode === 'none' ? '' : MODE_LABELS[draft.bodyMode].toLowerCase())}
        {tabButton('checks', 'Checks', draft.expect.length)}
      </div>
      {tab === 'params' && <ParamsEditor url={draft.url} onUrl={(url) => onChange({ ...draft, url })} />}
      {tab === 'headers' && (
        <>
          <KeyValueTable label="Headers" rows={draft.headers} onChange={(headers) => onChange({ ...draft, headers })} suggest="common-headers" />
          <datalist id="common-headers">
            {COMMON_HEADERS.map((h) => <option key={h} value={h} />)}
          </datalist>
          {auto && (
            <p className="quiet small">
              <span className="mono">Content-Type: {auto}</span> is added for the body. Add your own Content-Type to change it.
            </p>
          )}
          {draft.headers.some((h) => h.on && isSecretName(h.name) && h.value.trim() && findVars(h.value).length === 0) && (
            <p className="help small">
              Tip: put this value in an environment as a secret and write <span className="mono">{'Bearer {{token}}'}</span> here. Then it stays in the system keyring and
              is hidden if the server sends it back.
            </p>
          )}
          <p className="quiet small">Values of Authorization, Cookie and names with token, key or secret are hidden and never kept in history.</p>
        </>
      )}
      {tab === 'body' && <BodyEditor draft={draft} onChange={onChange} />}
      {tab === 'checks' && (
        <>
          <LineList label="Check" lines={draft.expect} onChange={(expect) => onChange({ ...draft, expect })} placeholder="status 200" />
          <p className="quiet small">
            One check per line: <span className="mono">status 2xx</span>, <span className="mono">time &lt; 500ms</span>, <span className="mono">header content-type contains json</span>,{' '}
            <span className="mono">body contains ok</span>, <span className="mono">json $.items length 3</span>, <span className="mono">{'json $.id = {{user_id}}'}</span>.
          </p>
          <h4 className="lbl">Keep for later requests</h4>
          <LineList label="Save" lines={draft.save} onChange={(save) => onChange({ ...draft, save })} placeholder="token = json $.access_token" />
          <p className="quiet small">Run all uses these: a value saved here can be used as a variable by the requests after it. Saved values count as secrets.</p>
        </>
      )}
    </>
  )
}
