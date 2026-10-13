// Small form fields for codes: TOE attachment and research questions.

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import type { ResearchQuestion, Toe } from "@/lib/types"

export function ToePicker({ value, onChange }: { value: Toe | null; onChange: (v: Toe | null) => void }) {
  const opts: { v: Toe | null; label: string }[] = [
    { v: "T", label: "T, technological" },
    { v: "O", label: "O, organisational" },
    { v: "E", label: "E, environmental" },
    { v: null, label: "none" },
  ]
  return (
    <div className="flex flex-wrap gap-1" role="radiogroup" aria-label="TOE attachment">
      {opts.map((o) => (
        <Button key={String(o.v)} type="button" size="xs" variant={value === o.v ? "default" : "outline"} onClick={() => onChange(o.v)}>
          {o.label}
        </Button>
      ))}
    </div>
  )
}

export function QuestionPicker({ questions, value, onChange }: { questions: ResearchQuestion[]; value: string[]; onChange: (v: string[]) => void }) {
  if (!questions.length) return <span className="text-muted-foreground text-xs">Aucune question de recherche définie pour l'instant (voir les paramètres de l'étude).</span>
  return (
    <div className="flex flex-col gap-1">
      {questions.map((q) => (
        <label key={q.id} className="flex items-start gap-2 text-xs">
          <Checkbox
            checked={value.includes(q.id)}
            onCheckedChange={(v) => onChange(v === true ? [...value, q.id].sort() : value.filter((x) => x !== q.id))}
          />
          <span>
            <span className="ident font-semibold">{q.id}</span> {q.text}
          </span>
        </label>
      ))}
    </div>
  )
}
