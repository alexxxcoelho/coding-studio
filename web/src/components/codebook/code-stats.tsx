// Counts of a code (F-45): passages, interviews and cases, apart and never
// added, for the code alone and with its sub-codes, and the values of each
// interview attribute among the interviews that carry it. A filter on one
// attribute value keeps only the interviews with that value.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { NativeSelect } from "@/components/settings/fields"
import { Skeleton } from "@/components/ui/skeleton"
import { t } from "@/i18n"
import { cn } from "@/lib/utils"
import { v2, type StatBlock } from "@/lib/v2-api"

type Scope = "own" | "rollup"

export function CodeStats({ codeId }: { codeId: string }) {
  const [scope, setScope] = useState<Scope>("own")
  const [attribute, setAttribute] = useState("")
  const [value, setValue] = useState("")
  const filter = attribute && value ? { attribute, value } : undefined

  const all = useQuery({ queryKey: ["code-stats", codeId, null], queryFn: () => v2.codeStats(codeId) })
  const q = useQuery({ queryKey: ["code-stats", codeId, filter ?? null], queryFn: () => v2.codeStats(codeId, filter), enabled: !!filter })
  const stats = filter ? q.data : all.data

  if (all.isLoading) return <Skeleton className="h-40 w-full" />
  if (all.error || !all.data) return <p className="small l2">{t("codebook.code_stats.les_chiffres_ne_sont_pas", { message: all.error?.message })}</p>

  const attrs = all.data.own.attributes
  const options = attribute
    ? [...new Set([...(attrs.find((a) => a.attribute_id === attribute)?.values ?? []), ...(all.data.rollup.attributes.find((a) => a.attribute_id === attribute)?.values ?? [])].map((v) => v.value))]
    : []
  const block: StatBlock | undefined = stats?.[scope]

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div className="grp" role="table" aria-label={t("codebook.code_stats.chiffres_du_code")}>
        <div className="row" role="row" style={{ minHeight: 30 }}>
          <span className="k" role="columnheader" />
          <span className="v b" role="columnheader" style={{ width: 60, textAlign: "right" }}>{t("codebook.code_stats.direct")}</span>
          <span className="v b" role="columnheader" style={{ width: 118, textAlign: "right" }}>{t("codebook.code_stats.avec_sous_codes")}</span>
        </div>
        {(
          [
            [t("codebook.code_stats.passages"), "passages"],
            [t("codebook.code_stats.entretiens"), "interviews"],
            ["Cas", "cases"],
          ] as const
        ).map(([label, key]) => (
          <div key={key} className="row" role="row" style={{ minHeight: 32 }}>
            <span className="k" role="rowheader">{label}</span>
            <span role="cell" className={cn("v", scope === "own" && "b")} style={{ width: 60, textAlign: "right", color: scope === "own" ? "var(--label)" : undefined }}>{stats?.own[key] ?? "…"}</span>
            <span role="cell" className={cn("v", scope === "rollup" && "b")} style={{ width: 118, textAlign: "right", color: scope === "rollup" ? "var(--label)" : undefined }}>{stats?.rollup[key] ?? "…"}</span>
          </div>
        ))}
      </div>

      <div className="r8" style={{ justifyContent: "space-between" }}>
        <span className="hs">{t("codebook.code_stats.detail_par_attribut")}</span>
        <div className="seg" role="tablist" aria-label={t("codebook.code_stats.portee_des_chiffres")}>
          <button type="button" role="tab" aria-selected={scope === "own"} className={cn("s", scope === "own" && "on")} onClick={() => setScope("own")}>
            {t("codebook.code_stats.direct")}
          </button>
          <button type="button" role="tab" aria-selected={scope === "rollup"} className={cn("s", scope === "rollup" && "on")} onClick={() => setScope("rollup")}>
            {t("codebook.code_stats.avec_sous_codes")}
          </button>
        </div>
      </div>

      {attrs.length > 0 && (
        <div className="r8">
          <NativeSelect
            aria-label={t("common.filtrer_sur_un_attribut")}
            style={{ width: 150 }}
            value={attribute}
            onChange={(e) => {
              setAttribute(e.target.value)
              setValue("")
            }}
          >
            <option value="">{t("common.tous_les_entretiens")}</option>
            {attrs.map((a) => (
              <option key={a.attribute_id} value={a.attribute_id}>
                {a.label}
              </option>
            ))}
          </NativeSelect>
          {attribute && (
            <NativeSelect aria-label={t("codebook.code_stats.valeur_de_l_attribut")} style={{ width: 130 }} value={value} onChange={(e) => setValue(e.target.value)}>
              <option value="">{t("common.valeur_2")}</option>
              {options.map((v) => (
                <option key={v} value={v}>
                  {v}
                </option>
              ))}
            </NativeSelect>
          )}
        </div>
      )}
      {filter && block && (
        <p className="small l2" style={{ margin: 0 }} role="status">
          {t("codebook.code_stats.filtre_label_value_items_items2", { label: attrs.find((a) => a.attribute_id === attribute)?.label, value, items: t("common.n_passages", { n: block.passages }), items2: t("common.n_entretiens", { n: block.interviews }), items3: t("codebook.code_stats.n_cas", { n: block.cases }) })}
        </p>
      )}

      {block && block.attributes.length === 0 && <p className="small l2" style={{ margin: 0 }}>{t("codebook.code_stats.aucun_attribut_d_entretien_n")}</p>}
      {block && block.attributes.length > 0 && (
        <div className="grp" aria-label={t("codebook.code_stats.valeurs_des_attributs")}>
          {block.attributes.map((a) => (
            <div key={a.attribute_id} className="row" style={{ alignItems: "flex-start", flexDirection: "column", gap: 3 }}>
              <span className="b">
                {a.label} <span className="l2" style={{ fontWeight: 400 }}>{t("codebook.code_stats.n_valeurs", { n: a.distinct_values })}</span>
              </span>
              {a.values.length === 0 && <span className="small l2">{t("codebook.code_stats.aucune_valeur_sur_les_entretiens")}</span>}
              {a.values.map((v) => (
                <span key={v.value} className="small" style={{ display: "flex", justifyContent: "space-between", width: "100%" }}>
                  <span>{v.value}</span>
                  <span className="l2">{t("common.n_entretiens", { n: v.interviews })}</span>
                </span>
              ))}
              {a.interviews_without_value > 0 && <span className="small l2">{t("codebook.code_stats.items_sans_valeur_comptes_nulle", { items: t("common.n_entretiens", { n: a.interviews_without_value }) })}</span>}
            </div>
          ))}
        </div>
      )}
      <p className="small l2" style={{ margin: 0 }}>
        {t("codebook.code_stats.passages_entretiens_et_cas_sont")}
      </p>
    </div>
  )
}
