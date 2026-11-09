import { useCallback, useEffect, useRef, useState } from 'react'
import {
  type BoxStatus,
  type PullProgress,
  type RunRequest,
  boxLogs,
  boxPull,
  boxRemove,
  boxRemoveImage,
  boxRun,
  boxStatus,
  boxStop,
  onPullProgress,
} from './api'

const STATUS_EVERY_MS = 2000
const LOGS_EVERY_MS = 1000

export interface Boxes {
  status: BoxStatus | null
  problem: string | null
  pulling: { name: string; progress: PullProgress | null } | null
  selected: string | null
  logs: string
  select: (name: string | null) => void
  pull: (name: string) => Promise<boolean>
  removeImage: (name: string) => Promise<void>
  run: (req: RunRequest) => Promise<boolean>
  stop: (name: string) => Promise<void>
  remove: (name: string) => Promise<void>
}

/** Container state, refreshed while the view is open. */
export function useBoxes(): Boxes {
  const [status, setStatus] = useState<BoxStatus | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const [pulling, setPulling] = useState<Boxes['pulling']>(null)
  const [selected, select] = useState<string | null>(null)
  const [logs, setLogs] = useState('')
  const alive = useRef(true)

  const refresh = useCallback(async () => {
    try {
      const s = await boxStatus()
      if (alive.current) setStatus(s)
    } catch (e) {
      if (alive.current) setProblem((e as Error).message)
    }
  }, [])

  useEffect(() => {
    alive.current = true
    void refresh()
    const timer = setInterval(() => void refresh(), STATUS_EVERY_MS)
    return () => {
      alive.current = false
      clearInterval(timer)
    }
  }, [refresh])

  useEffect(() => {
    if (!selected) return
    let stop = false
    const load = () => boxLogs(selected).then((l) => !stop && setLogs(l), () => undefined)
    void load()
    const timer = setInterval(load, LOGS_EVERY_MS)
    return () => {
      stop = true
      clearInterval(timer)
    }
  }, [selected])

  const attempt = async (work: () => Promise<void>) => {
    try {
      await work()
      setProblem(null)
      await refresh()
      return true
    } catch (e) {
      setProblem((e as Error).message)
      return false
    }
  }

  return {
    status,
    problem,
    pulling,
    selected,
    logs,
    select: (name) => {
      setLogs('')
      select(name)
    },
    pull: async (name) => {
      setPulling({ name, progress: null })
      const unlisten = await onPullProgress((progress) => setPulling({ name, progress }))
      const ok = await attempt(async () => void (await boxPull(name)))
      unlisten()
      setPulling(null)
      return ok
    },
    removeImage: async (name) => void (await attempt(() => boxRemoveImage(name))),
    run: async (req) => {
      const ok = await attempt(() => boxRun(req))
      if (ok) select(req.name)
      return ok
    },
    stop: async (name) => void (await attempt(() => boxStop(name))),
    remove: async (name) => {
      if (selected === name) select(null)
      await attempt(() => boxRemove(name))
    },
  }
}
