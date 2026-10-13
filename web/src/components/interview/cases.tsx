// Synthèse des cas : une fiche par projet identifiable dans l'entretien, cinq
// champs (déclencheur, alternatives, contraintes, choix, résultat), chacun
// appuyé sur des unités du cas. Les unités d'un cas peuvent venir de n'importe
// quel entretien ; les preuves d'un champ doivent en faire partie.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Plus, X } from "lucide-react"

import { Banner, EmptyState, Section } from "@/components/bits"
import { Button } from "@/components/ui/button"
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command"
import { FIELD } from "@/components/ui/field-styles"
import { Input } from "@/components/ui/input"
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { ACCOUNT_TYPES, DECISIONS, fieldFilled, FIELD_LABELS, GRANULARITIES, parseList, parseUnitIds, unattributedEvidence } from "@/lib/crosscase"
import { dateTime } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref, unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import { CASE_FIELDS, type Case, type CaseField, type CaseFieldKey, type InterviewDetail, type Unit } from "@/lib/types"

const STAGES = ["STAGE-INIT", "STAGE-PILOT", "STAGE-ROLLOUT", "STAGE-ROUTINE", "STAGE-EXTEND"]
const STATUSES = ["STATUS-STABLE", "STATUS-SCALED", "STATUS-STALLED", "STATUS-DISCONT", "STATUS-FAILED", "STATUS-MIXED", "STATUS-UNCLEAR"]
const OTHER = "__other"

/** Display labels of the closed values sent to the server. */
const OPTION_LABEL: Record<string, string> = {
  get adopted() { return t("interview.cases.adoptee") },
  get deferred() { return t("interview.cases.differee") },
  get rejected() { return t("interview.cases.refusee") },
  item: "article",
  batch: "lot",
  get model() { return t("interview.cases.modele") },
  get "first-hand"() { return t("interview.cases.de_premiere_main") },
  get reported() { return t("interview.cases.rapporte") },
  get ecosystem() { return t("interview.cases.ecosysteme") },
  anticipation: "anticipation",
}

function short(unitId: string): string {
  return `U${unitId.split("-U").pop()}`
}

const selectCls = FIELD

/** A unit used as evidence: chip linking to the unit in Coding, removable.
 * Shown as a warning when the unit is not attributed to the case. */
function EvidenceChip({ unitId, attributed, onRemove }: { unitId: string; attributed: boolean; onRemove?: () => void }) {
  return (
    <span
      className={cn("inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs", attributed ? "bg-muted" : "bg-warn-soft text-warn")}
      title={attributed ? undefined : t("interview.cases.absente_des_unites_attribuees_au")}
    >
      <a href={unitHref(unitId)} className="ident hover:underline" title={t("interview.cases.ouvrir_unitid_dans_le_codage", { unitId })}>
        {short(unitId)}
      </a>
      {onRemove && (
        <button type="button" onClick={onRemove} aria-label={`Retirer ${unitId}`} className="text-muted-foreground hover:text-foreground">
          <X className="size-3" />
        </button>
      )}
    </span>
  )
}

/** Picks evidence for a field among the attributed units of the case only. */
function UnitPicker({ attributed, textOf, chosen, onPick }: { attributed: string[]; textOf: (id: string) => string | undefined; chosen: string[]; onPick: (unitId: string) => void }) {
  const [open, setOpen] = useState(false)
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="xs" variant="outline">
          <Plus /> {t("interview.cases.preuve")}
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[30rem] p-0" align="start">
        <Command
          filter={(value, search) => {
            const q = search.toLowerCase()
            return value.toLowerCase().includes(q) || (textOf(value) ?? "").toLowerCase().includes(q) ? 1 : 0
          }}
        >
          <CommandInput placeholder={t("interview.cases.chercher_parmi_les_unites_attribuees")} />
          <CommandList className="max-h-80">
            <CommandEmpty>{attributed.length ? t("interview.cases.aucune_unite_attribuee_ne_correspond") : t("interview.cases.aucune_unite_attribuee_pour_l")}</CommandEmpty>
            <CommandGroup>
              {attributed.map((u) => (
                <CommandItem
                  key={u}
                  value={u}
                  disabled={chosen.includes(u)}
                  onSelect={() => {
                    onPick(u)
                    setOpen(false)
                  }}
                  className="items-start"
                >
                  <span className="ident text-muted-foreground w-24 shrink-0 truncate">{u}</span>
                  <span className="line-clamp-2 text-xs">{textOf(u) ?? t("interview.cases.unite_d_un_autre_entretien")}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  )
}

/** A value from a short list of documented values, with a free text fallback. */
function OptionField({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: string | null
  options: readonly string[]
  onChange: (v: string | null) => void
}) {
  const [other, setOther] = useState(() => value !== null && !options.includes(value))
  return (
    <div className="flex flex-col gap-1 text-xs">
      <span className="text-muted-foreground">{label}</span>
      <select
        aria-label={label}
        className={selectCls}
        value={other ? OTHER : (value ?? "")}
        onChange={(e) => {
          const v = e.target.value
          if (v === OTHER) {
            setOther(true)
            onChange(null)
          } else {
            setOther(false)
            onChange(v || null)
          }
        }}
      >
        <option value="">{t("common.non_renseigne")}</option>
        {options.map((o) => (
          <option key={o} value={o}>
            {OPTION_LABEL[o] ?? o}
          </option>
        ))}
        <option value={OTHER}>{t("interview.cases.autre_texte_libre")}</option>
      </select>
      {other && <Input value={value ?? ""} onChange={(e) => onChange(e.target.value || null)} placeholder={t("common.texte_libre")} aria-label={`${label}, texte libre`} />}
    </div>
  )
}

function CaseEditor({ initial, units, onSaved }: { initial: Case; units: Unit[]; onSaved: (c: Case) => void }) {
  const qc = useQueryClient()
  const [c, setC] = useState<Case>(initial)
  // Text as typed, so that separators are not rewritten while the user types.
  const [unitsText, setUnitsText] = useState(initial.units.join("\n"))
  const [carriersText, setCarriersText] = useState(initial.carriers_rejected.join(", "))
  const dirty = JSON.stringify(c) !== JSON.stringify(initial)
  const setField = (k: CaseFieldKey, patch: Partial<CaseField>) => setC((x) => ({ ...x, [k]: { ...x[k], ...patch } }))
  const save = useMutation({
    mutationFn: () => api.editCase(c),
    onSuccess: (saved) => {
      toast.success(t("interview.cases.case_id_enregistre", { case_id: saved.case_id }))
      qc.invalidateQueries({ queryKey: ["cases"] })
      qc.invalidateQueries({ queryKey: ["cross-case"] })
      invalidateInterview(qc, c.interview_id)
      onSaved(saved)
    },
    onError: (e) => reportError(e, t("interview.cases.cas_non_enregistre")),
  })
  const textOf = (id: string) => units.find((u) => u.unit_id === id)?.text

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-3 md:grid-cols-[1fr_12rem_12rem]">
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("interview.cases.intitule_du_cas")}</span>
          <Input value={c.label} onChange={(e) => setC({ ...c, label: e.target.value })} placeholder={t("interview.cases.rfid_a_l_article_dans")} />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("common.etape")}</span>
          <select aria-label={t("common.etape")} className={selectCls} value={c.stage ?? ""} onChange={(e) => setC({ ...c, stage: e.target.value || null })}>
            <option value="">{t("common.non_renseigne")}</option>
            {STAGES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("interview.cases.statut_du_resultat")}</span>
          <select aria-label={t("interview.cases.statut_du_resultat")} className={selectCls} value={c.outcome_status ?? ""} onChange={(e) => setC({ ...c, outcome_status: e.target.value || null })}>
            <option value="">{t("common.non_renseigne")}</option>
            {STATUSES.map((s) => (
              <option key={s}>{s}</option>
            ))}
          </select>
        </label>
      </div>

      <div className="grid gap-3 md:grid-cols-3">
        <OptionField label={t("common.decision")} value={c.decision} options={DECISIONS} onChange={(v) => setC((x) => ({ ...x, decision: v }))} />
        <OptionField label={t("interview.cases.granularite")} value={c.granularity} options={GRANULARITIES} onChange={(v) => setC((x) => ({ ...x, granularity: v }))} />
        <OptionField label={t("interview.cases.type_de_recit")} value={c.account_type} options={ACCOUNT_TYPES} onChange={(v) => setC((x) => ({ ...x, account_type: v }))} />
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("interview.cases.support_retenu")}</span>
          <Input value={c.carrier_chosen ?? ""} onChange={(e) => setC((x) => ({ ...x, carrier_chosen: e.target.value || null }))} />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("interview.cases.supports_ecartes_separes_par_des")}</span>
          <Input
            value={carriersText}
            onChange={(e) => {
              setCarriersText(e.target.value)
              setC((x) => ({ ...x, carriers_rejected: parseList(e.target.value) }))
            }}
          />
        </label>
        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("interview.cases.lien_avec_la_suisse")}</span>
          <Input value={c.swiss_link ?? ""} onChange={(e) => setC((x) => ({ ...x, swiss_link: e.target.value || null }))} />
        </label>
      </div>

      <Section title={t("interview.cases.unites_attribuees")} aside={t("interview.cases.de_n_importe_quel_entretien")}>
        <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_16rem]">
          <div className="flex flex-col gap-1.5">
            <Textarea
              rows={4}
              className="ident text-xs"
              aria-label={t("interview.cases.identifiants_des_unites_attribuees")}
              placeholder={"ECO-07-U012\nRET-22-U003"}
              value={unitsText}
              onChange={(e) => {
                setUnitsText(e.target.value)
                setC((x) => ({ ...x, units: parseUnitIds(e.target.value) }))
              }}
            />
            <span className="text-muted-foreground text-xs">{t("interview.cases.un_identifiant_d_unite_par")}</span>
          </div>
          <div className="flex flex-col gap-2 text-xs">
            <span className="tabular-nums">{c.units.length === 1 ? t("interview.cases.1_unite_attribuee") : t("interview.cases.length_unites_attribuees", { length: c.units.length })}</span>
            <div>
              <span className="text-muted-foreground">{t("interview.cases.entretiens_en_lecture_seule_deduits")}</span>
              <p className="ident mt-0.5">{initial.interviews.length ? initial.interviews.join(", ") : t("interview.cases.aucun_enregistre")}</p>
            </div>
          </div>
        </div>
      </Section>

      {CASE_FIELDS.map((k) => {
        const f = c[k]
        const loose = unattributedEvidence(c, k)
        return (
          <section key={k} className="grid gap-2 md:grid-cols-[10rem_1fr]">
            <div>
              <h3 className="text-sm font-semibold">{FIELD_LABELS[k].title}</h3>
              <p className="text-muted-foreground text-xs">{FIELD_LABELS[k].hint}</p>
            </div>
            <div className="flex flex-col gap-2">
              <Textarea rows={2} value={f.text} onChange={(e) => setField(k, { text: e.target.value })} placeholder={t("interview.cases.dans_vos_mots_appuye_sur")} />
              <div className="flex flex-wrap items-center gap-1.5">
                {f.evidence.map((u) => (
                  <EvidenceChip
                    key={u}
                    unitId={u}
                    attributed={c.units.includes(u)}
                    onRemove={() => setField(k, { evidence: f.evidence.filter((x) => x !== u) })}
                  />
                ))}
                <UnitPicker
                  attributed={c.units}
                  textOf={textOf}
                  chosen={f.evidence}
                  onPick={(u) => setField(k, { evidence: [...f.evidence, u] })}
                />
              </div>
              {loose.length > 0 && (
                <p className="text-warn text-xs">
                  {t("interview.cases.absentes_des_unites_attribuees_join", { join: loose.join(", ") })}
                </p>
              )}
              <div className="grid gap-2 sm:grid-cols-2">
                <label className="flex flex-col gap-1 text-xs">
                  <span className="text-muted-foreground">{t("interview.cases.contradictions")}</span>
                  <Textarea rows={2} value={f.contradictions} onChange={(e) => setField(k, { contradictions: e.target.value })} placeholder={t("interview.cases.la_ou_les_donnees_divergent")} />
                </label>
                <label className="flex flex-col gap-1 text-xs">
                  <span className="text-muted-foreground">{t("interview.cases.manquant")}</span>
                  <Textarea rows={2} value={f.missing} onChange={(e) => setField(k, { missing: e.target.value })} placeholder={t("interview.cases.ce_que_les_donnees_ne")} />
                </label>
              </div>
            </div>
          </section>
        )
      })}

      <label className="flex flex-col gap-1 text-xs">
        <span className="text-muted-foreground">{t("interview.cases.notes")}</span>
        <Textarea rows={2} value={c.notes ?? ""} onChange={(e) => setC({ ...c, notes: e.target.value || null })} />
      </label>

      <div className="flex items-center justify-between border-t pt-3">
        <span className={cn("text-xs", dirty ? "text-warn" : "text-muted-foreground")}>
          {dirty ? t("interview.cases.modifications_non_enregistrees") : initial.updated_at ? t("common.enregistre_le_datetime", { dateTime: dateTime(initial.updated_at) }) : t("common.enregistre")}
        </span>
        <Button onClick={() => save.mutate()} disabled={!dirty || save.isPending}>
          {save.isPending ? t("common.enregistrement") : t("interview.cases.enregistrer_le_cas")}
        </Button>
      </div>
    </div>
  )
}

const emptyField = (): CaseField => ({ text: "", contradictions: "", missing: "", evidence: [] })

export function CasesTab({ id, detail }: { id: string; detail: InterviewDetail }) {
  const qc = useQueryClient()
  const cases = useQuery({ queryKey: ["cases", id], queryFn: () => api.cases(id) })
  const [selected, setSelected] = useState<string | null>(null)
  const units = useMemo(() => detail.segmentation?.units ?? [], [detail.segmentation])

  const add = useMutation({
    mutationFn: () =>
      api.addCase(id, {
        label: `Cas ${(cases.data?.length ?? 0) + 1}`,
        trigger: emptyField(),
        alternatives: emptyField(),
        constraints: emptyField(),
        choice: emptyField(),
        result: emptyField(),
        units: [],
        decision: null,
        granularity: null,
        carrier_chosen: null,
        carriers_rejected: [],
        account_type: null,
        swiss_link: null,
        stage: null,
        outcome_status: null,
        notes: null,
      }),
    onSuccess: (c) => {
      qc.invalidateQueries({ queryKey: ["cases"] })
      qc.invalidateQueries({ queryKey: ["cross-case"] })
      invalidateInterview(qc, id)
      setSelected(c.case_id)
      toast.success(t("interview.cases.case_id_cree", { case_id: c.case_id }))
    },
    onError: (e) => reportError(e, t("interview.cases.cas_non_cree")),
  })

  if (!detail.segmentation)
    return (
      <EmptyState
        title={t("interview.cases.aucune_unite_pour_l_instant")}
        action={
          <Button asChild>
            <a href={interviewHref(id, "sources")}>{t("interview.cases.aller_aux_sources")}</a>
          </Button>
        }
      >
        {t("interview.cases.un_cas_s_appuie_sur")}
      </EmptyState>
    )
  if (cases.isLoading) return <Skeleton className="h-72 w-full" />
  if (cases.error) return <Banner tone="danger" title={t("interview.cases.cas_indisponibles")}>{cases.error.message}</Banner>

  const list = cases.data ?? []
  const current = list.find((c) => c.case_id === selected) ?? list[0] ?? null

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[17rem_minmax(0,1fr)]">
      <aside className="bg-card flex flex-col rounded-md border">
        <div className="flex items-center justify-between border-b px-3 py-2">
          <h2 className="text-sm font-semibold">{t("interview.cases.cas")}</h2>
          <Button size="xs" variant="outline" onClick={() => add.mutate()} disabled={add.isPending}>
            <Plus /> {t("interview.cases.nouveau_cas")}
          </Button>
        </div>
        {list.length === 0 ? (
          <p className="text-muted-foreground p-3 text-sm">
            {t("interview.cases.aucun_cas_pour_l_instant")}
          </p>
        ) : (
          <ul className="flex flex-col">
            {list.map((c) => {
              const filled = CASE_FIELDS.filter((k) => fieldFilled(c, k)).length
              return (
                <li key={c.case_id}>
                  <button
                    type="button"
                    onClick={() => setSelected(c.case_id)}
                    className={cn("flex w-full flex-col items-start gap-0.5 border-b px-3 py-2 text-left", current?.case_id === c.case_id ? "bg-accent/60" : "hover:bg-muted/50")}
                  >
                    <span className="ident text-xs">{c.case_id}</span>
                    <span className="text-sm">{c.label || t("interview.cases.cas_sans_titre")}</span>
                    <span className="text-muted-foreground text-xs">
                      {t("interview.cases.filled_champs_remplis_sur_5", { n: c.units.length, filled })}
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>
        )}
      </aside>
      <section className="bg-card rounded-md border p-4">
        {current ? (
          <CaseEditor key={`${current.case_id}:${current.updated_at ?? ""}`} initial={current} units={units} onSaved={(c) => setSelected(c.case_id)} />
        ) : (
          <p className="text-muted-foreground text-sm">{t("interview.cases.creez_un_cas_pour_synthetiser")}</p>
        )}
      </section>
    </div>
  )
}
