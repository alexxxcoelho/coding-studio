// Explorer tab of Analyser (guide, "Explorer les données"): keyword in context
// and similar passages (the Lexique), passages by code, and codes by attribute.
// These tools explore, they do not conclude; a click on a result opens the
// passage in its interview.

import { useState } from "react"
import { useQueries, useQuery } from "@tanstack/react-query"

import { PassageCard } from "@/components/analysis/shared"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { plural } from "@/lib/analysis-text"
import { api } from "@/lib/api"
import { buildRows, isApplicable } from "@/lib/code-tree"
import { useCodes } from "@/lib/hooks"
import type { ProjectConfig } from "@/lib/types"

/** Where the Lexique lives (keyword in context, similar passages). */
export const LEXIQUE_HREF = "#/lexique"

function Tool({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="grp flex min-w-0 flex-col gap-2 p-3" aria-label={title}>
      <h3 className="text-sm font-semibold">{title}</h3>
      {children}
    </section>
  )
}

function PassagesByCode() {
  const codesQ = useCodes()
  const [codeId, setCodeId] = useState("")
  const rows = buildRows(codesQ.data?.codes ?? []).filter((r) => isApplicable(r.node))
  const q = useQuery({ queryKey: ["code-passages", codeId], queryFn: () => api.codePassages(codeId, true), enabled: !!codeId })
  const interviews = new Set((q.data ?? []).map((c) => c.passage.interview_id)).size
  return (
    <Tool title="Passages par code">
      <p className="text-muted-foreground text-[11.5px]">Tous les passages d'un code, sous-codes compris, tous entretiens confondus.</p>
      <NativeSelect aria-label="Code" value={codeId} onChange={(e) => setCodeId(e.target.value)}>
        <option value="">Choisir un code…</option>
        {rows.map(({ node, depth }) => (
          <option key={node.code_id} value={node.code_id}>
            {`${"· ".repeat(depth)}${node.name}`}
          </option>
        ))}
      </NativeSelect>
      {q.isLoading && <Skeleton className="h-16 w-full" />}
      {q.data && (
        <>
          <p className="text-muted-foreground text-[11.5px]">
            {plural(q.data.length, "passage")} dans {plural(interviews, "entretien")}
          </p>
          <div className="flex max-h-96 flex-col gap-2 overflow-y-auto">
            {q.data.map((c) => (
              <PassageCard key={c.passage.passage_id} passage={{ passage_id: c.passage.passage_id, interview_id: c.passage.interview_id, unit_id: c.passage.unit_id, text: c.passage.text, verification: null }} />
            ))}
          </div>
        </>
      )}
    </Tool>
  )
}

function CodesByAttribute({ project }: { project: ProjectConfig | undefined }) {
  const codesQ = useCodes()
  const attributes = (project?.attributes ?? []).filter((a) => a.kind === "choice" && a.choices.length > 0)
  const [attr, setAttr] = useState("")
  const def = attributes.find((a) => a.id === attr)
  const results = useQueries({
    queries: (def?.choices ?? []).map((value) => ({ queryKey: ["codes-by", attr, value], queryFn: () => api.codesBy(attr, value) })),
  })
  const rows = buildRows(codesQ.data?.codes ?? []).filter((r) => isApplicable(r.node))
  const loading = results.some((r) => r.isLoading)
  return (
    <Tool title="Codes par attribut">
      <p className="text-muted-foreground text-[11.5px]">Le nombre de passages et d'entretiens de chaque code, selon la valeur d'un attribut des entretiens.</p>
      {attributes.length === 0 ? (
        <p className="text-muted-foreground text-sm">Aucun attribut à choix dans l'étude : ajoutez-en dans les Réglages.</p>
      ) : (
        <NativeSelect aria-label="Attribut" value={attr} onChange={(e) => setAttr(e.target.value)}>
          <option value="">Choisir un attribut…</option>
          {attributes.map((a) => (
            <option key={a.id} value={a.id}>
              {a.label}
            </option>
          ))}
        </NativeSelect>
      )}
      {def && loading && <Skeleton className="h-24 w-full" />}
      {def && !loading && (
        <div className="overflow-x-auto">
          <table className="tbl" aria-label="Codes par attribut">
            <thead>
              <tr>
                <th>Code</th>
                {def.choices.map((v) => (
                  <th key={v}>{v}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map(({ node, depth }) => (
                <tr key={node.code_id}>
                  <td style={{ paddingLeft: 10 + depth * 14 }}>{node.name}</td>
                  {results.map((r, i) => {
                    const c = r.data?.codes.find((x) => x.code_id === node.code_id)
                    return (
                      <td key={def.choices[i]} className="tabular-nums">
                        {c ? (
                          <>
                            {plural(c.counts.passages, "passage")}
                            <span className="text-muted-foreground text-[11.5px]">, {plural(c.counts.interviews, "entretien")}</span>
                          </>
                        ) : (
                          "0"
                        )}
                      </td>
                    )
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Tool>
  )
}

export function ExplorerTab({ project }: { project: ProjectConfig | undefined }) {
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <p className="text-muted-foreground text-sm">Ces outils explorent, ils ne concluent pas. Un clic sur un résultat ouvre le passage dans son entretien.</p>
      <div className="grid gap-3 md:grid-cols-2">
        <Tool title="Mot en contexte (KWIC)">
          <p className="text-muted-foreground text-[11.5px]">Cherchez un mot sur tous les entretiens, avec les mots voisins de chaque occurrence. L'outil est dans le Lexique.</p>
          <div>
            <Button asChild variant="outline" size="sm">
              <a href={LEXIQUE_HREF}>Ouvrir le Lexique</a>
            </Button>
          </div>
        </Tool>
        <Tool title="Passages proches par vocabulaire">
          <p className="text-muted-foreground text-[11.5px]">Retrouvez les passages qui ressemblent à un passage donné, d'après leur vocabulaire. Aucune IA générative n'intervient.</p>
          <div>
            <Button asChild variant="outline" size="sm">
              <a href={LEXIQUE_HREF}>Ouvrir le Lexique</a>
            </Button>
          </div>
        </Tool>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <PassagesByCode />
        <CodesByAttribute project={project} />
      </div>
    </div>
  )
}
