import { useCallback, useEffect, useState } from 'react'
import {
  type CollectionView,
  type Finding,
  type SavedRequest,
  closeCollection,
  collectionTree,
  createFolder,
  deleteRequest,
  newRequestPath,
  pickCollection,
  readRequest,
  saveRequest,
} from './api'
import { type Draft, emptyDraft, fromSaved, moveToVariable, sameAsSaved, toSaved } from './request'
import { type Environments, fileOf } from './useEnvironments'

/** The request file on screen, as it is on disk. */
export interface OpenFile {
  path: string
  saved: SavedRequest
  /** Set when Crane must not write this file back; saving makes a new one. */
  readOnly: string | null
}

/** Secrets Rust found while saving; nothing was written yet. */
export interface Review {
  path: string
  name: string
  findings: Finding[]
}

export interface Collection {
  view: CollectionView | null
  open: OpenFile | null
  dirty: boolean
  review: Review | null
  problem: string | null
  openFolder: () => Promise<void>
  closeFolder: () => Promise<void>
  load: (path: string) => Promise<void>
  newRequest: () => void
  /** Saves over the open file; `false` means a name and folder are needed first. */
  save: () => Promise<boolean>
  saveAs: (folder: string, name: string) => Promise<void>
  addFolder: (parent: string, name: string) => Promise<void>
  remove: (path: string) => Promise<void>
  /** Puts the found secrets in the keyring as variables, then saves again. */
  moveSecrets: (names: Record<number, string>) => Promise<void>
  saveAnyway: () => Promise<void>
  dismissReview: () => void
}

export function useCollection(draft: Draft, setDraft: (d: Draft) => void, envs: Environments): Collection {
  const [view, setView] = useState<CollectionView | null>(null)
  const [open, setOpen] = useState<OpenFile | null>(null)
  const [review, setReview] = useState<Review | null>(null)
  const [problem, setProblem] = useState<string | null>(null)

  const attempt = useCallback(async (work: () => Promise<void>) => {
    try {
      await work()
      setProblem(null)
    } catch (e) {
      setProblem((e as Error).message)
    }
  }, [])

  useEffect(() => {
    void attempt(async () => setView(await collectionTree()))
  }, [attempt])

  const refresh = async () => setView(await collectionTree())

  const write = useCallback(
    async (path: string, name: string, d: Draft, allowSecrets: boolean) => {
      const saved = toSaved(d, name)
      const result = await saveRequest(path, saved, allowSecrets)
      if (result.findings.length) {
        setReview({ path, name, findings: result.findings })
        return
      }
      setReview(null)
      setOpen({ path: result.saved ?? path, saved, readOnly: null })
      setView(await collectionTree())
    },
    [],
  )

  const dirty = open ? !sameAsSaved(draft, open.saved) : draft.url.trim() !== ''

  return {
    view,
    open,
    dirty,
    review,
    problem,
    openFolder: () =>
      attempt(async () => {
        const picked = await pickCollection()
        if (picked) setView(picked)
      }),
    closeFolder: () =>
      attempt(async () => {
        await closeCollection()
        setView(null)
        setOpen(null)
      }),
    load: (path) =>
      attempt(async () => {
        const read = await readRequest(path)
        setDraft(fromSaved(read.request))
        setOpen({ path, saved: read.request, readOnly: read.readOnly })
      }),
    newRequest: () => {
      setDraft(emptyDraft())
      setOpen(null)
    },
    save: async () => {
      if (!open || open.readOnly) return false
      await attempt(() => write(open.path, open.saved.name, draft, false))
      return true
    },
    saveAs: (folder, name) => attempt(async () => write(await newRequestPath(folder, name), name.trim(), draft, false)),
    addFolder: (parent, name) =>
      attempt(async () => {
        await createFolder(parent, name)
        await refresh()
      }),
    remove: (path) =>
      attempt(async () => {
        await deleteRequest(path)
        if (open?.path === path) setOpen(null)
        await refresh()
      }),
    moveSecrets: (names) =>
      attempt(async () => {
        if (!review) return
        const env = envs.active
        if (!env) throw new Error('Pick an environment first. The secrets go into it as variables.')
        const clash = review.findings.map((_, i) => names[i]).find((n) => env.vars.some((v) => v.name === n && !v.secret))
        if (clash) throw new Error(`${env.name} already has a plain {{${clash}}}. Pick another name.`)
        const added = review.findings
          .map((f, i) => ({ name: names[i], value: f.value }))
          .filter((s) => !env.vars.some((v) => v.name === s.name))
          .map((s) => ({ name: s.name, value: '', secret: true }))
        const file = fileOf(envs.list)
        await envs.save(
          { ...file, environments: file.environments.map((e) => (e.id === env.id ? { ...e, vars: [...e.vars, ...added] } : e)) },
          review.findings.map((f, i) => ({ envId: env.id, name: names[i], value: f.value })),
        )
        const moved = review.findings.reduce((d, f, i) => moveToVariable(d, f, names[i]), draft)
        setDraft(moved)
        await write(review.path, review.name, moved, false)
      }),
    saveAnyway: () => attempt(async () => (review ? write(review.path, review.name, draft, true) : undefined)),
    dismissReview: () => setReview(null),
  }
}
