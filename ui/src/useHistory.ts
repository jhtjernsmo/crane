import { useCallback, useEffect, useRef, useState } from 'react'
import { type HistoryEntry, type Response, historyAdd, historyClear, historyList } from './api'
import { type Draft, forHistory, fromSaved, toSaved } from './request'

/** One send: from this session with its answer, or from an earlier one with a summary. */
export interface Sent {
  id: number
  at: Date
  /** Secret values already removed. */
  draft: Draft
  envName: string | null
  res?: Response
  error?: string
  /** What is kept from earlier sessions instead of the answer. */
  summary?: { status: number | null; totalMs: number | null }
}

const SHOWN = 100

export interface History {
  items: Sent[]
  current: Sent | null
  setCurrent: (s: Sent | null) => void
  record: (draft: Draft, envName: string | null, outcome: { res?: Response; error?: string }) => Sent
  clear: () => Promise<void>
  problem: string | null
}

const fromEntry = (e: HistoryEntry, id: number): Sent => ({
  id,
  at: new Date(e.at),
  draft: fromSaved(e.request),
  envName: e.envName,
  error: e.error ?? undefined,
  summary: { status: e.status, totalMs: e.totalMs },
})

export function useHistory(): History {
  const [items, setItems] = useState<Sent[]>([])
  const [current, setCurrent] = useState<Sent | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
  const nextId = useRef(1)

  useEffect(() => {
    historyList().then(
      (entries) => setItems((now) => [...now, ...entries.slice(-SHOWN).reverse().map((e) => fromEntry(e, nextId.current++))].slice(0, SHOWN)),
      (e: Error) => setProblem(`History could not be read: ${e.message}`),
    )
  }, [])

  const record = useCallback((draft: Draft, envName: string | null, outcome: { res?: Response; error?: string }) => {
    const kept = forHistory({ ...draft, url: draft.url.trim() })
    const sent: Sent = { id: nextId.current++, at: new Date(), draft: kept, envName, ...outcome }
    setItems((h) => [sent, ...h].slice(0, SHOWN))
    setCurrent(sent)
    historyAdd({
      at: sent.at.getTime(),
      envName,
      request: toSaved(kept, ''),
      status: outcome.res?.status ?? null,
      totalMs: outcome.res?.timing.totalMs ?? null,
      size: outcome.res?.size ?? null,
      error: outcome.error ?? null,
    }).catch((e: Error) => setProblem(`History could not be saved: ${e.message}`))
    return sent
  }, [])

  const clear = useCallback(async () => {
    await historyClear()
    setItems([])
    setCurrent(null)
  }, [])

  return { items, current, setCurrent, record, clear, problem }
}
