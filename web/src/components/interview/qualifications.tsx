// Qualifications of one unit, shown under its coding in phase 2: claims about
// statements inside the unit, flags, a correction of its text or speaker, and
// the verification against the audio. Each write refreshes the interview.

import { useState, type ReactNode, type RefObject } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeChip } from "@/components/bits"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Textarea } from "@/components/ui/textarea"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { roleLabel } from "@/lib/format"
import {
  CLAIM_CHOICES,
  claimIssues,
  claimPayload,
  CO_CONSTRUCTED_HELP,
  draftOf,
  emptyClaimDraft,
  FLAG_HELP,
  flagsProblem,
  isSubstring,
  normalizeFlags,
  verificationLabel,
  verificationTone,
  type ClaimDraft,
} from "@/lib/qualifications"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { cn } from "@/lib/utils"
import {
  CLAIM_ROLES,
  CO_CONSTRUCTED_KINDS,
  SPEAKER_ROLES,
  UNIT_FLAGS,
  type Claim,
  type ClaimInput,
  type CoConstructedKind,
  type SpeakerRole,
  type UnitRow,
  type VerificationState,
} from "@/lib/types"

const NONE = "__none"

/** A select over a closed list; `null` is the "Non renseigné" choice. */
function ChoiceSelect({
  label,
  value,
  options,
  onChange,
  labelOf = (o: string) => o,
  allowNone = true,
  disabled,
}: {
  label: string
  value: string | null
  options: readonly string[]
  onChange: (v: string | null) => void
  labelOf?: (o: string) => string
  allowNone?: boolean
  disabled?: boolean
}) {
  return (
    <label className="flex flex-col gap-1 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <Select value={value ?? NONE} onValueChange={(v) => onChange(v === NONE ? null : v)} disabled={disabled}>
        <SelectTrigger size="sm" className="w-full">
          <SelectValue placeholder={t("common.non_renseigne")} />
        </SelectTrigger>
        <SelectContent>
          {allowNone && <SelectItem value={NONE}>{t("common.non_renseigne")}</SelectItem>}
          {options.map((o) => (
            <SelectItem key={o} value={o}>
              {labelOf(o)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </label>
  )
}

function Block({ title, aside, children }: { title: string; aside?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-2 border-t pt-3">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-xs font-semibold">{title}</h3>
        {aside && <div className="text-muted-foreground text-xs">{aside}</div>}
      </div>
      {children}
    </div>
  )
}

// ------------------------------------------------------------------ claims

function ClaimForm({
  initial,
  unitId,
  unitText,
  unitCodes,
  textRef,
  saving,
  onCancel,
  onSubmit,
}: {
  initial: ClaimDraft
  unitId: string
  unitText: string
  unitCodes: string[]
  textRef: RefObject<HTMLDivElement | null>
  saving: boolean
  onCancel: () => void
  onSubmit: (input: ClaimInput) => void
}) {
  const [d, setD] = useState<ClaimDraft>(initial)
  const set = <K extends keyof ClaimDraft>(k: K, v: ClaimDraft[K]) => setD((x) => ({ ...x, [k]: v }))
  const issues = d.excerpt ? claimIssues(d.excerpt, d.codes, unitText, unitCodes) : []
  const ready = !!d.excerpt && claimIssues(d.excerpt, d.codes, unitText, unitCodes).length === 0

  // The mouse-down is cancelled so the text selection survives the click.
  const takeSelection = () => {
    const sel = window.getSelection()
    const el = textRef.current
    if (!sel || sel.isCollapsed || !el) return toast.message(t("interview.qualifications.selectionnez_d_abord_des_mots"))
    if (!el.contains(sel.anchorNode) || !el.contains(sel.focusNode)) return toast.message(t("interview.qualifications.selectionnez_d_abord_des_mots"))
    const text = sel.toString().trim()
    if (!isSubstring(text, unitText)) return toast.error(t("interview.qualifications.la_selection_ne_se_retrouve"))
    set("excerpt", text)
  }

  return (
    <form
      className="bg-muted/30 flex flex-col gap-2.5 rounded-md border p-3"
      onSubmit={(e) => {
        e.preventDefault()
        if (ready) onSubmit(claimPayload(d, unitId))
      }}
    >
      <div className="flex flex-col gap-1 text-xs">
        <div className="flex items-center justify-between gap-2">
          <span className="text-muted-foreground">{t("interview.qualifications.extrait_mot_pour_mot_de")}</span>
          <Button type="button" size="xs" variant="outline" onMouseDown={(e) => e.preventDefault()} onClick={takeSelection}>
            {t("interview.qualifications.utiliser_le_texte_selectionne")}
          </Button>
        </div>
        <Input
          className="verbatim text-sm"
          value={d.excerpt}
          onChange={(e) => set("excerpt", e.target.value)}
          aria-invalid={issues.length > 0}
          placeholder={t("interview.qualifications.selectionnez_du_texte_ci_dessus")}
        />
        {issues.map((m) => (
          <span key={m} className="text-destructive">
            {m}
          </span>
        ))}
      </div>

      <div className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">{t("interview.qualifications.codes_de_cette_affirmation_parmi")}</span>
        {unitCodes.length === 0 ? (
          <span className="text-muted-foreground">{t("interview.qualifications.l_unite_n_a_pas")}</span>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {unitCodes.map((c) => {
              const on = d.codes.includes(c)
              return (
                <button
                  key={c}
                  type="button"
                  aria-pressed={on}
                  onClick={() => set("codes", on ? d.codes.filter((x) => x !== c) : [...d.codes, c])}
                  className={cn("rounded-md border px-1 py-0.5", on ? "bg-accent" : "opacity-60 hover:opacity-100")}
                >
                  <CodeChip code={c} />
                </button>
              )
            })}
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-2.5">
        <ChoiceSelect label={t("interview.qualifications.niveau_de_preuve")} value={d.ev_level} options={CLAIM_CHOICES.ev} onChange={(v) => set("ev_level", v)} />
        <ChoiceSelect label={t("common.origine")} value={d.provenance} options={CLAIM_CHOICES.provenance} onChange={(v) => set("provenance", v)} />
        <div className="col-span-2">
          <ChoiceSelect label={t("interview.qualifications.role_de_l_enonce")} value={d.role} options={CLAIM_ROLES} onChange={(v) => set("role", v)} />
        </div>
      </div>
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">{t("interview.qualifications.memo_sur_l_affirmation")}</span>
        <Textarea rows={2} value={d.memo} onChange={(e) => set("memo", e.target.value)} />
      </label>
      <div className="flex justify-end gap-2">
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>
          {t("common.annuler")}
        </Button>
        <Button type="submit" size="sm" disabled={!ready || saving}>
          {saving ? "Enregistrement…" : t("interview.qualifications.enregistrer_l_affirmation")}
        </Button>
      </div>
    </form>
  )
}

function ClaimsSection({
  interviewId,
  row,
  textRef,
}: {
  interviewId: string
  row: UnitRow
  textRef: RefObject<HTMLDivElement | null>
}) {
  const qc = useQueryClient()
  const unitId = row.unit.unit_id
  const unitText = row.unit.text
  const unitCodes = row.coding?.codes ?? []
  const claims = row.claims ?? []
  // null: closed; "new": the add form; a claim id: that claim's edit form.
  const [editing, setEditing] = useState<"new" | number | null>(null)

  const save = useMutation({
    mutationFn: (args: { claimId: number | null; input: ClaimInput }) =>
      args.claimId === null ? api.addClaim(interviewId, args.input) : api.editClaim(args.claimId, args.input),
    onSuccess: () => {
      toast.success(t("interview.qualifications.affirmation_enregistree"))
      setEditing(null)
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, t("interview.qualifications.affirmation_non_enregistree")),
  })
  const remove = useMutation({
    mutationFn: (claimId: number) => api.deleteClaim(claimId),
    onSuccess: () => {
      toast.success(t("interview.qualifications.affirmation_supprimee_le_journal_d"))
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, t("interview.qualifications.affirmation_non_supprimee")),
  })

  const formFor = (claim: Claim | null) => (
    <ClaimForm
      key={claim?.claim_id ?? "new"}
      initial={claim ? draftOf(claim) : emptyClaimDraft()}
      unitId={unitId}
      unitText={unitText}
      unitCodes={unitCodes}
      textRef={textRef}
      saving={save.isPending}
      onCancel={() => setEditing(null)}
      onSubmit={(input) => save.mutate({ claimId: claim?.claim_id ?? null, input })}
    />
  )

  return (
    <Block title={t("interview.qualifications.affirmations")} aside={t("interview.qualifications.length_sur_cette_unite", { length: claims.length })}>
      {claims.length === 0 && editing === null && (
        <p className="text-muted-foreground text-xs">
          {t("interview.qualifications.une_affirmation_qualifie_un_enonce")}
        </p>
      )}
      <ul className="flex flex-col gap-2">
        {claims.map((c) => {
          const issues = claimIssues(c.excerpt, c.codes, unitText, unitCodes)
          if (editing === c.claim_id) return <li key={c.claim_id}>{formFor(c)}</li>
          return (
            <li key={c.claim_id} className="rounded-md border p-2.5 text-xs">
              <blockquote className="verbatim border-primary/40 border-l-2 pl-2 text-sm">{c.excerpt}</blockquote>
              <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
                {c.codes.length ? c.codes.map((code) => <CodeChip key={code} code={code} />) : <span className="text-muted-foreground">{t("common.aucun_code")}</span>}
                {c.ev_level && <Badge variant="outline">{c.ev_level}</Badge>}
                {c.provenance && <Badge variant="outline">{c.provenance}</Badge>}
                {c.role && <Badge variant="outline">{c.role}</Badge>}
                <span className="ml-auto flex gap-1">
                  <Button size="xs" variant="ghost" onClick={() => setEditing(c.claim_id)}>
                    {t("common.modifier")}
                  </Button>
                  <Button size="xs" variant="ghost" onClick={() => remove.mutate(c.claim_id)} disabled={remove.isPending}>
                    {t("common.supprimer")}
                  </Button>
                </span>
              </div>
              {c.memo && <p className="text-muted-foreground mt-1.5">{c.memo}</p>}
              {issues.length > 0 && (
                <p className="text-warn mt-1.5">{t("interview.qualifications.a_revoir_join", { join: issues.join(" ") })}</p>
              )}
            </li>
          )
        })}
      </ul>
      {editing === "new" ? (
        formFor(null)
      ) : (
        <Button size="xs" variant="outline" className="self-start" onClick={() => setEditing("new")} disabled={editing !== null}>
          {t("interview.qualifications.ajouter_une_affirmation")}
        </Button>
      )}
    </Block>
  )
}

// ------------------------------------------------------------------- flags

function FlagsForm({
  interviewId,
  unitId,
  savedFlags,
  savedKind,
}: {
  interviewId: string
  unitId: string
  savedFlags: string[]
  savedKind: CoConstructedKind | null
}) {
  const qc = useQueryClient()
  const [flags, setFlags] = useState<string[]>(savedFlags)
  const [kind, setKind] = useState<CoConstructedKind | null>(savedKind)
  const problem = flagsProblem(flags, kind)
  const normalized = normalizeFlags(flags, kind)
  const dirty =
    normalized.flags.join() !== normalizeFlags(savedFlags, savedKind).flags.join() || normalized.co_constructed_kind !== savedKind

  const toggle = (f: string, on: boolean) => {
    const next = on ? [...flags, f] : flags.filter((x) => x !== f)
    setFlags(next)
    if (f === "CO-CONSTRUCTED" && !on) setKind(null)
  }
  const save = useMutation({
    mutationFn: () => api.setFlags(interviewId, unitId, normalized.flags, normalized.co_constructed_kind),
    onSuccess: () => {
      toast.success(t("interview.qualifications.drapeaux_enregistres"))
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, t("interview.qualifications.drapeaux_non_enregistres")),
  })

  return (
    <div className="flex flex-col gap-2">
      {UNIT_FLAGS.map((f) => (
        <label key={f} className="flex items-start gap-2 text-xs">
          <Checkbox className="mt-0.5" checked={flags.includes(f)} onCheckedChange={(v) => toggle(f, v === true)} />
          <span>
            <span className="ident font-medium">{f}</span>
            <span className="text-muted-foreground block">{FLAG_HELP[f]}</span>
          </span>
        </label>
      ))}
      {flags.includes("CO-CONSTRUCTED") && (
        <div className="flex flex-col gap-1">
          <ChoiceSelect
            label={t("interview.qualifications.type_de_co_construction_obligatoire")}
            value={kind}
            options={CO_CONSTRUCTED_KINDS}
            onChange={(v) => setKind(v as CoConstructedKind | null)}
            allowNone
          />
          {kind && <span className="text-muted-foreground text-xs">{CO_CONSTRUCTED_HELP[kind]}</span>}
        </div>
      )}
      {problem && dirty && <span className="text-warn text-xs">{problem}</span>}
      <div className="flex justify-end">
        <Button size="xs" onClick={() => save.mutate()} disabled={!dirty || !!problem || save.isPending}>
          {save.isPending ? "Enregistrement…" : t("interview.qualifications.enregistrer_les_drapeaux")}
        </Button>
      </div>
    </div>
  )
}

// -------------------------------------------------------------- correction

function CorrectionForm({ interviewId, row, onDone }: { interviewId: string; row: UnitRow; onDone: () => void }) {
  const qc = useQueryClient()
  const unitId = row.unit.unit_id
  const [text, setText] = useState(row.unit.text)
  const [role, setRole] = useState(row.unit.role)
  const [reason, setReason] = useState("")
  const history = useQuery({
    queryKey: ["corrections", interviewId, unitId],
    queryFn: () => api.corrections(interviewId, unitId),
  })
  const correct = useMutation({
    mutationFn: () => api.correctUnit(interviewId, unitId, { text: text.trim(), role, reason: reason.trim() }),
    onSuccess: () => {
      toast.success(t("interview.qualifications.correction_enregistree_la_segmentati"))
      onDone()
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, t("interview.qualifications.correction_refusee")),
  })
  const ready = !!text.trim() && !!reason.trim()
  return (
    <>
      <DialogHeader>
        <DialogTitle>{t("interview.qualifications.corriger_unitid", { unitId })}</DialogTitle>
        <DialogDescription>
          {t("interview.qualifications.le_texte_ou_le_locuteur")}
        </DialogDescription>
      </DialogHeader>
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          if (ready) correct.mutate()
        }}
      >
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("interview.qualifications.texte")}</span>
          <Textarea rows={6} className="verbatim text-sm" value={text} onChange={(e) => setText(e.target.value)} />
        </label>
        <ChoiceSelect
          label={t("interview.qualifications.locuteur")}
          value={role}
          options={SPEAKER_ROLES}
          onChange={(v) => {
            if (v) setRole(v as SpeakerRole)
          }}
          labelOf={roleLabel}
          allowNone={false}
        />
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("common.motif_obligatoire")}</span>
          <Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder={t("interview.qualifications.ce_que_dit_l_audio")} />
        </label>
        {(history.data?.length ?? 0) > 0 && (
          <div className="text-muted-foreground flex flex-col gap-1 text-xs">
            <span>{t("interview.qualifications.corrections_anterieures_la_plus_anci")}</span>
            {history.data!.map((c) => (
              <span key={c.correction_id}>
                {t("interview.qualifications.created_at_by_created_by", { created_at: c.created_at, created_by: c.created_by, reason: c.reason })}
              </span>
            ))}
          </div>
        )}
        <DialogFooter>
          <Button type="submit" disabled={!ready || correct.isPending}>
            {correct.isPending ? "Enregistrement…" : t("interview.qualifications.enregistrer_la_correction")}
          </Button>
        </DialogFooter>
      </form>
    </>
  )
}

function CorrectionSection({ interviewId, row }: { interviewId: string; row: UnitRow }) {
  const [open, setOpen] = useState(false)
  return (
    <Block title={t("interview.qualifications.correction")} aside={row.unit.corrected ? <Badge variant="outline" className="text-warn">{t("interview.qualifications.corrigee")}</Badge> : t("interview.qualifications.texte_et_locuteur_tels_que")}>
      <div className="flex justify-end">
        <Button size="xs" variant="outline" onClick={() => setOpen(true)}>
          {t("interview.qualifications.corriger_l_unite")}
        </Button>
      </div>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-xl">
          {open && <CorrectionForm interviewId={interviewId} row={row} onDone={() => setOpen(false)} />}
        </DialogContent>
      </Dialog>
    </Block>
  )
}

// ----------------------------------------------------------- verification

function VerificationSection({ interviewId, row }: { interviewId: string; row: UnitRow }) {
  const qc = useQueryClient()
  const unitId = row.unit.unit_id
  const v = row.verification ?? { state: "unverified" as VerificationState, stale: false }
  const [note, setNote] = useState("")
  const mark = useMutation({
    mutationFn: (state: VerificationState) => api.verifyUnit(interviewId, unitId, state, note.trim() || null),
    onSuccess: (_, state) => {
      toast.success(state === "verified" ? t("interview.qualifications.unitid_marquee_verifiee", { unitId }) : t("interview.qualifications.unitid_marquee_inutilisable", { unitId }))
      setNote("")
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, t("interview.qualifications.verification_non_enregistree")),
  })
  return (
    <Block title={t("interview.qualifications.verification_a_l_audio")} aside={<VerificationBadge v={v} />}>
      {v.stale && (
        <p className="text-warn text-xs">{t("interview.qualifications.le_texte_ou_le_locuteur_2")}</p>
      )}
      <Input value={note} onChange={(e) => setNote(e.target.value)} placeholder={t("interview.qualifications.note_ce_que_vous_avez")} className="text-sm" />
      <div className="flex gap-2">
        <Button size="xs" onClick={() => mark.mutate("verified")} disabled={mark.isPending}>
          {t("interview.qualifications.marquer_comme_verifiee")}
        </Button>
        <Button size="xs" variant="outline" onClick={() => mark.mutate("unusable")} disabled={mark.isPending}>
          {t("interview.qualifications.marquer_comme_inutilisable")}
        </Button>
      </div>
    </Block>
  )
}

/** The state badge with the stale marker; the same colours as the coverage table. */
export function VerificationBadge({ v }: { v: { state: VerificationState; stale: boolean } }) {
  const tone = verificationTone(v)
  return (
    <Badge
      variant="outline"
      className={cn(tone === "ok" && "text-ok border-ok/40", tone === "danger" && "text-destructive border-destructive/40", v.stale && "border-dashed")}
      data-state={v.state}
      data-stale={v.stale ? "true" : undefined}
    >
      {verificationLabel(v)}
    </Badge>
  )
}

// ----------------------------------------------------------------- panel

/** Qualifications of the selected unit (phase 2 panel). */
export function QualificationPanel({ interviewId, row, textRef }: { interviewId: string; row: UnitRow; textRef: RefObject<HTMLDivElement | null> }) {
  const flagsQ = useQuery({ queryKey: ["flags", interviewId], queryFn: () => api.flags(interviewId) })
  const unitId = row.unit.unit_id
  const saved = flagsQ.data?.find((f) => f.unit_id === unitId)
  const savedFlags = row.flags ?? saved?.flags ?? []
  const savedKind = saved?.co_constructed_kind ?? null

  return (
    <section aria-label={t("interview.qualifications.qualifications_de_l_unite")} className="flex flex-col gap-3">
      <ClaimsSection interviewId={interviewId} row={row} textRef={textRef} />
      <Block title={t("interview.qualifications.drapeaux")} aside={savedFlags.length ? savedFlags.join(", ") : "aucun"}>
        {flagsQ.isLoading ? (
          <span className="text-muted-foreground text-xs">{t("interview.qualifications.chargement_des_drapeaux")}</span>
        ) : (
          <FlagsForm
            key={`${savedFlags.join()}|${savedKind ?? ""}`}
            interviewId={interviewId}
            unitId={unitId}
            savedFlags={savedFlags}
            savedKind={savedKind}
          />
        )}
      </Block>
      <CorrectionSection interviewId={interviewId} row={row} />
      <VerificationSection interviewId={interviewId} row={row} />
    </section>
  )
}
