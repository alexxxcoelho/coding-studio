import {Table, TableHeader, TableBody, TableRow} from '@/components/ui/table'
import { useEffect, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { request } from "@/lib/api"
import { t } from "@/i18n"

type Entry = { kind: string; original: string; pseudonym: string }
type Project = { entries: Entry[]; manual_entries: Entry[]; interviews: { id: string; reviewed: boolean }[]; prompt: string }
type Preview = { digest: string; reviewed: boolean; units: { id: string; speaker: string; speaker_anonymised: string; original: string; anonymised: string }[]; speaker_suggestions: string[]; candidates: {text: string; unit: string}[] }
export function Anonymizer({ interviewId }: { interviewId?: string }) {
  const [open, setOpen] = useState(false)
  return <><Button variant="outline" onClick={() => setOpen(true)}>{t("anonymizer.title")}</Button>{open && <AnonymizerDialog initial={interviewId} close={() => setOpen(false)} />}</>
}
function AnonymizerDialog({ initial, close }: { initial?: string; close: () => void }) {
  const qc = useQueryClient()
  const q = useQuery({ queryKey: ["anonymisation"], queryFn: () => request<Project>("GET", "/api/anonymisation") })
  const [id, setId] = useState(initial ?? "")
  const [original, setOriginal] = useState("")
  const [label, setLabel] = useState("")
  const [kind, setKind] = useState("person")
  const [search, setSearch] = useState("")
  const [page, setPage] = useState(0)
  const [confirmed, setConfirmed] = useState(false)
  const [error, setError] = useState("")
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState("")
  const p = useQuery({ queryKey: ["anonymisation",id], enabled: !!id, queryFn: () => request<Preview>("GET", `/api/anonymisation/interviews/${encodeURIComponent(id)}`) })
  useEffect(() => { if (!id && q.data?.interviews.length) setId(q.data.interviews[0].id) }, [q.data,id])
  useEffect(() => { setConfirmed(false); setPage(0); setResult("") }, [id,p.data?.digest])
  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true); setError("")
    try { await fn(); await qc.invalidateQueries({queryKey:["anonymisation"]}); await qc.invalidateQueries({queryKey:["prompts"]}); await qc.invalidateQueries({queryKey:["privacy-batch"]}) }
    catch(e) { setError(e instanceof Error ? e.message : String(e)) }
    finally { setBusy(false) }
  }
  const add = (name: string) => {
    setOriginal(name)
    const used = new Set(q.data?.entries.map(e => e.pseudonym))
    let n=1; while (used.has(`[Person ${n}]`)) n++
    setLabel(`[Person ${n}]`)
  }
  const save = () => act(async () => {
    const rows = (q.data?.manual_entries ?? []).filter(e => e.original.toLowerCase() !== original.trim().toLowerCase())
    rows.push({kind,original:original.trim(),pseudonym:label.trim()})
    await request("PUT","/api/anonymisation",{entries:rows})
    setOriginal(""); setLabel(""); setConfirmed(false)
  })
  const filtered = p.data?.units.filter(u => !search || `${u.id} ${u.original} ${u.anonymised}`.toLocaleLowerCase().includes(search.toLocaleLowerCase())) ?? []
  return <Dialog open onOpenChange={v => {if(!v && !busy) close()}}><DialogContent style={{width:"min(1200px, calc(100vw - 32px))",maxWidth:"none",maxHeight:"90vh"}} className="flex flex-col" showCloseButton={!busy}>
    <DialogHeader><DialogTitle>{t("anonymizer.title")}</DialogTitle><DialogDescription>{t("anonymizer.local")}</DialogDescription></DialogHeader>
    <div className="flex flex-wrap gap-3 items-center"><label>{t("anonymizer.interview")} <select aria-label={t("anonymizer.interview")} value={id} onChange={e => setId(e.target.value)}>{q.data?.interviews.map(i => <option key={i.id} value={i.id}>{i.id} · {i.reviewed ? t("anonymizer.reviewed") : t("anonymizer.pending")}</option>)}</select></label><span className="text-sm text-label2">{t("anonymizer.remaining", {n:q.data?.interviews.filter(i => !i.reviewed).length ?? 0})}</span></div>
    {(error || q.error || p.error) && <p role="alert" className="text-destructive">{error || q.error?.message || p.error?.message}</p>}
    <div className="overflow-y-auto space-y-4 min-h-0">
      <p className="text-sm">{t("anonymizer.limits")}</p>
      <details className="rounded border border-sep p-3"><summary>{t("anonymizer.private_table")} ({q.data?.entries.length ?? 0})</summary><p className="text-xs text-label2 my-2">{t("anonymizer.private_help")}</p><Table className="tbl"><TableHeader><TableRow><th>{t("anonymizer.original")}</th><th>{t("anonymizer.replacement")}</th><th /></TableRow></TableHeader><TableBody>{q.data?.entries.map(e => <TableRow key={e.original}><td>{e.original}</td><td>{e.pseudonym}</td><td><Button variant="ghost" onClick={() => {setOriginal(e.original);setLabel(e.pseudonym);setKind(e.kind)}}>{t("anonymizer.edit")}</Button>{q.data?.manual_entries.some(m=>m.original.toLowerCase()===e.original.toLowerCase()) && <Button variant="ghost" disabled={busy} onClick={()=>act(()=>request("PUT","/api/anonymisation",{entries:q.data!.manual_entries.filter(m=>m.original.toLowerCase()!==e.original.toLowerCase())}))}>{t("anonymizer.remove")}</Button>}</td></TableRow>)}</TableBody></Table></details>
      <div className="flex gap-2 flex-wrap items-end rounded bg-fill p-3"><label className="text-xs">{t("anonymizer.original")}<Input value={original} onChange={e => setOriginal(e.target.value)} /></label><label className="text-xs">{t("anonymizer.replacement")}<Input value={label} onChange={e => setLabel(e.target.value)} list="anonymizer-labels" /></label><datalist id="anonymizer-labels">{[...new Set(q.data?.entries.map(e => e.pseudonym))].map(v => <option key={v} value={v} />)}</datalist><select aria-label={t("anonymizer.kind")} value={kind} onChange={e => setKind(e.target.value)}>{["person","organisation","contact","other"].map(v => <option value={v} key={v}>{v}</option>)}</select><Button disabled={busy || !original.trim() || !label.trim()} onClick={save}>{t("anonymizer.save_rule")}</Button><span className="text-xs text-label2">{t("anonymizer.same_person")}</span></div>
      {!!p.data?.speaker_suggestions.length && <div><h3 className="font-semibold">{t("anonymizer.speakers")}</h3><div className="flex flex-wrap gap-2">{p.data.speaker_suggestions.map(name => <Button variant="outline" key={name} onClick={() => add(name)}>{name}</Button>)}</div></div>}
      <details><summary>{t("anonymizer.candidates")} ({p.data?.candidates.length ?? 0})</summary><p className="text-xs text-label2">{t("anonymizer.candidates_help")}</p><div className="flex flex-wrap gap-1 max-h-36 overflow-y-auto">{p.data?.candidates.map(c => <button type="button" className="rounded border border-sep px-2 py-1 text-xs" key={c.text} onClick={() => {add(c.text);setSearch(c.text)}}>{c.text} · {c.unit}</button>)}</div></details>
      <Input aria-label={t("anonymizer.search")} placeholder={t("anonymizer.search")} value={search} onChange={e => {setSearch(e.target.value);setPage(0)}} />
      <div className="grid grid-cols-2 gap-3 font-semibold"><span>{t("anonymizer.original_local")}</span><span>{t("anonymizer.copy")}</span></div>
      {p.isLoading ? <p role="status">{t("anonymizer.loading")}</p> : filtered.slice(page*25,(page+1)*25).map(u => <div key={u.id} className="grid grid-cols-2 gap-3 border-t border-sep pt-3 text-sm"><div><p className="text-xs text-label2">{u.id} · {u.speaker}</p><p className="whitespace-pre-wrap break-words select-text">{u.original}</p></div><div><p className="text-xs text-label2">{u.id} · {u.speaker_anonymised}</p><p className="whitespace-pre-wrap break-words select-text">{u.anonymised}</p></div></div>)}
      <div className="flex gap-3 items-center"><Button variant="outline" disabled={!page} onClick={() => setPage(page-1)}>{t("anonymizer.previous")}</Button><span>{Math.min(page*25+1,filtered.length)}–{Math.min((page+1)*25,filtered.length)} / {filtered.length}</span><Button variant="outline" disabled={(page+1)*25>=filtered.length} onClick={() => setPage(page+1)}>{t("anonymizer.next")}</Button></div>
      <details><summary>{t("anonymizer.prompt")}</summary><pre className="whitespace-pre-wrap text-xs p-3 bg-fill">{q.data?.prompt}</pre></details>
    </div>
    <div className="border-t border-sep pt-3 space-y-2"><label className="flex gap-2 text-sm"><input type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} />{t("anonymizer.attest")}</label><div className="flex flex-wrap gap-3 items-center"><Button disabled={!confirmed || busy || !p.data} onClick={() => act(() => request("PUT",`/api/anonymisation/interviews/${encodeURIComponent(id)}`,{digest:p.data!.digest,reviewed:true}))}>{t("anonymizer.approve")}</Button><Button variant="outline" disabled={!p.data?.reviewed || busy} onClick={() => act(async () => {const r=await request<{dir:string}>("POST",`/api/anonymisation/interviews/${encodeURIComponent(id)}/export`);setResult(r.dir)})}>{t("anonymizer.export")}</Button><span role="status" className="text-xs text-label2 break-all">{result || (p.data?.reviewed ? t("anonymizer.reviewed") : t("anonymizer.pending"))}</span></div></div>
  </DialogContent></Dialog>
}

export function PromptPrivacy({id,batch}: {id:string;batch:number}) {
  const qc=useQueryClient();const [confirmed,setConfirmed]=useState(false);const [error,setError]=useState("")
  const q=useQuery({queryKey:["privacy-batch",id,batch],queryFn:()=>request<{digest:string;reviewed:boolean;transcript_reviewed:boolean}>("GET",`/api/anonymisation/interviews/${encodeURIComponent(id)}/batches/${batch}`)})
  useEffect(()=>setConfirmed(false),[q.data?.digest])
  return <div className="rounded border border-sep p-3 space-y-2"><Anonymizer interviewId={id}/><p className="text-sm">{t("anonymizer.prompt_help")}</p><label className="flex gap-2 text-sm"><input type="checkbox" checked={confirmed} onChange={e=>setConfirmed(e.target.checked)}/>{t("anonymizer.prompt_attest")}</label><Button disabled={!confirmed || !q.data?.transcript_reviewed} onClick={async()=>{try{await request("PUT",`/api/anonymisation/interviews/${encodeURIComponent(id)}/batches/${batch}`,{digest:q.data!.digest,reviewed:true});await qc.invalidateQueries({queryKey:["privacy-batch",id,batch]})}catch(e){setError(e instanceof Error?e.message:String(e))}}}>{t("anonymizer.approve_prompt")}</Button><p role="status" className="text-xs">{q.data?.reviewed?t("anonymizer.prompt_ready"):t("anonymizer.prompt_blocked")}</p>{(error||q.error)&&<p role="alert">{error||q.error?.message}</p>}</div>
}
