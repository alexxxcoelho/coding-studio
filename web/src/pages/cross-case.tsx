// Cross-case comparison: research questions as row groups, cases as columns.
// Cells list the codes mapped to the question that the case's final coding
// carries, with the units behind them. Then the five synthesis fields side
// by side. The point is to compare mechanisms across cases, not to count.

import { useQuery } from "@tanstack/react-query"

import { CodeChip, EmptyState, PageTitle, Section } from "@/components/bits"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import { familyOf } from "@/lib/codes"
import { cellCodes, cellFor, cellState, FIELD_LABELS, sharedCodes } from "@/lib/crosscase"
import { interviewHref, unitHref } from "@/lib/router"
import { cn } from "@/lib/utils"
import { CASE_FIELDS, type Case, type Code } from "@/lib/types"

function CaseHead({ c }: { c: Case }) {
  return (
    <th scope="col" className="min-w-[13rem] border-b px-3 py-2 text-left align-bottom font-normal">
      <a href={interviewHref(c.interview_id, "cases")} className="ident text-xs hover:underline">
        {c.case_id}
      </a>
      <div className="text-sm font-medium">{c.label || "Cas sans titre"}</div>
      <div className="text-muted-foreground text-xs">
        {[c.stage, c.outcome_status].filter(Boolean).join(", ") || "étape et statut non renseignés"}
      </div>
    </th>
  )
}

function UnitLinks({ units }: { units: string[] }) {
  return (
    <span className="inline-flex flex-wrap gap-x-1.5">
      {units.map((u) => (
        <a key={u} href={unitHref(u)} className="ident text-muted-foreground text-[0.7rem] hover:underline" title={`Ouvrir ${u}`}>
          U{u.split("-U").pop()}
        </a>
      ))}
    </span>
  )
}

export function CrossCasePage() {
  const data = useQuery({ queryKey: ["cross-case"], queryFn: api.crossCase })
  const book = useQuery({ queryKey: ["codebook"], queryFn: api.codebook })
  const codes: Code[] = book.data?.codes ?? []

  if (data.isLoading) return <Skeleton className="h-96 w-full" />
  if (data.error) return <EmptyState title="Vue transversale indisponible">{data.error.message}</EmptyState>
  const d = data.data!
  const questions = [...d.questions].sort((a, b) => a.ord - b.ord)
  const cases = d.cases

  return (
    <>
      <PageTitle
        title="Transversal"
        sub="Pour chaque question de recherche, les codes que porte chaque cas dans son codage final, avec les unités qui les soutiennent. À lire pour la convergence, le contraste et les conditions, pas pour la fréquence."
      />
      {cases.length === 0 ? (
        <EmptyState title="Aucun cas pour l'instant">Ouvrez un entretien, onglet Cas, et synthétisez ses projets : déclencheur, alternatives, contraintes, choix, résultat.</EmptyState>
      ) : questions.length === 0 ? (
        <EmptyState
          title="Aucune question de recherche pour l'instant"
          action={
            <a href="#/codebook" className="text-primary text-sm hover:underline">
              Définir les questions de recherche dans les paramètres de l'étude
            </a>
          }
        >
          La matrice relie les codes aux questions : définissez les questions, puis rattachez-y des codes.
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-6">
          <div className="bg-card overflow-x-auto rounded-md border">
            <table className="w-full border-collapse text-sm">
              <thead>
                <tr>
                  <th scope="col" className="w-64 border-b px-3 py-2 text-left align-bottom text-xs font-semibold">
                    Question de recherche
                  </th>
                  {cases.map((c) => (
                    <CaseHead key={c.case_id} c={c} />
                  ))}
                </tr>
              </thead>
              <tbody>
                {questions.map((q) => {
                  const shared = sharedCodes(d, q.id)
                  return (
                    <tr key={q.id} className="align-top">
                      <th scope="row" className="border-b px-3 py-3 text-left font-normal">
                        <div className="ident font-semibold">{q.id}</div>
                        <div className="text-muted-foreground text-xs leading-snug">{q.text}</div>
                        {shared.length > 0 && (
                          <div className="mt-2 flex flex-col gap-1 text-xs">
                            <span className="text-muted-foreground">Dans plusieurs cas</span>
                            <span className="flex flex-wrap gap-1">
                              {shared.map((s) => (
                                <CodeChip key={s.code} code={s.code} family={familyOf(s.code, codes)} title={s.cases.join(", ")} />
                              ))}
                            </span>
                          </div>
                        )}
                      </th>
                      {cases.map((c) => {
                        const cell = cellFor(d.cells, q.id, c.case_id)
                        const state = cellState(cell)
                        const list = cellCodes(cell)
                        return (
                          <td key={c.case_id} className={cn("border-b border-l px-3 py-3", state === "undocumented" && "bg-muted/50")}>
                            {state === "undocumented" ? (
                              <span className="text-muted-foreground text-xs italic">non documenté</span>
                            ) : state === "unmapped" ? (
                              <span className="text-muted-foreground text-xs">aucun code rattaché à {q.id}</span>
                            ) : (
                              <ul className="flex flex-col gap-1.5">
                                {list.map((x) => (
                                  <li key={x.code} className="flex flex-col gap-0.5">
                                    <span className="flex items-center gap-1.5">
                                      <CodeChip code={x.code} family={familyOf(x.code, codes)} />
                                      <span className="text-muted-foreground text-xs tabular-nums">{x.n}</span>
                                    </span>
                                    <UnitLinks units={x.units} />
                                  </li>
                                ))}
                              </ul>
                            )}
                          </td>
                        )
                      })}
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
          <p className="text-muted-foreground -mt-4 text-xs">
            <span className="bg-muted/50 mr-1 rounded px-1 italic">non documenté</span>
            le cas n'a aucune unité attribuée, la matrice ne dit donc rien sur cette question. « aucun code rattaché » signifie que ses unités attribuées ne portent aucun code lié à la question.
          </p>

          <Section title="Champs de synthèse, cas par cas" aside="tels qu'écrits dans chaque cas, avec les unités citées">
            <div className="-m-4 overflow-x-auto">
              <table className="w-full border-collapse text-sm">
                <thead>
                  <tr>
                    <th scope="col" className="w-40 border-b px-3 py-2 text-left text-xs font-semibold">
                      Champ
                    </th>
                    {cases.map((c) => (
                      <th key={c.case_id} scope="col" className="min-w-[13rem] border-b px-3 py-2 text-left text-xs font-semibold">
                        {c.case_id}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {CASE_FIELDS.map((k) => (
                    <tr key={k} className="align-top">
                      <th scope="row" className="border-b px-3 py-2.5 text-left font-medium">
                        {FIELD_LABELS[k].title}
                      </th>
                      {cases.map((c) => {
                        const f = c[k]
                        return (
                          <td key={c.case_id} className="border-b border-l px-3 py-2.5">
                            {f.text.trim() ? <p className="line-clamp-4 text-sm">{f.text}</p> : <span className="text-muted-foreground text-xs">vide</span>}
                            {f.evidence.length > 0 && (
                              <div className="mt-1">
                                <UnitLinks units={f.evidence} />
                              </div>
                            )}
                          </td>
                        )
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>
        </div>
      )}
    </>
  )
}
