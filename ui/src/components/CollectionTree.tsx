import { useState } from 'react'
import type { TreeNode } from '../api'
import type { Collection } from '../useCollection'

interface Props {
  col: Collection
  onOpen: (path: string) => void
  onDelete: (path: string, name: string) => void
}

function Nodes({ nodes, depth, props, closed, toggle }: { nodes: TreeNode[]; depth: number; props: Props; closed: ReadonlySet<string>; toggle: (p: string) => void }) {
  const current = props.col.open?.path
  return (
    <ul className="tree" role={depth === 0 ? 'tree' : 'group'} aria-label={depth === 0 ? 'Requests' : undefined}>
      {nodes.map((n) =>
        n.kind === 'folder' ? (
          <li key={n.path} role="treeitem" aria-expanded={!closed.has(n.path)}>
            <button type="button" className="folder" style={{ paddingLeft: 18 + depth * 14 }} onClick={() => toggle(n.path)}>
              <svg width="9" height="9" viewBox="0 0 10 10" aria-hidden="true" className={closed.has(n.path) ? '' : 'turned'}><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>
              {n.name}
            </button>
            {!closed.has(n.path) && <Nodes nodes={n.children} depth={depth + 1} props={props} closed={closed} toggle={toggle} />}
          </li>
        ) : (
          <li key={n.path} role="treeitem" aria-selected={current === n.path} className="leaf">
            <button type="button" className={current === n.path ? 'sel' : ''} style={{ paddingLeft: 18 + depth * 14 }} onClick={() => props.onOpen(n.path)} title={n.path}>
              <span className={`m m-${n.method}`}>{n.method}</span>
              <span className="u">{n.name}</span>
            </button>
            <button type="button" className="del-file" onClick={() => props.onDelete(n.path, n.name)} aria-label={`Delete ${n.name}`} title="Delete file">
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M1 1l8 8M9 1l-8 8" stroke="currentColor" strokeWidth="1.5" /></svg>
            </button>
          </li>
        ),
      )}
    </ul>
  )
}

export function CollectionTree(props: Props) {
  const { col } = props
  const [closed, setClosed] = useState<ReadonlySet<string>>(new Set())
  const [newFolder, setNewFolder] = useState<string | null>(null)
  const toggle = (p: string) =>
    setClosed((s) => {
      const next = new Set(s)
      if (!next.delete(p)) next.add(p)
      return next
    })

  if (!col.view) {
    return (
      <div className="pad empty-col">
        <p className="quiet small">Keep requests as .http files in a folder, for example next to your code in git. Secrets stay in the keyring.</p>
        <button type="button" className="btn-quiet" onClick={() => void col.openFolder()}>Open a folder</button>
      </div>
    )
  }
  return (
    <>
      <div className="col-head pad">
        <span className="col-name" title={col.view.root}>{col.view.name}</span>
        <button type="button" className="link" onClick={col.newRequest}>New</button>
        <button type="button" className="link" onClick={() => setNewFolder('')}>Folder</button>
        <button type="button" className="link quiet-link" onClick={() => void col.closeFolder()} title="Close the folder">Close</button>
      </div>
      {newFolder !== null && (
        <form
          className="pad inline-form"
          onSubmit={(e) => {
            e.preventDefault()
            if (newFolder.trim()) void col.addFolder('', newFolder)
            setNewFolder(null)
          }}
        >
          <input value={newFolder} onChange={(e) => setNewFolder(e.target.value)} placeholder="folder name" aria-label="New folder name" autoFocus onBlur={() => setNewFolder(null)} />
        </form>
      )}
      {col.view.tree.length === 0 ? (
        <p className="quiet small pad">No .http files here yet. Make a request and press Save.</p>
      ) : (
        <Nodes nodes={col.view.tree} depth={0} props={props} closed={closed} toggle={toggle} />
      )}
    </>
  )
}
