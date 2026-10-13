// Par thème: the retained themes in chapter order, their passages, and the five
// parts of the selected theme with the passages linked to each one.

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { ThemePassageList } from "@/components/analysis/theme-passages"
import { PartEditor } from "@/components/writing/part-editor"
import { ThemeOrderList } from "@/components/writing/theme-order-list"
import { EmptyState } from "@/components/bits"
import { Skeleton } from "@/components/ui/skeleton"
import { analysisApi, THEME_PARTS, type PartEdit, type ThemePartKey, type ThemeSummary, type ThemeWriting } from "@/lib/analysis-api"
import { mergeRetainedOrder, planLink } from "@/lib/analysis-dnd"
import { akeys, invalidateThemes, invalidateWriting, retainedThemes, useThemeWriting } from "@/lib/analysis-hooks"
import { reportError } from "@/lib/notify"
import type { ProjectConfig } from "@/lib/types"

export function ByThemeTab({ themes, project }: { themes: ThemeSummary[]; project: ProjectConfig }) {
  const qc = useQueryClient()
  const retained = retainedThemes(themes)
  const [pickedId, setPickedId] = useState<string | null>(null)
  const selectedId = retained.some((t) => t.theme_id === pickedId) ? pickedId : (retained[0]?.theme_id ?? null)
  const writingQ = useThemeWriting(selectedId)

  const reorder = useMutation({
    mutationFn: (retainedAfter: string[]) => {
      const all = [...themes].sort((a, b) => a.ord - b.ord).map((t) => t.theme_id)
      return analysisApi.reorderThemes(mergeRetainedOrder(all, retained.map((t) => t.theme_id), retainedAfter))
    },
    onSuccess: (list) => {
      qc.setQueryData(akeys.themes, list)
      void invalidateThemes(qc)
    },
    onError: (e) => reportError(e, "Ordre non enregistré"),
  })

  if (retained.length === 0) {
    return (
      <EmptyState title="Aucun thème retenu pour l'instant">
        Rédiger part des thèmes que vous avez construits et retenus dans Analyser : seuls les thèmes au statut « retenu » apparaissent ici.
        <a href="#/analyser" className="text-accent-text ml-1 hover:underline">
          Ouvrir Analyser
        </a>
      </EmptyState>
    )
  }

  const writing = writingQ.data
  const allLinks = writing ? THEME_PARTS.flatMap((p) => writing.parts[p.key]?.links ?? []) : []

  const savePart = (part: ThemePartKey) => async (edit: PartEdit) => {
    if (!selectedId) return
    try {
      const saved = await analysisApi.setThemePart(selectedId, part, edit)
      qc.setQueryData<ThemeWriting>(akeys.writing(selectedId), (w) => (w ? { ...w, parts: { ...w.parts, [part]: { ...w.parts[part], text: saved.text, nature: saved.nature, updated_at: saved.updated_at } } } : w))
      void qc.invalidateQueries({ queryKey: ["synthesis"] })
    } catch (e) {
      reportError(e, "Partie non enregistrée")
      throw e
    }
  }

  const createLink = async (part: ThemePartKey, passageId: string, linkType: string, note = "") => {
    if (!selectedId) return
    try {
      await analysisApi.createLink({ passage_id: passageId, target: { kind: "theme_part", theme_id: selectedId, part }, link_type: linkType, note })
      await invalidateWriting(qc, selectedId)
    } catch (e) {
      reportError(e, "Lien non créé")
    }
  }

  const updateLink = async (linkId: string, edit: { link_type?: string; note?: string }) => {
    try {
      await analysisApi.updateLink(linkId, edit)
      if (selectedId) await invalidateWriting(qc, selectedId)
    } catch (e) {
      reportError(e, "Lien non modifié")
      throw e
    }
  }

  const removeLink = async (linkId: string) => {
    try {
      await analysisApi.removeLink(linkId)
      if (selectedId) await invalidateWriting(qc, selectedId)
    } catch (e) {
      reportError(e, "Lien non retiré")
    }
  }

  const keyboardLink = (passageId: string, part: ThemePartKey) => {
    if (!selectedId) return
    const plan = planLink({ passageId, target: { kind: "theme_part", theme_id: selectedId, part }, links: allLinks, linkTypes: project.link_types })
    if (plan.action === "create") void createLink(part, plan.body.passage_id, plan.body.link_type, plan.body.note)
    else reportError(new Error(plan.action === "duplicate" ? "Ce passage est déjà lié à cette partie." : plan.reason))
  }

  return (
    <div className="grid min-w-0 gap-6 lg:grid-cols-[minmax(0,330px)_minmax(0,1fr)]">
      <div className="flex min-w-0 flex-col gap-4">
        <ThemeOrderList themes={retained} selectedId={selectedId} onSelect={setPickedId} onReorder={(ids) => reorder.mutate(ids)} />
        {selectedId && (
          <section aria-label="Passages du thème" className="flex flex-col gap-2">
            <h2 className="hs px-1.5">Passages du thème</h2>
            <ThemePassageList themeId={selectedId} draggable links={allLinks} onLinkTo={(p, part) => keyboardLink(p.passage_id, part)} />
          </section>
        )}
      </div>
      <div className="mx-auto flex w-full min-w-0 max-w-[720px] flex-col gap-4">
        {writingQ.isLoading || !writing ? (
          writingQ.error ? (
            <p className="text-destructive text-sm">{writingQ.error.message}</p>
          ) : (
            <Skeleton className="h-96 w-full" />
          )
        ) : (
          <>
            <header className="flex flex-col gap-1">
              <h1 className="m-0 font-serif text-[26px] leading-tight font-semibold">{writing.title}</h1>
              <p className="text-muted-foreground text-sm">
                {writing.research_questions.length > 0 ? `Questions : ${writing.research_questions.join(", ")}` : "Aucune question de recherche rattachée à ce thème"}
              </p>
            </header>
            {THEME_PARTS.map((p, i) => (
              <PartEditor
                key={`${writing.theme_id}:${p.key}`}
                index={i + 1}
                partKey={p.key}
                title={p.title}
                target={{ kind: "theme_part", theme_id: writing.theme_id, part: p.key }}
                state={writing.parts[p.key]}
                allLinks={allLinks}
                linkTypes={project.link_types}
                natures={project.evidence_natures}
                onText={(text) => savePart(p.key)({ text })}
                onNature={(nature) => savePart(p.key)({ nature }).catch(() => {})}
                onCreateLink={(b) => createLink(p.key, b.passage_id, b.link_type, b.note)}
                onUpdateLink={updateLink}
                onRemoveLink={(id) => void removeLink(id)}
              />
            ))}
          </>
        )}
      </div>
    </div>
  )
}
