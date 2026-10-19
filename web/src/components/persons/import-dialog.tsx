import {TableMapping} from "./table-mapping"
import type {WorkbookPreview} from "@/lib/persons-table"
import {request} from "@/lib/api"
// Import of persons and organisations from a CSV file: paste or choose the
// file, a dry run reports what would be created, updated and linked (and every
// problem, row by row), then "Appliquer" writes it in one transaction. A file
// that changes after the dry run must be checked again. The template is the
// header row the server reads, with one column per person attribute.

import { useRef, useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Alerts } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { TEXTAREA } from "@/components/settings/fields"
import { t } from "@/i18n"
import { downloadText } from "@/lib/download"
import { invalidateRegister, personsApi, type ImportReport } from "@/lib/persons-api"
import { personAttributes } from "@/lib/attributes"
import { templateCsv } from "@/lib/persons-csv"
import { plainError, plainImportMessage } from "@/lib/persons-errors"
import { useProjectConfig } from "@/lib/hooks"

export function ImportDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const qc = useQueryClient()
  const config = useProjectConfig()
  const [csv, setCsv] = useState("")
  const [book,setBook]=useState<WorkbookPreview|null>(null)
  const [reading,setReading]=useState(false)
  const [fileName, setFileName] = useState<string | null>(null)
  const [report, setReport] = useState<{ csv: string; r: ImportReport } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)

  const run = useMutation({
    mutationFn: (dry: boolean) => personsApi.importCsv(csv, dry).then((r) => ({ dry, r })),
    onSuccess: async ({ dry, r }) => {
      setError(null)
      setReport({ csv, r })
      if (!dry && r.applied) {
        await invalidateRegister(qc)
        toast.success(t("persons.import_done", { created: r.n_create, updated: r.n_update }))
      }
    },
    onError: (e) => setError(plainError(e)),
  })

  const current = report && report.csv === csv ? report.r : null
  const applied = current?.applied === true
  const canApply = !!current && current.ok && current.dry_run && !run.isPending

  const readFile = async (file: File | undefined) => {
    if (!file) return
    setFileName(file.name);setReport(null);setError(null);setBook(null);setCsv("");setReading(true)
    try {
      if(/\.xls[xm]$/i.test(file.name)) {
        if(file.size>5*1024*1024)throw new Error(t("persons.excel_too_large"))
        const data=Array.from(new Uint8Array(await file.arrayBuffer()));setBook(await request<WorkbookPreview>("POST","/api/persons/import-table",{data}))
      } else if(/\.xls$/i.test(file.name))throw new Error(t("persons.excel_old_format"))
      else setCsv(await file.text())
    } catch(e){setError(e instanceof Error?e.message:String(e))} finally {setReading(false)}
  }

  const download = () => {
    const ids = personAttributes(config.data?.attributes).map((a) => a.id)
    downloadText("personnes_modele.csv", templateCsv(ids), "text/csv;charset=utf-8")
  }

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-2xl" style={{maxHeight:"90vh",overflowY:"auto"}}>
        <DialogHeader>
          <DialogTitle>{t("persons.import_title")}</DialogTitle>
          <DialogDescription>{t("persons.import_help")}</DialogDescription>
        </DialogHeader>

        <div className="r8">
          <Button variant="outline" onClick={download}>
            {t("persons.download_template")}
          </Button>
          <Button variant="outline" onClick={() => fileRef.current?.click()}>
            {t("persons.choose_csv")}
          </Button>
          <input ref={fileRef} type="file" accept=".xlsx,.xlsm,.csv,text/csv,text/plain,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" hidden aria-label={t("persons.csv_file_input")} onChange={(e) => void readFile(e.target.files?.[0])} />
          {fileName && <span className="small l2">{fileName}</span>}
        </div>

        {book&&<TableMapping key={fileName} book={book} onCsv={setCsv}/>}
        {reading&&<p role="status">{t("persons.excel_reading")}</p>}
        <textarea
          className={TEXTAREA}
          style={{ fontFamily: "ui-monospace, Menlo, monospace", fontSize: 12 }}
          rows={7}
          aria-label={t("persons.csv_text")}
          placeholder={t("persons.csv_placeholder")}
          value={csv}
          onChange={(e) => {
            setCsv(e.target.value)
            setError(null)
          }}
        />

        {error && <Alerts messages={[error]} />}
        {current && <Report r={current} />}
        {report && !current && <p className="small l2">{t("persons.file_changed")}</p>}

        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {applied ? t("common.fermer") : t("common.annuler")}
          </Button>
          <Button variant="outline" disabled={reading||!csv.trim() || run.isPending} onClick={() => run.mutate(true)}>
            {t("persons.check_file")}
          </Button>
          <Button disabled={!canApply} onClick={() => run.mutate(false)}>
            {t("persons.apply_import")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function Report({ r }: { r: ImportReport }) {
  return (
    <div className="flex flex-col gap-2" data-testid="import-report" aria-live="polite">
      <p className="m-0 text-sm">
        <b>{r.applied ? t("persons.report_applied") : r.ok ? t("persons.report_dry_ok") : t("persons.report_dry_problems")}</b>{" "}
        {t("persons.report_counts", { rows: r.n_rows, created: r.n_create, updated: r.n_update, unchanged: r.n_unchanged, links: r.n_links })}
      </p>
      {r.groups_to_create.length > 0 && (
        <p className="small l2 m-0">
          {r.applied ? t("persons.report_groups_done") : t("persons.report_groups")} {r.groups_to_create.join(", ")}
        </p>
      )}
      {r.problems.length > 0 && (
        <table className="tbl" aria-label={t("persons.report_problems")}>
          <thead>
            <tr>
              <th style={{ width: 70 }}>{t("persons.report_row")}</th>
              <th>{t("persons.report_problem")}</th>
            </tr>
          </thead>
          <tbody>
            {r.problems.map((p, i) => (
              <tr key={i}>
                <td>{p.row === 0 ? t("persons.report_file") : p.row === 1 ? t("persons.report_header") : p.row}</td>
                <td>{plainImportMessage(p.message)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {r.warnings.length > 0 && <Alerts messages={r.warnings.map(plainImportMessage)} />}
      {r.problems.length === 0 && r.rows.length > 0 && (
        <div style={{ maxHeight: 180, overflow: "auto" }}>
          <table className="tbl" aria-label={t("persons.report_rows")}>
            <thead>
              <tr>
                <th style={{ width: 70 }}>{t("persons.report_row")}</th>
                <th>{t("common.nom")}</th>
                <th>{t("persons.report_action")}</th>
              </tr>
            </thead>
            <tbody>
              {r.rows.map((row) => (
                <tr key={row.row}>
                  <td>{row.row}</td>
                  <td>
                    {row.name} <span className="small l2 mono">{row.person_id}</span>
                  </td>
                  <td>{row.action === "create" ? t("persons.action_create") : row.action === "update" ? t("persons.action_update") : t("persons.action_unchanged")}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
