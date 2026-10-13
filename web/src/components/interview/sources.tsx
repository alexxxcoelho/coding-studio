import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, Section } from "@/components/bits"
import { t } from "@/i18n"
import { reportError } from "@/lib/notify"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { api } from "@/lib/api"
import { bytes, dateTime, shortHash } from "@/lib/format"
import { invalidateInterview } from "@/lib/queries"
import { interviewHref } from "@/lib/router"
import type { InterviewDetail, SourceKind } from "@/lib/types"

const KIND_LABEL: Record<SourceKind, string> = {
  get vtt() { return t("common.transcription_de_plateforme_vtt") },
  get timestamped_text() { return t("interview.sources.texte_brut_de_reconnaissance_vocale") },
  get cleaned_markdown() { return t("common.transcription_epuree") },
  get budgie_markdown() { return t("common.export_budgie_echo") },
  get written_response() { return t("common.reponse_ecrite") },
  get docx() { return t("common.transcription_word_docx") },
  get pdf() { return t("common.transcription_pdf") },
  get summary() { return t("common.synthese") },
  get record_sheet() { return t("common.fiche_d_enregistrement") },
  get toe_coding() { return t("common.codage_anterieur") },
  get other() { return t("interview.sources.autre") },
}

const TRANSCRIPT: SourceKind[] = ["vtt", "timestamped_text", "cleaned_markdown", "budgie_markdown", "written_response", "docx", "pdf"]

function FileViewer({ id, path, onClose }: { id: string; path: string | null; onClose: () => void }) {
  const file = useQuery({ queryKey: ["file", id, path], queryFn: () => api.file(id, path!), enabled: !!path })
  return (
    <Sheet open={!!path} onOpenChange={(o) => !o && onClose()}>
      <SheetContent side="right" className="w-[min(52rem,92vw)] sm:max-w-none">
        <SheetHeader>
          <SheetTitle className="ident break-all">{path}</SheetTitle>
          <SheetDescription>{t("interview.sources.lecture_seule_le_fichier_sur")}</SheetDescription>
        </SheetHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-4 pb-4">
          {file.data?.changed_since_scan && (
            <Banner tone="warn" title={t("interview.sources.ce_fichier_a_change_depuis")}>
              {t("interview.sources.balayez_de_nouveau_la_bibliotheque")}
            </Banner>
          )}
          {file.isLoading && <Skeleton className="h-64 w-full" />}
          {file.error && <Banner tone="danger" title={t("interview.sources.le_fichier_n_a_pas")}>{file.error.message}</Banner>}
          {file.data && (
            <pre className="bg-muted/40 min-h-0 flex-1 overflow-auto rounded-md border p-4 font-serif text-[0.95rem] leading-relaxed whitespace-pre-wrap">
              {file.data.text}
            </pre>
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}

export function SourcesTab({ detail }: { detail: InterviewDetail }) {
  const qc = useQueryClient()
  const id = detail.entry.interview_id
  const [viewing, setViewing] = useState<string | null>(null)
  const seg = detail.segmentation
  const coded = detail.phase1.n_human_coded > 0 || detail.phase2.n_human_coded > 0

  const primary = useMutation({
    mutationFn: (rel: string) => api.setPrimary(id, rel),
    onSuccess: () => invalidateInterview(qc, id),
    onError: (e) => reportError(e),
  })
  const segment = useMutation({
    mutationFn: () => api.segment(id, detail.entry.primary_source ?? undefined),
    onSuccess: (s) => {
      toast.success(t("interview.sources.segmente_en_length_unites", { length: s.units.length }))
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, t("common.segmentation_refusee")),
  })

  return (
    <div className="flex flex-col gap-5">
      <Section
        title={t("common.segmentation")}
        aside={seg ? t("interview.sources.regle_v", { v: seg.rule.split(":")[0] }) : undefined}
      >
        {seg ? (
          <div className="flex flex-wrap items-start justify-between gap-4 text-sm">
            <div className="flex flex-col gap-1">
              <div>
                <span className="font-semibold tabular-nums">{t("interview.sources.length_unites", { length: seg.units.length })}</span> {t("interview.sources.issues_de")}{" "}
                <span className="ident">{seg.source_rel_path}</span>
              </div>
              <div className="text-muted-foreground text-xs">
                {t("interview.sources.empreinte_sha_256_de_la")} <span className="ident">{shortHash(seg.source_sha256, 16)}</span>{t("interview.sources.empreinte_de_la_liste_des")}{" "}
                <span className="ident">{shortHash(seg.hash, 16)}</span>
              </div>
              <div className="text-muted-foreground max-w-prose text-xs">{seg.rule}</div>
              {seg.warnings.length > 0 && (
                <details className="text-warn mt-1 text-xs">
                  <summary className="cursor-pointer">{t("interview.sources.length_lignes_non_converties_en", { length: seg.warnings.length })}</summary>
                  <ul className="mt-1 list-inside list-disc">
                    {seg.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
            <Button asChild>
              <a href={interviewHref(id, "coding", 1)}>{t("interview.sources.commencer_le_codage_phase_1")}</a>
            </Button>
          </div>
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
            <p className="text-muted-foreground max-w-prose">
              {detail.entry.primary_source
                ? t("interview.sources.segmentez_la_transcription_principal")
                : t("interview.sources.aucune_transcription_dans_ce_dossier")}
            </p>
            <Button disabled={!detail.entry.primary_source || segment.isPending} onClick={() => segment.mutate()}>
              {segment.isPending ? "Segmentation…" : t("interview.sources.segmenter_la_transcription_principal")}
            </Button>
          </div>
        )}
        {seg && coded && (
          <p className="text-muted-foreground mt-3 text-xs">
            {t("interview.sources.le_codage_a_commence_la")}
          </p>
        )}
      </Section>

      <Section title={t("interview.sources.fichiers_du_dossier_de_l")} aside={t("interview.sources.length_inventories", { length: detail.entry.sources.length })}>
        <div className="-m-4 overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="pl-4">{t("common.fichier")}</TableHead>
                <TableHead>{t("common.type")}</TableHead>
                <TableHead>SHA-256</TableHead>
                <TableHead className="text-right">{t("common.taille")}</TableHead>
                <TableHead>{t("common.modifie")}</TableHead>
                <TableHead className="pr-4 text-right">{t("common.actions")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {detail.entry.sources.map((f) => {
                const isPrimary = f.rel_path === detail.entry.primary_source
                const transcript = TRANSCRIPT.includes(f.kind)
                return (
                  <TableRow key={f.rel_path}>
                    <TableCell className="pl-4">
                      <span className="ident break-all">{f.rel_path}</span>
                      {isPrimary && (
                        <Badge variant="secondary" className="ml-2">
                          {t("interview.sources.principale")}
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">{KIND_LABEL[f.kind]}</TableCell>
                    <TableCell className="ident text-muted-foreground">{shortHash(f.sha256, 12)}</TableCell>
                    <TableCell className="text-right text-xs tabular-nums">{bytes(f.size)}</TableCell>
                    <TableCell className="text-muted-foreground text-xs">{dateTime(f.modified)}</TableCell>
                    <TableCell className="pr-4 text-right whitespace-nowrap">
                      <Button size="sm" variant="ghost" onClick={() => setViewing(f.rel_path)}>
                        {t("interview.sources.voir")}
                      </Button>
                      {transcript && !isPrimary && !seg && (
                        <Button size="sm" variant="outline" disabled={primary.isPending} onClick={() => primary.mutate(f.rel_path)}>
                          {t("interview.sources.utiliser_comme_principale")}
                        </Button>
                      )}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        </div>
      </Section>
      <FileViewer id={id} path={viewing} onClose={() => setViewing(null)} />
    </div>
  )
}
