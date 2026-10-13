// Labels to clarify, across interviews: each label is resolved by creating a
// field code or by mapping it to an existing code, for every unit carrying it.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeChip, EmptyState, PageTitle, PendingChip, Section } from "@/components/bits"
import { CodePicker } from "@/components/code-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { groupByLabel, type LabelGroup } from "@/lib/clarify"
import { codeKeyFrom, phase2Codes } from "@/lib/codes"
import { QuestionPicker, ToePicker } from "@/components/code-fields"
import { dateTime } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { Code, ResearchQuestion, Toe } from "@/lib/types"

function ResolveForm({ group, codes, questions }: { group: LabelGroup; codes: Code[]; questions: ResearchQuestion[] }) {
  const qc = useQueryClient()
  const [mode, setMode] = useState<"new" | "map">("new")
  const [key, setKey] = useState(codeKeyFrom(group.label))
  const [label, setLabel] = useState(group.label)
  const [definition, setDefinition] = useState("")
  const [toe, setToe] = useState<Toe | null>(null)
  const [rqs, setRqs] = useState<string[]>([])
  const [example, setExample] = useState(group.items[0]?.unit_id ?? "")
  const [mapped, setMapped] = useState<string[]>([])
  const [rationale, setRationale] = useState("")
  const [all, setAll] = useState(true)

  const target = mode === "new" ? key.trim().toUpperCase() : mapped[0] ?? ""
  const resolve = useMutation({
    mutationFn: () => {
      const newCode: Code | undefined =
        mode === "new"
          ? {
              code: target,
              family: toe === "T" ? "TECH" : toe === "O" ? "ORG" : toe === "E" ? "ENV" : "OTHER",
              label: label.trim(),
              definition: definition.trim(),
              origin: "field",
              active: true,
              in_prompt: true,
              toe,
              research_questions: rqs,
              merged_into: null,
              version: 1,
              example: example.trim() || null,
              reason: t("to_clarify.issu_de_l_etiquette_a", { label: group.label }),
              created_at: null,
            }
          : undefined
      return api.resolveToClarify(group.items[0].id, {
        kind: mode === "new" ? "new_code" : "mapped",
        code: target,
        rationale: rationale.trim(),
        all_with_label: all,
        new_code: newCode,
      })
    },
    onSuccess: (res) => {
      qc.setQueryData(["codebook"], res.codebook)
      qc.invalidateQueries({ queryKey: ["to-clarify"] })
      qc.invalidateQueries({ predicate: (q) => ["units", "interview", "interviews", "chain", "review", "final"].includes(String(q.queryKey[0])) })
      toast.success(t("to_clarify.label_resolue_en_target_sur", { label: group.label, target, items: t("common.n_unites", { n: res.resolved.length }) }))
    },
    onError: (e) => reportError(e, t("to_clarify.etiquette_non_resolue")),
  })
  const valid = mode === "new" ? !!target && !!label.trim() && !!definition.trim() : mapped.length === 1

  return (
    <div className="flex flex-col gap-3 border-t pt-3">
      <div className="flex gap-1.5" role="radiogroup" aria-label={t("to_clarify.type_de_resolution")}>
        <Button size="sm" variant={mode === "new" ? "default" : "outline"} onClick={() => setMode("new")}>
          {t("to_clarify.creer_un_code_de_terrain")}
        </Button>
        <Button size="sm" variant={mode === "map" ? "default" : "outline"} onClick={() => setMode("map")}>
          {t("common.rattacher_a_un_code_existant")}
        </Button>
      </div>
      {mode === "new" ? (
        <div className="grid gap-3 md:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">{t("to_clarify.cle_du_code")}</span>
            <Input value={key} onChange={(e) => setKey(e.target.value)} className="ident uppercase" />
          </label>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">{t("common.libelle")}</span>
            <Input value={label} onChange={(e) => setLabel(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1 text-xs md:col-span-2">
            <span className="text-muted-foreground">{t("to_clarify.definition_obligatoire_ce_qu_une")}</span>
            <Textarea rows={2} value={definition} onChange={(e) => setDefinition(e.target.value)} />
          </label>
          <div className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">{t("to_clarify.rattachement_toe")}</span>
            <ToePicker value={toe} onChange={setToe} />
          </div>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">{t("to_clarify.exemple_identifiant_d_unite_ou")}</span>
            <Input value={example} onChange={(e) => setExample(e.target.value)} />
          </label>
          <div className="flex flex-col gap-1 text-xs md:col-span-2">
            <span className="text-muted-foreground">{t("to_clarify.questions_de_recherche_concernees")}</span>
            <QuestionPicker questions={questions} value={rqs} onChange={setRqs} />
          </div>
        </div>
      ) : (
        <CodePicker allowed={codes} value={mapped} onChange={(v) => setMapped(v.filter((c) => c !== "NA").slice(-1))} phase={2} phase1Contexts={[]} />
      )}
      <Textarea rows={2} placeholder={t("to_clarify.justification_pourquoi_cette_etiquet")} value={rationale} onChange={(e) => setRationale(e.target.value)} />
      <div className="flex flex-wrap items-center gap-4">
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={all} onCheckedChange={(v) => setAll(v === true)} />
          {t("to_clarify.appliquer_aux_items_portant_cette", { items: t("common.n_unites", { n: group.items.length }) })}
        </label>
        <Button size="sm" disabled={!valid || resolve.isPending} onClick={() => resolve.mutate()}>
          {t("to_clarify.resoudre_en")} {target || t("to_clarify.un_code")}
        </Button>
      </div>
    </div>
  )
}

function GroupCard({ group, codes, questions }: { group: LabelGroup; codes: Code[]; questions: ResearchQuestion[] }) {
  const [open, setOpen] = useState(false)
  return (
    <article className="bg-card rounded-md border">
      <header className="flex flex-wrap items-center justify-between gap-2 px-4 py-2.5">
        <div className="flex flex-wrap items-center gap-2">
          <PendingChip label={group.label} />
          <span className="text-muted-foreground text-xs">
            {t("to_clarify.items_dans_join", { items: t("common.n_unites", { n: group.items.length }), join: group.interviews.join(", ") })}
          </span>
        </div>
        <Button size="sm" variant={open ? "ghost" : "outline"} onClick={() => setOpen((o) => !o)}>
          {open ? t("to_clarify.fermer") : t("to_clarify.resoudre")}
        </Button>
      </header>
      <ul className="flex flex-col border-t">
        {group.items.map((tt) => (
          <li key={tt.id} className="grid gap-x-3 gap-y-0.5 border-b px-4 py-2 last:border-b-0 md:grid-cols-[8rem_1fr]">
            <a href={unitHref(tt.unit_id)} className="ident text-xs hover:underline">
              {tt.unit_id}
            </a>
            <div className="min-w-0">
              {tt.unit_text && <p className="verbatim line-clamp-2 text-sm">{tt.unit_text}</p>}
              {tt.note && <p className="text-muted-foreground text-xs">{t("common.note_note", { note: tt.note })}</p>}
            </div>
          </li>
        ))}
      </ul>
      {open && (
        <div className="px-4 pb-4">
          <ResolveForm group={group} codes={codes} questions={questions} />
        </div>
      )}
    </article>
  )
}

export function ToClarifyPage() {
  const items = useQuery({ queryKey: ["to-clarify", "all"], queryFn: () => api.toClarify() })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const questions = useQuery({ queryKey: ["questions"], queryFn: api.questions })

  const open = useMemo(() => groupByLabel((items.data ?? []).filter((tt) => !tt.resolved)), [items.data])
  const resolved = useMemo(() => (items.data ?? []).filter((tt) => tt.resolved).sort((a, b) => (b.resolved!.at ?? "").localeCompare(a.resolved!.at ?? "")), [items.data])
  const codes = phase2Codes(book.data?.codes ?? [])

  return (
    <>
      <PageTitle
        title={t("common.a_clarifier")}
        sub={t("to_clarify.etiquettes_provisoires_posees_sur_de")}
      />
      {items.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : open.length === 0 ? (
        <EmptyState title={t("to_clarify.rien_a_clarifier")}>
          {t("to_clarify.en_phase_2_utilisez_sur")}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-3">
          {open.map((g) => (
            <GroupCard key={g.key} group={g} codes={codes} questions={questions.data ?? []} />
          ))}
        </div>
      )}

      {resolved.length > 0 && (
        <Section title={t("to_clarify.resolues")} aside={t("to_clarify.n_etiquettes", { n: resolved.length })} className="mt-6">
          <ul className="flex flex-col gap-2">
            {resolved.map((tt) => (
              <li key={tt.id} className="flex flex-wrap items-center gap-2 text-sm">
                <PendingChip label={tt.label} className="opacity-60" />
                <span className="text-muted-foreground">{t("to_clarify.vers")}</span>
                <CodeChip code={tt.resolved!.code} />
                <Badge variant="outline" className={cn("text-xs")}>
                  {tt.resolved!.kind === "new_code" ? t("to_clarify.nouveau_code_de_terrain") : t("to_clarify.rattachee")}
                </Badge>
                <a href={unitHref(tt.unit_id)} className="ident text-xs hover:underline">
                  {tt.unit_id}
                </a>
                <span className="text-muted-foreground text-xs">
                  {tt.resolved!.by}, {dateTime(tt.resolved!.at)}
                  {tt.resolved!.rationale ? `: ${tt.resolved!.rationale}` : ""}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      )}
    </>
  )
}
