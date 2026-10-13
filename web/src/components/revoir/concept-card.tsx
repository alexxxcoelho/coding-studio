// A concept the model found outside the codebook (guide, action 8). It is a
// candidate, never a code applied alone: create a code, attach it to an existing
// code, or discard it, always with a written reason that is dated.

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Textarea } from "@/components/ui/textarea"
import { t } from "@/i18n"
import { codeName } from "@/lib/code-tree"
import { dateTime } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { CANDIDATE_PROBLEM_TEXT, candidateProblem, type CandidateAction } from "@/lib/review-model"
import { reviewApi, type ConceptCandidate, type ConceptDecisionBody } from "@/lib/review-api"
import { unitHref } from "@/lib/router"
import type { CodeNode, Unit } from "@/lib/types"

import "./revoir.css"

/** The text of a unit with the model's quotations marked. */
function Marked({ text, quotes }: { text: string; quotes: string[] }) {
  const q = quotes.find((x) => x && text.includes(x))
  if (!q) return <>{text}</>
  const at = text.indexOf(q)
  return (
    <>
      {text.slice(0, at)}
      <mark>{q}</mark>
      {text.slice(at + q.length)}
    </>
  )
}

const DECISION_LABEL = { get create() { return t("revoir.concept_card.code_cree") }, get map() { return t("revoir.concept_card.rattache_a_un_code") }, get reject() { return t("revoir.concept_card.ecarte") } } as const

export function ConceptCard({
  id,
  candidate,
  units,
  codes,
}: {
  id: string
  candidate: ConceptCandidate
  units: Map<string, Unit>
  /** Existing codes a concept can be attached to. */
  codes: CodeNode[]
}) {
  const qc = useQueryClient()
  const [kind, setKind] = useState<CandidateAction["kind"] | null>(null)
  const [name, setName] = useState(candidate.label)
  const [target, setTarget] = useState("")
  const [rationale, setRationale] = useState("")
  const [attempted, setAttempted] = useState(false)

  const action: CandidateAction | null = kind === "create" ? { kind, name } : kind === "map" ? { kind, code: target } : kind === "reject" ? { kind } : null
  const problem = action ? candidateProblem(action, rationale) : null

  const save = useMutation({
    mutationFn: () => {
      const body: ConceptDecisionBody =
        action!.kind === "create"
          ? { action: "create", new_code_name: name.trim(), rationale: rationale.trim() }
          : action!.kind === "map"
            ? { action: "map", code_id: target, rationale: rationale.trim() }
            : { action: "reject", rationale: rationale.trim() }
      return reviewApi.decideConcept(id, candidate.candidate_id, body)
    },
    onSuccess: () => {
      toast.success(t("revoir.concept_card.concept_label_traite", { label: candidate.label }))
      setKind(null)
      setRationale("")
      setAttempted(false)
      void invalidateInterview(qc, id)
      qc.invalidateQueries({ queryKey: ["codes"] })
      qc.invalidateQueries({ queryKey: ["codebook"] })
    },
    onError: (e) => reportError(e, t("common.decision_refusee")),
  })

  const submit = () => {
    setAttempted(true)
    if (!problem) save.mutate()
  }
  const d = candidate.decision
  const nameOf = (codeId: string) => {
    const c = codes.find((x) => x.code_id === codeId)
    return c ? codeName(c) : codeId
  }

  return (
    <article className="flex flex-col gap-3.5" aria-label={`Concept ${candidate.label}`}>
      <div>
        <div className="hs">{t("revoir.concept_card.concept_propose_par_la_revue")}{candidate.run_label ? `, ${candidate.run_label}` : ""}</div>
        <p style={{ fontSize: 17, fontWeight: 600, margin: "2px 0" }}>« {candidate.label} »</p>
        {candidate.description && <p className="small l2" style={{ margin: 0 }}>{candidate.description}</p>}
        <p className="small l2" style={{ margin: "4px 0 0" }}>{t("revoir.concept_card.jamais_applique_sans_decision_motive")}</p>
      </div>

      <div>
        <div className="hs" style={{ marginBottom: 4 }}>
          {t("revoir.concept_card.items_cette_proposition", { items: t("revoir.concept_card.n_unites_motivent", { n: candidate.unit_ids.length }) })}
        </div>
        <div className="flex flex-col gap-2">
          {candidate.unit_ids.map((uid) => {
            const quotes = candidate.evidence.filter((e) => e.unit_id === uid).map((e) => e.quote)
            return (
              <div key={uid}>
                <a className="mono" href={unitHref(uid, 2)}>
                  {uid.split("-U").pop() ? `U${uid.split("-U").pop()}` : uid}
                </a>
                {units.get(uid) ? (
                  <p className="rv-quote verbatim" style={{ marginTop: 4 }}>
                    <Marked text={units.get(uid)!.text} quotes={quotes} />
                  </p>
                ) : (
                  quotes.map((q) => (
                    <p key={q} className="rv-quote verbatim" style={{ marginTop: 4 }}>
                      « {q} »
                    </p>
                  ))
                )}
              </div>
            )
          })}
        </div>
        {candidate.evidence.length > 0 && <p className="small l2" style={{ margin: "6px 0 0" }}>{t("revoir.concept_card.les_citations_du_modele_sont")}</p>}
      </div>

      {d && !kind ? (
        <div className="grp">
          {[...candidate.history].reverse().map((h, i) => (
            <div key={h.decided_at + i} className="row">
              <span className="k">
                {i === 0 ? "" : t("revoir.concept_card.decision_precedente")}
                {DECISION_LABEL[h.action]}
                {h.code_id ? ` : ${nameOf(h.code_id)}` : ""}
                <small>
                  {h.rationale} · {dateTime(h.decided_at)}
                  {h.decided_by ? ` · ${h.decided_by}` : ""}
                </small>
              </span>
              {i === 0 && (
                <Button variant="outline" onClick={() => setKind(h.action === "create" ? "map" : h.action)}>
                  {t("revoir.concept_card.decider_de_nouveau")}
                </Button>
              )}
            </div>
          ))}
        </div>
      ) : (
        <>
          <div className="r8" role="group" aria-label={t("revoir.concept_card.decision_sur_le_concept")}>
            <Button variant={kind === "create" ? "default" : "outline"} aria-pressed={kind === "create"} onClick={() => setKind("create")}>
              {t("revoir.concept_card.creer_un_code")}
            </Button>
            <Button variant={kind === "map" ? "default" : "outline"} aria-pressed={kind === "map"} onClick={() => setKind("map")}>
              {t("common.rattacher_a_un_code_existant")}
            </Button>
            <Button variant={kind === "reject" ? "default" : "outline"} aria-pressed={kind === "reject"} onClick={() => setKind("reject")}>
              {t("revoir.concept_card.ecarter")}
            </Button>
          </div>
          {kind === "create" && (
            <label className="flex flex-col gap-1 text-xs">
              <span className="hs">{t("revoir.concept_card.nom_du_nouveau_code")}</span>
              <Input value={name} onChange={(e) => setName(e.target.value)} />
            </label>
          )}
          {kind === "map" && (
            <label className="flex flex-col gap-1 text-xs">
              <span className="hs">{t("revoir.concept_card.code_existant")}</span>
              <NativeSelect value={target} onChange={(e) => setTarget(e.target.value)}>
                <option value="">{t("common.choisir_un_code")}</option>
                {codes.map((c) => (
                  <option key={c.code_id} value={c.code_id}>
                    {codeName(c)}
                  </option>
                ))}
              </NativeSelect>
            </label>
          )}
          {kind && (
            <>
              <label className="hs" htmlFor={`rv-cj-${candidate.candidate_id}`}>
                {t("revoir.concept_card.justification_obligatoire")}
              </label>
              <Textarea
                id={`rv-cj-${candidate.candidate_id}`}
                rows={2}
                placeholder={t("revoir.concept_card.pourquoi_cette_decision_elle_est")}
                value={rationale}
                aria-invalid={attempted && problem === "rationale"}
                onChange={(e) => setRationale(e.target.value)}
              />
              {attempted && problem && (
                <p role="alert" className="rv-err" style={{ margin: 0 }}>
                  {CANDIDATE_PROBLEM_TEXT[problem]}
                </p>
              )}
              <div className="r8">
                <Button variant="default" size="lg" disabled={save.isPending} onClick={submit}>
                  {t("revoir.concept_card.enregistrer_la_decision")}
                </Button>
                <Button variant="ghost" onClick={() => setKind(null)}>
                  {t("common.annuler")}
                </Button>
              </div>
            </>
          )}
        </>
      )}
    </article>
  )
}
