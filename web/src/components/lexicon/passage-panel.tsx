// Lexical help for one passage: its YAKE keyphrases and stems with a
// provisional label (SPEC F-50), and the passages of other interviews that
// share its vocabulary (SPEC F-53). Statistical and deterministic: nothing
// here calls a model.

import { useMemo } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { GuideLink, ToolIntro } from "@/components/lexicon/parts"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { codeName, isApplicable } from "@/lib/code-tree"
import { invalidatePassages, useCodes } from "@/lib/hooks"
import { EMPTY } from "@/lib/lexicon-copy"
import { lexiconApi, type SimilarItem } from "@/lib/lexicon-api"
import { scoreText } from "@/lib/lexicon"
import { reportError } from "@/lib/notify"
import { interviewHref } from "@/lib/router"

const MAX_TAGS = 6
const QUOTE = 240

function shortUnit(unitId: string): string {
  return unitId.replace(/^.*-(U\d+)$/, "$1")
}

/** Keyphrases, stems and provisional label of the selected passage. */
export function PassageLexicon({ passageId, onUseLabel }: { passageId: string | null; onUseLabel?: (label: string) => void }) {
  const q = useQuery({
    queryKey: ["lexicon", "passage", passageId],
    queryFn: () => lexiconApi.passage(passageId!),
    enabled: passageId !== null,
  })
  if (passageId === null) return <ToolIntro title={EMPTY.passage.title}>{EMPTY.passage.text}</ToolIntro>
  if (q.isLoading) return <Skeleton className="h-20 w-full" />
  if (q.isError) return <p role="alert" className="note-err">{q.error.message}</p>
  const d = q.data!
  return (
    <div className="flex flex-col gap-2" data-testid="passage-lexicon">
      <div className="hs">{t("lexicon.passage_panel.expressions_cles")}</div>
      {d.keyphrases.length === 0 ? (
        <p className="small l2" style={{ margin: 0 }}>
          {t("lexicon.passage_panel.ce_passage_est_trop_court")}
        </p>
      ) : (
        <div className="r8">
          {d.keyphrases.slice(0, MAX_TAGS).map((k) => (
            <span key={k.phrase} className="tag" data-no-i18n title={t("lexicon.passage_panel.score_yake_tolocalestring_plus_bas", { toLocaleString: k.score.toLocaleString("fr-CH", { maximumFractionDigits: 4 }) })}>
              {k.phrase}
            </span>
          ))}
        </div>
      )}
      {d.stems.length > 0 && (
        <div className="small l2">
          {t("lexicon.passage_panel.racines")} <span data-no-i18n>{d.stems.map((s) => s.stem).join(", ")}</span>
        </div>
      )}
      {onUseLabel && d.label && (
        <Button type="button" variant="outline" onClick={() => onUseLabel(d.label)}>
          {t("lexicon.passage_panel.utiliser_label_comme_libelle_proviso", { label: d.label })}
        </Button>
      )}
      <p className="small l2" style={{ margin: 0 }}>
        {t("lexicon.passage_panel.extraction_statistique_deterministe")}
      </p>
    </div>
  )
}

/** Top similar passages across interviews, with score, shared terms, "Coder avec…" and a link to Coder. */
export function SimilarPassages({ passageId }: { passageId: string | null }) {
  const q = useQuery({
    queryKey: ["lexicon", "similar", passageId],
    queryFn: () => lexiconApi.similar(passageId!, { k: 8 }),
    enabled: passageId !== null,
  })
  if (passageId === null) return null
  if (q.isLoading) return <Skeleton className="h-24 w-full" />
  if (q.isError) return <p role="alert" className="note-err">{q.error.message}</p>
  const items = q.data!.items
  return (
    <div className="flex flex-col gap-2" data-testid="similar-passages">
      {items.length === 0 ? (
        <ToolIntro title={EMPTY.similar.title}>{EMPTY.similar.text}</ToolIntro>
      ) : (
        <>
          {items.map((it) => (
            <SimilarCard key={`${it.interview_id}:${it.unit_id}`} item={it} />
          ))}
          <span className="small l2">{q.data!.note || t("lexicon.passage_panel.proximite_lexicale_tf_idf_sur")}</span>
          <GuideLink />
        </>
      )}
    </div>
  )
}

function SimilarCard({ item }: { item: SimilarItem }) {
  const qc = useQueryClient()
  const codesQ = useCodes()
  const usable = useMemo(() => (codesQ.data?.codes ?? []).filter(isApplicable), [codesQ.data])
  const code = useMutation({
    mutationFn: (codeId: string) => api.createPassage(item.interview_id, { unit_id: item.unit_id, code: codeId }),
    onSuccess: async (_r, codeId) => {
      toast.success(t("lexicon.passage_panel.interview_id_shortunit_code_avec", { interview_id: item.interview_id, shortUnit: shortUnit(item.unit_id), v: usable.find((c) => c.code_id === codeId)?.name ?? codeId }))
      await invalidatePassages(qc, item.interview_id)
      await qc.invalidateQueries({ queryKey: ["lexicon", "similar"] })
    },
    onError: (e) => reportError(e, t("common.code_non_pose")),
  })
  const shared = item.shared_terms.map((t) => t.form).join(", ")
  const text = item.text.length > QUOTE ? `${item.text.slice(0, QUOTE).trimEnd()}…` : item.text
  return (
    <article className="grp" style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 4, fontSize: 12.5, lineHeight: 1.4 }} aria-label={`${item.interview_id} ${shortUnit(item.unit_id)}`}>
      <span className="r8">
        <a className="mono b" href={interviewHref(item.interview_id, "coding", 1, item.unit_id)} title={t("common.ouvrir_dans_coder")}>
          {item.interview_id} {shortUnit(item.unit_id)}
        </a>
        <span className="small l2" title={t("lexicon.passage_panel.similarite_cosinus_de_0_a")}>
          {scoreText(item.score)}
        </span>
      </span>
      <span data-no-i18n>« {text} »</span>
      {shared && (
        <span className="small l2">
          {t("lexicon.passage_panel.mots_communs")} <span data-no-i18n>{shared}</span>
        </span>
      )}
      {item.coded_with.length > 0 && (
        <span className="r8">
          {item.coded_with.map((c) => (
            <span key={c.code_id} className="tag">
              {c.name}
            </span>
          ))}
        </span>
      )}
      <span className="r8">
        <NativeSelect
          aria-label={t("lexicon.passage_panel.coder_interview_id_shortunit_avec", { interview_id: item.interview_id, shortUnit: shortUnit(item.unit_id) })}
          style={{ width: 170 }}
          value=""
          disabled={code.isPending}
          onChange={(e) => e.target.value && code.mutate(e.target.value)}
        >
          <option value="">{t("common.coder_avec")}</option>
          {usable.map((c) => (
            <option key={c.code_id} value={c.code_id}>
              {codeName(c)}
            </option>
          ))}
        </NativeSelect>
        <a className="small" href={interviewHref(item.interview_id, "coding", 1, item.unit_id)}>
          {t("common.ouvrir_dans_coder")}
        </a>
      </span>
    </article>
  )
}
