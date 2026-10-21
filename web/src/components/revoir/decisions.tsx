// Revoir, part "Décisions" (guide, actions 6 to 8): the queue at the left
// (disagreements, sampled agreements, concepts outside the codebook), the card
// of the selected one at the right. Review is optional: without a run, the part
// explains it and points to the guide.

import { useMemo, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { Check, Download } from "lucide-react"
import { toast } from "sonner"
import { downloadText } from "@/lib/download"
import { disagreementFilename, disagreementMarkdown } from "@/lib/review-markdown"

import { JsonImportDialog } from "@/components/json-import-dialog"
import { Banner, EmptyState } from "@/components/bits"
import { ConceptCard } from "@/components/revoir/concept-card"
import { DecisionCard } from "@/components/revoir/decision-card"
import { Button } from "@/components/ui/button"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { isApplicable } from "@/lib/code-tree"
import { shortHash } from "@/lib/format"
import { useCodes, useUnits } from "@/lib/hooks"
import { reviewProgress } from "@/lib/review"
import { filterQueue, nextInQueue, precedingQuestion, type QueueFilter } from "@/lib/review-model"
import { reviewApi } from "@/lib/review-api"
import { interviewHref } from "@/lib/router"
import type { AiRun, ReviewAnswer, ReviewItem } from "@/lib/types"

import "./revoir.css"

const GUIDE = "#/guide/revoir"
const shortUnit = (unitId: string) => `U${unitId.split("-U").pop()}`

type Selection = { kind: "unit" | "concept"; id: string }

const FILTERS: { key: QueueFilter; label: string }[] = [
  { key: "remaining", get label() { return t("revoir.decisions.a_decider") } },
  { key: "decided", get label() { return t("revoir.decisions.decides") } },
  { key: "all", get label() { return t("common.tous") } },
]

export function Decisions({ id, review, runList }: { id: string; review: ReviewAnswer; runList: AiRun[] }) {
  const unitsQ = useUnits(id)
  const codesQ = useCodes()
  const bookQ = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const conceptsQ = useQuery({ queryKey: ["concepts", id], queryFn: () => reviewApi.conceptCandidates(id) })
  const [importOpen, setImportOpen] = useState(false)
  const [filter, setFilter] = useState<QueueFilter>("remaining")
  const [sel, setSel] = useState<Selection | null>(null)

  const items = useMemo(() => review.items ?? [], [review.items])
  const runs = review.runs
  const units = useMemo(() => (unitsQ.data ?? []).map((r) => r.unit), [unitsQ.data])
  const unitMap = useMemo(() => new Map(units.map((u) => [u.unit_id, u])), [units])
  const candidates = useMemo(() => conceptsQ.data ?? [], [conceptsQ.data])
  const applicable = useMemo(() => (codesQ.data?.codes ?? []).filter(isApplicable), [codesQ.data])

  if (runs.length === 0) return <NoReview id={id} started={runList.length > 0} />

  const prog = reviewProgress(items)
  const queue = filterQueue(items, filter)
  const disagreements = items.filter((i) => i.kind === "disagreement")
  const samples = items.filter((i) => i.kind !== "disagreement")
  const current: Selection | null = sel ?? (queue[0] ? { kind: "unit", id: queue[0].unit_id } : null)
  const currentItem: ReviewItem | undefined = current?.kind === "unit" ? items.find((i) => i.unit_id === current.id) : undefined
  const currentConcept = current?.kind === "concept" ? candidates.find((c) => c.candidate_id === current.id) : undefined
  const fallback = !current ? items[0] : undefined

  const show = (list: ReviewItem[]) => filterQueue(list, filter)
  const position = (item: ReviewItem): string => {
    const group = item.kind === "disagreement" ? disagreements : samples
    const ordered = filterQueue(group, "all")
    const n = ordered.findIndex((i) => i.unit_id === item.unit_id) + 1
    return `${item.kind === "disagreement" ? t("common.desaccord") : t("revoir.decisions.accord_tire_au_sort")} ${n} sur ${ordered.length}`
  }
  const go = (dir: 1 | -1) => {
    const target = nextInQueue(queue, currentItem?.unit_id ?? null, dir)
    if (target) setSel({ kind: "unit", id: target })
  }

  const card = currentItem ?? fallback
  const snaps = review.snapshots ?? []
  const completeRuns = runList.filter((r) => r.complete)

  return (
    <div className="content">
      <JsonImportDialog open={importOpen} onOpenChange={setImportOpen} endpoint={`/api/interviews/${encodeURIComponent(id)}/review/import`} title={t("json_import.arbitrations")} description={t("json_import.arbitrations_help")} />
      <aside className="rv-queue" aria-label={t("revoir.decisions.file_d_arbitrage")}>
        <div>
          <div className="hs" style={{ padding: "0 2px 4px" }}>
            {t("common.revue_ia")}
          </div>
          <Step ok title={t("revoir.decisions.codage_fige_prompt_genere")} sub={snaps.length ? `capture ${snaps[0].snapshot_id} · SHA-256 ${shortHash(snaps[0].sha256, 10)}` : undefined} />
          {completeRuns.map((r) => (
            <Step key={r.run_id} ok title={t("revoir.decisions.run_run_label_importe", { run_label: r.meta.run_label })} sub={`${r.meta.model_name || t("revoir.decisions.modele_non_nomme")} · complet`} />
          ))}
          <Step ok={prog.complete && prog.disagreements > 0} n={3} title={t("revoir.decisions.arbitrage")} sub={t("revoir.decisions.items_sur_disagreements", { items: t("revoir.decisions.n_decisions", { n: prog.disagreementsDecided }), disagreements: prog.disagreements })} />
        </div>

        <Button variant="outline" onClick={() => setImportOpen(true)}>{t("json_import.arbitrations")}</Button>
        <Button variant="outline" onClick={() => {
          if (!downloadText(disagreementFilename(id), disagreementMarkdown(id, review, bookQ.data?.codes))) {
            toast.error(t("review_export.failed"))
          }
        }}><Download size={14} aria-hidden />{t("review_export.download")}</Button>

        <div className="seg" role="group" aria-label={t("revoir.decisions.filtre_de_la_file")} style={{ alignSelf: "flex-start" }}>
          {FILTERS.map((f) => (
            <button key={f.key} type="button" className={`s${filter === f.key ? " on" : ""}`} aria-pressed={filter === f.key} onClick={() => setFilter(f.key)}>
              {f.label}
            </button>
          ))}
        </div>

        <QueueGroup title={t("revoir.decisions.desaccords")} count={`${prog.disagreementsDecided} / ${prog.disagreements}`} empty={disagreements.length === 0 ? t("revoir.decisions.aucun_desaccord") : filter === "remaining" ? t("revoir.decisions.tout_est_decide") : t("common.rien_a_afficher")} items={show(disagreements)} current={card?.unit_id} onPick={(u) => setSel({ kind: "unit", id: u })} />
        {samples.length > 0 && (
          <QueueGroup title={t("revoir.decisions.echantillon_d_accords")} count={`${prog.sampledReviewed} / ${prog.sampled}`} empty={filter === "remaining" ? t("revoir.decisions.tout_est_regarde") : t("common.rien_a_afficher")} items={show(samples)} current={card?.unit_id} onPick={(u) => setSel({ kind: "unit", id: u })} sample />
        )}
        {conceptsQ.data && candidates.length > 0 && (
          <div>
            <div className="rv-qh">
              <span className="hs">{t("revoir.decisions.concepts_hors_codebook")}</span>
              <span className="small l2">{candidates.length}</span>
            </div>
            <ul className="rv-ql">
              {candidates.map((c) => (
                <li key={c.candidate_id}>
                  <button type="button" className="rv-qi" aria-current={current?.kind === "concept" && current.id === c.candidate_id} onClick={() => setSel({ kind: "concept", id: c.candidate_id })}>
                    {c.label}
                    <span className="st">{c.decision ? t("revoir.decisions.traite") : t("revoir.decisions.a_trier")}</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </aside>

      <section className="rv-dec" aria-label={t("common.decision")}>
        {prog.complete && prog.disagreements > 0 && (
          <Banner tone="ok" title={t("revoir.decisions.tous_les_desaccords_sont_decides")}>
            {t("revoir.decisions.le_codage_final_reprend_vos")} <a href={interviewHref(id, "coding")}>{t("common.retourner_a_coder")}</a>
          </Banner>
        )}
        {items.length === 0 && !currentConcept && <p className="l2">{t("revoir.decisions.votre_codage_et_celui_du")}</p>}
        {currentConcept ? (
          <ConceptCard key={currentConcept.candidate_id} id={id} candidate={currentConcept} units={unitMap} codes={applicable} />
        ) : card ? (
          <DecisionCard
            key={card.unit_id}
            id={id}
            item={card}
            runs={runs}
            book={bookQ.data?.codes ?? []}
            question={precedingQuestion(units, card.unit_id)}
            position={position(card)}
            hasPrev={nextInQueue(queue, card.unit_id, -1) !== null}
            onPrev={() => go(-1)}
            onNext={() => {
              const target = nextInQueue(queue, card.unit_id, 1) ?? queue.find((i) => i.unit_id !== card.unit_id)?.unit_id ?? null
              if (target) setSel({ kind: "unit", id: target })
            }}
          />
        ) : null}
      </section>
    </div>
  )
}

function Step({ ok, n, title, sub }: { ok?: boolean; n?: number; title: string; sub?: string }) {
  return (
    <div className="rv-step">
      <span className={`ic${ok ? " ok" : ""}`}>{ok ? <Check size={10} aria-hidden /> : n}</span>
      <div>
        <div className="t">{title}</div>
        {sub && <div className="s">{sub}</div>}
      </div>
    </div>
  )
}

function QueueGroup({
  title,
  count,
  items,
  empty,
  current,
  onPick,
  sample,
}: {
  title: string
  count: string
  items: ReviewItem[]
  empty: string
  current: string | undefined
  onPick: (unitId: string) => void
  sample?: boolean
}) {
  return (
    <div>
      <div className="rv-qh">
        <span className="hs">{title}</span>
        <span className="small l2">{count}</span>
      </div>
      {items.length === 0 ? (
        <p className="small l2" style={{ margin: "0 8px" }}>
          {empty}
        </p>
      ) : (
        <ul className="rv-ql">
          {items.map((i) => (
            <li key={i.unit_id}>
              <button type="button" className="rv-qi" aria-current={current === i.unit_id} onClick={() => onPick(i.unit_id)}>
                <span className="mono" style={{ fontSize: 12.5 }}>
                  {shortUnit(i.unit_id)}
                </span>
                <span className="st">{i.decision ? (sample ? t("revoir.decisions.regarde") : t("common.decide")) : sample ? t("revoir.decisions.a_confirmer") : t("revoir.decisions.a_decider_2")}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Review is optional: this is what the part says without any run. */
export function NoReview({ id, started }: { id: string; started: boolean }) {
  return (
    <div className="page">
      <div className="rv-col">
        <EmptyState
          title={started ? t("revoir.decisions.aucun_run_complet_pour_l") : t("revoir.decisions.la_revue_ia_est_facultative")}
          action={
            <div className="r8">
              <Button variant="default" asChild>
                <a href={interviewHref(id, "ai")}>{started ? t("revoir.decisions.terminer_l_import_des_reponses") : t("revoir.decisions.preparer_une_revue_ia")}</a>
              </Button>
              <Button variant="outline" asChild>
                <a href={GUIDE}>{t("common.lire_la_section_revoir_du")}</a>
              </Button>
            </div>
          }
        >
          {started
            ? t("revoir.decisions.les_desaccords_apparaissent_ici_quan")
            : t("revoir.decisions.vous_pouvez_passer_a_l")}
        </EmptyState>
      </div>
    </div>
  )
}
