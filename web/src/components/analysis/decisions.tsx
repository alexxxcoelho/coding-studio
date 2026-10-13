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
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { analysisApi, DECISION_OUTCOMES, type DecisionOutcome, type DecisionRecord } from "@/lib/analysis-api"
import { akeys, useDecisions } from "@/lib/analysis-hooks"
import { isDocumented, outcomeLabel } from "@/lib/analysis-text"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"

export function DecisionsPanel() {
  const qc = useQueryClient()
  const decisions = useDecisions()
  const interviews = useQuery({ queryKey: akeys.interviews, queryFn: api.interviews })
  const [adding, setAdding] = useState(false)
  const [name, setName] = useState("")
  const [outcome, setOutcome] = useState<DecisionOutcome>("adopt")
  const [interviewId, setInterviewId] = useState("")

  const create = useMutation({
    mutationFn: () => analysisApi.createDecision(interviewId || interviews.data![0].interview_id, name.trim(), outcome),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: akeys.decisions })
      void qc.invalidateQueries({ queryKey: akeys.cases })
      void qc.invalidateQueries({ queryKey: ["cross-case"] })
      setAdding(false)
      setName("")
    },
    onError: (e) => reportError(e, t("analysis.decisions.decision_non_creee")),
  })

  const setOutcome_ = useMutation({
    mutationFn: ({ d, value }: { d: DecisionRecord; value: DecisionOutcome }) => analysisApi.updateDecision(d.decision_id, { ...d, outcome: value }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: akeys.decisions }),
    onError: (e) => reportError(e, t("analysis.decisions.issue_non_enregistree")),
  })

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-muted-foreground max-w-prose text-sm">
          {t("analysis.decisions.les_decisions_concretes_que_l")}
        </p>
        {!adding && (
          <Button size="sm" onClick={() => setAdding(true)} disabled={!interviews.data?.length}>
            {t("analysis.decisions.nouvelle_decision")}
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
            <span className="hs">{t("analysis.decisions.nom_de_la_decision")}</span>
            <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder={t("analysis.decisions.bibliotheque_de_la_commune_x")} />
          </label>
          <label className="flex flex-col gap-1">
            <span className="hs">{t("analysis.decisions.issue")}</span>
            <NativeSelect value={outcome} onChange={(e) => setOutcome(e.target.value as DecisionOutcome)} style={{ width: 120 }}>
              {DECISION_OUTCOMES.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </NativeSelect>
          </label>
          <label className="flex flex-col gap-1">
            <span className="hs">{t("analysis.decisions.entretien_d_origine")}</span>
            <NativeSelect value={interviewId} onChange={(e) => setInterviewId(e.target.value)} style={{ width: 160 }}>
              {(interviews.data ?? []).map((i) => (
                <option key={i.interview_id} value={i.interview_id}>
                  {i.interview_id}
                </option>
              ))}
            </NativeSelect>
          </label>
          <Button type="button" variant="ghost" size="sm" onClick={() => setAdding(false)}>
            {t("common.annuler")}
          </Button>
          <Button type="submit" size="sm" disabled={!name.trim() || create.isPending}>
            {t("analysis.decisions.creer_la_decision")}
          </Button>
        </form>
      )}
      {decisions.isLoading && <Skeleton className="h-24 w-full" />}
      {decisions.data && decisions.data.length === 0 && <p className="text-muted-foreground text-sm">{t("analysis.decisions.aucune_decision_documentee_pour_l")}</p>}
      <ul className="grp">
        {(decisions.data ?? []).map((c) => (
          <li key={c.decision_id} className="row">
            <div className="k">
              <span className="font-medium">{c.name || t("analysis.decisions.decision_sans_nom")}</span>
              <small>
                {outcomeLabel(c.outcome)}
                {c.granularity ? t("analysis.decisions.granularite_granularity", { granularity: c.granularity }) : ""}
                {c.carrier_chosen ? ` · support ${c.carrier_chosen}` : ""}
                {" · "}
                {isDocumented(c) ? t("analysis.decisions.n_elements_rattaches", { n: c.units.length + c.passages.length }) : <span className="text-warn">{t("analysis.decisions.non_documentee")}</span>}
              </small>
            </div>
            <NativeSelect aria-label={t("analysis.decisions.issue_de_v", { v: c.name || c.decision_id })} value={c.outcome ?? ""} onChange={(e) => e.target.value && setOutcome_.mutate({ d: c, value: e.target.value as DecisionOutcome })} style={{ width: 120 }}>
              {!c.outcome && <option value="">{t("common.a_choisir")}</option>}
              {DECISION_OUTCOMES.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </NativeSelect>
            <Button asChild variant="ghost" size="sm">
              <a href={interviewHref(c.interview_id, "cases")} aria-label={`Ouvrir ${c.name || c.decision_id}`}>
                {t("common.ouvrir")}
              </a>
            </Button>
          </li>
        ))}
      </ul>
    </div>
  )
}
