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
          <SelectValue placeholder="Non renseigné" />
        </SelectTrigger>
        <SelectContent>
          {allowNone && <SelectItem value={NONE}>Non renseigné</SelectItem>}
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
    if (!sel || sel.isCollapsed || !el) return toast.message("Sélectionnez d'abord des mots dans le texte de l'unité")
    if (!el.contains(sel.anchorNode) || !el.contains(sel.focusNode)) return toast.message("Sélectionnez d'abord des mots dans le texte de l'unité")
    const text = sel.toString().trim()
    if (!isSubstring(text, unitText)) return toast.error("La sélection ne se retrouve pas mot pour mot dans l'unité")
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
          <span className="text-muted-foreground">Extrait, mot pour mot de l'unité</span>
          <Button type="button" size="xs" variant="outline" onMouseDown={(e) => e.preventDefault()} onClick={takeSelection}>
            Utiliser le texte sélectionné
          </Button>
        </div>
        <Input
          className="verbatim text-sm"
          value={d.excerpt}
          onChange={(e) => set("excerpt", e.target.value)}
          aria-invalid={issues.length > 0}
          placeholder="Sélectionnez du texte ci-dessus, ou saisissez-le"
        />
        {issues.map((m) => (
          <span key={m} className="text-destructive">
            {m}
          </span>
        ))}
      </div>

      <div className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">Codes de cette affirmation, parmi ceux de l'unité</span>
        {unitCodes.length === 0 ? (
          <span className="text-muted-foreground">L'unité n'a pas encore de code enregistré. Enregistrez ses codes pour les rattacher.</span>
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
        <ChoiceSelect label="Niveau de preuve" value={d.ev_level} options={CLAIM_CHOICES.ev} onChange={(v) => set("ev_level", v)} />
        <ChoiceSelect label="Origine" value={d.provenance} options={CLAIM_CHOICES.provenance} onChange={(v) => set("provenance", v)} />
        <div className="col-span-2">
          <ChoiceSelect label="Rôle de l'énoncé" value={d.role} options={CLAIM_ROLES} onChange={(v) => set("role", v)} />
        </div>
      </div>
      <label className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">Mémo sur l'affirmation</span>
        <Textarea rows={2} value={d.memo} onChange={(e) => set("memo", e.target.value)} />
      </label>
      <div className="flex justify-end gap-2">
        <Button type="button" size="sm" variant="ghost" onClick={onCancel}>
          Annuler
        </Button>
        <Button type="submit" size="sm" disabled={!ready || saving}>
          {saving ? "Enregistrement…" : "Enregistrer l'affirmation"}
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
      toast.success("Affirmation enregistrée")
      setEditing(null)
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, "Affirmation non enregistrée"),
  })
  const remove = useMutation({
    mutationFn: (claimId: number) => api.deleteClaim(claimId),
    onSuccess: () => {
      toast.success("Affirmation supprimée ; le journal d'audit la conserve")
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, "Affirmation non supprimée"),
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
    <Block title="Affirmations" aside={`${claims.length} sur cette unité`}>
      {claims.length === 0 && editing === null && (
        <p className="text-muted-foreground text-xs">
          Une affirmation qualifie un énoncé de l'unité : son niveau de preuve, son origine et son rôle dans le récit.
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
                {c.codes.length ? c.codes.map((code) => <CodeChip key={code} code={code} />) : <span className="text-muted-foreground">aucun code</span>}
                {c.ev_level && <Badge variant="outline">{c.ev_level}</Badge>}
                {c.provenance && <Badge variant="outline">{c.provenance}</Badge>}
                {c.role && <Badge variant="outline">{c.role}</Badge>}
                <span className="ml-auto flex gap-1">
                  <Button size="xs" variant="ghost" onClick={() => setEditing(c.claim_id)}>
                    Modifier
                  </Button>
                  <Button size="xs" variant="ghost" onClick={() => remove.mutate(c.claim_id)} disabled={remove.isPending}>
                    Supprimer
                  </Button>
                </span>
              </div>
              {c.memo && <p className="text-muted-foreground mt-1.5">{c.memo}</p>}
              {issues.length > 0 && (
                <p className="text-warn mt-1.5">À revoir : {issues.join(" ")}</p>
              )}
            </li>
          )
        })}
      </ul>
      {editing === "new" ? (
        formFor(null)
      ) : (
        <Button size="xs" variant="outline" className="self-start" onClick={() => setEditing("new")} disabled={editing !== null}>
          Ajouter une affirmation
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
      toast.success("Drapeaux enregistrés")
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, "Drapeaux non enregistrés"),
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
            label="Type de co-construction, obligatoire"
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
          {save.isPending ? "Enregistrement…" : "Enregistrer les drapeaux"}
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
      toast.success("Correction enregistrée ; la segmentation reste inchangée")
      onDone()
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, "Correction refusée"),
  })
  const ready = !!text.trim() && !!reason.trim()
  return (
    <>
      <DialogHeader>
        <DialogTitle>Corriger {unitId}</DialogTitle>
        <DialogDescription>
          Le texte ou le locuteur tel qu'il doit se lire. Le texte segmenté reste dans le journal d'audit ; cette correction s'ajoute, et l'unité apparaît comme corrigée.
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
          <span className="text-muted-foreground">Texte</span>
          <Textarea rows={6} className="verbatim text-sm" value={text} onChange={(e) => setText(e.target.value)} />
        </label>
        <ChoiceSelect
          label="Locuteur"
          value={role}
          options={SPEAKER_ROLES}
          onChange={(v) => {
            if (v) setRole(v as SpeakerRole)
          }}
          labelOf={roleLabel}
          allowNone={false}
        />
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">Motif, obligatoire</span>
          <Textarea rows={2} value={reason} onChange={(e) => setReason(e.target.value)} placeholder="Ce que dit l'audio, et à quel moment" />
        </label>
        {(history.data?.length ?? 0) > 0 && (
          <div className="text-muted-foreground flex flex-col gap-1 text-xs">
            <span>Corrections antérieures, la plus ancienne d'abord</span>
            {history.data!.map((c) => (
              <span key={c.correction_id}>
                {c.created_at} by {c.created_by}: {c.reason}
              </span>
            ))}
          </div>
        )}
        <DialogFooter>
          <Button type="submit" disabled={!ready || correct.isPending}>
            {correct.isPending ? "Enregistrement…" : "Enregistrer la correction"}
          </Button>
        </DialogFooter>
      </form>
    </>
  )
}

function CorrectionSection({ interviewId, row }: { interviewId: string; row: UnitRow }) {
  const [open, setOpen] = useState(false)
  return (
    <Block title="Correction" aside={row.unit.corrected ? <Badge variant="outline" className="text-warn">corrigée</Badge> : "texte et locuteur tels que segmentés"}>
      <div className="flex justify-end">
        <Button size="xs" variant="outline" onClick={() => setOpen(true)}>
          Corriger l'unité
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
      toast.success(state === "verified" ? `${unitId} marquée vérifiée` : `${unitId} marquée inutilisable`)
      setNote("")
      invalidateInterview(qc, interviewId)
    },
    onError: (e) => reportError(e, "Vérification non enregistrée"),
  })
  return (
    <Block title="Vérification à l'audio" aside={<VerificationBadge v={v} />}>
      {v.stale && (
        <p className="text-warn text-xs">Le texte ou le locuteur a changé après le dernier contrôle : contrôlez de nouveau l'unité.</p>
      )}
      <Input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Note : ce que vous avez entendu, et quand (facultatif)" className="text-sm" />
      <div className="flex gap-2">
        <Button size="xs" onClick={() => mark.mutate("verified")} disabled={mark.isPending}>
          Marquer comme vérifiée
        </Button>
        <Button size="xs" variant="outline" onClick={() => mark.mutate("unusable")} disabled={mark.isPending}>
          Marquer comme inutilisable
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
    <section aria-label="Qualifications de l'unité" className="flex flex-col gap-3">
      <ClaimsSection interviewId={interviewId} row={row} textRef={textRef} />
      <Block title="Drapeaux" aside={savedFlags.length ? savedFlags.join(", ") : "aucun"}>
        {flagsQ.isLoading ? (
          <span className="text-muted-foreground text-xs">Chargement des drapeaux…</span>
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
