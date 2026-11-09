import { useEffect, useState } from 'react'
import { OPEN_POLICY, type Policy, getPolicy } from './api'

/** The organisation's rules, so the UI can hide what Rust would refuse anyway. */
export function usePolicy(): Policy {
  const [policy, setPolicy] = useState<Policy>(OPEN_POLICY)
  useEffect(() => {
    getPolicy().then(setPolicy, () => undefined)
  }, [])
  return policy
}

/** "… is turned off by your organisation's policy. Ask x if you need it." */
export const refusal = (p: Policy, what: string) =>
  `${what} is turned off by your organisation's policy.${p.contact ? ` Ask ${p.contact} if you need it.` : ''}`
