// Codebook (design/macos/Codebook.dc.html): the tree of codes with counts
// (roll-up to the parent), and the inspector of the selected code. A code is
// moved by dragging it onto its future parent or with "Parent"; renaming,
// definition, includes and excludes, questions, aliases, merge, deactivation
// and the history all go through /api/codes. Every move, merge and
// deactivation carries a rationale and is dated by the server.

import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { CodeStats } from "@/components/codebook/code-stats"
import { CodeJournal, JournalDialog } from "@/components/codebook/journal"
import { SplitDialog } from "@/components/codebook/split-dialog"
import { WorklistDialog } from "@/components/codebook/worklist"
import { EmptyState, PageTitle } from "@/components/bits"
import { Alerts, Field, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Capsule, CapsuleItem, ToolbarActions, ToolbarSearch } from "@/components/toolbar"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { analysisApi } from "@/lib/analysis-api"
import { ApiError, api } from "@/lib/api"
import { codeColor, descendantIds, LATER_ID, possibleParents, wouldCycle } from "@/lib/code-tree"
import { useCodeSearchRows } from "@/lib/code-search"
import { matchedLine } from "@/lib/v2-text"
import { invalidateHarmonisation, keys, useCodes } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import { cn } from "@/lib/utils"
import type { CodeNode } from "@/lib/types"

const msg = (e: unknown) => (e instanceof ApiError ? e.message : String(e))

function useInvalidateCodes() {
  const qc = useQueryClient()
  return () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: keys.codes }),
      qc.invalidateQueries({ queryKey: ["codebook"] }),
      qc.invalidateQueries({ queryKey: ["chain"] }),
      qc.invalidateQueries({ queryKey: ["state"] }),
      invalidateHarmonisation(qc),
    ])
}

/** "ECO-12" style origin text of a code: literature, field, system. */
function originLabel(c: CodeNode): string {
  if (c.system) return "système"
  if (c.origin === "literature") return "littérature"
  if (c.origin === "field") return "terrain"
  if (c.origin === "phase1") return "catégorie"
  return c.origin
}

export function CodebookPage() {
  const codesQ = useCodes()
  const questionsQ = useQuery({ queryKey: ["questions"], queryFn: api.questions })
  const refresh = useInvalidateCodes()
  const [search, setSearch] = useState("")
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set())
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [dragId, setDragId] = useState<string | null>(null)
  const [dropId, setDropId] = useState<string | null>(null)
  const [dialog, setDialog] = useState<"new" | "history" | "csv" | "journal" | null>(null)
  const [worklist, setWorklist] = useState<number | null>(null)

  const codes = useMemo(() => codesQ.data?.codes ?? [], [codesQ.data])
  const found = useCodeSearchRows(codes, search, { layout: "tree", collapsed })
  const rows = found.rows
  const selected = codes.find((c) => c.code_id === selectedId) ?? null
  const nActive = codes.filter((c) => c.active && !c.system && !c.merged_into).length
  const nMerged = codes.filter((c) => c.merged_into).length
  const maxCount = Math.max(1, ...codes.map((c) => c.counts.passages_rollup))

  const move = useMutation({
    mutationFn: (v: { id: string; parent: string | null }) => api.moveCode(v.id, v.parent, "Déplacé dans l'arbre"),
    onSuccess: () => refresh(),
    onError: (e) => reportError(e, "Déplacement refusé"),
  })

  const drop = (target: CodeNode) => {
    const id = dragId
    setDragId(null)
    setDropId(null)
    if (!id || id === target.code_id) return
    if (wouldCycle(codes, id, target.code_id)) {
      toast.error("Un code ne peut pas devenir sous-code de l'un de ses sous-codes.")
      return
    }
    move.mutate({ id, parent: target.code_id })
  }

  const toggle = (id: string) =>
    setCollapsed((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })

  return (
    <>
      <PageTitle
        title="Codebook"
        sub={codesQ.data ? `Version ${codesQ.data.codebook_version} · ${nActive} codes actifs · ${nMerged} fusionné${nMerged > 1 ? "s" : ""}` : " "}
      />
      <ToolbarActions>
        <Capsule>
          <CapsuleItem onClick={() => setDialog("history")}>Historique</CapsuleItem>
          <span className="vs" />
          <CapsuleItem onClick={() => setDialog("journal")}>Journal</CapsuleItem>
          <span className="vs" />
          <CapsuleItem onClick={() => setDialog("csv")}>Importer CSV…</CapsuleItem>
        </Capsule>
        <Capsule>
          <CapsuleItem aria-label="Nouveau code" title="Nouveau code" onClick={() => setDialog("new")}>
            +
          </CapsuleItem>
        </Capsule>
        <ToolbarSearch value={search} onChange={setSearch} placeholder="Filtrer les codes" />
      </ToolbarActions>

      <div className="content">
        <section className="tree" aria-label="Arbre des codes">
          {codesQ.isLoading ? (
            <Skeleton className="h-64 w-full" />
          ) : codes.length === 0 ? (
            <EmptyState
              title="Le codebook est vide"
              action={
                <Button onClick={() => setDialog("new")}>Nouveau code</Button>
              }
            >
              Créez un premier code, importez un fichier CSV, ou créez vos codes depuis les passages en codant.
            </EmptyState>
          ) : (
            <>
              <table className="tbl ol">
                <thead>
                  <tr>
                    <th>Code</th>
                    <th>Origine</th>
                    <th>Questions</th>
                    <th>Passages</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map(({ node: c, depth, hasChildren }) => {
                    const on = c.code_id === selectedId
                    const open = !collapsed.has(c.code_id)
                    return (
                      <tr
                        key={c.code_id}
                        className={cn("row-click", on && "sel", dropId === c.code_id && "drop", depth > 0 && "child")}
                        data-state={on ? "selected" : undefined}
                        aria-selected={on}
                        draggable={!c.system}
                        onDragStart={() => setDragId(c.code_id)}
                        onDragEnd={() => {
                          setDragId(null)
                          setDropId(null)
                        }}
                        onDragOver={(e) => {
                          if (dragId && dragId !== c.code_id) {
                            e.preventDefault()
                            setDropId(c.code_id)
                          }
                        }}
                        onDrop={(e) => {
                          e.preventDefault()
                          drop(c)
                        }}
                        onClick={() => setSelectedId(c.code_id)}
                      >
                        <td>
                          <span className="nm" style={{ paddingLeft: depth * 18, opacity: c.active ? 1 : 0.5 }}>
                            <button
                              type="button"
                              className="tri"
                              aria-label={hasChildren ? (open ? `Replier ${c.name}` : `Déplier ${c.name}`) : undefined}
                              disabled={!hasChildren}
                              aria-hidden={!hasChildren || undefined}
                              tabIndex={hasChildren ? undefined : -1}
                              onClick={(e) => {
                                e.stopPropagation()
                                toggle(c.code_id)
                              }}
                            >
                              {hasChildren && (
                                <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor" aria-hidden="true">
                                  <path d={open ? "M1.5 3h7L5 7.5z" : "M3 1.5v7L7.5 5z"} />
                                </svg>
                              )}
                            </button>
                            <span className="sw" style={{ ["--k" as string]: codeColor(c.code_id) }} />
                            <span style={{ display: "flex", flexDirection: "column", minWidth: 0 }}>
                              <span>
                                {c.name}
                                {!c.active && !c.system && <span className="small l2"> inactif</span>}
                              </span>
                              {matchedLine(found.matched.get(c.code_id)) && <span className="small l2">{matchedLine(found.matched.get(c.code_id))}</span>}
                            </span>
                          </span>
                        </td>
                        <td className="l2">{originLabel(c)}</td>
                        <td>
                          <span className="sq">
                            {c.research_questions.map((q) => (
                              <span key={q} className="badge">
                                {q}
                              </span>
                            ))}
                          </span>
                        </td>
                        <td>
                          {c.system ? (
                            <span className="l2">{c.counts.passages_rollup} à revoir</span>
                          ) : (
                            <span className="uses" title={`${c.counts.interviews_rollup} entretien${c.counts.interviews_rollup > 1 ? "s" : ""}`}>
                              <i className="bar">
                                <b style={{ width: `${(c.counts.passages_rollup / maxCount) * 100}%` }} />
                              </i>
                              {c.counts.passages_rollup}
                              {c.counts.passages_rollup !== c.counts.passages && <span className="small l2"> ({c.counts.passages} directs)</span>}
                            </span>
                          )}
                        </td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
              {rows.length === 0 && <p className="small l2">Aucun code ne correspond à « {search} ».</p>}
              <p className="small l2" style={{ margin: "2px 10px" }}>
                Glisser un code sur un autre en fait un sous-code. Un code posé sur un sous-code compte aussi pour son parent dans les exports.
              </p>
            </>
          )}
        </section>
        {selected && (
          <aside className="insp insp-sticky" style={{ flexBasis: 340 }} aria-label={selected.name}>
            <Inspector key={`${selected.code_id}:${selected.version}`} code={selected} codes={codes} questions={questionsQ.data ?? []} onMerged={() => setSelectedId(null)} onWorklist={setWorklist} />
          </aside>
        )}
      </div>

      <NewCodeDialog open={dialog === "new"} onClose={() => setDialog(null)} codes={codes} onCreated={(id) => setSelectedId(id)} />
      <HistoryDialog open={dialog === "history"} onClose={() => setDialog(null)} />
      <CsvDialog open={dialog === "csv"} onClose={() => setDialog(null)} />
      <JournalDialog open={dialog === "journal"} onClose={() => setDialog(null)} onOpenWorklist={setWorklist} />
      <WorklistDialog opId={worklist} onClose={() => setWorklist(null)} />
    </>
  )
}

// ------------------------------------------------------------- inspector

function Inspector({
  code,
  codes,
  questions,
  onMerged,
  onWorklist,
}: {
  code: CodeNode
  codes: CodeNode[]
  questions: { id: string; text: string }[]
  onMerged: () => void
  onWorklist: (opId: number) => void
}) {
  const [part, setPart] = useState<"sheet" | "stats" | "journal">("sheet")
  return (
    <div className="insp-in">
      <div className="seg" role="tablist" aria-label="Fiche du code" style={{ alignSelf: "stretch", display: "flex" }}>
        {(
          [
            ["sheet", "Fiche"],
            ["stats", "Chiffres"],
            ["journal", "Journal"],
          ] as const
        ).map(([k, label]) => (
          <button key={k} type="button" role="tab" aria-selected={part === k} className={cn("s", part === k && "on")} style={{ flex: 1 }} onClick={() => setPart(k)}>
            {label}
          </button>
        ))}
      </div>
      {part === "sheet" && <CodeSheet code={code} codes={codes} questions={questions} onMerged={onMerged} onWorklist={onWorklist} />}
      {part === "stats" && <CodeStats codeId={code.code_id} />}
      {part === "journal" && <CodeJournal code={code.code_id} onOpenWorklist={onWorklist} empty="Ce code n'a été ni fusionné ni scindé." />}
    </div>
  )
}

function CodeSheet({
  code,
  codes,
  questions,
  onMerged,
  onWorklist,
}: {
  code: CodeNode
  codes: CodeNode[]
  questions: { id: string; text: string }[]
  onMerged: () => void
  onWorklist: (opId: number) => void
}) {
  const refresh = useInvalidateCodes()
  const [name, setName] = useState(code.name)
  const [definition, setDefinition] = useState(code.definition)
  const [includes, setIncludes] = useState(code.includes)
  const [excludes, setExcludes] = useState(code.excludes)
  const [rqs, setRqs] = useState(code.research_questions)
  const [memo, setMemo] = useState(code.memo ?? "")
  const [alias, setAlias] = useState("")
  const [mergeOpen, setMergeOpen] = useState(false)
  const [splitOpen, setSplitOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const locked = code.system || code.code_id === LATER_ID

  const dirty =
    name !== code.name ||
    definition !== code.definition ||
    includes !== code.includes ||
    excludes !== code.excludes ||
    memo !== (code.memo ?? "") ||
    rqs.join("|") !== code.research_questions.join("|")

  const save = useMutation({
    mutationFn: async () => {
      const saved = await api.updateCode(code.code_id, {
        name: name.trim(),
        definition: definition.trim(),
        includes,
        excludes,
        research_questions: rqs,
      })
      // The memo has its own route and its own history; the reason of a change is no longer used for it.
      if (memo !== (code.memo ?? "")) await analysisApi.setCodeMemo(code.code_id, memo)
      return saved
    },
    onSuccess: async () => {
      setError(null)
      await refresh()
      toast.success("Code enregistré")
    },
    onError: (e) => setError(msg(e)),
  })
  const reparent = useMutation({
    mutationFn: (parent: string | null) => api.moveCode(code.code_id, parent, "Parent changé depuis la fiche du code"),
    onSuccess: () => refresh(),
    onError: (e) => reportError(e, "Déplacement refusé"),
  })
  const active = useMutation({
    mutationFn: () => api.setCodeActive(code.code_id, !code.active),
    onSuccess: () => refresh(),
    onError: (e) => reportError(e, "Changement refusé"),
  })
  const addAlias = useMutation({
    mutationFn: (a: string) => api.addAlias(code.code_id, a),
    onSuccess: () => {
      setAlias("")
      return refresh()
    },
    onError: (e) => reportError(e, "Alias refusé"),
  })
  const removeAlias = useMutation({
    mutationFn: (a: string) => api.removeAlias(code.code_id, a),
    onSuccess: () => refresh(),
    onError: (e) => reportError(e, "Alias non retiré"),
  })

  const parents = possibleParents(codes, code.code_id)
  const nameClash = name.trim() !== code.name && codes.some((c) => c.code_id !== code.code_id && c.name.toLowerCase() === name.trim().toLowerCase())

  return (
    <>
      <div className="r8" style={{ justifyContent: "space-between" }}>
        <span className="r8">
          <span className="sw" style={{ ["--k" as string]: codeColor(code.code_id) }} />
          <span className="b" style={{ fontSize: 14 }}>
            {code.name}
          </span>
        </span>
        <span className="small l2" title={`${code.counts.interviews_rollup} entretiens`}>
          {code.counts.passages_rollup} passage{code.counts.passages_rollup > 1 ? "s" : ""} · {code.counts.interviews_rollup} entretien
          {code.counts.interviews_rollup > 1 ? "s" : ""}
        </span>
      </div>

      <Field label="Nom" error={nameClash ? "Un autre code porte déjà ce nom." : undefined}>
        <Input value={name} disabled={locked} onChange={(e) => setName(e.target.value)} />
      </Field>
      <Field label="Définition">
        <textarea className={TEXTAREA} rows={3} value={definition} disabled={locked} onChange={(e) => setDefinition(e.target.value)} />
      </Field>
      <Field label="Inclut">
        <textarea className={TEXTAREA} rows={3} value={includes} disabled={locked} onChange={(e) => setIncludes(e.target.value)} />
      </Field>
      <Field label="Exclut">
        <textarea className={TEXTAREA} rows={3} value={excludes} disabled={locked} onChange={(e) => setExcludes(e.target.value)} />
      </Field>

      <div className="grp">
        <label className="row">
          <span className="k">Parent</span>
          <NativeSelect
            style={{ width: 150 }}
            value={code.parent_id ?? ""}
            disabled={locked || reparent.isPending}
            onChange={(e) => reparent.mutate(e.target.value || null)}
          >
            <option value="">Aucun</option>
            {parents.map((p) => (
              <option key={p.code_id} value={p.code_id}>
                {p.name}
              </option>
            ))}
          </NativeSelect>
        </label>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <span className="k">Questions</span>
          {questions.length === 0 && <span className="l2 small">Aucune question de recherche</span>}
          {questions.map((q) => (
            <label key={q.id} className="chk" title={q.text} style={{ display: "inline-flex", gap: 4, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={rqs.includes(q.id)}
                disabled={locked}
                onChange={(e) => setRqs(e.target.checked ? [...rqs, q.id] : rqs.filter((x) => x !== q.id))}
              />
              {q.id}
            </label>
          ))}
        </div>
        <div className="row">
          <span className="k">
            Origine
            <small>
              {originLabel(code)}
              {code.created_at ? `, créé le ${code.created_at.slice(0, 10)}` : ""}
            </small>
          </span>
        </div>
        <div className="row" style={{ flexWrap: "wrap" }}>
          <span className="k">
            Alias
            <small>Autres noms sous lesquels ce code est retrouvé, par exemple après une fusion.</small>
          </span>
          {code.aliases.map((a) => (
            <span key={a.alias} className="tok" title={a.reason}>
              {a.alias}
              <button type="button" aria-label={`Retirer l'alias ${a.alias}`} onClick={() => removeAlias.mutate(a.alias)}>
                ×
              </button>
            </span>
          ))}
          {!locked && (
            <form
              style={{ display: "flex", gap: 4 }}
              onSubmit={(e) => {
                e.preventDefault()
                if (alias.trim()) addAlias.mutate(alias.trim())
              }}
            >
              <Input value={alias} onChange={(e) => setAlias(e.target.value)} placeholder="Nouvel alias" aria-label="Nouvel alias" style={{ width: 130 }} />
              <Button type="submit" variant="outline" disabled={!alias.trim() || addAlias.isPending}>
                Ajouter
              </Button>
            </form>
          )}
        </div>
      </div>

      <Field label="Mémo du code">
        <textarea className={TEXTAREA} rows={3} value={memo} disabled={locked} onChange={(e) => setMemo(e.target.value)} />
      </Field>
      {error && <Alerts messages={[error]} />}
      {code.merged_into && <p className="small l2">Fusionné dans {codes.find((c) => c.code_id === code.merged_into)?.name ?? code.merged_into}.</p>}
      {!locked && (
        <div className="r8">
          <Button variant="outline" disabled={active.isPending} onClick={() => active.mutate()}>
            {code.active ? "Désactiver" : "Réactiver"}
          </Button>
          <Button variant="outline" onClick={() => setMergeOpen(true)}>
            Fusionner dans…
          </Button>
          <Button variant="outline" disabled={!code.active || !!code.merged_into} onClick={() => setSplitOpen(true)}>
            Scinder…
          </Button>
          <Button style={{ marginLeft: "auto" }} disabled={!dirty || !name.trim() || !definition.trim() || nameClash || save.isPending} onClick={() => save.mutate()}>
            Enregistrer
          </Button>
        </div>
      )}
      <span className="small l2">Chaque action est datée et motivée. Fusionner recode les passages en gardant la trace.</span>
      <MergeDialog open={mergeOpen} code={code} codes={codes} onClose={() => setMergeOpen(false)} onMerged={onMerged} />
      <SplitDialog open={splitOpen} code={code} codes={codes} onClose={() => setSplitOpen(false)} onSplit={onWorklist} />
    </>
  )
}

// ---------------------------------------------------------------- merge

function MergeDialog({
  open,
  code,
  codes,
  onClose,
  onMerged,
}: {
  open: boolean
  code: CodeNode
  codes: CodeNode[]
  onClose: () => void
  onMerged: () => void
}) {
  const refresh = useInvalidateCodes()
  const [into, setInto] = useState("")
  const [why, setWhy] = useState("")
  const [error, setError] = useState<string | null>(null)
  const exclude = useMemo(() => descendantIds(codes, code.code_id), [codes, code.code_id])
  const targets = codes.filter((c) => c.code_id !== code.code_id && !exclude.has(c.code_id) && c.active && !c.merged_into && !c.system)
  const merge = useMutation({
    mutationFn: () => api.mergeCodeV1(code.code_id, into, why.trim()),
    onSuccess: async (res) => {
      await refresh()
      toast.success(`${res.outcome.moved_passage_codings} codage${res.outcome.moved_passage_codings > 1 ? "s" : ""} déplacé${res.outcome.moved_passage_codings > 1 ? "s" : ""}`)
      onClose()
      onMerged()
    },
    onError: (e) => setError(msg(e)),
  })
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Fusionner « {code.name} »</DialogTitle>
          <DialogDescription>
            Les passages codés « {code.name} » prennent le code choisi. Le nom reste comme alias, la fusion est datée dans l'historique.
          </DialogDescription>
        </DialogHeader>
        <Field label="Fusionner dans">
          <NativeSelect value={into} onChange={(e) => setInto(e.target.value)}>
            <option value="">Choisir un code</option>
            {targets.map((c) => (
              <option key={c.code_id} value={c.code_id}>
                {c.name}
              </option>
            ))}
          </NativeSelect>
        </Field>
        <Field label="Motif, obligatoire">
          <Input value={why} onChange={(e) => setWhy(e.target.value)} />
        </Field>
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Annuler
          </Button>
          <Button disabled={!into || !why.trim() || merge.isPending} onClick={() => merge.mutate()}>
            Fusionner
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// ------------------------------------------------------------------ new

function NewCodeDialog({
  open,
  onClose,
  codes,
  onCreated,
}: {
  open: boolean
  onClose: () => void
  codes: CodeNode[]
  onCreated: (id: string) => void
}) {
  const refresh = useInvalidateCodes()
  const qc = useQueryClient()
  const [name, setName] = useState("")
  const [definition, setDefinition] = useState("")
  const [parent, setParent] = useState("")
  const [error, setError] = useState<string | null>(null)
  const clash = name.trim() !== "" && codes.some((c) => c.name.toLowerCase() === name.trim().toLowerCase())
  const create = useMutation({
    // One call: the server names the code as typed and generates its token.
    mutationFn: async () => (await analysisApi.createCode({ name: name.trim(), definition: definition.trim(), parent_id: parent || null })).code_id,
    onSuccess: async (id) => {
      qc.removeQueries({ queryKey: keys.codes })
      await refresh()
      toast.success(`Code « ${name.trim()} » créé`)
      if (id) onCreated(id)
      setName("")
      setDefinition("")
      setParent("")
      setError(null)
      onClose()
    },
    onError: (e) => setError(msg(e)),
  })
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Nouveau code</DialogTitle>
          <DialogDescription>Nommez un mécanisme avec des mots simples, et dites en une phrase ce qu'un passage doit exprimer pour le porter.</DialogDescription>
        </DialogHeader>
        <Field label="Nom" error={clash ? "Un code porte déjà ce nom." : undefined}>
          <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Obligation légale" />
        </Field>
        <Field label="Définition">
          <textarea className={TEXTAREA} rows={3} value={definition} onChange={(e) => setDefinition(e.target.value)} />
        </Field>
        <Field label="Parent, facultatif">
          <NativeSelect value={parent} onChange={(e) => setParent(e.target.value)}>
            <option value="">Aucun</option>
            {codes
              .filter((c) => c.active && !c.merged_into && !c.system)
              .map((c) => (
                <option key={c.code_id} value={c.code_id}>
                  {c.name}
                </option>
              ))}
          </NativeSelect>
        </Field>
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Annuler
          </Button>
          <Button disabled={!name.trim() || !definition.trim() || clash || create.isPending} onClick={() => create.mutate()}>
            Créer le code
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

// -------------------------------------------------------------- history

function HistoryDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const q = useQuery({ queryKey: ["codebook-revisions"], queryFn: api.codebookRevisions, enabled: open })
  const revisions = [...(q.data?.revisions ?? [])].reverse()
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Historique du codebook</DialogTitle>
          <DialogDescription>
            {q.data ? `Version ${q.data.codebook_version}. Chaque changement est daté, attribué et motivé.` : "Chargement..."}
          </DialogDescription>
        </DialogHeader>
        <div style={{ maxHeight: 360, overflow: "auto" }}>
          {revisions.length === 0 && !q.isLoading && <p className="small l2">Aucun changement enregistré.</p>}
          <table className="tbl">
            <tbody>
              {revisions.map((r) => (
                <tr key={r.rev}>
                  <td className="mono">v{r.rev}</td>
                  <td className="l2">{r.ts.slice(0, 16).replace("T", " ")}</td>
                  <td>{r.target}</td>
                  <td>{r.reason}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ------------------------------------------------------------------ csv

function CsvDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const refresh = useInvalidateCodes()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state, enabled: open })
  const [path, setPath] = useState("")
  const [report, setReport] = useState<{ added: number; updated: number; absent: number } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const run = useMutation({
    mutationFn: () => api.importCodebookFrom("csv", path.trim()),
    onSuccess: async (a) => {
      setReport(a.report)
      setError(null)
      await refresh()
    },
    onError: (e) => {
      setReport(null)
      setError(msg(e))
    },
  })
  const choose = async () => {
    const picked = await api.pick("file", "Choisir le fichier CSV")
    if (picked.path) setPath(picked.path)
  }
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Importer un codebook CSV</DialogTitle>
          <DialogDescription>Les codes sont ajoutés ou mis à jour, jamais supprimés. Les codes absents du fichier sont comptés et laissés tels quels.</DialogDescription>
        </DialogHeader>
        <Field label="Chemin du fichier">
          <div style={{ display: "flex", gap: 8 }}>
            <Input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/chemin/vers/codebook.csv" className="font-mono text-xs" />
            {state.data?.native_dialogs && (
              <Button variant="outline" onClick={choose}>
                Choisir…
              </Button>
            )}
          </div>
        </Field>
        {report && (
          <p role="status" className="text-sm">
            {`${report.added} ajouté${report.added > 1 ? "s" : ""}, ${report.updated} mis à jour, ${report.absent} absent${report.absent > 1 ? "s" : ""}`}
          </p>
        )}
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            Fermer
          </Button>
          <Button disabled={!path.trim() || run.isPending} onClick={() => run.mutate()}>
            Importer
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
