import { useEffect, useRef, useState } from 'react'
import { type EnvFile, type Network, NO_NETWORK } from '../api'
import { isSecretName } from '../request'
import { looksLikeSecret } from '../secretScan'
import { NetworkFields } from './NetworkFields'
import type { Environments } from '../useEnvironments'
import { isVarName } from '../vars'

interface EditVar {
  key: number
  name: string
  value: string
  secret: boolean
  /** The name it was saved under, to warn that renaming drops a stored secret. */
  savedName: string | null
}

interface EditEnv {
  id: string
  name: string
  production: boolean
  vars: EditVar[]
  network: Network
}

let nextKey = 1
const editVar = (name = '', value = '', secret = false, savedName: string | null = null): EditVar => ({ key: nextKey++, name, value, secret, savedName })

const newId = () => crypto.randomUUID()

function VarRow({ v, stored, typed, onChange, onTyped, onRemove, isBlank }: {
  v: EditVar
  stored: boolean
  typed: string | undefined
  onChange: (patch: Partial<EditVar>) => void
  onTyped: (value: string) => void
  onRemove: () => void
  isBlank: boolean
}) {
  const warn =
    !v.secret && v.value && (looksLikeSecret(v.value) || isSecretName(v.name))
      ? 'This looks like a key or password. Tick secret to keep it in the system keyring instead of a file.'
      : v.secret && stored && v.savedName !== null && v.name !== v.savedName
        ? 'Renaming a secret drops its stored value. Type it again.'
        : !isBlank && v.name && !isVarName(v.name)
          ? 'Start with a letter, then use letters, digits, _, - or .'
          : null
  return (
    <>
      <tr>
        <td>
          <input value={v.name} onChange={(e) => onChange({ name: e.target.value })} placeholder={isBlank ? 'name' : ''} aria-label="Variable name" spellCheck={false} />
        </td>
        <td className="val">
          {v.secret ? (
            <input
              type="password"
              value={typed ?? ''}
              onChange={(e) => onTyped(e.target.value)}
              placeholder={stored && v.name === v.savedName ? 'stored in keyring · type to replace' : 'type the secret value'}
              aria-label={`Secret value for ${v.name || 'new variable'}`}
              autoComplete="off"
            />
          ) : (
            <input value={v.value} onChange={(e) => onChange({ value: e.target.value })} placeholder={isBlank ? 'value' : ''} aria-label={`Value for ${v.name || 'new variable'}`} spellCheck={false} />
          )}
        </td>
        <td className="tick">
          {!isBlank && (
            <input
              type="checkbox"
              checked={v.secret}
              onChange={(e) => onChange({ secret: e.target.checked, value: e.target.checked ? '' : v.value })}
              aria-label={`${v.name || 'Variable'} is secret`}
              title="Secret: kept in the system keyring, never in a file"
            />
          )}
        </td>
        <td className="del">
          {!isBlank && (
            <button type="button" onClick={onRemove} aria-label={`Remove ${v.name || 'variable'}`} title="Remove">
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.5" /></svg>
            </button>
          )}
        </td>
      </tr>
      {warn && (
        <tr className="note">
          <td colSpan={4}>{warn}</td>
        </tr>
      )}
    </>
  )
}

export function EnvEditor({ envs, onClose, insecureOff = null }: { envs: Environments; onClose: () => void; insecureOff?: string | null }) {
  const [list, setList] = useState<EditEnv[]>(() =>
    envs.list.environments.map((e) => ({ ...e, network: e.network ?? NO_NETWORK, vars: e.vars.map((v) => editVar(v.name, v.value, v.secret, v.name)) })),
  )
  const [selected, setSelected] = useState<string | null>(envs.list.active ?? envs.list.environments[0]?.id ?? null)
  const [typed, setTyped] = useState<Record<number, string>>({})
  const [blank, setBlank] = useState(() => editVar())
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)
  const dialog = useRef<HTMLDivElement>(null)

  useEffect(() => {
    dialog.current?.querySelector<HTMLElement>('input, button')?.focus()
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  const env = list.find((e) => e.id === selected) ?? null
  const updateEnv = (patch: Partial<EditEnv>) => setList((l) => l.map((e) => (e.id === selected ? { ...e, ...patch } : e)))
  const updateVar = (key: number, patch: Partial<EditVar>) => {
    if (!env) return
    if (key === blank.key) {
      updateEnv({ vars: [...env.vars, { ...blank, ...patch }] })
      setBlank(editVar())
      return
    }
    updateEnv({ vars: env.vars.map((v) => (v.key === key ? { ...v, ...patch } : v)) })
  }

  const add = () => {
    const id = newId()
    const taken = new Set(list.map((e) => e.name.toLowerCase()))
    const name = ['Local', 'Test', 'Staging', 'Prod'].find((n) => !taken.has(n.toLowerCase())) ?? `Environment ${list.length + 1}`
    setList((l) => [...l, { id, name, production: name === 'Prod', vars: [], network: NO_NETWORK }])
    setSelected(id)
    setConfirmDelete(false)
  }

  const remove = () => {
    setList((l) => l.filter((e) => e.id !== selected))
    setSelected(list.find((e) => e.id !== selected)?.id ?? null)
    setConfirmDelete(false)
  }

  const save = async () => {
    setSaving(true)
    setError(null)
    const kept = (e: EditEnv) => e.vars.filter((v) => v.name.trim() !== '' || v.value !== '')
    const file: EnvFile = {
      active: list.some((e) => e.id === envs.list.active) ? envs.list.active : null,
      environments: list.map((e) => ({
        id: e.id,
        name: e.name.trim(),
        production: e.production,
        network: e.network,
        vars: kept(e).map((v) => ({ name: v.name.trim(), value: v.secret ? '' : v.value, secret: v.secret })),
      })),
    }
    const secrets = list.flatMap((e) =>
      kept(e)
        .filter((v) => v.secret && typed[v.key] !== undefined && typed[v.key] !== '')
        .map((v) => ({ envId: e.id, name: v.name.trim(), value: typed[v.key] })),
    )
    try {
      await envs.save(file, secrets)
      onClose()
    } catch (e) {
      setError((e as Error).message)
      setSaving(false)
    }
  }

  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog env-editor" role="dialog" aria-modal="true" aria-labelledby="env-title" ref={dialog}>
        <div className="dialog-head">
          <h2 id="env-title" className="lbl">Environments</h2>
          <button type="button" className="link push" onClick={onClose}>Close</button>
        </div>
        <div className="env-cols">
          <nav aria-label="Environment list">
            <ul>
              {list.map((e) => (
                <li key={e.id}>
                  <button type="button" className={e.id === selected ? 'sel' : ''} onClick={() => { setSelected(e.id); setConfirmDelete(false) }}>
                    {e.name || 'Unnamed'} {e.production && <span className="prod-tag">prod</span>}
                  </button>
                </li>
              ))}
            </ul>
            <button type="button" className="link" onClick={add}>Add environment</button>
          </nav>
          {env ? (
            <div className="env-body">
              <div className="env-fields">
                <label>
                  <span className="lbl">Name</span>
                  <input value={env.name} onChange={(e) => updateEnv({ name: e.target.value })} aria-label="Environment name" />
                </label>
                <label className="check">
                  <input type="checkbox" checked={env.production} onChange={(e) => updateEnv({ production: e.target.checked, network: e.target.checked ? { ...env.network, insecure: false } : env.network })} />
                  Production: ask before POST, PUT, PATCH and DELETE
                </label>
              </div>
              <table className="kv env-vars" aria-label="Variables">
                <thead>
                  <tr>
                    <th>Variable</th>
                    <th>Value</th>
                    <th className="tick">Secret</th>
                    <th className="del" />
                  </tr>
                </thead>
                <tbody>
                  {[...env.vars, blank].map((v) => (
                    <VarRow
                      key={v.key}
                      v={v}
                      isBlank={v.key === blank.key}
                      stored={v.savedName !== null && envs.isSet(env.id, v.savedName)}
                      typed={typed[v.key]}
                      onChange={(patch) => updateVar(v.key, patch)}
                      onTyped={(value) => setTyped((t) => ({ ...t, [v.key]: value }))}
                      onRemove={() => updateEnv({ vars: env.vars.filter((x) => x.key !== v.key) })}
                    />
                  ))}
                </tbody>
              </table>
              <p className="quiet small">
                Use them as <span className="mono">{'{{name}}'}</span> in the address, headers or body. Secret values go straight to the system keyring: they are never
                written to a file, never shown again, and hidden if a server sends them back.
              </p>
              <NetworkFields
                network={env.network}
                production={env.production}
                insecureOff={insecureOff}
                onChange={(network) => updateEnv({ network })}
              />
              <div className="row-gap">
                {confirmDelete ? (
                  <>
                    <span className="small">Delete {env.name || 'this environment'} and its stored secrets?</span>
                    <button type="button" className="link danger" onClick={remove}>Delete</button>
                    <button type="button" className="link" onClick={() => setConfirmDelete(false)}>Keep</button>
                  </>
                ) : (
                  <button type="button" className="link danger" onClick={() => setConfirmDelete(true)}>Delete environment</button>
                )}
              </div>
            </div>
          ) : (
            <div className="env-body">
              <p className="quiet">No environments yet. Add one for each place you send to, like Local and Prod.</p>
            </div>
          )}
        </div>
        {(error || envs.list.keyringProblem) && (
          <p className="problem small" role="alert">{error ?? `The system keyring did not answer: ${envs.list.keyringProblem}`}</p>
        )}
        <div className="dialog-foot">
          <button type="button" className="btn-quiet" onClick={onClose}>Cancel</button>
          <button type="button" className="btn-main" onClick={() => void save()} disabled={saving}>{saving ? 'Saving…' : 'Save'}</button>
        </div>
      </div>
    </div>
  )
}
