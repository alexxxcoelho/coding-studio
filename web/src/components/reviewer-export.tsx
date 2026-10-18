import { useEffect, useMemo, useRef, useState } from "react"
import { Anonymizer } from "@/components/anonymizer"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { request } from "@/lib/api"
import { t } from "@/i18n"
import { checkState, exportTree, toggleFields, type ExportNode, type ReviewerCatalogue, type ReviewerField } from "@/lib/reviewer-tree"

interface Result { dir: string; reader: string; zip: string; fields: number; files: number; source_bytes: number; pseudonym_replacements: number; url: string | null; published: false }

function Check({ state, label, change }: { state: boolean | "mixed"; label: string; change: (on: boolean) => void }) {
  const ref = useRef<HTMLInputElement>(null)
  useEffect(() => { if (ref.current) ref.current.indeterminate = state === "mixed" }, [state])
  return <input ref={ref} type="checkbox" checked={state === true} aria-checked={state} aria-label={label} onChange={e => change(e.target.checked)} className="size-4 shrink-0 accent-[var(--accent)]" />
}

function Branch({ node, selected, toggle, preview }: { node: ExportNode; selected: Set<string>; toggle: (ids: string[], on: boolean) => void; preview: (f: ReviewerField) => void }) {
  const [open, setOpen] = useState(false)
  const label = node.label || t("reviewer.empty_target")
  const state = checkState(node.ids, selected)
  return <li>
    <div className="flex min-h-8 items-center gap-2 rounded px-1 hover:bg-fill">
      <Check state={state} label={label} change={on => toggle(node.ids, on)} />
      {node.field ? <button type="button" className="min-w-0 flex-1 truncate text-left py-1" onClick={() => preview(node.field!)}>{label}</button>
        : <button type="button" aria-expanded={open} className="min-w-0 flex-1 truncate text-left py-1" onClick={() => setOpen(!open)}><span aria-hidden="true" className="mr-2">{open ? "▾" : "▸"}</span>{label} <span className="text-label2 text-xs">{node.ids.length}</span></button>}
    </div>
    {open && !node.field && <ul className="ml-4 border-l border-sep pl-2">{node.children.map(child => <Branch key={child.key} node={child} selected={selected} toggle={toggle} preview={preview} />)}</ul>}
  </li>
}

export function ReviewerExport({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  // Unmounting this inner wizard clears its password and prior selection.
  return open ? <Wizard onClose={() => onOpenChange(false)} /> : null
}

function Wizard({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState(0)
  const [catalogue, setCatalogue] = useState<ReviewerCatalogue | null>(null)
  const [selected, setSelected] = useState(new Set<string>())
  const pseudonymise = true
  const [search, setSearch] = useState("")
  const [preview, setPreview] = useState<ReviewerField | null>(null)
  const [password, setPassword] = useState("")
  const [confirmation, setConfirmation] = useState("")
  const [showPassword, setShowPassword] = useState(false)
  const [url, setUrl] = useState("")
  const [busy, setBusy] = useState(false)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState("")
  const [result, setResult] = useState<Result | null>(null)
  const [acknowledged, setAcknowledged] = useState(false)
  const initialized = useRef(false)
  useEffect(() => {
    let active = true
    setLoading(true); setError(""); setPreview(null)
    request<ReviewerCatalogue>("GET", `/api/export/reviewer/catalogue?pseudonymise=${pseudonymise}`).then(data => {
      if (!active) return
      setCatalogue(data)
      const available = new Set(data.fields.map(f => f.id))
      if (!initialized.current) { setSelected(new Set(data.fields.filter(f => f.selected_by_default).map(f => f.id))); initialized.current = true }
      else setSelected(previous => new Set([...previous].filter(id => available.has(id))))
    }).catch(e => { if (active) setError(String(e.message || e)) }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [pseudonymise])
  const tree = useMemo(() => exportTree(catalogue?.fields ?? []), [catalogue])
  const matches = useMemo(() => search.trim() ? (catalogue?.fields ?? []).filter(f => (f.path.join(" / ") + " " + JSON.stringify(f.value)).toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())) : [], [catalogue, search])
  const attachments = Object.entries(catalogue?.file_ids ?? {}).filter(([id]) => selected.has(id))
  const bytes = attachments.reduce((sum, [, size]) => sum + size, 0)
  const validPassword = Array.from(password).length >= 16 && password === confirmation
  let validUrl = !url.trim()
  try { const u = new URL(url); validUrl = u.protocol === "https:" && !u.username && !u.password && !u.search && !u.hash } catch { /* Empty is deliberately allowed. */ }
  const toggle = (ids: string[], on: boolean) => setSelected(s => toggleFields(s, ids, on))
  const generate = async () => {
    if (!catalogue || !validPassword || !validUrl || !selected.size || !acknowledged) return
    setBusy(true); setError("")
    const body = { snapshot: catalogue.snapshot, selected: [...selected], password, pseudonymise, url, privacy_reviewed: acknowledged }
    setPassword(""); setConfirmation("")
    try { setResult(await request<Result>("POST", "/api/export/reviewer", body)) }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); setStep(1) }
    finally { body.password = ""; setBusy(false) }
  }
  const openResult = async (kind: "folder" | "reader") => {
    try { await request("POST", "/api/export/reviewer/open", { dir: result?.dir, kind }) }
    catch (e) { setError(e instanceof Error ? e.message : String(e)) }
  }
  return <Dialog open onOpenChange={value => { if (!value && !busy) onClose() }}>
    <DialogContent style={{ width: "min(1020px, calc(100vw - 32px))", maxWidth: "none", maxHeight: "88vh" }} className="flex flex-col" showCloseButton={!busy}>
      <DialogHeader><DialogTitle>{t("reviewer.title")}</DialogTitle><DialogDescription>{t("reviewer.description")}</DialogDescription></DialogHeader>
      {!result && <ol aria-label={t("reviewer.steps")} className="flex gap-6 border-b border-sep pb-3 text-sm">{[t("reviewer.content"), t("reviewer.access"), t("reviewer.verify")].map((label, i) => <li key={label} aria-current={step === i ? "step" : undefined} className={step === i ? "font-semibold" : "text-label2"}>{i + 1}. {label}</li>)}</ol>}
      {error && <p role="alert" className="text-destructive whitespace-pre-wrap">{error}</p>}
      <div className="overflow-y-auto min-h-0 flex-1">
        {result ? <div className="space-y-4 py-4">
          <h2 className="text-lg font-semibold">{t("reviewer.ready")}</h2><p>{t("reviewer.result", { fields: result.fields, files: result.files, replacements: result.pseudonym_replacements })}</p>
          <p>{t("reviewer.local_only")}</p><p className="rounded bg-fill p-3 break-all select-text">{result.zip}</p>
          <div className="flex gap-2"><Button variant="outline" onClick={() => openResult("folder")}>{t("reviewer.open_folder")}</Button><Button variant="outline" onClick={() => openResult("reader")}>{t("reviewer.open_reader")}</Button></div>
          <p className="text-label2">{t("reviewer.password_separate")}</p>{result.url && <p className="break-all select-text">{t("reviewer.url_prepared", { url: result.url })}</p>}
        </div> : loading ? <p role="status">{t("reviewer.loading")}</p> : step === 0 ? <>
          <div className="flex flex-wrap items-center gap-3 mb-3">
            <span>{t("anonymizer.export_only")}</span><Anonymizer />
            <span className="text-xs text-label2">{t("reviewer.rules", { n: catalogue?.pseudonym_rules ?? 0 })}</span>
          </div>
          {catalogue?.coverage && <p className="text-xs text-label2 mb-2">{t("reviewer.coverage", catalogue.coverage)}</p>}
          <p className="text-xs text-label2 mb-3">{t("reviewer.selection_help")}</p>
          <div className="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-4" style={{ minHeight: 300, maxHeight: "48vh" }}>
            <section className="overflow-y-auto border border-sep rounded-lg p-3" aria-label={t("reviewer.field_tree")}>
              <Input value={search} onChange={e => setSearch(e.target.value)} aria-label={t("reviewer.search")} placeholder={t("reviewer.search")} className="mb-2" />
              <div className="flex gap-2 mb-2"><Button variant="ghost" size="sm" onClick={() => setSelected(new Set(catalogue?.fields.map(f => f.id)))}>{t("reviewer.select_all")}</Button><Button variant="ghost" size="sm" onClick={() => setSelected(new Set())}>{t("reviewer.clear")}</Button></div>
              {search.trim() ? <><p className="text-xs text-label2">{t("reviewer.search_count", { n: matches.length })}</p><ul>{matches.slice(0, 100).map(f => <li key={f.id} className="flex items-center gap-2 py-1"><Check state={selected.has(f.id)} label={f.path.join(" / ")} change={on => toggle([f.id], on)} /><button type="button" className="text-left text-xs break-all" onClick={() => setPreview(f)}>{f.path.join(" / ")}</button></li>)}</ul></>
                : <ul>{tree.map(node => <Branch key={node.key} node={node} selected={selected} toggle={toggle} preview={setPreview} />)}</ul>}
            </section>
            <section className="overflow-y-auto rounded-lg bg-fill p-4" aria-label={t("reviewer.preview")}><h2 className="font-semibold mb-2">{t("reviewer.preview")}</h2>{preview ? <><p className="text-xs text-label2 break-all mb-3">{preview.path.join(" / ")}</p><pre className="whitespace-pre-wrap break-words text-sm font-[inherit]">{String(typeof preview.value === "string" ? preview.value : JSON.stringify(preview.value, null, 2)).slice(0, 16000)}</pre><p className="text-xs text-label2 mt-3">{selected.has(preview.id) ? t("reviewer.included") : t("reviewer.excluded")}</p></> : <p className="text-label2">{t("reviewer.preview_help")}</p>}</section>
          </div>
        </> : step === 1 ? <div className="space-y-4 max-w-xl py-3">
          <p>{t("reviewer.password_help")}</p>
          <label className="block space-y-1"><span>{t("reviewer.password")}</span><Input type={showPassword ? "text" : "password"} autoComplete="new-password" value={password} onChange={e => setPassword(e.target.value)} /></label>
          <label className="block space-y-1"><span>{t("reviewer.confirm")}</span><Input type={showPassword ? "text" : "password"} autoComplete="new-password" value={confirmation} onChange={e => setConfirmation(e.target.value)} /></label>
          <label className="flex items-center gap-2"><input type="checkbox" checked={showPassword} onChange={e => setShowPassword(e.target.checked)} />{t("reviewer.show_password")}</label>
          <label className="block space-y-1"><span>{t("reviewer.url")}</span><Input type="url" value={url} onChange={e => setUrl(e.target.value)} placeholder="https://…" /></label>
          <p className="text-xs text-label2">{t("reviewer.url_help")}</p>{!validUrl && <p role="alert">{t("reviewer.url_invalid")}</p>}
        </div> : <div className="space-y-4 py-3">
          <p>{t("reviewer.summary", { fields: selected.size, files: attachments.length, mb: (bytes / 1024 / 1024).toFixed(1) })}</p>
          <p>{pseudonymise ? t("reviewer.masked_copy") : t("reviewer.original_copy")}</p>
          <p>{url.trim() ? t("reviewer.with_url") : t("reviewer.without_url")}</p>
          <p className="text-label2">{t("reviewer.review_help")}</p>
          {attachments.length > 0 && <p className="text-label2">{t("reviewer.attachments_help")}</p>}
          <label className="flex items-start gap-2"><input type="checkbox" checked={acknowledged} onChange={e => setAcknowledged(e.target.checked)} className="mt-1" />{t("reviewer.acknowledge")}</label>
        </div>}
      </div>
      <DialogFooter><span className="mr-auto self-center text-xs text-label2">{!result && t("reviewer.selected", { n: selected.size })}</span>
        {result ? <Button onClick={onClose}>{t("common.fermer")}</Button> : <><Button variant="outline" disabled={busy} onClick={() => step ? setStep(step - 1) : onClose()}>{step ? t("reviewer.back") : t("reviewer.cancel")}</Button>
          {step < 2 ? <Button disabled={loading || !catalogue || !selected.size || (step === 1 && (!validPassword || !validUrl))} onClick={() => setStep(step + 1)}>{t("reviewer.next")}</Button> : <Button disabled={busy || !acknowledged} onClick={generate}>{busy ? t("reviewer.generating") : t("reviewer.generate")}</Button>}</>}
      </DialogFooter>
    </DialogContent>
  </Dialog>
}
