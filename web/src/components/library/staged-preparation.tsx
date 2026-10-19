import {useEffect,useMemo,useRef,useState} from 'react'
import {useQueryClient} from '@tanstack/react-query'
import {Dialog,DialogContent,DialogDescription,DialogHeader,DialogTitle} from '@/components/ui/dialog'
import {Button} from '@/components/ui/button'
import {request} from '@/lib/api'
import {t} from '@/i18n'
import {textDiff,reviewedText} from '@/lib/transcript-diff'
import {PromptExchange,type PromptPackage} from './manual-preparation'
import type {PreparationOptions} from './local-preparation'
type Unit={unit_id:string;speaker_raw:string;timestamp:string;text:string}
export type Draft={entries?:{original:string;pseudonym:string;kind:string}[];draft_digest:string;reviewed:boolean;stage:string;anonymisation_approved:boolean;transformation_approved:boolean;coding_will_be_preserved:boolean;original:{units:Unit[]};anonymised:{units:Unit[]};prepared:{units:Unit[]};options:PreparationOptions}
function InterviewReview({id:reviewId,initialDraft,options,onFinished,onBusy,manualMode=false}:{id:string;initialDraft:Draft;options:PreparationOptions;manualMode?:boolean;onFinished:(saved:boolean)=>void;onBusy:(busy:boolean)=>void}){
 const ids=[reviewId],initialSelected=[reviewId],onClose=()=>onFinished(false)
 const qc=useQueryClient(),[selected,setSelected]=useState(new Set(initialSelected)),[queue,setQueue]=useState<string[]|null>([reviewId]),[index,setIndex]=useState(0),[draft,setDraft]=useState<Draft|null>(initialDraft),[busy,setBusy]=useState(false),[error,setError]=useState(''),[page,setPage]=useState(0),[attest,setAttest]=useState(false),[decisions,setDecisions]=useState<Record<string,'accept'|'reject'>>({}),[feedback,setFeedback]=useState<Record<string,string>>({}),[manual,setManual]=useState<Record<string,string>>({}),[overrides,setOverrides]=useState<Record<string,string>>({}),[reviewAgain,setReviewAgain]=useState(false)
 useEffect(()=>{onBusy(busy);return()=>onBusy(false)},[busy,onBusy])
 const [exchange,setExchange]=useState<{prompt:PromptPackage;body:Record<string,unknown>;stage:string}|null>(null),[pasted,setPasted]=useState('')
 const [progress,setProgress]=useState<{stage:string;done:number;total:number;cancel_requested:boolean}|null>(null)
 const id=queue?.[index],done=!!queue&&index>=queue.length,base=id?`/api/interviews/${encodeURIComponent(id)}/local-preparation`:''
 useEffect(()=>{if(!busy||!id)return;let active=true;const poll=()=>request<typeof progress>('GET',`${base}/progress`).then(v=>{if(active)setProgress(v)}).catch(()=>{});void poll();const timer=setInterval(poll,1500);return()=>{active=false;clearInterval(timer)}},[busy,id,base])
 const step=draft?.stage??(options.anonymise?'anonymisation':'transformation'),transformRequested=options.cleanup||!!options.target_language.trim(),anonDone=!!draft?.anonymisation_approved&&step==='anonymisation'&&!reviewAgain
 const source=draft?(step==='anonymisation'?draft.original.units:draft.anonymised.units):[],target=draft?(step==='anonymisation'?draft.anonymised.units:draft.prepared.units):[]
 const rows=useMemo(()=>source.map((u,i)=>({u,v:target[i],text:textDiff(u.text,target[i]?.text??u.text,u.unit_id+':text'),speaker:textDiff(u.speaker_raw,target[i]?.speaker_raw??u.speaker_raw,u.unit_id+':speaker')})),[source,target])
 const changes=rows.flatMap(r=>[...r.text,...r.speaker].filter(p=>p.changed)),pending=rows.reduce((n,r)=>n+r.speaker.filter(p=>p.changed&&!decisions[p.key]).length+(overrides[r.u.unit_id]===undefined?r.text.filter(p=>p.changed&&!decisions[p.key]).length:0),0)
 const next=(saved=false)=>{onFinished(saved);setIndex(i=>i+1);setDraft(null);setDecisions({});setOverrides({});setManual({});setFeedback({});setAttest(false);setPage(0);setError('')}
 const prepare=async(stage:string,revise=false)=>{setBusy(true);setError('');setAttest(false);setProgress(null);try{
  if(!manualMode)await request('PUT','/api/local-preparation/config',options)
  const notes=revise?Object.entries(feedback).filter(([,v])=>v.trim()).map(([unit_id,instruction])=>({unit_id,instruction})):[]
  const body={...options,stage,feedback:notes,draft_digest:revise||stage==='transformation'?draft?.draft_digest:undefined}
  if(manualMode){const prompt=await request<PromptPackage>('POST',base,{...body,prompt_only:true});setExchange({prompt,body,stage});setPasted('');return}
  const d=await request<Draft>('POST',base,body);setDraft(d);if(stage!==draft?.stage)setDecisions({});setReviewAgain(false);setOverrides({});setManual({});setPage(0)
 }catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}
 const approve=async()=>{if(!draft||pending||!attest)return;setBusy(true);setError('');try{
  const units=rows.map(r=>({unit_id:r.u.unit_id,speaker_raw:reviewedText(r.speaker,decisions),text:overrides[r.u.unit_id]??reviewedText(r.text,decisions)}))
  const d=await request<Draft>('POST',`${base}/review`,{digest:draft.draft_digest,reviewed:true,pending_diffs:0,units});setDraft(d);setReviewAgain(false);setAttest(false)
  if(d.stage==='transformation'||!transformRequested){await request('POST',`${base}/apply`,{digest:d.draft_digest,reviewed:true,use_prepared:d.stage==='transformation'});await qc.invalidateQueries();next(true)}
 }catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}
 const importResponse=async()=>{if(!exchange)return;setBusy(true);setError('');try{
  const d=await request<Draft>('POST',base,{...exchange.body,pasted_response:pasted,prompt_digest:exchange.prompt.prompt_digest});setDraft(d);if(exchange.stage!==draft?.stage)setDecisions({});setReviewAgain(false);setOverrides({});setManual({});setPage(0);setExchange(null)
 }catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}
 if(exchange)return <div className="space-y-3 overflow-auto">{error&&<p role="alert">{error}</p>}<PromptExchange id={reviewId} prompt={exchange.prompt} value={pasted} onChange={setPasted}/><Button disabled={busy||!pasted.trim()} onClick={importResponse}>{t('manual.show_diffs')}</Button><Button disabled={busy} variant="ghost" onClick={()=>setExchange(null)}>{t('common.fermer')}</Button></div>
 return <div className="flex min-h-0 flex-col gap-3">
 <h3 className="font-semibold">{reviewId}</h3><p className="text-sm text-label2">{t('staged.help')}</p>
 {error&&<p role="alert" className="text-destructive whitespace-pre-wrap">{error}</p>}
 {!queue?<><p>{t('preparation.select_help')}</p><div className="overflow-auto max-h-[45vh]">{ids.map(id=><label className="flex gap-2" key={id}><input type="checkbox" checked={selected.has(id)} onChange={e=>setSelected(s=>{const next=new Set(s);if(e.target.checked)next.add(id);else next.delete(id);return next})}/>{id}</label>)}</div><Button disabled={!selected.size} onClick={()=>setQueue(ids.filter(id=>selected.has(id)))}>{t('preparation.continue')}</Button></>:done?<><p>{t('preparation.done',{n:queue.length})}</p><Button onClick={onClose}>{t('common.fermer')}</Button></>:<>
 <p className="text-sm font-semibold">{index+1}/{queue.length} · {step==='anonymisation'?t('staged.phase1'):t('staged.phase23')}</p>
 {busy?<><p role="status">{t('preparation.processing')}</p>{progress&&<p>{progress.stage} · {progress.done}/{progress.total}</p>}<Button variant="outline" disabled={progress?.cancel_requested} onClick={async()=>{await request('POST',`${base}/cancel`);setProgress(p=>({...p??{stage:'',done:0,total:0},cancel_requested:true}))}}>{t('preparation.stop')}</Button></>:!draft?<><Button onClick={()=>prepare(step)}>{t('preparation.run')}</Button><Button variant="ghost" onClick={()=>next()}>{t('preparation.skip')}</Button></>:anonDone?<><p role="status">{t('staged.anon_approved')}</p><Button onClick={()=>prepare('transformation')}>{t(manualMode?'manual.generate_transform':'staged.run_transform')}</Button><Button variant="outline" onClick={()=>{setReviewAgain(true);setAttest(false)}}>{t('staged.review_again')}</Button></>:<>
 {draft.coding_will_be_preserved&&<p className="text-sm">{t('preparation.coded_copy')}</p>}
 <div className="flex items-center gap-3"><span>{t('staged.pending',{pending,total:changes.length})}</span><Button variant="outline" onClick={()=>setDecisions(d=>({...d,...Object.fromEntries(changes.map(c=>[c.key,'accept' as const]))}))}>{t('staged.accept_all')}</Button></div>
 <div className="overflow-y-auto min-h-0 space-y-4">{rows.slice(page*20,(page+1)*20).map(r=><section key={r.u.unit_id} className="rounded border border-sep p-3 space-y-2 text-sm"><p className="font-semibold">{r.u.unit_id} · {r.u.speaker_raw} · {r.u.timestamp}</p>
 <div className="grid grid-cols-2 gap-4"><div><p className="text-xs text-label2">{step==='anonymisation'?t('preparation.original'):t('staged.approved_base')}</p><p className="whitespace-pre-wrap">{r.u.text}</p></div><div><p className="text-xs text-label2">{t('staged.proposal')}</p><p className="whitespace-pre-wrap">{overrides[r.u.unit_id]??r.v?.text}</p></div></div>
 {[...r.speaker,...r.text].filter(p=>p.changed).map(c=><div key={c.key} className="rounded bg-fill p-2"><div className="grid grid-cols-2 gap-3 whitespace-pre-wrap"><del>{c.before||'∅'}</del><ins>{c.after||'∅'}</ins></div><div className="flex gap-2 mt-1"><Button variant={decisions[c.key]==='accept'?'default':'outline'} aria-pressed={decisions[c.key]==='accept'} onClick={()=>{setDecisions(d=>({...d,[c.key]:'accept'}));setAttest(false)}}>{t('staged.accept')}</Button><Button variant={decisions[c.key]==='reject'?'default':'outline'} aria-pressed={decisions[c.key]==='reject'} onClick={()=>{setDecisions(d=>({...d,[c.key]:'reject'}));setAttest(false)}}>{t('staged.reject')}</Button></div></div>)}
 <details><summary>{t('staged.flag_segment')}</summary><label className="block">{t('staged.feedback')}<textarea className="w-full border rounded p-2" aria-label={t('staged.feedback_for',{id:r.u.unit_id})} value={feedback[r.u.unit_id]??''} onChange={e=>setFeedback(f=>({...f,[r.u.unit_id]:e.target.value}))}/></label><label className="block">{t('staged.manual')}<textarea className="w-full border rounded p-2" aria-label={t('staged.manual_for',{id:r.u.unit_id})} value={manual[r.u.unit_id]??overrides[r.u.unit_id]??r.v?.text??''} onChange={e=>setManual(m=>({...m,[r.u.unit_id]:e.target.value}))}/></label><Button variant="outline" disabled={!(manual[r.u.unit_id]??r.v?.text)?.trim()} onClick={()=>{setOverrides(o=>({...o,[r.u.unit_id]:manual[r.u.unit_id]??r.v.text}));setAttest(false)}}>{t('staged.use_manual')}</Button>{overrides[r.u.unit_id]!==undefined&&<p>{t('staged.manual_applied')}</p>}</details>
 </section>)}</div>
 <div className="flex gap-2 items-center"><Button variant="ghost" disabled={!page} onClick={()=>setPage(p=>p-1)}>{t('anonymizer.previous')}</Button><span>{Math.min(page*20+1,rows.length)}–{Math.min((page+1)*20,rows.length)}/{rows.length}</span><Button variant="ghost" disabled={(page+1)*20>=rows.length} onClick={()=>setPage(p=>p+1)}>{t('anonymizer.next')}</Button><Button variant="outline" disabled={!Object.values(feedback).some(v=>v.trim())} onClick={()=>prepare(step,true)}>{t(manualMode?'manual.regenerate':'staged.reprompt')}</Button></div>
 <label className="flex gap-2 text-sm"><input type="checkbox" checked={attest} onChange={e=>setAttest(e.target.checked)}/>{t('staged.attest')}</label><div className="flex gap-2"><Button disabled={pending>0||!attest} onClick={approve}>{step==='anonymisation'&&transformRequested?t('staged.approve_anon'):t('staged.finish')}</Button><Button variant="ghost" onClick={()=>next()}>{t('preparation.reject')}</Button></div>
 </>}
 </>}
 </div>
}


export function PreparationWizard({ids,initialSelected,options,onClose,preloadedDrafts,manualMode=false}:{ids:string[];initialSelected:string[];options:PreparationOptions;onClose:()=>void;preloadedDrafts?:Record<string,Draft>;manualMode?:boolean}){
 const [selected,setSelected]=useState(new Set(initialSelected)),[queue,setQueue]=useState<string[]|null>(preloadedDrafts?ids:null)
 const [running,setRunning]=useState(false),[finished,setFinished]=useState(!!preloadedDrafts),[active,setActive]=useState(preloadedDrafts?ids[0]:''),[current,setCurrent]=useState('')
 const [drafts,setDrafts]=useState<Record<string,Draft>>(preloadedDrafts??{}),[errors,setErrors]=useState<Record<string,string>>({}),[completed,setCompleted]=useState<Record<string,boolean>>({}),[settled,setSettled]=useState(0)
 const [reviewBusy,setReviewBusy]=useState(false)
 const stop=useRef(false),[stopping,setStopping]=useState(false)
 const run=async()=>{
  if(!queue)return
  stop.current=false;setStopping(false);setRunning(true);setFinished(false);setErrors({});setSettled(0)
  const results={...drafts};const failures:Record<string,string>={}
  try{
   await request('PUT','/api/local-preparation/config',options)
   for(let i=0;i<queue.length;i++){
    const id=queue[i]
    if(results[id]){setSettled(i+1);continue}
    if(stop.current){failures[id]=t('batch.not_processed');setSettled(i+1);continue}
    setCurrent(id)
    try{results[id]=await request<Draft>('POST',`/api/interviews/${encodeURIComponent(id)}/local-preparation`,{...options,stage:options.anonymise?'anonymisation':'transformation',feedback:[],batch_entries:Object.values(results).flatMap(d=>d.entries??[])});setDrafts({...results})}
    catch(e){failures[id]=e instanceof Error?e.message:String(e)}
    setSettled(i+1)
   }
  }catch(e){for(const id of queue)if(!results[id])failures[id]=e instanceof Error?e.message:String(e)}
  finally{setErrors(failures);setCurrent('');setRunning(false);setFinished(true);setActive(queue.find(id=>results[id])??'')}
 }
 return <Dialog open onOpenChange={v=>{if(!v&&!running&&!reviewBusy)onClose()}}><DialogContent style={{width:'min(1180px, calc(100vw - 32px))',maxWidth:'none',maxHeight:'90vh'}} className="flex flex-col" showCloseButton={!running&&!reviewBusy}>
 <DialogHeader><DialogTitle>{t('preparation.title')}</DialogTitle><DialogDescription>{t('batch.help')}</DialogDescription></DialogHeader>
 {!queue?<><div className="flex gap-2"><Button variant="outline" onClick={()=>setSelected(new Set(ids))}>{t('batch.select_all')}</Button><Button variant="ghost" onClick={()=>setSelected(new Set())}>{t('batch.select_none')}</Button></div><p>{t('preparation.select_help')}</p><div className="overflow-auto max-h-[45vh]">{ids.map(id=><label className="flex gap-2" key={id}><input type="checkbox" checked={selected.has(id)} onChange={e=>setSelected(s=>{const n=new Set(s);if(e.target.checked)n.add(id);else n.delete(id);return n})}/>{id}</label>)}</div><Button disabled={!selected.size} onClick={()=>setQueue(ids.filter(id=>selected.has(id)))}>{t('preparation.continue')}</Button></>:running?<><p role="status">{t('batch.progress',{done:settled,total:queue.length})}{current?` · ${current}`:''}</p><progress className="w-full" max={queue.length} value={settled}/><Button variant="outline" disabled={stopping} onClick={async()=>{stop.current=true;setStopping(true);if(current)try{await request('POST',`/api/interviews/${encodeURIComponent(current)}/local-preparation/cancel`)}catch{/* Stop still prevents further interviews. */}}}>{t('preparation.stop')}</Button></>:!finished?<><p>{t('batch.ready',{n:queue.length})}</p><Button onClick={run}>{t('preparation.run')}</Button></>:<>
 <p role="status">{t('batch.results',{done:Object.keys(drafts).length,total:queue.length})}</p>
 {Object.keys(errors).length>0&&<div role="alert"><ul>{Object.entries(errors).map(([id,error])=><li key={id}>{id}: {error}</li>)}</ul><Button variant="outline" disabled={reviewBusy} onClick={run}>{t('batch.retry')}</Button></div>}
 <div className="flex flex-wrap gap-2" role="tablist" aria-label={t('batch.results_label')}>{queue.filter(id=>drafts[id]).map(id=><button type="button" role="tab" disabled={reviewBusy} aria-selected={active===id} aria-controls={`review-${id}`} id={`tab-${id}`} className="rounded border px-3 py-1" key={id} onClick={()=>setActive(id)}>{id}{completed[id]===true?' ✓':completed[id]===false?' —':''}</button>)}</div>
 {queue.filter(id=>drafts[id]).map(id=><div role="tabpanel" aria-labelledby={`tab-${id}`} id={`review-${id}`} key={id} hidden={active!==id} className={active===id?'flex flex-col min-h-0 overflow-y-auto':''}>{completed[id]!==undefined?<p>{t(completed[id]?'batch.reviewed':'batch.skipped')}</p>:<InterviewReview manualMode={manualMode} id={id} initialDraft={drafts[id]} options={options} onBusy={setReviewBusy} onFinished={saved=>setCompleted(s=>({...s,[id]:saved}))}/>}</div>)}
 <Button variant="outline" disabled={reviewBusy} onClick={onClose}>{t('common.fermer')}</Button>
 </>}
 </DialogContent></Dialog>
}
