import {useState} from 'react'
import {Button} from '@/components/ui/button'
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription} from '@/components/ui/dialog'
import {apiUrl,toApiError} from '@/lib/api'
import {t} from '@/i18n'
export function ParticipantExport({id}:{id:string}){
 const [open,setOpen]=useState(false);const [codes,setCodes]=useState(true);const [themes,setThemes]=useState(true);const [comments,setComments]=useState(true);const [busy,setBusy]=useState(false);const [error,setError]=useState('')
 const generate=async()=>{setBusy(true);setError('');try{
  const params=new URLSearchParams({codes:String(codes),themes:String(themes),comments:String(comments)});const r=await fetch(apiUrl(`/api/interviews/${encodeURIComponent(id)}/participant-review.docx?${params}`));
  if(!r.ok)throw toApiError(r.status,await r.json());const blob=await r.blob();const url=URL.createObjectURL(blob);const a=document.createElement('a');a.href=url;a.download=`validation-${id}.docx`;document.body.append(a);a.click();a.remove();URL.revokeObjectURL(url);setOpen(false)
 }catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}
 return <><Button variant="outline" onClick={()=>setOpen(true)}>{t('participant.action')}</Button><Dialog open={open} onOpenChange={v=>{if(!busy)setOpen(v)}}><DialogContent><DialogHeader><DialogTitle>{t('participant.title',{id})}</DialogTitle><DialogDescription>{t('participant.help')}</DialogDescription></DialogHeader>
 <p className="text-sm">{t('participant.transcripts')}</p>
 {[[codes,setCodes,'participant.codes'],[themes,setThemes,'participant.themes'],[comments,setComments,'participant.comments']].map(([value,set,key])=><label className="flex gap-2" key={String(key)}><input type="checkbox" checked={value as boolean} onChange={e=>(set as (v:boolean)=>void)(e.target.checked)}/>{t(key as 'participant.codes'|'participant.themes'|'participant.comments')}</label>)}
 {error&&<p role="alert" className="text-destructive">{error}</p>}<Button disabled={busy} onClick={generate}>{busy?t('participant.generating'):t('participant.download')}</Button></DialogContent></Dialog></>
}
