import type { ImportDone } from '../api'

/** What the last import wrote, and what still needs a hand. */
export function ImportResult({ done, onClose }: { done: ImportDone; onClose: () => void }) {
  const r = done.report
  return (
    <div className="import-result pad small" role="status">
      <p>
        {r.folder ? (
          <>
            Imported {r.requests} {r.requests === 1 ? 'request' : 'requests'} from {done.from} into <strong>{r.folder}</strong>.
          </>
        ) : (
          <>Imported from {done.from}.</>
        )}
        {r.environments.length > 0 && <> Added {r.environments.length === 1 ? 'the environment' : 'environments'} {r.environments.join(', ')}.</>}
      </p>
      {r.secretsMoved.length > 0 && (
        <p>Secrets typed into requests now live in the keyring as {r.secretsMoved.map((n) => `{{${n}}}`).join(', ')}.</p>
      )}
      {r.secretsToSet.length > 0 && <p>Give these secrets a value under Environments: {r.secretsToSet.join(', ')}.</p>}
      {r.notes.length > 0 && (
        <>
          <p className="quiet">Not brought over:</p>
          <ul>
            {r.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </>
      )}
      <button type="button" className="link" onClick={onClose}>
        Done
      </button>
    </div>
  )
}
