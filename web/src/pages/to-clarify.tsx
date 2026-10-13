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
import { api } from "@/lib/api"
import { groupByLabel, type LabelGroup } from "@/lib/clarify"
import { codeKeyFrom, phase2Codes } from "@/lib/codes"
import { QuestionPicker, ToePicker } from "@/components/code-fields"
import { dateTime, plural } from "@/lib/format"
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
              reason: `Issu de l'étiquette à clarifier « ${group.label} »`,
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
      toast.success(`« ${group.label} » résolue en ${target} sur ${plural(res.resolved.length, "unité")}`)
    },
    onError: (e) => reportError(e, "Étiquette non résolue"),
  })
  const valid = mode === "new" ? !!target && !!label.trim() && !!definition.trim() : mapped.length === 1

  return (
    <div className="flex flex-col gap-3 border-t pt-3">
      <div className="flex gap-1.5" role="radiogroup" aria-label="Type de résolution">
        <Button size="sm" variant={mode === "new" ? "default" : "outline"} onClick={() => setMode("new")}>
          Créer un code de terrain
        </Button>
        <Button size="sm" variant={mode === "map" ? "default" : "outline"} onClick={() => setMode("map")}>
          Rattacher à un code existant
        </Button>
      </div>
      {mode === "new" ? (
        <div className="grid gap-3 md:grid-cols-2">
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Clé du code</span>
            <Input value={key} onChange={(e) => setKey(e.target.value)} className="ident uppercase" />
          </label>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Libellé</span>
            <Input value={label} onChange={(e) => setLabel(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1 text-xs md:col-span-2">
            <span className="text-muted-foreground">Définition, obligatoire : ce qu'une unité doit dire pour porter le code</span>
            <Textarea rows={2} value={definition} onChange={(e) => setDefinition(e.target.value)} />
          </label>
          <div className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Rattachement TOE</span>
            <ToePicker value={toe} onChange={setToe} />
          </div>
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Exemple : identifiant d'unité ou courte citation</span>
            <Input value={example} onChange={(e) => setExample(e.target.value)} />
          </label>
          <div className="flex flex-col gap-1 text-xs md:col-span-2">
            <span className="text-muted-foreground">Questions de recherche concernées</span>
            <QuestionPicker questions={questions} value={rqs} onChange={setRqs} />
          </div>
        </div>
      ) : (
        <CodePicker allowed={codes} value={mapped} onChange={(v) => setMapped(v.filter((c) => c !== "NA").slice(-1))} phase={2} phase1Contexts={[]} />
      )}
      <Textarea rows={2} placeholder="Justification : pourquoi cette étiquette correspond à ce code" value={rationale} onChange={(e) => setRationale(e.target.value)} />
      <div className="flex flex-wrap items-center gap-4">
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={all} onCheckedChange={(v) => setAll(v === true)} />
          Appliquer aux {plural(group.items.length, "unité")} portant cette étiquette
        </label>
        <Button size="sm" disabled={!valid || resolve.isPending} onClick={() => resolve.mutate()}>
          Résoudre en {target || "un code"}
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
            {plural(group.items.length, "unité")} dans {group.interviews.join(", ")}
          </span>
        </div>
        <Button size="sm" variant={open ? "ghost" : "outline"} onClick={() => setOpen((o) => !o)}>
          {open ? "Fermer" : "Résoudre"}
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
              {tt.note && <p className="text-muted-foreground text-xs">Note : {tt.note}</p>}
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
        title="À clarifier"
        sub="Étiquettes provisoires posées sur des unités en phase 2. Chacune devient un code de terrain ou rejoint un code existant."
      />
      {items.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : open.length === 0 ? (
        <EmptyState title="Rien à clarifier">
          En phase 2, utilisez ? sur une unité qu'aucun code ne couvre encore pour lui donner une étiquette provisoire et une courte note.
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-3">
          {open.map((g) => (
            <GroupCard key={g.key} group={g} codes={codes} questions={questions.data ?? []} />
          ))}
        </div>
      )}

      {resolved.length > 0 && (
        <Section title="Résolues" aside={plural(resolved.length, "étiquette")} className="mt-6">
          <ul className="flex flex-col gap-2">
            {resolved.map((tt) => (
              <li key={tt.id} className="flex flex-wrap items-center gap-2 text-sm">
                <PendingChip label={tt.label} className="opacity-60" />
                <span className="text-muted-foreground">vers</span>
                <CodeChip code={tt.resolved!.code} />
                <Badge variant="outline" className={cn("text-xs")}>
                  {tt.resolved!.kind === "new_code" ? "nouveau code de terrain" : "rattachée"}
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
