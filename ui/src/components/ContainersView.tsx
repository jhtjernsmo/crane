import { useEffect, useRef, useState } from 'react'
import type { ImageView, PortMap } from '../api'
import { formatBytes } from '../format'
import { type Row, row } from '../request'
import type { Boxes } from '../useBoxes'
import type { Ask } from './AskDialog'
import { KeyValueTable } from './KeyValueTable'

/** Ports below 1024 need special rights on the host, so 80 becomes 8080. */
export const hostPortFor = (p: number) => (p < 1024 ? 8000 + p : p)

/** `postgres:16-alpine` becomes `postgres`, `ghcr.io/org/api:1` becomes `api`. */
export const nameFor = (image: string) =>
  (image.split('/').pop() ?? 'box').split(/[:@]/)[0].toLowerCase().replace(/[^a-z0-9_.-]/g, '-').replace(/^[^a-z0-9]+/, '') || 'box'

function PullForm({ boxes }: { boxes: Boxes }) {
  const [name, setName] = useState('')
  const p = boxes.pulling
  const pct = p?.progress && p.progress.total > 0 ? Math.round((p.progress.done / p.progress.total) * 100) : null
  return (
    <form
      className="pad pull-form"
      onSubmit={(e) => {
        e.preventDefault()
        if (name.trim()) void boxes.pull(name.trim()).then((ok) => ok && setName(''))
      }}
    >
      <input value={name} onChange={(e) => setName(e.target.value)} placeholder="postgres:16-alpine" aria-label="Image to pull" spellCheck={false} disabled={p !== null} />
      <button type="submit" className="btn-quiet" disabled={p !== null || !name.trim()}>Pull</button>
      {p && (
        <div className="pull-progress" role="status">
          <span className="small">{p.progress?.step ?? 'Starting'} {p.name}{pct !== null && ` ${pct}%`}</span>
          <div className="bar"><b className="down" style={{ width: `${pct ?? 5}%` }} /></div>
        </div>
      )}
    </form>
  )
}

function RunForm({ boxes, image, onDone }: { boxes: Boxes; image: ImageView; onDone: () => void }) {
  const [name, setName] = useState(nameFor(image.name))
  const [ports, setPorts] = useState<Row[]>(() => image.exposedPorts.map((p) => row(String(hostPortFor(p)), String(p))))
  const [env, setEnv] = useState<Row[]>([])
  const [args, setArgs] = useState('')
  const portMaps = (): PortMap[] | null => {
    const maps = ports.filter((r) => r.name.trim() || r.value.trim()).map((r) => ({ host: Number(r.name), container: Number(r.value) }))
    return maps.every((m) => Number.isInteger(m.host) && Number.isInteger(m.container) && m.host > 0 && m.host < 65536 && m.container > 0 && m.container < 65536) ? maps : null
  }
  const bad = portMaps() === null
  return (
    <form
      className="run-form"
      aria-label="Run a container"
      onSubmit={(e) => {
        e.preventDefault()
        const maps = portMaps()
        if (!maps) return
        void boxes
          .run({
            name: name.trim(),
            image: image.name,
            ports: maps,
            env: env.filter((r) => r.on && r.name.trim()).map((r) => [r.name.trim(), r.value]),
            args: args.trim() ? args.trim().split(/\s+/) : null,
          })
          .then((ok) => ok && onDone())
      }}
    >
      <div className="row-gap">
        <h3 className="lbl">Run {image.name}</h3>
        <button type="button" className="link push" onClick={onDone}>Cancel</button>
      </div>
      <label className="net-field">
        <span className="lbl">Name</span>
        <input value={name} onChange={(e) => setName(e.target.value)} aria-label="Container name" spellCheck={false} />
      </label>
      <div className="net-field">
        <span className="lbl">Ports: this computer → container</span>
        <KeyValueTable label="Ports" rows={ports} onChange={setPorts} toggles={false} placeholders={['8080', '80']} />
        {bad && <span className="small bad-text">Ports are numbers from 1 to 65535.</span>}
        <span className="small quiet">Opened on 127.0.0.1 only, so nothing else on your network can reach them.</span>
      </div>
      <div className="net-field">
        <span className="lbl">Environment variables</span>
        <KeyValueTable label="Container env" rows={env} onChange={setEnv} />
      </div>
      <label className="net-field">
        <span className="lbl">Command (optional)</span>
        <input value={args} onChange={(e) => setArgs(e.target.value)} placeholder="the image's own" aria-label="Command" spellCheck={false} />
      </label>
      <div className="dialog-foot">
        <button type="submit" className="btn-main" disabled={bad || !name.trim()}>Run</button>
      </div>
    </form>
  )
}

function Logs({ text }: { text: string }) {
  const box = useRef<HTMLPreElement>(null)
  useEffect(() => {
    if (box.current) box.current.scrollTop = box.current.scrollHeight
  }, [text])
  return <pre className="code logs" ref={box} aria-label="Container log">{text || 'Nothing written yet.'}</pre>
}

export function ContainersView({ boxes, onUseAddress, onAsk }: { boxes: Boxes; onUseAddress: (url: string) => void; onAsk: (a: Ask) => void }) {
  const [running, setRunning] = useState<ImageView | null>(null)
  const s = boxes.status
  if (s && !s.available) {
    return (
      <main className="boxes-off">
        <section>
          <div className="lbl"><span className="num">03</span>Containers</div>
          <p className="help">{s.problem ?? 'Containers are not available here.'}</p>
        </section>
      </main>
    )
  }
  const current = s?.containers.find((c) => c.name === boxes.selected) ?? null
  return (
    <>
      <aside>
        <h4 className="lbl side-title">Images</h4>
        <PullForm boxes={boxes} />
        {s && s.images.length === 0 && <p className="quiet small pad">Pull an image to start, like postgres:16-alpine or redis:7.</p>}
        <ul className="history images">
          {s?.images.map((i) => (
            <li key={i.name} className="leaf">
              <button type="button" onClick={() => setRunning(i)} title={`Run ${i.name}`}>
                <span className="u mono">{i.name}</span>
                <span className="meta">{formatBytes(i.size)}{i.exposedPorts.length > 0 && ` · port ${i.exposedPorts.join(', ')}`}</span>
              </button>
              <button
                type="button"
                className="del-file"
                aria-label={`Remove ${i.name}`}
                title="Remove image"
                onClick={() => onAsk({ title: `Remove ${i.name}?`, text: 'The image is deleted from this computer. You can pull it again.', yes: 'Remove', danger: true, onYes: () => void boxes.removeImage(i.name) })}
              >
                <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.5" /></svg>
              </button>
            </li>
          ))}
        </ul>
      </aside>
      <main>
        <section>
          <div className="lbl"><span className="num">03</span>Containers</div>
          {boxes.problem && <p className="problem small" role="alert">{boxes.problem}</p>}
          {running ? (
            <RunForm key={running.name} boxes={boxes} image={running} onDone={() => setRunning(null)} />
          ) : (
            <>
              {s?.containers.length === 0 && <p className="quiet">No containers yet. Pick an image on the left to run it.</p>}
              <ul className="boxes" aria-label="Containers">
                {s?.containers.map((c) => (
                  <li key={c.name} className={boxes.selected === c.name ? 'sel' : ''}>
                    <button type="button" className="box-main" onClick={() => boxes.select(c.name)}>
                      <span className={`dot ${c.running ? 'on' : 'off'}`} aria-hidden="true" />
                      <span className="box-name">{c.name}</span>
                      <span className="quiet mono small">{c.image}</span>
                      <span className="small quiet push">{c.running ? 'running' : `stopped${c.exitCode !== null ? ` (${c.exitCode})` : ''}`}</span>
                    </button>
                    <div className="box-actions">
                      {c.ports.map((p) => (
                        <button key={p.host} type="button" className="link mono" title="Use this address in a request" onClick={() => onUseAddress(`localhost:${p.host}`)}>
                          localhost:{p.host}
                        </button>
                      ))}
                      <span className="push" />
                      {c.running && <button type="button" className="link" onClick={() => void boxes.stop(c.name)}>Stop</button>}
                      <button
                        type="button"
                        className="link danger"
                        onClick={() => onAsk({ title: `Remove ${c.name}?`, text: 'The container stops and its files are deleted. Its log is kept.', yes: 'Remove', danger: true, onYes: () => void boxes.remove(c.name) })}
                      >
                        Remove
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>
        <section>
          <div className="lbl"><span className="num">04</span>Log{current && ` · ${current.name}`}</div>
          {current ? <Logs text={boxes.logs} /> : <p className="quiet">Pick a container to see what it writes.</p>}
          <p className="small quiet">Containers run in Crane's own engine: rootless, in their own namespaces, with no internet.</p>
        </section>
      </main>
    </>
  )
}
