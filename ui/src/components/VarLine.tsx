import type { Environments } from '../useEnvironments'

/** Shows what each `{{variable}}` in the request turns into, before sending. */
export function VarLine({ names, envs, onEdit }: { names: string[]; envs: Environments; onEdit: () => void }) {
  if (names.length === 0) return null
  const env = envs.active
  return (
    <ul className="var-line" aria-label="Variables in this request">
      {names.map((n) => {
        const v = env?.vars.find((x) => x.name === n)
        let shown: string
        let tone = ''
        if (!env) {
          shown = 'pick an environment'
          tone = 'warn'
        } else if (!v) {
          shown = `not in ${env.name}`
          tone = 'bad'
        } else if (v.secret) {
          const set = envs.isSet(env.id, n)
          shown = set ? '•••••• from keyring' : 'secret has no value'
          tone = set ? 'secret' : 'warn'
        } else {
          shown = v.value === '' ? '(empty)' : v.value
        }
        return (
          <li key={n} className={tone}>
            <button type="button" onClick={onEdit} title="Edit environments">
              <span className="mono">{`{{${n}}}`}</span> <span className="arrow">→</span> <span className="mono v">{shown}</span>
            </button>
          </li>
        )
      })}
    </ul>
  )
}
