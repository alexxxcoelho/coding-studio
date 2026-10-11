import { useEffect, useState } from "react"
import { Monitor, Moon, Sun } from "lucide-react"

import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { applyTheme, readChoice, saveChoice, type ThemeChoice } from "@/lib/theme"

export function ThemeSwitch() {
  const [choice, setChoice] = useState<ThemeChoice>(() => readChoice(window.localStorage))

  // While on "system", follow live changes of the OS setting.
  useEffect(() => {
    if (choice !== "system") return
    const mq = window.matchMedia("(prefers-color-scheme: dark)")
    const on = () => applyTheme("system")
    mq.addEventListener("change", on)
    return () => mq.removeEventListener("change", on)
  }, [choice])

  return (
    <ToggleGroup
      type="single"
      size="sm"
      variant="outline"
      value={choice}
      onValueChange={(v) => {
        if (!v) return
        const next = v as ThemeChoice
        setChoice(next)
        saveChoice(next)
      }}
      aria-label="Theme"
    >
      <ToggleGroupItem className="data-[state=on]:bg-sidebar-accent data-[state=on]:text-foreground text-muted-foreground" value="system" aria-label="Follow the system theme" title="System">
        <Monitor />
      </ToggleGroupItem>
      <ToggleGroupItem className="data-[state=on]:bg-sidebar-accent data-[state=on]:text-foreground text-muted-foreground" value="light" aria-label="Light theme" title="Light">
        <Sun />
      </ToggleGroupItem>
      <ToggleGroupItem className="data-[state=on]:bg-sidebar-accent data-[state=on]:text-foreground text-muted-foreground" value="dark" aria-label="Dark theme" title="Dark">
        <Moon />
      </ToggleGroupItem>
    </ToggleGroup>
  )
}
