// Documented decisions (cases): the tab of Analyser and of Rédiger. A decision
// is documented only by the units attributed to it; one without a unit is shown
// as undocumented. The tab completes the analysis by themes, it does not
// replace it.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { akeys, useCasesAll } from "@/lib/analysis-hooks"
import { isDocumented, outcomeLabel, plural } from "@/lib/analysis-text"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"

const OUTCOMES = [
  { value: "adopted", label: "Adopter" },
  { value: "deferred", label: "Différer" },
  { value: "rejected", label: "Refuser" },
]

export function DecisionsPanel() {
  const qc = useQueryClient()
  const cases = useCasesAll()
  const interviews = useQuery({ queryKey: akeys.interviews, queryFn: api.interviews })
  const [adding, setAdding] = useState(false)
  const [name, setName] = useState("")
  const [outcome, setOutcome] = useState("adopted")
  const [interviewId, setInterviewId] = useState("")

  const create = useMutation({
    mutationFn: () => api.addCase(interviewId || interviews.data![0].interview_id, { label: name.trim(), decision: outcome }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: akeys.cases })
      void qc.invalidateQueries({ queryKey: ["cross-case"] })
      setAdding(false)
      setName("")
    },
    onError: (e) => reportError(e, "Décision non créée"),
  })

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-muted-foreground max-w-prose text-sm">
          Les décisions concrètes que l'étude observe servent d'illustrations, pas d'ossature. Une décision se documente en lui rattachant des unités de n'importe quel entretien ; sans unité, elle est marquée non documentée.
        </p>
        {!adding && (
          <Button size="sm" onClick={() => setAdding(true)} disabled={!interviews.data?.length}>
            Nouvelle décision
          </Button>
        )}
      </div>
      {adding && (
        <form
          className="grp flex flex-wrap items-end gap-3 p-3"
          onSubmit={(e) => {
            e.preventDefault()
            if (name.trim()) create.mutate()
          }}
        >
          <label className="flex min-w-60 flex-1 flex-col gap-1">
            <span className="hs">Nom de la décision</span>
            <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Bibliothèque de la commune X, nouveau système de prêt" />
          </label>
          <label className="flex flex-col gap-1">
            <span className="hs">Issue</span>
            <NativeSelect value={outcome} onChange={(e) => setOutcome(e.target.value)} style={{ width: 120 }}>
              {OUTCOMES.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </NativeSelect>
          </label>
          <label className="flex flex-col gap-1">
            <span className="hs">Entretien d'origine</span>
            <NativeSelect value={interviewId} onChange={(e) => setInterviewId(e.target.value)} style={{ width: 160 }}>
              {(interviews.data ?? []).map((i) => (
                <option key={i.interview_id} value={i.interview_id}>
                  {i.interview_id}
                </option>
              ))}
            </NativeSelect>
          </label>
          <Button type="button" variant="ghost" size="sm" onClick={() => setAdding(false)}>
            Annuler
          </Button>
          <Button type="submit" size="sm" disabled={!name.trim() || create.isPending}>
            Créer la décision
          </Button>
        </form>
      )}
      {cases.isLoading && <Skeleton className="h-24 w-full" />}
      {cases.data && cases.data.length === 0 && <p className="text-muted-foreground text-sm">Aucune décision documentée pour l'instant.</p>}
      <ul className="grp">
        {(cases.data ?? []).map((c) => (
          <li key={c.case_id} className="row">
            <div className="k">
              <span className="font-medium">{c.label || "Décision sans nom"}</span>
              <small>
                {outcomeLabel(c.decision)}
                {c.granularity ? ` · granularité ${c.granularity}` : ""}
                {c.carrier_chosen ? ` · support ${c.carrier_chosen}` : ""}
                {" · "}
                {isDocumented(c) ? `${plural(c.units.length, "unité rattachée", "unités rattachées")}` : <span className="text-warn">non documentée</span>}
              </small>
            </div>
            <Button asChild variant="ghost" size="sm">
              <a href={interviewHref(c.interview_id, "cases")} aria-label={`Ouvrir ${c.label || c.case_id}`}>
                Ouvrir
              </a>
            </Button>
          </li>
        ))}
      </ul>
    </div>
  )
}
