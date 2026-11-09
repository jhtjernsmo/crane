import { useState } from 'react'
import type { Collection } from '../useCollection'
import type { Environments } from '../useEnvironments'
import { isVarName } from '../vars'

/** Shown when Rust refused to write secrets to a file. */
export function SecretReview({ col, envs }: { col: Collection; envs: Environments }) {
  const review = col.review
  const [names, setNames] = useState<Record<number, string>>(() => Object.fromEntries((review?.findings ?? []).map((f, i) => [i, f.suggestedName])))
  if (!review) return null
  const env = envs.active
  const bad = review.findings.some((_, i) => !isVarName(names[i] ?? ''))
  return (
    <div className="scrim">
      <div className="dialog confirm review" role="alertdialog" aria-modal="true" aria-labelledby="review-title">
        <h2 id="review-title">This would put a secret in the file</h2>
        <p>Files often end up in git, where secrets are hard to take back. Move them to the keyring and the file only says <span className="mono">{'{{name}}'}</span>.</p>
        <table className="findings">
          <tbody>
            {review.findings.map((f, i) => (
              <tr key={i}>
                <td>{f.place}</td>
                <td className="mono quiet">{f.preview}</td>
                <td>
                  <span className="mono">{'{{'}</span>
                  <input className="mono" value={names[i] ?? ''} onChange={(e) => setNames((n) => ({ ...n, [i]: e.target.value }))} aria-label={`Variable name for ${f.place}`} spellCheck={false} />
                  <span className="mono">{'}}'}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="small quiet">
          {env ? <>They become secret variables in <b>{env.name}</b>.</> : 'Pick an environment at the top first; the secrets go into it.'}
        </p>
        {col.problem && <p className="problem small" role="alert">{col.problem}</p>}
        <div className="dialog-foot">
          <button type="button" className="link danger" onClick={() => void col.saveAnyway()}>Save with the secret</button>
          <span className="push" />
          <button type="button" className="btn-quiet" onClick={col.dismissReview}>Cancel</button>
          <button type="button" className="btn-main" disabled={!env || bad} onClick={() => void col.moveSecrets(names)}>Move to keyring and save</button>
        </div>
      </div>
    </div>
  )
}
