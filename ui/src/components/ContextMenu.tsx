import { useEffect, useLayoutEffect, useRef, useState } from 'react'

export interface MenuItem {
  label: string
  onSelect: () => void
  danger?: boolean
}

export interface MenuAt {
  x: number
  y: number
  items: MenuItem[]
}

/** Crane's own right-click menu. Closes on a click outside, Escape, scrolling or leaving the window. */
export function ContextMenu({ menu, onClose }: { menu: MenuAt; onClose: () => void }) {
  const ref = useRef<HTMLUListElement>(null)
  const [at, setAt] = useState({ left: menu.x, top: menu.y })

  // Keep the menu inside the window when opened near an edge.
  useLayoutEffect(() => {
    const box = ref.current?.getBoundingClientRect()
    if (!box) return
    setAt({ left: Math.min(menu.x, window.innerWidth - box.width - 4), top: Math.min(menu.y, window.innerHeight - box.height - 4) })
  }, [menu])

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>('button')?.focus()
    const outside = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && onClose()
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') return onClose()
      if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
      e.preventDefault()
      const buttons = [...(ref.current?.querySelectorAll<HTMLButtonElement>('button') ?? [])]
      const now = buttons.indexOf(document.activeElement as HTMLButtonElement)
      const next = (now + (e.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
      buttons[next]?.focus()
    }
    window.addEventListener('mousedown', outside)
    window.addEventListener('keydown', key)
    window.addEventListener('blur', onClose)
    window.addEventListener('resize', onClose)
    document.addEventListener('scroll', onClose, true)
    return () => {
      window.removeEventListener('mousedown', outside)
      window.removeEventListener('keydown', key)
      window.removeEventListener('blur', onClose)
      window.removeEventListener('resize', onClose)
      document.removeEventListener('scroll', onClose, true)
    }
  }, [onClose])

  return (
    <ul className="ctx-menu" role="menu" ref={ref} style={at} onContextMenu={(e) => e.preventDefault()}>
      {menu.items.map((item) => (
        <li key={item.label} role="none">
          <button
            type="button"
            role="menuitem"
            className={item.danger ? 'danger' : undefined}
            onClick={() => {
              onClose()
              item.onSelect()
            }}
          >
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  )
}
