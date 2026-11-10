import { useCallback, useEffect, useState } from 'react'
import type { Update as Found } from '@tauri-apps/plugin-updater'
import { IN_TAURI } from './api'

/** Wait a little after start, so checking never slows the first screen. */
const CHECK_AFTER_MS = 4000

export interface Update {
  /** The newer version, once one is found. */
  version: string | null
  /** Downloading or installing; 0 to 100 when the size is known. */
  progress: number | null
  problem: string | null
  install: () => Promise<void>
}

/**
 * Asks GitHub once per start for a newer release. Tauri only installs it if
 * it is signed with Crane's key; the policy can turn the check off.
 */
export function useUpdate(allowed: boolean): Update {
  const [found, setFound] = useState<Found | null>(null)
  const [progress, setProgress] = useState<number | null>(null)
  const [problem, setProblem] = useState<string | null>(null)

  useEffect(() => {
    if (!IN_TAURI || !allowed) return
    let gone = false
    const timer = setTimeout(() => {
      import('@tauri-apps/plugin-updater')
        .then(({ check }) => check())
        .then((u) => !gone && setFound(u))
        // Offline, or a build the updater cannot replace (.deb, .rpm): stay quiet.
        .catch(() => undefined)
    }, CHECK_AFTER_MS)
    return () => {
      gone = true
      clearTimeout(timer)
    }
  }, [allowed])

  const install = useCallback(async () => {
    if (!found) return
    setProblem(null)
    setProgress(0)
    let total = 0
    let done = 0
    try {
      await found.downloadAndInstall((e) => {
        if (e.event === 'Started') total = e.data.contentLength ?? 0
        if (e.event === 'Progress') {
          done += e.data.chunkLength
          if (total) setProgress(Math.min(100, Math.round((done / total) * 100)))
        }
      })
      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (e) {
      setProgress(null)
      setProblem(`The update could not be installed: ${String(e)}`)
    }
  }, [found])

  return { version: found?.version ?? null, progress, problem, install }
}
