// "Importer des transcriptions": the folder with one sub-folder per interview.
// A scan records every added, removed or changed file with its SHA-256.

import { useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { api } from "@/lib/api"
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
      toast.success(`${res.interviews.length} entretien${res.interviews.length > 1 ? "s" : ""} trouvé${res.interviews.length > 1 ? "s" : ""}`)
      onOpenChange(false)
    },
    onError: (e) => reportError(e, "Le scan a échoué"),
  })

  const browse = async () => {
    try {
      const res = await api.pick("folder", "Choisir le dossier des entretiens")
      if (res.path) setTyped(res.path)
    } catch (e) {
      reportError(e)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Importer des transcriptions</DialogTitle>
          <DialogDescription>
            Un sous-dossier par entretien, nommé avec son identifiant (par exemple BIB-01). Les transcriptions sont lues, jamais modifiées.
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
            <span className="l2 small">Dossier des entretiens</span>
            <div className="flex gap-2">
              <Input value={root} onChange={(e) => setTyped(e.target.value)} placeholder="/chemin/vers/Interviews" />
              {state.data?.native_dialogs && (
                <Button type="button" variant="outline" onClick={browse}>
                  Parcourir…
                </Button>
              )}
            </div>
          </label>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Annuler
            </Button>
            <Button type="submit" disabled={scan.isPending}>
              {scan.isPending ? "Scan en cours…" : "Importer"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
