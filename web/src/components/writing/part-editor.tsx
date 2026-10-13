// One of the five parts of a theme (guide, "Écrire les cinq parties du thème"):
// a text that saves itself, the nature of its evidence, and the passages linked
// to it with a type and a one-line note. A passage dragged from the list of the
// theme and dropped on the part creates the link.

import { useState, type DragEvent } from "react"
import { toast } from "sonner"

import { AutoInput, AutoTextarea } from "@/components/analysis/shared"
import { NativeSelect } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { t } from "@/i18n"
import type { LinkTarget, PartState, PassageLink, ThemePartKey } from "@/lib/analysis-api"
import { carries, planLink, readDrag } from "@/lib/analysis-dnd"
import { linkChipLabel, passageRef } from "@/lib/analysis-text"
import { cn } from "@/lib/utils"
import type { ListItem } from "@/lib/types"

function LinkRow({
  link,
  types,
  onChange,
  onRemove,
}: {
  link: PassageLink
  types: ListItem[]
  onChange: (edit: { link_type?: string; note?: string }) => Promise<unknown>
  onRemove: () => void
}) {
  const label = linkChipLabel(link, types)
  return (
    <li className="flex flex-wrap items-center gap-2" data-link={link.link_id}>
      <span data-no-i18n className="bg-fill2 rounded-full px-2 py-0.5 text-[11.5px] font-medium whitespace-nowrap" title={link.passage.text}>
        {label}
      </span>
      <NativeSelect
        aria-label={t("writing.part_editor.type_du_lien_passageref", { passageRef: passageRef(link.passage) })}
        value={link.link_type}
        onChange={(e) => void onChange({ link_type: e.target.value }).catch(() => {})}
        style={{ width: 120 }}
      >
        {!types.some((t) => t.id === link.link_type) && <option value={link.link_type}>{link.link_type}</option>}
        {types.map((t) => (
          <option key={t.id} value={t.id}>
            {t.label}
          </option>
        ))}
      </NativeSelect>
      <AutoInput
        label={t("writing.part_editor.note_du_lien_passageref", { passageRef: passageRef(link.passage) })}
        hideLabel
        className="min-w-48 flex-1"
        placeholder={t("writing.part_editor.ce_que_ce_passage_apporte")}
        value={link.note}
        onSave={(note) => onChange({ note })}
      />
      <Button variant="ghost" size="icon-xs" aria-label={t("writing.part_editor.retirer_le_lien_label", { label })} onClick={onRemove}>
        ×
      </Button>
    </li>
  )
}

export function PartEditor({
  index,
  partKey,
  title,
  target,
  state,
  allLinks,
  linkTypes,
  natures,
  onText,
  onNature,
  onCreateLink,
  onUpdateLink,
  onRemoveLink,
}: {
  index: number
  partKey: ThemePartKey
  title: string
  target: LinkTarget
  state: PartState
  /** Every link of the theme, to refuse a passage linked twice to the same part. */
  allLinks: PassageLink[]
  linkTypes: ListItem[]
  natures: ListItem[]
  onText: (text: string) => Promise<unknown>
  onNature: (nature: string | null) => Promise<unknown>
  onCreateLink: (body: { passage_id: string; link_type: string; note: string }) => Promise<unknown>
  onUpdateLink: (linkId: string, edit: { link_type?: string; note?: string }) => Promise<unknown>
  onRemoveLink: (linkId: string) => void
}) {
  const [over, setOver] = useState(false)

  const onDrop = (e: DragEvent) => {
    e.preventDefault()
    setOver(false)
    const p = readDrag(e.dataTransfer, "passage")
    if (!p) return
    const plan = planLink({ passageId: p.id, target, links: allLinks, linkTypes })
    if (plan.action === "refused") toast.error(plan.reason)
    else if (plan.action === "duplicate") toast.info(t("common.ce_passage_est_deja_lie"))
    else void onCreateLink({ passage_id: plan.body.passage_id, link_type: plan.body.link_type, note: plan.body.note })
  }

  return (
    <section
      aria-label={title}
      data-part={partKey}
      className={cn("flex flex-col gap-2 rounded-lg p-2 transition-colors", over && "bg-accent-fill/10 outline-2 outline-dashed outline-[var(--accent-c)]")}
      onDragOver={(e) => {
        if (carries(e.dataTransfer, "passage")) {
          e.preventDefault()
          setOver(true)
        }
      }}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
    >
      <AutoTextarea
        label={`${index}. ${title}`}
        value={state.text}
        onSave={onText}
        rows={5}
        serif
        extra={
          <label className="flex items-center gap-1.5 text-xs">
            <span className="text-muted-foreground">{t("writing.part_editor.nature_de_la_preuve")}</span>
            <NativeSelect aria-label={t("writing.part_editor.nature_de_la_preuve_title", { title })} value={state.nature ?? ""} onChange={(e) => void onNature(e.target.value || null)} style={{ width: 170 }}>
              <option value="">{t("writing.part_editor.non_precisee")}</option>
              {natures.map((n) => (
                <option key={n.id} value={n.id}>
                  {n.label}
                </option>
              ))}
            </NativeSelect>
          </label>
        }
      />
      {state.links.length > 0 && (
        <ul className="flex flex-col gap-1.5" aria-label={t("writing.part_editor.passages_lies_title", { title })}>
          {state.links.map((l) => (
            <LinkRow key={l.link_id} link={l} types={linkTypes} onChange={(edit) => onUpdateLink(l.link_id, edit)} onRemove={() => onRemoveLink(l.link_id)} />
          ))}
        </ul>
      )}
    </section>
  )
}
