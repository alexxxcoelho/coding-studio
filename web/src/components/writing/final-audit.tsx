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
    onError: (e) => reportError(e, "Verdict non enregistré"),
  })
  const unchanged = !!claim.verdict && claim.verdict.verdict === verdict && claim.verdict.note === note.trim()
  const typeLabel = (id: string) => linkTypes.find((t) => t.id === id)?.label ?? id

  return (
    <article className="grp flex flex-col gap-3 p-3" aria-label={`Affirmation : ${claim.sentence}`}>
      <header className="flex flex-col gap-0.5">
        <span className="hs">{claim.part_title}</span>
        <q className="font-serif text-base leading-snug [quotes:none]">{claim.sentence}</q>
      </header>

      <ol className="flex flex-col gap-2 text-sm" aria-label="Chemin de l'affirmation à la source">
        <li>
          <span className="hs">Thème</span>
          <div>{claim.theme ? claim.theme.title : claim.rq_id ? `Synthèse de la question ${claim.rq_id}` : "Aucun thème"}</div>
        </li>
        <li>
          <span className="hs">Codes</span>
          <div className="flex flex-wrap gap-1">
            {claim.codes.length === 0 ? <span className="text-muted-foreground">aucun</span> : claim.codes.map((c) => <span key={c.code_id} className="badge">{c.name}</span>)}
          </div>
        </li>
        <li className="flex flex-col gap-1.5">
          <span className="hs">Passages liés</span>
          {claim.links.length === 0 && <span className="text-muted-foreground">Aucun passage lié à cette affirmation.</span>}
          {claim.links.map((l) => (
            <PassageCard
              key={l.link_id}
              passage={l.passage}
              top={<span className="bg-fill2 rounded-full px-2 py-0.5 text-[11.5px] font-medium">{typeLabel(l.link_type)}</span>}
              foot={
                <>
                  {l.interview_version && <span className="text-muted-foreground text-[11.5px]">Version de l'entretien : {l.interview_version}</span>}
                  {l.note && <span className="text-[11.5px]">Note : {l.note}</span>}
                </>
              }
            />
          ))}
        </li>
      </ol>

      {claim.gaps.length > 0 && (
        <ul className="flex flex-col gap-1.5" aria-label="Maillons manquants">
          {claim.gaps.map((g) => (
            <Gap key={g.kind + g.message} text={g.message} />
          ))}
        </ul>
      )}

      <fieldset className="flex flex-col gap-2">
        <legend className="hs mb-1">Verdict</legend>
        <div className="flex flex-wrap gap-4">
          {AUDIT_VERDICTS.map((v) => (
            <label key={v} className="flex items-center gap-1.5 text-sm">
              <input type="radio" name={`verdict-${claim.claim_key}`} checked={verdict === v} onChange={() => setVerdict(v)} />
              {VERDICT_LABEL[v]}
            </label>
          ))}
        </div>
        <label className="flex flex-col gap-1">
          <span className="hs">Note du verdict</span>
          <Textarea rows={2} value={note} onChange={(e) => setNote(e.target.value)} placeholder="Ce qu'il faut nuancer ou retirer, ou pourquoi l'affirmation tient" />
        </label>
        {verdict && !check.ok && "reason" in check && check.reason && (
          <p role="alert" className="text-warn text-xs">
            {check.reason}
          </p>
        )}
        <div className="flex items-center gap-3">
          <Button size="sm" disabled={!verdict || !check.ok || save.isPending || unchanged} onClick={() => save.mutate()}>
            Enregistrer le verdict
          </Button>
          {claim.verdict && (
            <span className={cn("text-xs", claim.verdict.verdict === "supported" ? "text-ok" : "text-warn")}>Verdict enregistré : {VERDICT_LABEL[claim.verdict.verdict]}</span>
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
    onError: (e) => reportError(e, "Tirage impossible"),
  })
  const audit = latest.data ?? null
  const progress = audit ? auditProgress(audit.claims) : null

  return (
    <div className="mx-auto flex w-full max-w-[760px] min-w-0 flex-col gap-4">
      <p className="text-muted-foreground max-w-prose text-sm">
        Avant la remise, vérifiez que les affirmations ne sont pas hors-sol. L'outil tire au hasard trois affirmations de votre texte ; pour chacune, remontez aux passages liés et à la citation exacte, puis dites si elle est étayée, à nuancer ou à retirer. Si un maillon manque, corrigez-le avant de conclure.
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={() => draw.mutate()} disabled={draw.isPending}>
          {audit ? "Relancer un tirage" : "Tirer trois affirmations"}
        </Button>
        {audit && progress && (
          <span className="text-muted-foreground text-sm" role="status">
            {progress.judged} sur {progress.total} jugées
            {progress.complete ? ". Le résultat va dans l'export : joignez-le à votre dossier de remise." : ""}
          </span>
        )}
      </div>
      {audit && audit.claims.length < audit.n && audit.claims.length > 0 && (
        <p className="text-muted-foreground text-sm">Le texte ne contient que {audit.claims.length} affirmation{audit.claims.length > 1 ? "s" : ""} avec des passages liés.</p>
      )}
      {latest.isLoading && <Skeleton className="h-40 w-full" />}
      {audit && audit.claims.length === 0 && <p className="text-muted-foreground text-sm">Aucune affirmation à tirer : écrivez d'abord des parties de thème et liez-y des passages.</p>}
      {audit?.claims.map((c) => (
        <ClaimCard key={`${audit.audit_id}:${c.claim_key}:${c.verdict?.at ?? ""}`} audit={audit} claim={c} linkTypes={linkTypes} onJudged={(a) => qc.setQueryData(akeys.finalAudit, a)} />
      ))}
    </div>
  )
}
