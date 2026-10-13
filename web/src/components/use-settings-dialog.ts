import { useState } from "react"

export type SettingsTab = "general" | "providers" | "advanced"

/** Local state for a dialog opened by the shell. */
export function useSettingsDialog() {
  const [open, setOpen] = useState(false)
  const [tab, setTab] = useState<SettingsTab>("general")
  return { open, setOpen, tab, setTab }
}
