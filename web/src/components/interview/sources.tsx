import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Banner, Section } from "@/components/bits"
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
  vtt: "Transcription de plateforme (VTT)",
  timestamped_text: "Texte brut de reconnaissance vocale",
  cleaned_markdown: "Transcription épurée",
  budgie_markdown: "Export Budgie Echo",
  written_response: "Réponse écrite",
  summary: "Synthèse",
  record_sheet: "Fiche d'enregistrement",
  toe_coding: "Codage antérieur",
  other: "Autre",
}

const TRANSCRIPT: SourceKind[] = ["vtt", "timestamped_text", "cleaned_markdown", "budgie_markdown", "written_response"]

function FileViewer({ id, path, onClose }: { id: string; path: string | null; onClose: () => void }) {
  const file = useQuery({ queryKey: ["file", id, path], queryFn: () => api.file(id, path!), enabled: !!path })
  return (
    <Sheet open={!!path} onOpenChange={(o) => !o && onClose()}>
      <SheetContent side="right" className="w-[min(52rem,92vw)] sm:max-w-none">
        <SheetHeader>
          <SheetTitle className="ident break-all">{path}</SheetTitle>
          <SheetDescription>Lecture seule. Le fichier sur le disque n'est jamais modifié.</SheetDescription>
        </SheetHeader>
        <div className="flex min-h-0 flex-1 flex-col gap-3 px-4 pb-4">
          {file.data?.changed_since_scan && (
            <Banner tone="warn" title="Ce fichier a changé depuis le dernier balayage">
              Balayez de nouveau la bibliothèque pour que l'inventaire et le journal d'audit enregistrent la nouvelle empreinte SHA-256.
            </Banner>
          )}
          {file.isLoading && <Skeleton className="h-64 w-full" />}
          {file.error && <Banner tone="danger" title="Le fichier n'a pas pu être lu">{file.error.message}</Banner>}
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
      toast.success(`Segmenté en ${s.units.length} unités`)
      invalidateInterview(qc, id)
    },
    onError: (e) => reportError(e, "Segmentation refusée"),
  })

  return (
    <div className="flex flex-col gap-5">
      <Section
        title="Segmentation"
        aside={seg ? `règle ${seg.rule.split(":")[0]}` : undefined}
      >
        {seg ? (
          <div className="flex flex-wrap items-start justify-between gap-4 text-sm">
            <div className="flex flex-col gap-1">
              <div>
                <span className="font-semibold tabular-nums">{seg.units.length} unités</span> issues de{" "}
                <span className="ident">{seg.source_rel_path}</span>
              </div>
              <div className="text-muted-foreground text-xs">
                Empreinte SHA-256 de la source <span className="ident">{shortHash(seg.source_sha256, 16)}</span>, empreinte de la liste des unités{" "}
                <span className="ident">{shortHash(seg.hash, 16)}</span>
              </div>
              <div className="text-muted-foreground max-w-prose text-xs">{seg.rule}</div>
              {seg.warnings.length > 0 && (
                <details className="text-warn mt-1 text-xs">
                  <summary className="cursor-pointer">{seg.warnings.length} lignes non converties en unités</summary>
                  <ul className="mt-1 list-inside list-disc">
                    {seg.warnings.map((w, i) => (
                      <li key={i}>{w}</li>
                    ))}
                  </ul>
                </details>
              )}
            </div>
            <Button asChild>
              <a href={interviewHref(id, "coding", 1)}>Commencer le codage, phase 1</a>
            </Button>
          </div>
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3 text-sm">
            <p className="text-muted-foreground max-w-prose">
              {detail.entry.primary_source
                ? "Segmentez la transcription principale : une unité par tour de parole, les tours longs coupés en fin de phrase. Le même fichier donne toujours les mêmes unités."
                : "Aucune transcription dans ce dossier. Ajoutez-en une et balayez de nouveau la bibliothèque."}
            </p>
            <Button disabled={!detail.entry.primary_source || segment.isPending} onClick={() => segment.mutate()}>
              {segment.isPending ? "Segmentation…" : "Segmenter la transcription principale"}
            </Button>
          </div>
        )}
        {seg && coded && (
          <p className="text-muted-foreground mt-3 text-xs">
            Le codage a commencé, la segmentation est donc figée. Toute modification romprait le lien entre les codes et les unités.
          </p>
        )}
      </Section>

      <Section title="Fichiers du dossier de l'entretien" aside={`${detail.entry.sources.length} inventoriés`}>
        <div className="-m-4 overflow-x-auto">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="pl-4">Fichier</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>SHA-256</TableHead>
                <TableHead className="text-right">Taille</TableHead>
                <TableHead>Modifié</TableHead>
                <TableHead className="pr-4 text-right">Actions</TableHead>
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
                          principale
                        </Badge>
                      )}
                    </TableCell>
                    <TableCell className="text-muted-foreground text-xs">{KIND_LABEL[f.kind]}</TableCell>
                    <TableCell className="ident text-muted-foreground">{shortHash(f.sha256, 12)}</TableCell>
                    <TableCell className="text-right text-xs tabular-nums">{bytes(f.size)}</TableCell>
                    <TableCell className="text-muted-foreground text-xs">{dateTime(f.modified)}</TableCell>
                    <TableCell className="pr-4 text-right whitespace-nowrap">
                      <Button size="sm" variant="ghost" onClick={() => setViewing(f.rel_path)}>
                        Voir
                      </Button>
                      {transcript && !isPrimary && !seg && (
                        <Button size="sm" variant="outline" disabled={primary.isPending} onClick={() => primary.mutate(f.rel_path)}>
                          Utiliser comme principale
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
