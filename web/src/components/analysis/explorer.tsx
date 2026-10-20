import {Table, TableHeader, TableBody, TableRow} from '@/components/ui/table'
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
import { t } from "@/i18n"
import { interviewAttributes } from "@/lib/attributes"
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
    <Tool title={t("analysis.explorer.passages_par_code")}>
      <p className="text-muted-foreground text-[11.5px]">{t("analysis.explorer.tous_les_passages_d_un")}</p>
      <NativeSelect aria-label={t("common.code")} value={codeId} onChange={(e) => setCodeId(e.target.value)}>
        <option value="">{t("common.choisir_un_code")}</option>
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
            {t("analysis.explorer.items_dans_items2", { items: t("common.n_passages", { n: q.data.length }), items2: t("common.n_entretiens", { n: interviews }) })}
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
  const attributes = interviewAttributes(project?.attributes).filter((a) => a.kind === "choice" && a.choices.length > 0)
  const [attr, setAttr] = useState("")
  const def = attributes.find((a) => a.id === attr)
  const results = useQueries({
    queries: (def?.choices ?? []).map((value) => ({ queryKey: ["codes-by", attr, value], queryFn: () => api.codesBy(attr, value) })),
  })
  const rows = buildRows(codesQ.data?.codes ?? []).filter((r) => isApplicable(r.node))
  const loading = results.some((r) => r.isLoading)
  return (
    <Tool title={t("analysis.explorer.codes_par_attribut")}>
      <p className="text-muted-foreground text-[11.5px]">{t("analysis.explorer.le_nombre_de_passages_et")}</p>
      {attributes.length === 0 ? (
        <p className="text-muted-foreground text-sm">{t("analysis.explorer.aucun_attribut_a_choix_dans")}</p>
      ) : (
        <NativeSelect aria-label={t("common.attribut")} value={attr} onChange={(e) => setAttr(e.target.value)}>
          <option value="">{t("analysis.explorer.choisir_un_attribut")}</option>
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
          <Table className="tbl" aria-label={t("analysis.explorer.codes_par_attribut")}>
            <TableHeader>
              <TableRow>
                <th>{t("common.code")}</th>
                {def.choices.map((v) => (
                  <th key={v}>{v}</th>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map(({ node, depth }) => (
                <TableRow key={node.code_id} data-tree-id={node.code_id} data-tree-parent={node.parent_id??undefined}>
                  <td style={{ paddingLeft: 10 + depth * 14 }}>{node.name}</td>
                  {results.map((r, i) => {
                    const c = r.data?.codes.find((x) => x.code_id === node.code_id)
                    return (
                      <td key={def.choices[i]} className="tabular-nums">
                        {c ? (
                          <>
                            {t("common.n_passages", { n: c.counts.passages })}
                            <span className="text-muted-foreground text-[11.5px]">, {t("common.n_entretiens", { n: c.counts.interviews })}</span>
                          </>
                        ) : (
                          "0"
                        )}
                      </td>
                    )
                  })}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </Tool>
  )
}

export function ExplorerTab({ project }: { project: ProjectConfig | undefined }) {
  return (
    <div className="flex min-w-0 flex-col gap-3">
      <p className="text-muted-foreground text-sm">{t("analysis.explorer.ces_outils_explorent_ils_ne")}</p>
      <div className="grid gap-3 md:grid-cols-2">
        <Tool title={t("analysis.explorer.mot_en_contexte_kwic")}>
          <p className="text-muted-foreground text-[11.5px]">{t("analysis.explorer.cherchez_un_mot_sur_tous")}</p>
          <div>
            <Button asChild variant="outline" size="sm">
              <a href={LEXIQUE_HREF}>{t("analysis.explorer.ouvrir_le_lexique")}</a>
            </Button>
          </div>
        </Tool>
        <Tool title={t("analysis.explorer.passages_proches_par_vocabulaire")}>
          <p className="text-muted-foreground text-[11.5px]">{t("analysis.explorer.retrouvez_les_passages_qui_ressemble")}</p>
          <div>
            <Button asChild variant="outline" size="sm">
              <a href={LEXIQUE_HREF}>{t("analysis.explorer.ouvrir_le_lexique")}</a>
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
