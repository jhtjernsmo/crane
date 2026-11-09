import { useState } from 'react'
import type { TreeNode } from '../api'
import type { Collection } from '../useCollection'

function folders(nodes: TreeNode[]): string[] {
  return nodes.flatMap((n) => (n.kind === 'folder' ? [n.path, ...folders(n.children)] : []))
}

/** Which file is on screen, whether it has changes, and Save. */
export function FileBar({ col, naming, onNaming }: { col: Collection; naming: boolean; onNaming: (on: boolean) => void }) {
  const [name, setName] = useState('')
  const [folder, setFolder] = useState('')
  if (!col.view) return null

  const label = col.open ? col.open.path.replace(/\.http$/, '') : 'Unsaved request'
  if (naming) {
    return (
      <form
        className="file-bar naming"
        onSubmit={(e) => {
          e.preventDefault()
          if (!name.trim()) return
          void col.saveAs(folder, name).then(() => onNaming(false))
        }}
      >
        <input value={name} onChange={(e) => setName(e.target.value)} placeholder="Name, like Create order" aria-label="Request name" autoFocus />
        <select value={folder} onChange={(e) => setFolder(e.target.value)} aria-label="Folder">
          <option value="">{col.view.name} (top)</option>
          {folders(col.view.tree).map((f) => <option key={f} value={f}>{f}</option>)}
        </select>
        <button type="submit" className="btn-main small-btn">Save</button>
        <button type="button" className="link" onClick={() => onNaming(false)}>Cancel</button>
      </form>
    )
  }
  return (
    <div className="file-bar">
      <span className={`file-name mono${col.open ? '' : ' quiet'}`}>{label}</span>
      {col.dirty && <span className="dirty" title="Changed since it was saved">changed</span>}
      {col.open?.readOnly && <span className="small quiet" title={col.open.readOnly}>read-only</span>}
      <button type="button" className="link push" onClick={() => void col.save().then((done) => !done && onNaming(true))}>
        Save <kbd>Ctrl S</kbd>
      </button>
    </div>
  )
}
