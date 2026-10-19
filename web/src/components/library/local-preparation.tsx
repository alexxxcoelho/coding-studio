import {useState} from "react"
import {useQuery} from "@tanstack/react-query"
import {Dialog,DialogContent,DialogDescription,DialogHeader,DialogTitle} from "@/components/ui/dialog"
import {Button} from "@/components/ui/button"
import {Input} from "@/components/ui/input"
import {request} from "@/lib/api"
import {t} from "@/i18n"
export type PreparationOptions={url:string;model:string;anonymise:boolean;cleanup:boolean;target_language:string;context_tokens:number;reasoning:string}
export const DEFAULT_PREPARATION:PreparationOptions={url:"http://127.0.0.1:1234",model:"",anonymise:true,cleanup:false,target_language:"",context_tokens:131072,reasoning:"server"}
export function PreparationOptionsEditor({value,onChange,disabled=false}:{value:PreparationOptions;onChange:(v:PreparationOptions)=>void;disabled?:boolean}){
 const [models,setModels]=useState<string[]>([]);const [busy,setBusy]=useState(false);const [error,setError]=useState("")
 const set=(patch:Partial<PreparationOptions>)=>onChange({...value,...patch})
 return <div className="space-y-3 rounded border border-sep p-3"><p className="text-sm text-label2">{t("preparation.local_help")}</p>
 <label className="block text-sm">{t("preparation.url")}<Input disabled={disabled} value={value.url} onChange={e=>set({url:e.target.value})}/></label>
 <div className="flex gap-2 items-end"><label className="grow text-sm">{t("preparation.model")}<Input disabled={disabled} list="local-preparation-models" value={value.model} onChange={e=>set({model:e.target.value})}/></label><datalist id="local-preparation-models">{models.map(m=><option value={m} key={m}/>)}</datalist><Button type="button" variant="outline" disabled={busy||disabled||!value.url} onClick={async()=>{setBusy(true);setError("");try{const r=await request<{models:string[]}>("POST","/api/local-preparation/models",{url:value.url});setModels(r.models);const chat=r.models.find(m=>!/embed/i.test(m));if(!value.model&&chat)set({model:chat})}catch(e){setError(e instanceof Error?e.message:String(e))}finally{setBusy(false)}}}>{busy?t("preparation.testing"):t("preparation.test")}</Button></div>
 {models.length>0&&<p role="status" className="text-xs">{models.join(" · ")}</p>}{error&&<p role="alert" className="text-destructive text-sm">{error}</p>}
 <label className="block text-sm">{t("preparation.context")}<Input type="number" min={8192} max={1048576} disabled={disabled} value={value.context_tokens??131072} onChange={e=>set({context_tokens:Number(e.target.value)})}/></label>
 <p className="text-xs text-label2">{t("preparation.context_help")}</p>
 <label className="block text-sm">{t("preparation.reasoning")}<select className="block w-full rounded border border-sep p-1.5 bg-background" disabled={disabled} value={value.reasoning??"server"} onChange={e=>set({reasoning:e.target.value})}><option value="server">{t("preparation.reasoning_server")}</option><option value="off">{t("preparation.reasoning_off")}</option><option value="on">{t("preparation.reasoning_on")}</option></select></label>
 <label className="flex gap-2 text-sm"><input type="checkbox" disabled={disabled} checked={value.anonymise} onChange={e=>set({anonymise:e.target.checked})}/>{t("preparation.anonymise")}</label>
 <label className="flex gap-2 text-sm"><input type="checkbox" disabled={disabled} checked={value.cleanup} onChange={e=>set({cleanup:e.target.checked})}/>{t("preparation.cleanup")}</label>
 <label className="block text-sm">{t("preparation.language")}<Input disabled={disabled} value={value.target_language} onChange={e=>set({target_language:e.target.value})} placeholder={t("preparation.language_help")} list="preparation-languages"/><datalist id="preparation-languages">{["English","Français","Deutsch","Italiano","Español"].map(l=><option value={l} key={l}/>)}</datalist></label>
 </div>
}
export function PreparationSettings(){
 const q=useQuery({queryKey:["local-preparation-config"],queryFn:()=>request<PreparationOptions>("GET","/api/local-preparation/config")});const [editedOptions,setOpts]=useState<PreparationOptions|null>(null);const opts=editedOptions??q.data??DEFAULT_PREPARATION;const [message,setMessage]=useState("")
 return <div className="space-y-3"><PreparationOptionsEditor value={opts} onChange={setOpts}/><Button variant="outline" onClick={async()=>{try{await request("PUT","/api/local-preparation/config",opts);setMessage(t("preparation.saved"))}catch(e){setMessage(e instanceof Error?e.message:String(e))}}}>{t("preparation.save")}</Button><p role="status" className="text-sm">{message||q.error?.message}</p></div>
}
export {PreparationWizard} from "./staged-preparation"
import {PreparationWizard} from "./staged-preparation"

export function LibraryPreparation({ids,initialId,onClose}:{ids:string[];initialId:string|null;onClose:()=>void}) {
 const q=useQuery({queryKey:["local-preparation-config"],queryFn:()=>request<PreparationOptions>("GET","/api/local-preparation/config")});
 const [edited,setEdited]=useState<PreparationOptions|null>(null);const options=edited??q.data??DEFAULT_PREPARATION;const [started,setStarted]=useState(false);
 if(started)return <PreparationWizard ids={ids} initialSelected={initialId?[initialId]:[]} options={options} onClose={onClose}/>;
 return <Dialog open onOpenChange={v=>{if(!v)onClose()}}><DialogContent style={{maxHeight:"88vh",overflowY:"auto"}}><DialogHeader><DialogTitle>{t("preparation.title")}</DialogTitle><DialogDescription>{t("preparation.from_library_help")}</DialogDescription></DialogHeader><PreparationOptionsEditor value={options} onChange={setEdited}/><Button disabled={!options.model.trim()||(!options.anonymise&&!options.cleanup&&!options.target_language.trim())} onClick={()=>setStarted(true)}>{t("preparation.choose_interviews")}</Button></DialogContent></Dialog>
}
