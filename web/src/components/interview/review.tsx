// Décisions de la revue IA (phase 2) : chaque désaccord, puis un échantillon
// d'accords tiré avec une graine. Le candidat garde la décision finale ; les
// chiffres d'accord sont descriptifs et rangés dans un volet secondaire.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, ChipList, CodeChip, EmptyState, Section } from "@/components/bits"
import { CodePicker } from "@/components/code-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible"
import { Progress } from "@/components/ui/progress"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { api } from "@/lib/api"
import { diffCodes, familyOf, sameSet } from "@/lib/codes"
import { dateTime, percent, plural, roleLabel, shortHash } from "@/lib/format"
import { reportError } from "@/lib/notify"
import { invalidateInterview } from "@/lib/queries"
import { candidateView, orderReview, rationaleRequired, reviewProgress } from "@/lib/review"
import { interviewHref, unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import type { AgreementReport, Arbitration, ArbitrationChoice, Code, ReviewItem, ReviewSnapshotRef } from "@/lib/types"

function choiceLabel(c: ArbitrationChoice): string {
  if (c.type === "human") return "Votre codage gardé"
  if (c.type === "ai") return `${c.run_label} retenu`
  return "Décision composée"
}

function DiffRow({ label, codes, other, book }: { label: string; codes: string[]; other: string[][]; book: Code[] }) {
  const union = [...new Set(other.flat())]
  return (
    <div className="grid grid-cols-[8.5rem_1fr] items-center gap-2 text-sm">
      <span className="text-muted-foreground text-xs">{label}</span>
      <span className="flex flex-wrap gap-1">
        {codes.length === 0 && <span className="text-muted-foreground text-xs">aucun code</span>}
        {diffCodes(codes, union).map(({ code, mark }) => (
          <CodeChip key={code} code={code} family={familyOf(code, book)} mark={mark} title={mark === "only" ? "De ce côté seulement" : "Commun"} />
        ))}
      </span>
    </div>
  )
}

function KappaTable({ report }: { report: AgreementReport }) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Code</TableHead>
          <TableHead className="text-right">{report.label_a}</TableHead>
          <TableHead className="text-right">{report.label_b}</TableHead>
          <TableHead className="text-right">Accord brut</TableHead>
          <TableHead className="text-right">Kappa de Cohen</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {report.per_code.map((c) => (
          <TableRow key={c.code}>
            <TableCell>
              <CodeChip code={c.code} />
            </TableCell>
            <TableCell className="text-right tabular-nums">{c.n_a}</TableCell>
            <TableCell className="text-right tabular-nums">{c.n_b}</TableCell>
            <TableCell className="text-right tabular-nums">{percent(c.raw, 1)}</TableCell>
            <TableCell className="text-right tabular-nums">{c.kappa === null ? "indéfini" : c.kappa.toFixed(3)}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  )
}

function Decision({ a }: { a: Arbitration }) {
  return (
    <div className="text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{choiceLabel(a.choice)}</span>
        <ChipList codes={a.final_codes} />
      </div>
      {a.rationale && <p className="mt-1">{a.rationale}</p>}
      <p className="text-muted-foreground mt-0.5 text-xs">
        Décidé par {a.decided_by}, {dateTime(a.decided_at)}
      </p>
    </div>
  )
}

function ReviewCard({ id, item, runs, book }: { id: string; item: ReviewItem; runs: string[]; book: Code[] }) {
  const qc = useQueryClient()
  const decided = item.decision
  const required = rationaleRequired(item)
  const [editing, setEditing] = useState(!decided)
  const [choice, setChoice] = useState<ArbitrationChoice>(decided?.choice ?? { type: "human" })
  const [custom, setCustom] = useState<string[]>(decided?.choice.type === "custom" ? decided.final_codes : item.human)
  const [rationale, setRationale] = useState("")

  const finalCodes = choice.type === "human" ? item.human : choice.type === "ai" ? item.ai[choice.run_label] ?? [] : custom
  const save = useMutation({
    mutationFn: () => api.decide(id, item.unit_id, choice, finalCodes, rationale.trim()),
    onSuccess: () => {
      toast.success(`${item.unit_id} revue`)
      setEditing(false)
      setRationale("")
      qc.invalidateQueries({ queryKey: ["review", id] })
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Décision refusée"),
  })

  const view = candidateView(item)
  const aiSides: [string, string[]][] = runs.map((r) => [`Modèle ${r}`, item.ai[r] ?? []])
  const aiCodes = aiSides.map((s) => s[1])
  // Rows: the frozen candidate and, when revised, the current candidate, each set
  // against the AI codes; then each AI run set against the frozen candidate and the other runs.
  const rows: { label: string; codes: string[]; other: string[][] }[] = [
    { label: "Vous (figé)", codes: view.frozen, other: aiCodes },
    ...(view.revised ? [{ label: "Actuel", codes: view.current, other: aiCodes }] : []),
    ...aiSides.map(([label, codes], i) => ({ label, codes, other: [view.frozen, ...aiCodes.filter((_, j) => j !== i)] })),
  ]
  const sample = item.kind !== "disagreement"
  const canSave = finalCodes.length > 0 && (!required || rationale.trim().length > 0) && !save.isPending

  return (
    <article className={cn("bg-card rounded-md border", decided && !editing && "opacity-90", sample && "border-dashed")}>
      <header className="flex flex-wrap items-center justify-between gap-2 border-b px-4 py-2">
        <div className="flex items-center gap-2 text-sm">
          <a href={unitHref(item.unit_id)} className="ident font-semibold hover:underline">
            {item.unit_id}
          </a>
          <span className="text-muted-foreground tabular-nums">{item.unit.timestamp}</span>
          <Badge variant="outline">{roleLabel(item.unit.role)}</Badge>
          <span className="text-muted-foreground text-xs">
            {sample ? "accord tiré au sort : vérifiez que la lecture commune tient" : "votre codage diffère de celui du modèle"}
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5">
          {view.revised && <Badge variant="outline" className="text-warn border-warn/40">révisé depuis la capture</Badge>}
          {decided ? (
            <Badge className="bg-ok-soft text-ok">{sample ? "revue" : "décidé"}</Badge>
          ) : (
            <Badge className={sample ? "bg-muted text-muted-foreground" : "bg-warn-soft text-warn"}>{sample ? "à regarder" : "à décider"}</Badge>
          )}
        </div>
      </header>
      <div className="grid gap-4 p-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <p className="verbatim">{item.unit.text}</p>
        <div className="flex flex-col gap-2">
          {rows.map((row) => (
            <DiffRow key={row.label} label={row.label} codes={row.codes} other={row.other} book={book} />
          ))}
          {!sample && <p className="text-muted-foreground text-xs">Les codes cernés n'apparaissent que d'un côté.</p>}
        </div>
      </div>

      <div className="border-t px-4 py-3">
        {decided && !editing ? (
          <div className="flex items-start justify-between gap-3">
            <Decision a={decided} />
            <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
              Revoir
            </Button>
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Décision">
              <Button size="sm" variant={choice.type === "human" ? "default" : "outline"} onClick={() => setChoice({ type: "human" })}>
                Garder le mien
              </Button>
              {runs.map((r) => (
                <Button
                  key={r}
                  size="sm"
                  variant={choice.type === "ai" && choice.run_label === r ? "default" : "outline"}
                  onClick={() => setChoice({ type: "ai", run_label: r })}
                  disabled={sameSet(item.ai[r] ?? [], item.human)}
                >
                  Prendre {r}
                </Button>
              ))}
              <Button size="sm" variant={choice.type === "custom" ? "default" : "outline"} onClick={() => setChoice({ type: "custom" })}>
                Composer
              </Button>
            </div>
            {choice.type === "custom" && (
              <CodePicker allowed={book.filter((c) => c.active && !c.merged_into && c.origin !== "legacy")} value={custom} onChange={setCustom} phase={2} phase1Contexts={[]} />
            )}
            <div className="flex items-center gap-2 text-xs">
              <span className="text-muted-foreground">Codes finaux</span>
              <ChipList codes={finalCodes} book={book} empty="aucun choisi" />
            </div>
            <Textarea
              rows={2}
              placeholder={
                required
                  ? "Justification, obligatoire : ce que dit le répondant qui tranche la lecture."
                  : "Note, facultative : ce que cet accord montre du code."
              }
              value={rationale}
              onChange={(e) => setRationale(e.target.value)}
            />
            <div className="flex gap-2">
              <Button size="sm" disabled={!canSave} onClick={() => save.mutate()}>
                {decided ? "Enregistrer la décision révisée" : sample ? "Marquer comme revue" : "Enregistrer la décision"}
              </Button>
              {decided && (
                <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
                  Annuler
                </Button>
              )}
            </div>
          </div>
        )}
        {(item.history?.length ?? 0) > 1 && (
          <Collapsible className="mt-3">
            <CollapsibleTrigger className="text-primary text-xs hover:underline">{plural(item.history!.length, "décision")} dans l'historique</CollapsibleTrigger>
            <CollapsibleContent className="mt-2 flex flex-col gap-2 border-l-2 pl-3">
              {item.history!.map((a, i) => (
                <Decision key={i} a={a} />
              ))}
            </CollapsibleContent>
          </Collapsible>
        )}
      </div>
    </article>
  )
}

function SnapshotPanel({ refs, id }: { refs: ReviewSnapshotRef[]; id: string }) {
  const snaps = useQuery({ queryKey: ["snapshots", id], queryFn: () => api.snapshots(id) })
  if (!refs.length) return null
  return (
    <section className="bg-card rounded-md border px-4 py-3">
      <h2 className="mb-2 text-sm font-semibold">Références figées</h2>
      <p className="text-muted-foreground mb-2 text-xs">
        Le codage du candidat auquel chaque passe est comparée, figé à la génération des prompts. Les changements ultérieurs apparaissent comme révisés.
      </p>
      <ul className="flex flex-col gap-1 text-xs">
        {refs.map((s) => {
          const taken = snaps.data?.find((x) => x.snapshot_id === s.snapshot_id)?.created_at
          return (
            <li key={`${s.run_label}-${s.snapshot_id}`} className="flex flex-wrap items-center gap-x-3 gap-y-0.5">
              <span className="font-medium">{s.run_label}</span>
              <span className="text-muted-foreground">
                capture <span className="ident">{s.snapshot_id}</span>
              </span>
              <span className="text-muted-foreground">
                SHA-256 <span className="ident">{shortHash(s.sha256, 12)}</span>
              </span>
              {taken && <span className="text-muted-foreground">prise le {dateTime(taken)}</span>}
            </li>
          )
        })}
      </ul>
    </section>
  )
}

export function ReviewTab({ id }: { id: string }) {
  const review = useQuery({ queryKey: ["review", id], queryFn: () => api.review(id) })
  const agreement = useQuery({ queryKey: ["agreement", id], queryFn: () => api.agreement(id) })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const [openOnly, setOpenOnly] = useState(false)

  if (review.isLoading) return <Skeleton className="h-96 w-full" />
  if (review.error) return <Banner tone="danger" title="Décisions indisponibles">{review.error.message}</Banner>
  const r = review.data!
  const items = r.items ?? []
  const prog = reviewProgress(items)
  const shown = orderReview(items, openOnly)
  const reports = agreement.data?.reports ?? []

  if (!r.runs.length)
    return (
      <EmptyState
        title="Rien à revoir pour l'instant"
        action={
          <Button asChild>
            <a href={interviewHref(id, "ai")}>Aller à la revue IA</a>
          </Button>
        }
      >
        Importez une réponse du modèle qui couvre les unités d'un prompt ; ses codes sont alors confrontés aux vôtres ici.
      </EmptyState>
    )

  return (
    <div className="flex flex-col gap-5">
      <div className="bg-card flex flex-wrap items-center gap-x-8 gap-y-3 rounded-md border px-4 py-3">
        <div className="flex min-w-[14rem] flex-1 items-center gap-3">
          <Progress value={prog.disagreements ? (prog.disagreementsDecided / prog.disagreements) * 100 : 100} className="h-1.5 flex-1" />
          <span className="text-sm tabular-nums">
            {prog.disagreementsDecided} sur {plural(prog.disagreements, "désaccord")} décidés
          </span>
        </div>
        <span className="text-sm tabular-nums">
          {prog.sampledReviewed} sur {plural(prog.sampled, "accord tiré au sort", "accords tirés au sort")} regardés
        </span>
        <span className="text-muted-foreground text-xs">
          {r.seeds?.length ? (
            <>
              Échantillon tiré avec la graine{" "}
              {r.seeds.map((s, i) => (
                <span key={s.run_label}>
                  {i > 0 && ", "}
                  <span className="ident">{String(s.seed)}</span> ({s.run_label})
                </span>
              ))}
            </>
          ) : (
            "Aucun échantillon tiré"
          )}
        </span>
        <label className="ml-auto flex items-center gap-2 text-sm">
          <Checkbox checked={openOnly} onCheckedChange={(v) => setOpenOnly(v === true)} />
          À décider seulement
        </label>
      </div>

      <SnapshotPanel refs={r.snapshots ?? []} id={id} />

      {prog.complete && prog.disagreements > 0 && (
        <Banner tone="ok" title="Tous les désaccords sont décidés">
          Le codage final reprend vos décisions là où vous en avez pris une, et vos codes actuels ailleurs.
        </Banner>
      )}

      {shown.length === 0 ? (
        <p className="text-muted-foreground text-sm">{items.length ? "Plus rien à décider." : "Votre codage et celui du modèle concordent sur chaque unité."}</p>
      ) : (
        shown.map((item) => <ReviewCard key={`${item.kind}-${item.unit_id}`} id={id} item={item} runs={r.runs} book={book.data?.codes ?? []} />)
      )}

      {reports.length > 0 && (
        <Collapsible>
          <Section
            title={
              <CollapsibleTrigger className="hover:underline">Chiffres d'accord, à titre descriptif</CollapsibleTrigger>
            }
            aside="calculés sans aucun modèle ; rapportés, jamais une cible"
          >
            <div className="text-muted-foreground flex flex-col gap-1 text-sm">
              {reports.map((rep) => (
                <span key={`${rep.label_a}-${rep.label_b}`}>
                  {rep.label_a} et {rep.label_b} : mêmes codes sur {rep.n_exact} unités sur {rep.n_units} (
                  {percent(rep.n_units ? rep.n_exact / rep.n_units : 0)})
                </span>
              ))}
            </div>
            <CollapsibleContent className="mt-3 flex flex-col gap-3">
              {reports.map((rep) => (
                <div key={`${rep.label_a}-${rep.label_b}-t`} className="overflow-x-auto rounded-md border">
                  <KappaTable report={rep} />
                </div>
              ))}
              <p className="text-muted-foreground text-xs">
                Accord exact sur l'ensemble des codes, puis kappa de Cohen par code sur la présence ou l'absence, comme dans compute_agreement.py.
              </p>
            </CollapsibleContent>
          </Section>
        </Collapsible>
      )}
    </div>
  )
}
