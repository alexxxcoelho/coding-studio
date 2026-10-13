// "Paquet de vérification": for an examiner who checks the coding against the
// sources. Choose the interviews, the server writes the package (public list of
// persons, groups, the sources and the coded transcript masked with the
// effective pseudonymisation list) and this dialog reports where it is and what
// was masked: counts by kind of record, never the names.

import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"

import { Alerts } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { api } from "@/lib/api"
import { shortHash } from "@/lib/format"
import { personsApi, type VerificationResult } from "@/lib/persons-api"
import { plainError } from "@/lib/persons-errors"
import { reportError } from "@/lib/notify"

const KIND_WORDS: Record<string, () => string> = {
  person: () => t("persons.masked_person"),
  organisation: () => t("persons.masked_organisation"),
  researcher: () => t("persons.masked_researcher"),
}

export function VerificationDialog({ open, onClose, initialIds = [] }: { open: boolean; onClose: () => void; initialIds?: string[] }) {
  const interviews = useQuery({ queryKey: ["interviews"], queryFn: api.interviews })
  const [picked, setPicked] = useState<ReadonlySet<string>>(new Set(initialIds))
  const [outDir, setOutDir] = useState("")
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<VerificationResult | null>(null)
  const all = (interviews.data ?? []).map((s) => s.interview_id)

  const run = useMutation({
    mutationFn: () => personsApi.verificationExport({ interview_ids: all.filter((id) => picked.has(id)), ...(outDir.trim() ? { out_dir: outDir.trim() } : {}) }),
    onSuccess: (r) => {
      setError(null)
      setResult(r)
    },
    onError: (e) => {
      setResult(null)
      setError(plainError(e))
    },
  })

  const toggle = (id: string) =>
    setPicked((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })

  const browse = async () => {
    try {
      const res = await api.pick("folder", t("persons.verification_choose_folder"))
      if (res.path) setOutDir(res.path)
    } catch (e) {
      reportError(e)
    }
  }

  const total = result ? Object.values(result.replacements).reduce((a, b) => a + b, 0) : 0

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t("persons.verification_title")}</DialogTitle>
          <DialogDescription>{t("persons.verification_help")}</DialogDescription>
        </DialogHeader>

        <fieldset className="m-0 flex flex-col gap-1 border-0 p-0">
          <legend className="small l2 mb-1 flex w-full items-center justify-between">
            <span>{t("persons.verification_interviews")}</span>
            <span className="r8" style={{ gap: 4 }}>
              <Button variant="ghost" size="sm" disabled={all.length === 0} onClick={() => setPicked(new Set(all))}>
                {t("persons.select_all")}
              </Button>
              <Button variant="ghost" size="sm" disabled={picked.size === 0} onClick={() => setPicked(new Set())}>
                {t("persons.select_none")}
              </Button>
            </span>
          </legend>
          <div style={{ maxHeight: 160, overflow: "auto" }} className="grp">
            {all.length === 0 && (
              <div className="row">
                <span className="k l2">{t("persons.verification_no_interview")}</span>
              </div>
            )}
            {(interviews.data ?? []).map((s) => (
              <label className="row" key={s.interview_id} style={{ minHeight: 32, padding: "4px 12px" }}>
                <input type="checkbox" checked={picked.has(s.interview_id)} onChange={() => toggle(s.interview_id)} aria-label={s.interview_id} />
                <span className="k">
                  <span className="mono">{s.interview_id}</span>
                  <small>{s.folder_name}</small>
                </span>
              </label>
            ))}
          </div>
        </fieldset>

        <label className="flex flex-col gap-1 text-xs">
          <span className="text-muted-foreground">{t("persons.verification_out_dir")}</span>
          <span className="flex gap-2">
            <Input value={outDir} placeholder={t("persons.verification_out_dir_placeholder")} onChange={(e) => setOutDir(e.target.value)} />
            <Button type="button" variant="outline" onClick={() => void browse()}>
              {t("library.import_dialog.parcourir")}
            </Button>
          </span>
        </label>

        {error && <Alerts messages={[error]} />}

        {result && (
          <div className="flex flex-col gap-2" role="region" aria-label={t("persons.verification_result")} data-testid="verification-result">
            <p className="m-0 text-sm">
              <b>{t("persons.verification_done", { n: result.interview_ids.length })}</b>
            </p>
            <div className="grp">
              <div className="row">
                <span className="k">
                  {t("persons.verification_folder")}
                  <small className="mono" style={{ wordBreak: "break-all" }} data-testid="verification-dir">
                    {result.dir}
                  </small>
                </span>
              </div>
              <div className="row">
                <span className="k">
                  {t("persons.verification_masked")}
                  <small>
                    {total === 0
                      ? t("persons.verification_nothing_masked")
                      : Object.entries(result.replacements)
                          .filter(([, n]) => n > 0)
                          .map(([k, n]) => `${KIND_WORDS[k]?.() ?? k} : ${n}`)
                          .join(" · ")}
                  </small>
                </span>
              </div>
              <div className="row">
                <span className="k">
                  {t("persons.verification_files", { n: result.files.length })}
                  <small className="mono" style={{ wordBreak: "break-all" }}>
                    {result.files.map((f) => f.name).join(", ")}
                  </small>
                </span>
              </div>
              <div className="row">
                <span className="k">
                  {t("persons.verification_digests")}
                  <small className="mono">
                    <code>manifest</code> {shortHash(result.manifest_sha256, 12)} · {t("persons.verification_list")} {shortHash(result.pseudonymisation_digest, 12)}
                  </small>
                </span>
              </div>
            </div>
            {result.warnings.length > 0 && <Alerts messages={result.warnings} />}
            <p className="small l2 m-0">{t("persons.verification_note")}</p>
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.fermer")}
          </Button>
          <Button disabled={picked.size === 0 || run.isPending} onClick={() => run.mutate()}>
            {t("persons.verification_run")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
