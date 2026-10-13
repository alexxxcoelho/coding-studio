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
      <div className="text-sm font-medium">{c.label || "Untitled case"}</div>
      <div className="text-muted-foreground text-xs">
        {[c.stage, c.outcome_status].filter(Boolean).join(", ") || "stage and status not set"}
      </div>
    </th>
  )
}

function UnitLinks({ units }: { units: string[] }) {
  return (
    <span className="inline-flex flex-wrap gap-x-1.5">
      {units.map((u) => (
        <a key={u} href={unitHref(u)} className="ident text-muted-foreground text-[0.7rem] hover:underline" title={`Open ${u}`}>
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
  if (data.error) return <EmptyState title="Cross-case view unavailable">{data.error.message}</EmptyState>
  const d = data.data!
  const questions = [...d.questions].sort((a, b) => a.ord - b.ord)
  const cases = d.cases

  return (
    <>
      <PageTitle
        title="Cross-case"
        sub="For each research question, the codes each case carries in its final coding, with the units behind them. Read for convergence, contrast and conditions, not for frequency."
      />
      {cases.length === 0 ? (
        <EmptyState title="No case yet">Open an interview, tab Cases, and synthesise its projects: trigger, alternatives, constraints, choice, result.</EmptyState>
      ) : questions.length === 0 ? (
        <EmptyState
          title="No research question yet"
          action={
            <a href="#/codebook" className="text-primary text-sm hover:underline">
              Define the research questions in the Codebook
            </a>
          }
        >
          The matrix maps codes to questions: define the questions, then attach codes to them.
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-6">
          <div className="bg-card overflow-x-auto rounded-md border">
            <table className="w-full border-collapse text-sm">
              <thead>
                <tr>
                  <th scope="col" className="w-64 border-b px-3 py-2 text-left align-bottom text-xs font-semibold">
                    Research question
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
                            <span className="text-muted-foreground">In several cases</span>
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
                              <span className="text-muted-foreground text-xs italic">not documented</span>
                            ) : state === "unmapped" ? (
                              <span className="text-muted-foreground text-xs">no code mapped to {q.id}</span>
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
            <span className="bg-muted/50 mr-1 rounded px-1 italic">not documented</span>
            the case has no attributed unit, so the matrix says nothing on this question. "no code mapped" means its attributed units carry no code linked to the question.
          </p>

          <Section title="Synthesis fields across cases" aside="as written in each case, with the number of units cited">
            <div className="-m-4 overflow-x-auto">
              <table className="w-full border-collapse text-sm">
                <thead>
                  <tr>
                    <th scope="col" className="w-40 border-b px-3 py-2 text-left text-xs font-semibold">
                      Field
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
                            {f.text.trim() ? <p className="line-clamp-4 text-sm">{f.text}</p> : <span className="text-muted-foreground text-xs">empty</span>}
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
