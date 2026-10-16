// "Importer des transcriptions": the folder with one sub-folder per interview.
// A scan records every added, removed or changed file with its SHA-256.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { keys } from "@/lib/hooks"
import { reportError } from "@/lib/notify"

export function ImportDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const qc = useQueryClient()
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const [typed, setTyped] = useState<string | null>(null)
  const root = typed ?? state.data?.library_root ?? ""

  const scan = useMutation({
    mutationFn: () => api.scan(root.trim() || undefined),
    onSuccess: (res) => {
      qc.invalidateQueries({ queryKey: ["interviews"] })
      qc.invalidateQueries({ queryKey: ["state"] })
      qc.invalidateQueries({ queryKey: ["chain"] })
      qc.invalidateQueries({ queryKey: keys.libraryProgress })
      qc.invalidateQueries({ queryKey: keys.corpusReviewState })
      toast.success(t("library.import_dialog.n_entretiens_trouves", { n: res.interviews.length }))
      onOpenChange(false)
    },
    onError: (e) => reportError(e, t("common.le_scan_a_echoue")),
  })

  const browse = async () => {
    try {
      const res = await api.pick("folder", t("library.import_dialog.choisir_le_dossier_des_entretiens"))
      if (res.path) setTyped(res.path)
    } catch (e) {
      reportError(e)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("library.import_dialog.importer_des_transcriptions")}</DialogTitle>
          <DialogDescription>
            {t("library.import_dialog.un_sous_dossier_par_entretien")}
          </DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            scan.mutate()
          }}
        >
          <label className="flex flex-col gap-1.5 text-sm">
            <span className="l2 small">{t("common.dossier_des_entretiens")}</span>
            <div className="flex gap-2">
              <Input value={root} onChange={(e) => setTyped(e.target.value)} placeholder="/chemin/vers/Interviews" />
              {state.data?.native_dialogs && (
                <Button type="button" variant="outline" onClick={browse}>
                  {t("library.import_dialog.parcourir")}
                </Button>
              )}
            </div>
          </label>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              {t("common.annuler")}
            </Button>
            <Button type="submit" disabled={scan.isPending}>
              {scan.isPending ? t("library.import_dialog.scan_en_cours") : t("library.import_dialog.importer")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
