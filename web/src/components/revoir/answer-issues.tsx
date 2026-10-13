// The refusal of a model answer: every problem reported together, and the raw
// text that was refused, kept visible but never stored by the server.

import { useState } from "react"

import { Button } from "@/components/ui/button"
import { issueLines } from "@/lib/review-model"
import type { RejectedAnswer, ValidationIssue } from "@/lib/types"

import "./revoir.css"

export function AnswerIssues({ issues }: { issues: ValidationIssue[] }) {
  const { title, items } = issueLines(issues)
  return (
    <div role="alert" className="rv-err">
      <p className="m-0 font-medium">{title}</p>
      <ul>
        {items.map((i, k) => (
          <li key={k}>
            <span className="mono">{i.location}</span> <span>{i.message}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function RejectedRaw({ rejected }: { rejected: RejectedAnswer }) {
  const [open, setOpen] = useState(false)
  return (
    <div className="flex flex-col gap-1.5">
      <Button variant="ghost" className="self-start" onClick={() => setOpen((o) => !o)}>
        {open ? `Masquer la réponse refusée de la partie ${rejected.part}` : `Afficher la réponse refusée de la partie ${rejected.part}`}
      </Button>
      {open && <pre className="rv-raw mono m-0">{rejected.raw}</pre>}
    </div>
  )
}
