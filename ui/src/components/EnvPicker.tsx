import type { Environments } from '../useEnvironments'

export function EnvPicker({ envs, onEdit }: { envs: Environments; onEdit: () => void }) {
  const { list, active } = envs
  return (
    <div className={`env-pick${active?.production ? ' is-prod' : ''}`}>
      <label className="lbl" htmlFor="env-select">Env</label>
      <select id="env-select" value={list.active ?? ''} onChange={(e) => void envs.pick(e.target.value || null)} aria-label="Environment">
        <option value="">None</option>
        {list.environments.map((e) => (
          <option key={e.id} value={e.id}>
            {e.name}
          </option>
        ))}
      </select>
      {active?.production && <span className="prod-tag">production</span>}
      <button type="button" className="link" onClick={onEdit}>Environments</button>
    </div>
  )
}
