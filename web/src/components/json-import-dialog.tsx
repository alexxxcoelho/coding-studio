import { useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { request } from "@/lib/api"
import { t } from "@/i18n"

type Preview = {digest: string; count: number; replacements: number; changes: {code_id?:string; unit_id?:string; excerpt?:string; requested_decision?:string; before:unknown; after:unknown}[]}

/** Shared local preview/commit flow. Editing the source invalidates the preview. */
export function JsonImportDialog({open,onOpenChange,endpoint,title,description}:{open:boolean;onOpenChange:(v:boolean)=>void;endpoint:string;title:string;description:string}) {
  const qc=useQueryClient()
  const [raw,setRaw]=useState("")
  const [preview,setPreview]=useState<Preview|null>(null)
  const [replace,setReplace]=useState(false)
  const [busy,setBusy]=useState(false)
  const [error,setError]=useState("")
  const edit=(value:string)=>{setRaw(value);setPreview(null);setReplace(false);setError("")}
  const verify=async()=>{
    setBusy(true);setError("");setPreview(null);setReplace(false)
    try {setPreview(await request<Preview>("POST",`${endpoint}/preview`,{raw}))}
    catch(e){setError(e instanceof Error?e.message:String(e))}
    finally {setBusy(false)}
  }
  const apply=async()=>{
    if(!preview)return
    setBusy(true);setError("")
    try {
      const result=await request<{count:number}>("POST",endpoint,{raw,expected_digest:preview.digest,replace_existing:replace})
      await qc.invalidateQueries()
      toast.success(t("json_import.saved",{n:result.count}))
      edit("");onOpenChange(false)
    } catch(e){setError(e instanceof Error?e.message:String(e));setPreview(null)}
    finally{setBusy(false)}
  }
  return <Dialog open={open} onOpenChange={v=>{if(!busy)onOpenChange(v)}}>
    <DialogContent className="sm:max-w-4xl max-h-[90vh] overflow-y-auto" onKeyDown={e=>e.stopPropagation()}>
      <DialogHeader><DialogTitle>{title}</DialogTitle><DialogDescription>{description}</DialogDescription></DialogHeader>
      <label className="grid gap-2">{t("json_import.source")}
        <Textarea value={raw} onChange={e=>edit(e.target.value)} disabled={busy} className="font-mono min-h-44" spellCheck={false}/>
      </label>
      <label className="text-sm">{t("json_import.file")}
        <input type="file" accept=".json,application/json" disabled={busy} onChange={async e=>{
          const file=e.target.files?.[0]; if(!file)return
          try{edit(await file.text())}catch(err){setError(String(err))}
          e.target.value=""
        }}/>
      </label>
      {error&&<p role="alert" className="text-red-600 whitespace-pre-wrap">{error}</p>}
      {preview&&<section aria-label={t("json_import.preview")} className="grid gap-3">
        <p>{t("json_import.count",{n:preview.count})}</p>
        {preview.changes.map((change,index)=><article key={change.unit_id??change.code_id??index} className="rounded border p-3 grid gap-2">
          <strong>{change.unit_id??change.code_id}</strong>
          {change.excerpt&&<blockquote className="whitespace-pre-wrap border-l-2 pl-3">{change.excerpt}</blockquote>}
          {change.requested_decision&&<p>{change.requested_decision}</p>}
          <div className="grid sm:grid-cols-2 gap-3">
            <div><h3>{t("json_import.before")}</h3><pre className="text-xs whitespace-pre-wrap break-words">{JSON.stringify(change.before,null,2)}</pre></div>
            <div><h3>{t("json_import.after")}</h3><pre className="text-xs whitespace-pre-wrap break-words">{JSON.stringify(change.after,null,2)}</pre></div>
          </div>
        </article>)}
        {preview.replacements>0&&<label className="flex gap-2 items-center"><input type="checkbox" checked={replace} onChange={e=>setReplace(e.target.checked)} disabled={busy}/>{t("json_import.replace",{n:preview.replacements})}</label>}
      </section>}
      <DialogFooter>
        <Button variant="outline" disabled={busy} onClick={()=>onOpenChange(false)}>{t("common.annuler")}</Button>
        <Button variant="outline" disabled={busy||!raw.trim()} onClick={verify}>{t("json_import.verify")}</Button>
        <Button disabled={busy||!preview||preview.count===0||(preview.replacements>0&&!replace)} onClick={apply}>{busy?t("json_import.busy"):t("json_import.apply")}</Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>
}
