// The display name of a code, wherever the server hands out its older handle
// (the upper-case token of the first codebooks, an alias, the code id). The
// screens read it from here, so that a chip or a list never shows STOCK_ACCURACY
// for a code the researcher named "Exactitude des stocks".

import { createContext, useContext, useMemo, type ReactNode } from "react"

import { useCodes } from "./hooks"
import type { CodeNode } from "./types"

export type NameOf = (handle: string) => string

/** Handle (id, token, alias, name) to display name. A name is never replaced by another code's alias. */
export function buildNameMap(codes: CodeNode[]): Map<string, string> {
  const map = new Map<string, string>()
  for (const c of codes) {
    const name = c.name || c.label || c.code_id
    map.set(c.code_id, name)
    map.set(c.name, name)
  }
  for (const c of codes) {
    const name = c.name || c.label || c.code_id
    if (c.token && !map.has(c.token)) map.set(c.token, name)
  }
  for (const c of codes) {
    const name = c.name || c.label || c.code_id
    for (const a of c.aliases) if (!map.has(a.alias)) map.set(a.alias, name)
  }
  return map
}

const identity: NameOf = (h) => h

const Ctx = createContext<NameOf>(identity)

export function CodeNamesProvider({ children }: { children: ReactNode }) {
  const codes = useCodes()
  const nameOf = useMemo<NameOf>(() => {
    const map = buildNameMap(codes.data?.codes ?? [])
    return (h) => map.get(h) ?? h
  }, [codes.data])
  return <Ctx.Provider value={nameOf}>{children}</Ctx.Provider>
}

/** `nameOf("STOCK_ACCURACY")` gives the name of the code; an unknown handle (NA, a label to clarify) is returned as is. */
export function useCodeName(): NameOf {
  return useContext(Ctx)
}
