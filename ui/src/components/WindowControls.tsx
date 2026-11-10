// Crane draws its own title bar: the header drags the window, and these
// buttons replace the system's on Windows and Linux. macOS keeps its own
// traffic lights over the header (src-tauri/tauri.macos.conf.json).
import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { IN_TAURI } from '../api'

export const IS_MAC = typeof navigator !== 'undefined' && /Mac/i.test(navigator.userAgent)

function useMaximized(): boolean {
  const [maximized, setMaximized] = useState(false)
  useEffect(() => {
    if (!IN_TAURI) return
    const win = getCurrentWindow()
    const sync = () => void win.isMaximized().then(setMaximized, () => undefined)
    sync()
    let unlisten: (() => void) | undefined
    win.onResized(sync).then((fn) => (unlisten = fn), () => undefined)
    return () => unlisten?.()
  }, [])
  return maximized
}

const run = (action: (win: ReturnType<typeof getCurrentWindow>) => Promise<void>) => () => {
  if (IN_TAURI) void action(getCurrentWindow()).catch(() => undefined)
}

export function WindowControls() {
  const maximized = useMaximized()
  if (!IN_TAURI || IS_MAC) return null
  return (
    <div className="window-controls">
      <button type="button" onClick={run((w) => w.minimize())} aria-label="Minimize" title="Minimize">
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" stroke="currentColor" /></svg>
      </button>
      <button type="button" onClick={run((w) => w.toggleMaximize())} aria-label={maximized ? 'Restore' : 'Maximize'} title={maximized ? 'Restore' : 'Maximize'}>
        {maximized ? (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor"><path d="M2.5 2.5V.5h7v7h-2" /><path d="M.5 2.5h7v7h-7z" /></svg>
        ) : (
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor"><path d="M.5.5h9v9h-9z" /></svg>
        )}
      </button>
      <button type="button" className="close" onClick={run((w) => w.close())} aria-label="Close" title="Close">
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" /></svg>
      </button>
    </div>
  )
}
