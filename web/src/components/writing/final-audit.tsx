// Audit final (guide, "Faire l'audit final de trois affirmations"): the tool
// draws claims at random from the written text; for each one the researcher
// follows the whole path, from the sentence to the theme, the codes and the
// exact passages, then gives a verdict: étayée, à nuancer or à retirer.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { PassageCard } from "@/components/analysis/shared"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { t } from "@/i18n"
import { analysisApi, AUDIT_VERDICTS, type AuditClaim, type AuditVerdict, type FinalAudit } from "@/lib/analysis-api"
import { akeys } from "@/lib/analysis-hooks"
import { auditProgress, VERDICT_LABEL, verdictCheck } from "@/lib/audit-verdict"
import { reportError } from "@/lib/notify"
import { cn } from "@/lib/utils"
import type { ListItem } from "@/lib/types"

function Gap({ text }: { text: string }) {
  return (
    <li className="note-warn" role="alert">
      {text}
    </li>
  )
}

function ClaimCard({ audit, claim, linkTypes, onJudged }: { audit: FinalAudit; claim: AuditClaim; linkTypes: ListItem[]; onJudged: (a: FinalAudit) => void }) {
  const [verdict, setVerdict] = useState<AuditVerdict | null>(claim.verdict?.verdict ?? null)
  const [note, setNote] = useState(claim.verdict?.note ?? "")
  const check = verdict ? verdictCheck(claim, verdict, note) : { ok: false }
  const save = useMutation({
    mutationFn: () => analysisApi.setAuditVerdict(audit.audit_id, claim.claim_key, verdict!, note.trim()),
    onSuccess: onJudged,
    onError: (e) => reportError(e, t("writing.final_audit.verdict_non_enregistre")),
  })
  const unchanged = !!claim.verdict && claim.verdict.verdict === verdict && claim.verdict.note === note.trim()
  const typeLabel = (id: string) => linkTypes.find((t) => t.id === id)?.label ?? id

  return (
    <article className="grp flex flex-col gap-3 p-3" aria-label={`Affirmation : ${claim.sentence}`}>
      <header className="flex flex-col gap-0.5">
        <span className="hs">{claim.part_title}</span>
        <q className="font-serif text-base leading-snug [quotes:none]">{claim.sentence}</q>
      </header>

      <ol className="flex flex-col gap-2 text-sm" aria-label={t("writing.final_audit.chemin_de_l_affirmation_a")}>
        <li>
          <span className="hs">{t("common.theme")}</span>
          <div>{claim.theme ? claim.theme.title : claim.rq_id ? t("writing.final_audit.synthese_de_la_question_rq", { rq_id: claim.rq_id }) : t("writing.final_audit.aucun_theme")}</div>
        </li>
        <li>
          <span className="hs">{t("common.codes")}</span>
          <div className="flex flex-wrap gap-1">
            {claim.codes.length === 0 ? <span className="text-muted-foreground">{t("common.aucun")}</span> : claim.codes.map((c) => <span key={c.code_id} className="badge">{c.name}</span>)}
          </div>
        </li>
        <li className="flex flex-col gap-1.5">
          <span className="hs">{t("writing.final_audit.passages_lies")}</span>
          {claim.links.length === 0 && <span className="text-muted-foreground">{t("writing.final_audit.aucun_passage_lie_a_cette")}</span>}
          {claim.links.map((l) => (
            <PassageCard
              key={l.link_id}
              passage={l.passage}
              top={<span className="bg-fill2 rounded-full px-2 py-0.5 text-[11.5px] font-medium">{typeLabel(l.link_type)}</span>}
              foot={
                <>
                  {l.interview_version && <span className="text-muted-foreground text-[11.5px]">{t("writing.final_audit.version_de_l_entretien_interview", { interview_version: l.interview_version })}</span>}
                  {l.note && <span className="text-[11.5px]">{t("common.note_note", { note: l.note })}</span>}
                </>
              }
            />
          ))}
        </li>
      </ol>

      {claim.gaps.length > 0 && (
        <ul className="flex flex-col gap-1.5" aria-label={t("writing.final_audit.maillons_manquants")}>
          {claim.gaps.map((g) => (
            <Gap key={g.kind + g.message} text={g.message} />
          ))}
        </ul>
      )}

      <fieldset className="flex flex-col gap-2">
        <legend className="hs mb-1">{t("writing.final_audit.verdict")}</legend>
        <div className="flex flex-wrap gap-4">
          {AUDIT_VERDICTS.map((v) => (
            <label key={v} className="flex items-center gap-1.5 text-sm">
              <input type="radio" name={`verdict-${claim.claim_key}`} checked={verdict === v} onChange={() => setVerdict(v)} />
              {VERDICT_LABEL[v]}
            </label>
          ))}
        </div>
        <label className="flex flex-col gap-1">
          <span className="hs">{t("writing.final_audit.note_du_verdict")}</span>
          <Textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} placeholder={t("writing.final_audit.ce_qu_il_faut_nuancer")} />
        </label>
        {verdict && !check.ok && "reason" in check && check.reason && (
          <p role="alert" className="text-warn text-xs">
            {check.reason}
          </p>
        )}
        <div className="flex items-center gap-3">
          <Button size="sm" disabled={!verdict || !check.ok || save.isPending || unchanged} onClick={() => save.mutate()}>
            {t("writing.final_audit.enregistrer_le_verdict")}
          </Button>
          {claim.verdict && (
            <span className={cn("text-xs", claim.verdict.verdict === "supported" ? "text-ok" : "text-warn")}>{t("writing.final_audit.verdict_enregistre_v", { v: VERDICT_LABEL[claim.verdict.verdict] })}</span>
          )}
        </div>
      </fieldset>
    </article>
  )
}

export function FinalAuditTab({ linkTypes }: { linkTypes: ListItem[] }) {
  const qc = useQueryClient()
  const latest = useQuery({ queryKey: akeys.finalAudit, queryFn: analysisApi.latestFinalAudit })
  const draw = useMutation({
    mutationFn: () => analysisApi.drawFinalAudit(3),
    onSuccess: (a) => qc.setQueryData(akeys.finalAudit, a),
    onError: (e) => reportError(e, t("writing.final_audit.tirage_impossible")),
  })
  const audit = latest.data ?? null
  const progress = audit ? auditProgress(audit.claims) : null

  return (
    <div className="mx-auto flex w-full max-w-[760px] min-w-0 flex-col gap-4">
      <p className="text-muted-foreground max-w-prose text-sm">
        {t("writing.final_audit.avant_la_remise_verifiez_que")}
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={() => draw.mutate()} disabled={draw.isPending}>
          {audit ? t("writing.final_audit.relancer_un_tirage") : t("writing.final_audit.tirer_trois_affirmations")}
        </Button>
        {audit && progress && (
          <span className="text-muted-foreground text-sm" role="status">
            {t("writing.final_audit.judged_sur_total_jugees", { judged: progress.judged, total: progress.total })}
            {progress.complete ? t("writing.final_audit.le_resultat_va_dans_l") : ""}
          </span>
        )}
      </div>
      {audit && audit.claims.length < audit.n && audit.claims.length > 0 && (
        <p className="text-muted-foreground text-sm">{t("writing.final_audit.le_texte_ne_contient_que", { n: audit.claims.length })}</p>
      )}
      {latest.isLoading && <Skeleton className="h-40 w-full" />}
      {audit && audit.claims.length === 0 && <p className="text-muted-foreground text-sm">{t("writing.final_audit.aucune_affirmation_a_tirer_ecrivez")}</p>}
      {audit?.claims.map((c) => (
        <ClaimCard key={`${audit.audit_id}:${c.claim_key}:${c.verdict?.at ?? ""}`} audit={audit} claim={c} linkTypes={linkTypes} onJudged={(a) => qc.setQueryData(akeys.finalAudit, a)} />
      ))}
    </div>
  )
}
