import { useState } from 'react'
import type { TreeNode } from '../api'
import type { Collection } from '../useCollection'

function folders(nodes: TreeNode[]): string[] {
  return nodes.flatMap((n) => (n.kind === 'folder' ? [n.path, ...folders(n.children)] : []))
}

/**
 * Which file is on screen, whether it has changes, and Save. While `naming`
 * is a folder ('' is the top), it asks for a name to save the request under.
 */
export function FileBar({ col, naming, onNaming }: { col: Collection; naming: string | null; onNaming: (folder: string | null) => void }) {
  const [name, setName] = useState('')
  const [folder, setFolder] = useState(naming ?? '')
  if (!col.view) return null

  const label = col.open ? col.open.path.replace(/\.http$/, '') : 'Unsaved request'
  if (naming !== null) {
    return (
      <form
        className="file-bar naming"
        onSubmit={(e) => {
          e.preventDefault()
          if (!name.trim()) return
          void col.saveAs(folder, name).then(() => onNaming(null))
        }}
      >
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name, like Create order" aria-label="Request name" autoFocus />
        <select value={folder} onChange={(e) => setFolder(e.target.value)} aria-label="Folder">
          <option value="">{col.view.name} (top)</option>
          {folders(col.view.tree).map((f) => <option key={f} value={f}>{f}</option>)}
        </select>
        <button type="submit" className="btn-main small-btn">Save</button>
        <button type="button" className="link" onClick={() => onNaming(null)}>Cancel</button>
      </form>
    )
  }
  return (
    <div className="file-bar">
      <span className={`file-name mono${col.open ? '' : ' quiet'}`}>{label}</span>
      {col.dirty && <span className="dirty" title="Changed since it was saved">changed</span>}
      {col.open?.readOnly && <span className="small quiet" title={col.open.readOnly}>read-only</span>}
      <button type="button" className="link push" onClick={() => void col.save().then((done) => !done && onNaming(''))}>
        Save <kbd>Ctrl S</kbd>
      </button>
    </div>
  )
}
