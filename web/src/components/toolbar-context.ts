import { createContext } from "react"

export interface ToolbarSlots {
  title: HTMLElement | null
  actions: HTMLElement | null
}

/** Set by the shell; absent when a page is rendered on its own. */
export const ToolbarSlotsContext = createContext<ToolbarSlots | null>(null)
