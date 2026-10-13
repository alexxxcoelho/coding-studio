// Modèle de projet: the lists, templates, attributes and framework of the study
// as one JSON file, to start another study from the same method. Importing
// first asks the server for a dry run, shows what would change, and applies
// only on confirmation.

import { useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Alerts } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { ApiError, api } from "@/lib/api"
import { keys } from "@/lib/hooks"
import { reportError } from "@/lib/notify"
import type { TemplateReport } from "@/lib/types"

const msg = (e: unknown) => (e instanceof ApiError ? e.message : String(e))

/** Triggers the download of a JSON document. */
function download(name: string, data: unknown) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }))
  const a = document.createElement("a")
  a.href = url
  a.download = name
  a.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}

export function ProjectTemplateButtons() {
  const qc = useQueryClient()
  const input = useRef<HTMLInputElement>(null)
  const [pending, setPending] = useState<{ template: unknown; report: TemplateReport } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const exportIt = async () => {
    try {
      download("modele-de-projet.json", await api.projectTemplate())
    } catch (e) {
      reportError(e, "Export refusé")
    }
  }
  const pick = async (file: File | undefined) => {
    if (!file) return
    setError(null)
    try {
      const template: unknown = JSON.parse(await file.text())
      setPending({ template, report: await api.importTemplate(template, true) })
    } catch (e) {
      setPending(null)
      setError(e instanceof SyntaxError ? "Ce fichier n'est pas un JSON valide." : msg(e))
    }
    if (input.current) input.current.value = ""
  }
  const apply = async () => {
    if (!pending) return
    setBusy(true)
    try {
      await api.importTemplate(pending.template, false)
      await Promise.all([qc.invalidateQueries({ queryKey: keys.projectConfig }), qc.invalidateQueries({ queryKey: ["questions"] }), qc.invalidateQueries({ queryKey: ["config"] })])
      toast.success("Modèle de projet importé")
      setPending(null)
    } catch (e) {
      setError(msg(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <input ref={input} type="file" accept="application/json,.json" hidden aria-label="Fichier de modèle de projet" onChange={(e) => pick(e.target.files?.[0])} />
      <Button variant="outline" onClick={() => input.current?.click()}>
        Importer un modèle de projet…
      </Button>
      <Button variant="outline" onClick={exportIt}>
        Exporter…
      </Button>
      {error && !pending && (
        <div style={{ flexBasis: "100%" }}>
          <Alerts messages={[error]} />
        </div>
      )}
      <Dialog open={pending !== null} onOpenChange={(o) => !o && setPending(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Importer ce modèle de projet ?</DialogTitle>
            <DialogDescription>Voici ce que l'import changerait. Rien n'est modifié tant que vous ne confirmez pas.</DialogDescription>
          </DialogHeader>
          {pending && pending.report.problems.length > 0 && <Alerts messages={pending.report.problems} />}
          {pending && pending.report.changes.length === 0 && pending.report.problems.length === 0 && <p className="small l2">Ce modèle ne change rien.</p>}
          <ul className="small" style={{ margin: 0, paddingLeft: 18, maxHeight: 240, overflow: "auto" }}>
            {pending?.report.changes.map((c, i) => (
              <li key={i}>
                <b>{c.part}</b> : {c.action}
                {c.detail ? `, ${c.detail}` : ""}
              </li>
            ))}
          </ul>
          {error && <Alerts messages={[error]} />}
          <DialogFooter>
            <Button variant="outline" onClick={() => setPending(null)}>
              Annuler
            </Button>
            <Button disabled={busy || !pending || pending.report.problems.length > 0} onClick={apply}>
              Importer
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
