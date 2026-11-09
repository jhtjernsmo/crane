import { useState } from 'react'
import { type Network, pickCertFile } from '../api'

type ProxyMode = 'system' | 'none' | 'custom'

const modeOf = (proxy: string): ProxyMode => (proxy === '' ? 'system' : proxy === 'none' ? 'none' : 'custom')

function FileField({ label, hint, value, onChange }: { label: string; hint: string; value: string; onChange: (v: string) => void }) {
  return (
    <label className="net-field">
      <span className="lbl">{label}</span>
      <span className="file-pick">
        <input value={value} onChange={(e) => onChange(e.target.value)} placeholder="none" aria-label={label} spellCheck={false} />
        <button type="button" className="btn-quiet" onClick={() => void pickCertFile().then((p) => p && onChange(p))}>Browse</button>
        {value && <button type="button" className="link" onClick={() => onChange('')}>Clear</button>}
      </span>
      <span className="small quiet">{hint}</span>
    </label>
  )
}

/** Certificates and proxy for internal APIs. */
export function NetworkFields({ network, production, onChange }: { network: Network; production: boolean; onChange: (n: Network) => void }) {
  const [open, setOpen] = useState(() => network.caFile !== '' || network.clientCertFile !== '' || network.insecure || network.proxy !== '')
  const set = (patch: Partial<Network>) => onChange({ ...network, ...patch })
  const mode = modeOf(network.proxy)
  const summary = [network.caFile && 'own CA', network.clientCertFile && 'client certificate', network.insecure && 'no certificate check', mode === 'custom' && 'proxy', mode === 'none' && 'no proxy']
    .filter(Boolean)
    .join(', ')
  return (
    <div className="net">
      <button type="button" className="net-toggle" onClick={() => setOpen(!open)} aria-expanded={open}>
        <svg width="9" height="9" viewBox="0 0 10 10" aria-hidden="true" className={open ? 'turned' : ''}><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>
        <span className="lbl">Network</span>
        <span className="small quiet">{summary || 'system certificates and proxy'}</span>
      </button>
      {open && (
        <div className="net-body">
          <FileField label="CA certificates" hint="A PEM file with your company's CA or a self-signed server certificate, trusted on top of the system's." value={network.caFile} onChange={(caFile) => set({ caFile })} />
          <FileField label="Client certificate" hint="For servers that want one (mutual TLS): a PEM file with the certificate and its unencrypted key." value={network.clientCertFile} onChange={(clientCertFile) => set({ clientCertFile })} />
          <label className="check">
            <input type="checkbox" checked={network.insecure} disabled={production} onChange={(e) => set({ insecure: e.target.checked })} />
            Skip certificate checks
            <span className="small quiet">{production ? 'Not allowed in production.' : 'Only for local development: anyone in between could read the traffic.'}</span>
          </label>
          {network.insecure && <p className="help small">Certificates are not checked in this environment. Prefer adding the CA file above.</p>}
          <div className="net-field">
            <span className="lbl">Proxy</span>
            <div className="seg" role="radiogroup" aria-label="Proxy">
              {(['system', 'none', 'custom'] as const).map((m) => (
                <button
                  key={m}
                  type="button"
                  role="radio"
                  aria-checked={mode === m}
                  onClick={() => set({ proxy: m === 'system' ? '' : m === 'none' ? 'none' : mode === 'custom' ? network.proxy : 'http://' })}
                >
                  {m === 'system' ? 'System settings' : m === 'none' ? 'No proxy' : 'Custom'}
                </button>
              ))}
            </div>
            {mode === 'custom' && (
              <span className="proxy-fields">
                <input value={network.proxy} onChange={(e) => set({ proxy: e.target.value })} placeholder="http://proxy.corp:3128" aria-label="Proxy address" spellCheck={false} />
                <input value={network.noProxy} onChange={(e) => set({ noProxy: e.target.value })} placeholder="skip for: localhost, .internal" aria-label="Hosts without proxy" spellCheck={false} />
                <span className="small quiet">A password belongs in a secret: <span className="mono">{'http://me:{{proxy_password}}@proxy:3128'}</span></span>
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
