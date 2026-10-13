// Counts of a code (F-45): passages, interviews and cases, apart and never
// added, for the code alone and with its sub-codes, and the values of each
// interview attribute among the interviews that carry it. A filter on one
// attribute value keeps only the interviews with that value.

import { useState } from "react"
import { useQuery } from "@tanstack/react-query"

import { NativeSelect } from "@/components/settings/fields"
import { Skeleton } from "@/components/ui/skeleton"
import { cn } from "@/lib/utils"
import { v2, type StatBlock } from "@/lib/v2-api"

type Scope = "own" | "rollup"

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n < 2 ? one : many}`

export function CodeStats({ codeId }: { codeId: string }) {
  const [scope, setScope] = useState<Scope>("own")
  const [attribute, setAttribute] = useState("")
  const [value, setValue] = useState("")
  const filter = attribute && value ? { attribute, value } : undefined

  const all = useQuery({ queryKey: ["code-stats", codeId, null], queryFn: () => v2.codeStats(codeId) })
  const q = useQuery({ queryKey: ["code-stats", codeId, filter ?? null], queryFn: () => v2.codeStats(codeId, filter), enabled: !!filter })
  const stats = filter ? q.data : all.data

  if (all.isLoading) return <Skeleton className="h-40 w-full" />
  if (all.error || !all.data) return <p className="small l2">Les chiffres ne sont pas disponibles : {all.error?.message}</p>

  const attrs = all.data.own.attributes
  const options = attribute
    ? [...new Set([...(attrs.find((a) => a.attribute_id === attribute)?.values ?? []), ...(all.data.rollup.attributes.find((a) => a.attribute_id === attribute)?.values ?? [])].map((v) => v.value))]
    : []
  const block: StatBlock | undefined = stats?.[scope]

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div className="grp" role="table" aria-label="Chiffres du code">
        <div className="row" role="row" style={{ minHeight: 30 }}>
          <span className="k" role="columnheader" />
          <span className="v b" role="columnheader" style={{ width: 60, textAlign: "right" }}>Direct</span>
          <span className="v b" role="columnheader" style={{ width: 118, textAlign: "right" }}>Avec sous-codes</span>
        </div>
        {(
          [
            ["Passages", "passages"],
            ["Entretiens", "interviews"],
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
        <span className="hs">Détail par attribut</span>
        <div className="seg" role="tablist" aria-label="Portée des chiffres">
          <button type="button" role="tab" aria-selected={scope === "own"} className={cn("s", scope === "own" && "on")} onClick={() => setScope("own")}>
            Direct
          </button>
          <button type="button" role="tab" aria-selected={scope === "rollup"} className={cn("s", scope === "rollup" && "on")} onClick={() => setScope("rollup")}>
            Avec sous-codes
          </button>
        </div>
      </div>

      {attrs.length > 0 && (
        <div className="r8">
          <NativeSelect
            aria-label="Filtrer sur un attribut"
            style={{ width: 150 }}
            value={attribute}
            onChange={(e) => {
              setAttribute(e.target.value)
              setValue("")
            }}
          >
            <option value="">Tous les entretiens</option>
            {attrs.map((a) => (
              <option key={a.attribute_id} value={a.attribute_id}>
                {a.label}
              </option>
            ))}
          </NativeSelect>
          {attribute && (
            <NativeSelect aria-label="Valeur de l'attribut" style={{ width: 130 }} value={value} onChange={(e) => setValue(e.target.value)}>
              <option value="">Valeur…</option>
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
          Filtre : {attrs.find((a) => a.attribute_id === attribute)?.label} = {value}. {plural(block.passages, "passage")}, {plural(block.interviews, "entretien")}, {plural(block.cases, "cas", "cas")}.
        </p>
      )}

      {block && block.attributes.length === 0 && <p className="small l2" style={{ margin: 0 }}>Aucun attribut d'entretien n'est défini pour l'étude.</p>}
      {block && block.attributes.length > 0 && (
        <div className="grp" aria-label="Valeurs des attributs">
          {block.attributes.map((a) => (
            <div key={a.attribute_id} className="row" style={{ alignItems: "flex-start", flexDirection: "column", gap: 3 }}>
              <span className="b">
                {a.label} <span className="l2" style={{ fontWeight: 400 }}>{plural(a.distinct_values, "valeur")}</span>
              </span>
              {a.values.length === 0 && <span className="small l2">Aucune valeur sur les entretiens de ce code.</span>}
              {a.values.map((v) => (
                <span key={v.value} className="small" style={{ display: "flex", justifyContent: "space-between", width: "100%" }}>
                  <span>{v.value}</span>
                  <span className="l2">{plural(v.interviews, "entretien")}</span>
                </span>
              ))}
              {a.interviews_without_value > 0 && <span className="small l2">{plural(a.interviews_without_value, "entretien")} sans valeur, comptés nulle part.</span>}
            </div>
          ))}
        </div>
      )}
      <p className="small l2" style={{ margin: 0 }}>
        Passages, entretiens et cas sont comptés à part, jamais additionnés. Ces chiffres disent la couverture du corpus codé, pas la fréquence des idées.
      </p>
    </div>
  )
}
