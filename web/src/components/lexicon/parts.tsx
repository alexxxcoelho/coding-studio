// Small pieces shared by the tools of the Lexique screen.

import type { ReactNode } from "react"

import { t } from "@/i18n"
import { GUIDE_LEXIQUE } from "@/lib/lexicon-copy"
import { cn } from "@/lib/utils"

export function GuideLink({ children = t("lexicon.parts.lire_le_guide") }: { children?: ReactNode }) {
  return (
    <a href={GUIDE_LEXIQUE} className="small">
      {children}
    </a>
  )
}

/**
 * What a tool does, shown while it has nothing to show. Says what the tool
 * counts, what it does not do, and where the guide explains it.
 */
export function ToolIntro({ title, children, action, className }: { title: string; children: ReactNode; action?: ReactNode; className?: string }) {
  return (
    <div className={cn("border-border bg-card rounded-md border border-dashed px-4 py-4", className)} data-testid="tool-intro">
      <p className="text-sm font-medium">{title}</p>
      <p className="text-muted-foreground mt-1 max-w-prose text-sm">{children}</p>
      <div className="r8 mt-2">
        {action}
        <GuideLink />
      </div>
    </div>
  )
}
