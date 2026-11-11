import { useCallback, useState } from 'react'
import type { TreeNode } from '../api'
import type { Collection } from '../useCollection'
import { ContextMenu, type MenuAt, type MenuItem } from './ContextMenu'
import { ImportResult } from './ImportResult'

interface Props {
  col: Collection
  onOpen: (path: string) => void
  onDelete: (path: string, name: string) => void
  onRunAll: () => void
  running: boolean
  /** Name the request on screen and save it into `folder` ('' is the top). */
  onSaveIn: (folder: string) => void
}

/** Right-click on a folder, a file or the space around them. */
type MenuFor = (e: React.MouseEvent, node: TreeNode | null) => void

const parentOf = (path: string) => (path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '')

function Nodes({ nodes, depth, props, closed, toggle, onMenu }: {
  nodes: TreeNode[]
  depth: number
  props: Props
  closed: ReadonlySet<string>
  toggle: (p: string) => void
  onMenu: MenuFor
}) {
  const current = props.col.open?.path
  return (
    <ul className="tree" role={depth === 0 ? 'tree' : 'group'} aria-label={depth === 0 ? 'Requests' : undefined}>
      {nodes.map((n) =>
        n.kind === 'folder' ? (
          <li key={n.path} role="treeitem" aria-expanded={!closed.has(n.path)}>
            <button type="button" className="folder" style={{ paddingLeft: 18 + depth * 14 }} onClick={() => toggle(n.path)} onContextMenu={(e) => onMenu(e, n)}>
              <svg width="9" height="9" viewBox="0 0 10 10" aria-hidden="true" className={closed.has(n.path) ? '' : 'turned'}><path d="M3 1l4 4-4 4" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>
              {n.name}
            </button>
            {!closed.has(n.path) && <Nodes nodes={n.children} depth={depth + 1} props={props} closed={closed} toggle={toggle} onMenu={onMenu} />}
          </li>
        ) : (
          <li key={n.path} role="treeitem" aria-selected={current === n.path} className="leaf">
            <button type="button" className={current === n.path ? 'sel' : ''} style={{ paddingLeft: 18 + depth * 14 }} onClick={() => props.onOpen(n.path)} onContextMenu={(e) => onMenu(e, n)} title={n.path}>
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
  /** The folder a new folder goes into, and its name so far. */
  const [newFolder, setNewFolder] = useState<{ parent: string; name: string } | null>(null)
  const [menu, setMenu] = useState<MenuAt | null>(null)
  const closeMenu = useCallback(() => setMenu(null), [])
  const toggle = (p: string) =>
    setClosed((s) => {
      const next = new Set(s)
      if (!next.delete(p)) next.add(p)
      return next
    })

  const onMenu: MenuFor = (e, node) => {
    e.preventDefault()
    e.stopPropagation()
    const folder = node === null ? '' : node.kind === 'folder' ? node.path : parentOf(node.path)
    const where = folder || 'the top'
    const items: MenuItem[] =
      node?.kind === 'request'
        ? [
            { label: 'Open', onSelect: () => props.onOpen(node.path) },
            { label: `Save current request in ${where}`, onSelect: () => props.onSaveIn(folder) },
            { label: 'Delete', danger: true, onSelect: () => props.onDelete(node.path, node.name) },
          ]
        : [
            { label: `Save current request ${node ? `in ${where}` : 'here'}`, onSelect: () => props.onSaveIn(folder) },
            { label: node ? `New folder in ${where}` : 'New folder', onSelect: () => setNewFolder({ parent: folder, name: '' }) },
          ]
    setMenu({ x: e.clientX, y: e.clientY, items })
  }

  if (!col.view) {
    return (
      <div className="pad empty-col">
        <p className="quiet small">Keep requests as .http files in a folder, for example next to your code in git. Secrets stay in the keyring.</p>
        <button type="button" className="btn-quiet" onClick={() => void col.openFolder()}>Open a folder</button>
        <p className="quiet small">Coming from Postman, Bruno or Insomnia? Open a folder, then Import.</p>
      </div>
    )
  }
  return (
    <>
      <div className="col-head pad">
        <span className="col-name" title={col.view.root}>{col.view.name}</span>
        <button type="button" className="link" onClick={props.onRunAll} disabled={props.running} title="Send every request and check the answers">Run all</button>
        <button type="button" className="link" onClick={col.newRequest}>New</button>
        <button type="button" className="link" onClick={() => setNewFolder({ parent: '', name: '' })}>Folder</button>
        <button
          type="button"
          className="link"
          aria-haspopup="menu"
          title="Bring requests over from Postman, Bruno or Insomnia"
          onClick={(e) => {
            const box = e.currentTarget.getBoundingClientRect()
            setMenu({
              x: box.left,
              y: box.bottom + 4,
              items: [
                { label: 'Postman or Insomnia export…', onSelect: () => void col.importFrom(false) },
                { label: 'Bruno collection folder…', onSelect: () => void col.importFrom(true) },
              ],
            })
          }}
        >
          Import
        </button>
        <button type="button" className="link quiet-link" onClick={() => void col.closeFolder()} title="Close the folder">Close</button>
      </div>
      {newFolder !== null && (
        <form
          className="pad inline-form"
          onSubmit={(e) => {
            e.preventDefault()
            if (newFolder.name.trim()) void col.addFolder(newFolder.parent, newFolder.name)
            setNewFolder(null)
          }}
        >
          <input
            value={newFolder.name}
            onChange={(e) => setNewFolder({ ...newFolder, name: e.target.value })}
            placeholder={newFolder.parent ? `folder name, in ${newFolder.parent}` : 'folder name'}
            aria-label="New folder name"
            autoFocus
            onBlur={() => setNewFolder(null)}
          />
        </form>
      )}
      {col.imported && <ImportResult done={col.imported} onClose={col.dismissImport} />}
      <div className="tree-area" onContextMenu={(e) => onMenu(e, null)}>
        {col.view.tree.length === 0 ? (
          <p className="quiet small pad">No .http files here yet. Make a request and press Save.</p>
        ) : (
          <Nodes nodes={col.view.tree} depth={0} props={props} closed={closed} toggle={toggle} onMenu={onMenu} />
        )}
      </div>
      {menu && <ContextMenu menu={menu} onClose={closeMenu} />}
    </>
  )
}
