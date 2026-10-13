// One card of the arbitration (design/macos/Revoir.dc.html, SPEC F-69, F-70):
// the unit with the question before it, the frozen human codes against those of
// the model, four choices on the keys 1 to 4, a reason, and Enter to save.

import { useEffect, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Check } from "lucide-react"

import { ChipList, CodeChip } from "@/components/bits"
import { CodePicker } from "@/components/code-picker"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { useCodeName } from "@/lib/code-names"
import { familyOf } from "@/lib/codes"
import { isTypingTarget } from "@/lib/coder-keys"
import { dateTime } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { candidateView } from "@/lib/review"
import {
  AGREEMENT_LABEL,
  availableChoices,
  compareRows,
  decisionKey,
  draftFromDecision,
  draftProblem,
  finalCodesOf,
  newDraft,
  PROBLEM_TEXT,
  rationaleNeeded,
  toChoice,
  type DecisionDraft,
  type DecisionKind,
} from "@/lib/review-model"
import { unitHref } from "@/lib/router"
import type { Arbitration, Code, ReviewItem, Unit } from "@/lib/types"

import "./revoir.css"

const shortUnit = (unitId: string) => `U${unitId.split("-U").pop()}`

function kindLabel(kind: DecisionKind, sample: boolean): { title: string; hint?: string } {
  switch (kind) {
    case "human":
      return sample ? { title: "Confirmer l'accord" } : { title: "Garder le mien" }
    case "ai":
      return { title: "Prendre celui du modèle" }
    case "merge":
      return { title: "Fusionner", hint: "Vos codes et ceux du modèle, ensemble" }
    case "other":
      return sample ? { title: "Corriger", hint: "Choisir les codes" } : { title: "Autre code", hint: "Choisir les codes" }
  }
}

/** Radios and fields do not take the keys of the card; text fields and buttons keep theirs. */
function takesKeys(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false
  if (el instanceof HTMLInputElement && (el.type === "radio" || el.type === "checkbox")) return false
  return isTypingTarget(el) || el.tagName === "BUTTON" || el.tagName === "A"
}

export function DecisionSummary({ a, item }: { a: Arbitration; item: ReviewItem }) {
  const what =
    a.choice.type === "human" ? (item.kind === "disagreement" ? "Votre lecture gardée" : "Accord confirmé") : a.choice.type === "ai" ? `Lecture du modèle retenue (${a.choice.run_label})` : "Autre lecture"
  return (
    <div className="text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{what}</span>
        <ChipList codes={a.final_codes} />
      </div>
      {a.rationale && <p style={{ margin: "4px 0 0" }}>{a.rationale}</p>}
      <p className="small l2" style={{ margin: "2px 0 0" }}>
        Décidé par {a.decided_by}, {dateTime(a.decided_at)}
      </p>
    </div>
  )
}

export function DecisionCard({
  id,
  item,
  runs,
  book,
  question,
  position,
  hasPrev,
  onPrev,
  onNext,
}: {
  id: string
  item: ReviewItem
  runs: string[]
  book: Code[]
  question: Unit | null
  /** "désaccord 8 sur 9" */
  position: string
  hasPrev: boolean
  onPrev: () => void
  /** Moves on after a save; `fromUnit` is the card that was just saved. */
  onNext: () => void
}) {
  const qc = useQueryClient()
  const nameOf = useCodeName()
  const sample = item.kind !== "disagreement"
  const kinds = availableChoices(item)
  const decided = item.decision
  const [editing, setEditing] = useState(!decided)
  const [draft, setDraft] = useState<DecisionDraft>(() => draftFromDecision(item, runs) ?? newDraft(item, runs))
  const [attempted, setAttempted] = useState(false)
  const view = candidateView(item)
  const rows = compareRows(item, runs)
  const problem = draftProblem(item, draft)
  const needed = rationaleNeeded(item, draft)
  const final = finalCodesOf(item, draft)

  const save = useMutation({
    mutationFn: () => api.decide(id, item.unit_id, toChoice(draft), final, draft.rationale.trim()),
    onSuccess: () => {
      toast.success(`${shortUnit(item.unit_id)} décidée`)
      setEditing(false)
      setAttempted(false)
      qc.invalidateQueries({ queryKey: ["review", id] })
      invalidateInterview(qc, id)
      onNext()
    },
    onError: (e) => reportError(e, "Décision refusée"),
  })

  const trySave = () => {
    setAttempted(true)
    if (!problem && !save.isPending) save.mutate()
  }

  // The keys of the card, on the whole window while the card is in edit mode.
  useEffect(() => {
    if (!editing) return
    const onKey = (e: KeyboardEvent) => {
      const act = decisionKey(e, { typing: takesKeys(e.target), kinds })
      if (!act) return
      e.preventDefault()
      if (act.type === "choose") setDraft((d) => ({ ...d, kind: act.kind }))
      else if (act.type === "save") trySave()
      else if (act.type === "next") onNext()
      else if (hasPrev) onPrev()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  })

  const allowed = book.filter((c) => c.active && !c.merged_into && c.origin !== "legacy")

  return (
    <article aria-label={`Unité ${shortUnit(item.unit_id)}`} className="flex flex-col gap-3.5">
      <div className="r8" style={{ justifyContent: "space-between" }}>
        <div className="r8">
          <span className="mono b" style={{ fontSize: 15 }}>
            {shortUnit(item.unit_id)}
          </span>
          <span className="small l2">
            {item.unit.timestamp} · {position}
          </span>
          {view.revised && <span className="badge">révisé depuis la capture</span>}
          {decided && <span className="badge">{sample ? "revue" : "décidé"}</span>}
        </div>
        <a className="small" href={unitHref(item.unit_id, 1)}>
          Ouvrir dans l'entretien
        </a>
      </div>

      {question && (
        <div className="small l2">
          Après l'intervieweur : « <span data-no-i18n>{question.text}</span> »
        </div>
      )}
      <p className="rv-quote verbatim">{item.unit.text}</p>

      <table className="tbl rv-cmp">
        <thead>
          <tr>
            <th>Code</th>
            <th className="c">Vous (figé)</th>
            {runs.map((r) => (
              <th key={r} className="c">
                Modèle {r}
              </th>
            ))}
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 && (
            <tr>
              <td colSpan={runs.length + 3} className="l2 small">
                Ni vous ni le modèle n'avez posé de code.
              </td>
            </tr>
          )}
          {rows.map((r) => (
            <tr key={r.code}>
              <td>
                <CodeChip code={r.code} family={familyOf(r.code, book)} />
              </td>
              <td className="c">{r.human ? <Yes /> : <span className="rv-no">–</span>}</td>
              {runs.map((run) => (
                <td key={run} className="c">
                  {r.ai[run] ? <Yes /> : <span className="rv-no">–</span>}
                </td>
              ))}
              <td className="l2 small">{r.status === "both" ? AGREEMENT_LABEL.both : <span className="badge">{AGREEMENT_LABEL[r.status]}</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
      {view.revised && (
        <p className="small l2" style={{ margin: 0 }}>
          Votre codage a changé depuis la capture : <ChipList codes={view.current} empty="aucun code" />. La comparaison porte sur le codage figé.
        </p>
      )}

      {decided && !editing ? (
        <div className="grp">
          <div className="row">
            <DecisionSummary a={decided} item={item} />
            <span style={{ marginLeft: "auto" }} className="r8">
              <Button
                variant="outline"
                onClick={() => {
                  setDraft(draftFromDecision(item, runs) ?? newDraft(item, runs))
                  setEditing(true)
                }}
              >
                Modifier
              </Button>
            </span>
          </div>
        </div>
      ) : (
        <>
          <div className="grp" role="radiogroup" aria-label="Décision">
            {kinds.map((k, i) => {
              const l = kindLabel(k, sample)
              const codes = finalCodesOf(item, { ...draft, kind: k })
              return (
                <label key={k} className="row rv-rad">
                  <input type="radio" name={`decision-${item.unit_id}`} checked={draft.kind === k} onChange={() => setDraft((d) => ({ ...d, kind: k }))} />
                  <span className="k">
                    {l.title}
                    <small>{k === "other" ? l.hint : codes.length ? codes.map(nameOf).join(", ") : l.hint ?? "aucun code"}</small>
                  </span>
                  {k === "ai" && runs.length > 1 && (
                    <NativeSelect
                      aria-label="Run du modèle"
                      value={draft.runLabel ?? ""}
                      style={{ width: 90 }}
                      onChange={(e) => setDraft((d) => ({ ...d, kind: "ai", runLabel: e.target.value }))}
                    >
                      {runs.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </NativeSelect>
                  )}
                  <span className="kbd" aria-hidden>
                    {i + 1}
                  </span>
                </label>
              )
            })}
          </div>
          {draft.kind === "other" && (
            <CodePicker allowed={allowed} value={draft.custom} onChange={(custom) => setDraft((d) => ({ ...d, custom }))} phase={2} phase1Contexts={[]} />
          )}
          <label className="hs" htmlFor={`rv-just-${item.unit_id}`}>
            {needed ? (sample ? "Justification, obligatoire pour corriger" : "Justification, obligatoire pour un désaccord") : "Justification, facultative"}
          </label>
          <Textarea
            id={`rv-just-${item.unit_id}`}
            rows={3}
            placeholder="Pourquoi cette décision ? Elle est datée et conservée à côté des deux codages initiaux."
            value={draft.rationale}
            aria-invalid={attempted && problem === "rationale"}
            aria-describedby={attempted && problem ? `rv-problem-${item.unit_id}` : undefined}
            onChange={(e) => setDraft((d) => ({ ...d, rationale: e.target.value }))}
          />
          {attempted && problem && (
            <p id={`rv-problem-${item.unit_id}`} role="alert" className="rv-err" style={{ margin: 0 }}>
              {PROBLEM_TEXT[problem]}
            </p>
          )}
        </>
      )}

      <div className="r8">
        <Button variant="outline" disabled={!hasPrev} onClick={onPrev}>
          Précédent
        </Button>
        <span className="small l2">Votre codage, celui du modèle et votre décision restent trois objets distincts.</span>
        {decided && !editing ? (
          <Button variant="outline" size="lg" style={{ marginLeft: "auto" }} onClick={onNext}>
            Suivant
          </Button>
        ) : (
          <Button variant="default" size="lg" style={{ marginLeft: "auto" }} disabled={save.isPending} onClick={trySave}>
            {save.isPending ? "Enregistrement…" : "Enregistrer et suivant"}
          </Button>
        )}
      </div>
    </article>
  )
}

function Yes() {
  return (
    <span className="rv-yes">
      <Check size={13} role="img" aria-label="Oui" />
    </span>
  )
}
