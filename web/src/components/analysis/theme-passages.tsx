// The passages of a theme: those carried by its supporting codes (descendants
// included), with their interview, unit, extract and cell value. In Rédiger the
// cards are the source of the drags that create typed links.

import { useState } from "react"
import { Input } from "@/components/ui/input"
import { Button } from "@/components/ui/button"
import { useCodes } from "@/lib/hooks"
import { PassageCard } from "@/components/analysis/shared"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import type { PassageLink, ThemePassage } from "@/lib/analysis-api"
import { passageRef } from "@/lib/analysis-text"
import { useThemePassages } from "@/lib/analysis-hooks"
import { CELL_VALUE_LABEL } from "@/lib/matrix"
import { THEME_PARTS } from "@/lib/analysis-api"

export function ThemePassageList({
  themeId,
  draggable,
  links = [],
  onLinkTo,
}: {
  themeId: string
  draggable?: boolean
  /** The links of the theme, to tell which passages are already used. */
  links?: PassageLink[]
  /** The keyboard way to link: choose a part for a passage. */
  onLinkTo?: (passage: ThemePassage, part: (typeof THEME_PARTS)[number]["key"]) => void
}) {
  return <FilteredPassages key={themeId} themeId={themeId} draggable={draggable} links={links} onLinkTo={onLinkTo}/>
}

function FilteredPassages({themeId,draggable,links=[],onLinkTo}: Parameters<typeof ThemePassageList>[0]) {
  const [search,setSearch]=useState("")
  const [interview,setInterview]=useState("")
  const [code,setCode]=useState("")
  const [usage,setUsage]=useState("")
  const [verification,setVerification]=useState("")
  const [cell,setCell]=useState("")
  const codesQ=useCodes()
  const q = useThemePassages(themeId)
  if (q.isLoading) return <Skeleton className="h-32 w-full" />
  if (q.error) return <p className="text-destructive text-sm">{q.error.message}</p>
  const passages = q.data ?? []
  const interviews = new Set(passages.map((p) => p.interview_id)).size
  if (passages.length === 0) return <p className="text-muted-foreground text-sm">{t("analysis.theme_passages.aucun_passage_ajoutez_des_codes")}</p>
  const used = (id: string) => links.filter((l) => l.passage.passage_id === id).length
  const names=new Map((codesQ.data?.codes??[]).map(c=>[c.code_id,c.name]))
  const fold=(s:string)=>s.normalize("NFD").replace(/[\u0300-\u036f]/g,"").toLowerCase()
  const tokens=fold(search.trim()).split(/\s+/).filter(Boolean)
  const shown=passages.filter(p=>
    (!interview||p.interview_id===interview)&&(!code||p.code_ids.includes(code))&&
    (!cell||(cell==="unset"?!p.cell_value:p.cell_value===cell))&&
    (!verification||(verification==="stale"?p.stale:!p.stale&&(verification==="unverified"?!p.verification||p.verification==="unverified":p.verification===verification)))&&
    (!usage||(usage==="unused"?used(p.passage_id)===0:usage==="used"?used(p.passage_id)>0:links.some(l=>l.passage.passage_id===p.passage_id&&l.target.kind==="theme_part"&&l.target.part===usage)))&&
    tokens.every(token=>fold([p.text,p.interview_id,p.unit_id,...p.code_ids.map(id=>names.get(id)??id)].join(" ")).includes(token)))
  const filtered=!!(search||interview||code||usage||verification||cell)
  const reset=()=>{setSearch("");setInterview("");setCode("");setUsage("");setVerification("");setCell("")}
  const selectClass="ctl-field min-w-0 w-full rounded-md px-1 py-1 text-xs"

  return (
    <div className="flex flex-col gap-2">
      <div className="sticky top-0 z-10 grid gap-2 rounded-lg bg-background p-2 border border-sep" role="group" aria-label={t("writing.filters.title")}>
        <Input aria-label={t("writing.filters.search")} placeholder={t("writing.filters.search")} value={search} onChange={e=>setSearch(e.target.value)}/>
        <div className="grid grid-cols-2 gap-2">
          <select className={selectClass} aria-label={t("writing.filters.interview")} value={interview} onChange={e=>setInterview(e.target.value)}>
            <option value="">{t("writing.filters.all_interviews")}</option>
            {[...new Set(passages.map(p=>p.interview_id))].sort().map(id=><option key={id} value={id}>{id}</option>)}
          </select>
          <select className={selectClass} aria-label={t("writing.filters.code")} value={code} onChange={e=>setCode(e.target.value)}>
            <option value="">{t("writing.filters.all_codes")}</option>
            {[...new Set(passages.flatMap(p=>p.code_ids))].sort((a,b)=>(names.get(a)??a).localeCompare(names.get(b)??b)).map(id=><option key={id} value={id}>{names.get(id)??id}</option>)}
          </select>
          {draggable&&<select className={selectClass} aria-label={t("writing.filters.usage")} value={usage} onChange={e=>setUsage(e.target.value)}>
            <option value="">{t("writing.filters.all_usage")}</option><option value="unused">{t("writing.filters.unused")}</option><option value="used">{t("writing.filters.used")}</option>
            {THEME_PARTS.map(p=><option key={p.key} value={p.key}>{p.title}</option>)}
          </select>}
          <select className={selectClass} aria-label={t("writing.filters.verification")} value={verification} onChange={e=>setVerification(e.target.value)}>
            <option value="">{t("writing.filters.all_verification")}</option><option value="verified">{t("analysis.shared.verifie_a_l_audio")}</option><option value="unverified">{t("analysis.shared.non_verifie_a_l_audio")}</option><option value="unusable">{t("common.inutilisable")}</option><option value="stale">{t("writing.filters.stale")}</option>
          </select>
          <select className={selectClass} aria-label={t("writing.filters.matrix")} value={cell} onChange={e=>setCell(e.target.value)}>
            <option value="">{t("writing.filters.all_matrix")}</option><option value="unset">{t("writing.filters.unset")}</option>
            {Object.entries(CELL_VALUE_LABEL).map(([id,label])=><option key={id} value={id}>{label}</option>)}
          </select>
        </div>
        {filtered&&<Button variant="ghost" size="sm" onClick={reset}>{t("writing.filters.reset")}</Button>}
      </div>
      <p role="status" className="text-muted-foreground text-[11.5px]">
        {t("writing.filters.count",{shown:shown.length,total:passages.length})}{" · "}
        {t("analysis.theme_passages.items_dans_items2", { items: t("common.n_passages", { n: passages.length }), items2: t("common.n_entretiens", { n: interviews }) })}
        {draggable && t("analysis.theme_passages.glissez_un_passage_dans_une")}
      </p>
      {shown.length===0&&<p>{t("writing.filters.empty")}</p>}
      {shown.map((p) => (
        <PassageCard
          key={p.passage_id}
          passage={p}
          draggable={draggable}
          top={
            p.cell_value ? <span className="badge">{CELL_VALUE_LABEL[p.cell_value]}</span> : undefined
          }
          foot={
            <>
              {used(p.passage_id) > 0 && <span className="text-muted-foreground text-[11.5px]">{t("analysis.theme_passages.cite_used_fois", { used: used(p.passage_id) })}</span>}
              {onLinkTo && (
                <select
                  className="ctl-field ml-auto h-[22px] rounded-md px-1 text-xs"
                  aria-label={t("analysis.theme_passages.lier_passageref_a_une_partie", { passageRef: passageRef(p) })}
                  value=""
                  onChange={(e) => {
                    if (e.target.value) onLinkTo(p, e.target.value as (typeof THEME_PARTS)[number]["key"])
                  }}
                >
                  <option value="">{t("analysis.theme_passages.lier_a")}</option>
                  {THEME_PARTS.map((part) => (
                    <option key={part.key} value={part.key}>
                      {part.title}
                    </option>
                  ))}
                </select>
              )}
            </>
          }
        />
      ))}
    </div>
  )
}
