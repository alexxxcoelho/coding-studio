export type TableSort={column:number;direction:'asc'|'desc'}|null
export type TableRecord={key:string;values:string[];treeId?:string;parentId?:string;fixed?:string}
const fold=(value:string)=>value.normalize('NFD').replace(/[\u0300-\u036f]/g,'').toLocaleLowerCase().trim()
const collator=new Intl.Collator(undefined,{numeric:true,sensitivity:'base'})
export function tableNumber(value:string):number|null{
 const v=value.trim().replace(/[\s\u00a0\u202f']/g,'').replace(/%$/,'').replace(',','.')
 return /^[-+]?\d+(\.\d+)?$/.test(v)?Number(v):null
}
function dateNumber(v:string):number|null{
 if(/^\d{4}-\d{2}-\d{2}(T|$)/.test(v)){const n=Date.parse(v);return Number.isNaN(n)?null:n}
 const m=/^(\d{1,2})[./](\d{1,2})[./](\d{4})(?:\s+(\d{1,2}):(\d{2}))?$/.exec(v.trim())
 return m?Date.UTC(+m[3],+m[2]-1,+m[1],+(m[4]??0),+(m[5]??0)):null
}
export function compareTableValues(a:string,b:string):number{
 const na=tableNumber(a),nb=tableNumber(b);if(na!==null&&nb!==null)return na-nb
 const da=dateNumber(a),db=dateNumber(b);if(da!==null&&db!==null)return da-db
 return collator.compare(a,b)
}
export function matchesTableFilter(value:string,query:string):boolean{
 const q=fold(query),v=fold(value);if(!q)return true
 if(q==='∅')return !v
 if(q==='!∅')return !!v
 const numeric=/^(>=|<=|>|<|!=)\s*(.+)$/.exec(q)
 if(numeric){const a=tableNumber(value),b=tableNumber(numeric[2]);if(a!==null&&b!==null){switch(numeric[1]){case'>=':return a>=b;case'<=':return a<=b;case'>':return a>b;case'<':return a<b;case'!=':return a!==b}}}
 if(q.startsWith('!='))return v!==q.slice(2).trim()
 if(q.startsWith('='))return v===q.slice(1).trim()
 return v.includes(q)
}
/** Stable row groups keep expanded detail rows and child codes with their owner. */
export function tableView(records:TableRecord[],sort:TableSort,filters:Record<number,string>):{order:string[];visible:Set<string>}{
 const byTree=new Map(records.filter(r=>r.treeId).map(r=>[r.treeId!,r]))
 const visible=new Set(records.filter(r=>r.fixed||Object.entries(filters).every(([c,q])=>matchesTableFilter(r.values[+c]??'',q))).map(r=>r.key))
 for(const r of records)if(visible.has(r.key)){const seen=new Set<string>();let parent=r.parentId;while(parent&&!seen.has(parent)){seen.add(parent);const p=byTree.get(parent);if(!p)break;visible.add(p.key);parent=p.parentId}}
 const ordered=(list:TableRecord[])=>sort?[...list].sort((a,b)=>{const av=a.values[sort.column]??'',bv=b.values[sort.column]??'';if(!av.trim()||!bv.trim())return !av.trim()===!bv.trim()?0:!av.trim()?1:-1;return compareTableValues(av,bv)*(sort.direction==='asc'?1:-1)}):list
 const children=new Map<string|undefined,TableRecord[]>()
 for(const r of records.filter(r=>!r.fixed)){const parent=r.treeId&&r.parentId&&byTree.has(r.parentId)?r.parentId:undefined;children.set(parent,[...(children.get(parent)??[]),r])}
 const order=records.filter(r=>r.fixed==='top').map(r=>r.key),seen=new Set(order)
 const visit=(parent:string|undefined)=>{for(const r of ordered(children.get(parent)??[])){if(seen.has(r.key))continue;seen.add(r.key);order.push(r.key);if(r.treeId)visit(r.treeId)}}
 visit(undefined)
 for(const r of records)if(!seen.has(r.key)&&r.fixed!=='bottom'){seen.add(r.key);order.push(r.key)}
 order.push(...records.filter(r=>r.fixed==='bottom').map(r=>r.key))
 return {order,visible}
}
