import { useCallback, useEffect, useState } from 'react'
import { type EnvFile, type EnvList, type Environment, listEnvironments, saveEnvironments, setSecret } from './api'

const EMPTY: EnvList = { active: null, environments: [], secretsSet: [], keyringProblem: null }

export const fileOf = (list: EnvList): EnvFile => ({ active: list.active, environments: list.environments })

export interface Environments {
  list: EnvList
  active: Environment | null
  /** Could not load or save; shown to the user as it is. */
  problem: string | null
  pick: (id: string | null) => Promise<void>
  /** Saves the environments, then hands each typed secret to the keyring. */
  save: (file: EnvFile, secrets: { envId: string; name: string; value: string }[]) => Promise<void>
  isSet: (envId: string, name: string) => boolean
}

export function useEnvironments(): Environments {
  const [list, setList] = useState<EnvList>(EMPTY)
  const [problem, setProblem] = useState<string | null>(null)

  useEffect(() => {
    listEnvironments().then(setList, (e: Error) => setProblem(e.message))
  }, [])

  const pick = useCallback(
    async (id: string | null) => {
      try {
        setList(await saveEnvironments({ ...fileOf(list), active: id }))
        setProblem(null)
      } catch (e) {
        setProblem((e as Error).message)
      }
    },
    [list],
  )

  const save = useCallback(async (file: EnvFile, secrets: { envId: string; name: string; value: string }[]) => {
    let next = await saveEnvironments(file)
    for (const s of secrets) next = await setSecret(s.envId, s.name, s.value)
    setList(next)
    setProblem(null)
  }, [])

  const isSet = useCallback((envId: string, name: string) => list.secretsSet.includes(`${envId}/${name}`), [list])
  const active = list.environments.find((e) => e.id === list.active) ?? null

  return { list, active, problem, pick, save, isSet }
}
