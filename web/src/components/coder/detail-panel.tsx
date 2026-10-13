// The Détail tab: the selected passage (its text, its codes, its note, the
// text around it) and what can be done with it: remove a code, write a note
// for Plus tard, copy it as a quotation, delete it.

import { useState, type CSSProperties } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { X } from "lucide-react"
import { toast } from "sonner"

import { shortId } from "@/components/coder/turn"
import { NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { t } from "@/i18n"
import { analysisApi } from "@/lib/analysis-api"
import { codeColor, LATER_ID } from "@/lib/code-tree"
import { dateTime } from "@/lib/format"
import { invalidatePassages, useProjectConfig, type CodingActions } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import type { ListItem, Passage } from "@/lib/types"

export function quotation(p: Passage): string {
  return `« ${p.text.trim()} » (${shortId(p.unit_id)}, ${p.interview_id})`
}

function LaterNote({ passage, onSave }: { passage: Passage; onSave: (note: string) => void }) {
  const coding = passage.codings.find((c) => c.system || c.code_id === LATER_ID)
  const savedNote = coding?.note ?? ""
  const [draft, setDraft] = useState(savedNote)
  const [seen, setSeen] = useState(`${passage.passage_id}:${savedNote}`)
  if (seen !== `${passage.passage_id}:${savedNote}`) {
    setSeen(`${passage.passage_id}:${savedNote}`)
    setDraft(savedNote)
  }
  if (!coding) return null
  return (
    <>
      <label className="hs" htmlFor="later-note">
        {t("coder.detail_panel.note_pour_plus_tard")}
      </label>
      <textarea
        id="later-note"
        className={TEXTAREA}
        rows={2}
        value={draft}
        placeholder={t("coder.detail_panel.pourquoi_ce_passage_est_a")}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={() => draft !== coding.note && onSave(draft)}
      />
    </>
  )
}

/** Origin and role of the passage (guide, "Qualifier une citation"): saved at once, empty clears. */
/** The study's list, plus the value a passage already carries when the list no longer has it. */
function withCurrent(list: ListItem[] | undefined, current: string | null): ListItem[] {
  const items = list ?? []
  return current && !items.some((i) => i.id === current) ? [...items, { id: current, label: current }] : items
}

function Qualification({ passage }: { passage: Passage }) {
  const qc = useQueryClient()
  const cfg = useProjectConfig()
  const save = useMutation({
    mutationFn: (q: { origin?: string | null; role?: string | null }) => analysisApi.qualifyPassage(passage.passage_id, q),
    onSuccess: () => invalidatePassages(qc, passage.interview_id),
    onError: (e) => reportError(e, t("coder.detail_panel.qualification_non_enregistree")),
  })
  if (passage.passage_id.startsWith("tmp-")) return null
  return (
    <>
      <div className="hs">{t("coder.detail_panel.qualification")}</div>
      <label className="flex flex-col gap-1 text-xs">
        <span className="l2">{t("coder.detail_panel.origine_du_propos")}</span>
        <NativeSelect aria-label={t("coder.detail_panel.origine_du_propos")} value={passage.origin ?? ""} disabled={save.isPending} onChange={(e) => save.mutate({ origin: e.target.value || null })}>
          <option value="">{t("coder.detail_panel.non_renseignee")}</option>
          {withCurrent(cfg.data?.passage_origins, passage.origin ?? null).map((o) => (
            <option key={o.id} value={o.id}>
              {o.label}
            </option>
          ))}
        </NativeSelect>
      </label>
      <label className="flex flex-col gap-1 text-xs">
        <span className="l2">{t("coder.detail_panel.role_dans_la_decision")}</span>
        <NativeSelect aria-label={t("coder.detail_panel.role_dans_la_decision")} value={passage.role ?? ""} disabled={save.isPending} onChange={(e) => save.mutate({ role: e.target.value || null })}>
          <option value="">{t("common.non_renseigne")}</option>
          {withCurrent(cfg.data?.passage_roles, passage.role ?? null).map((r) => (
            <option key={r.id} value={r.id}>
              {r.label}
            </option>
          ))}
        </NativeSelect>
      </label>
    </>
  )
}

export function DetailPanel({
  passage,
  unitLabel,
  unitPassages,
  actions,
  onSelectPassage,
  onNote,
}: {
  passage: Passage | null
  unitLabel: string | null
  unitPassages: Passage[]
  actions: CodingActions
  onSelectPassage: (p: Passage) => void
  onNote: (p: Passage, note: string) => void
}) {
  if (!passage) {
    return (
      <div className="insp-in">
        <div className="hs">{t("common.passage_selectionne")}</div>
        <p className="small l2" style={{ margin: 0 }}>
          {unitLabel
            ? t("coder.detail_panel.cliquez_sur_un_passage_code", { unitLabel })
            : t("coder.detail_panel.selectionnez_un_tour_puis_un")}
        </p>
        {unitPassages.length > 0 && (
          <div className="grp">
            {unitPassages.map((p) => (
              <button key={p.passage_id} type="button" className="row" style={{ width: "100%", textAlign: "left", background: "transparent", border: 0, font: "inherit", color: "inherit", cursor: "pointer" }} onClick={() => onSelectPassage(p)}>
                <span className="k">
                  {p.text.length > 60 ? `${p.text.slice(0, 59)}…` : p.text}
                  <small>{p.codings.length ? p.codings.map((c) => c.code_name).join(", ") : "surlignage"}</small>
                </span>
              </button>
            ))}
          </div>
        )}
      </div>
    )
  }
  return (
    <div className="insp-in">
      <div className="hs">{t("common.passage_selectionne")}</div>
      <div className="small l2">
        <span className="mono">{shortId(passage.unit_id)}</span>
        {passage.whole_unit ? ", tour entier" : t("coder.detail_panel.caracteres_start_a_end", { start: passage.start, end: passage.end })}
      </div>
      {passage.stale && (
        <p className="note-warn" role="note">
          {t("coder.detail_panel.le_texte_du_tour_a")}{passage.stale_reason ?? t("coder.detail_panel.ancrage_obsolete")}{t("coder.detail_panel.le_passage_est_signale_il")}
        </p>
      )}
      <blockquote data-no-i18n className="verbatim" style={{ margin: 0, borderLeft: "3px solid var(--accent-c)", paddingLeft: 10 }}>
        {passage.before && <span className="l3">{passage.before}</span>}
        <mark style={{ background: "color-mix(in srgb, var(--yellow) 30%, transparent)", color: "inherit" }}>{passage.text}</mark>
        {passage.after && <span className="l3">{passage.after}</span>}
      </blockquote>

      <div className="hs">{t("common.codes")}</div>
      <div className="r8" style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        {passage.codings.length === 0 && <span className="small l2">{t("coder.detail_panel.surlignage_sans_code")}</span>}
        {passage.codings.map((c) => (
          <span key={c.coding_id} className="tag k" style={{ "--k": codeColor(c.code_id) } as CSSProperties}>
            <span className="sw" />
            {c.code_name}
            <button
              type="button"
              aria-label={`Retirer ${c.code_name}`}
              title={t("coder.detail_panel.retirer_ce_code")}
              style={{ border: 0, background: "transparent", padding: 0, cursor: "pointer", color: "var(--label2)", display: "inline-flex" }}
              onClick={() => actions.removeCode(passage, c.code_id, c.code_name)}
            >
              <X className="size-3" aria-hidden />
            </button>
          </span>
        ))}
      </div>

      <LaterNote passage={passage} onSave={(note) => onNote(passage, note)} />

      <Qualification passage={passage} />

      <div className="hs">{t("coder.detail_panel.citation")}</div>
      <div className="r8" style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Button
          variant="outline"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(quotation(passage))
              toast.success(t("coder.detail_panel.citation_copiee"))
            } catch {
              toast.error(t("coder.detail_panel.la_citation_n_a_pas"))
            }
          }}
        >
          {t("coder.detail_panel.copier_la_citation")}
        </Button>
        <Button variant="outline" onClick={() => actions.deletePassage(passage)}>
          {t("coder.detail_panel.supprimer_le_passage")}
        </Button>
      </div>
      <p className="small l2" style={{ margin: 0 }}>
        {t("coder.detail_panel.cree_par")} {passage.created_by || "…"} {t("coder.detail_panel.le")} {dateTime(passage.created_at) || "…"}{t("coder.detail_panel.un_surlignage_reste_visible_apres")}
      </p>
    </div>
  )
}
