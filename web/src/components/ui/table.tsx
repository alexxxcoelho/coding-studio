"use client"

import * as React from "react"
import { cn } from "cn"
import {ArrowDownAZ,ArrowUpAZ,Filter,X} from 'lucide-react'
import {t} from '@/i18n'
import {Popover,PopoverContent,PopoverTrigger} from '@/components/ui/popover'
import {tableView,type TableRecord,type TableSort} from '@/lib/table-view'

type TableState={labels:string[];records:TableRecord[];sort:TableSort;filters:Record<number,string>;setSort:(s:TableSort)=>void;setFilter:(column:number,value:string)=>void}
const TableContext=React.createContext<TableState|null>(null)
const RowContext=React.createContext<{key:string;hidden:boolean}|null>(null)
function cellValue(cell:HTMLTableCellElement):string{
 if(cell.dataset.sortValue!==undefined)return cell.dataset.sortValue
 const controls=Array.from(cell.querySelectorAll('input,select,textarea')).filter(e=>!(e as HTMLInputElement).disabled)
 const text=(cell.textContent??'').trim()
 return [text,...controls.map(e=>e instanceof HTMLInputElement&&e.type==='checkbox'?(e.checked?'✓':''):(e as HTMLInputElement).value)].filter(Boolean).join(' ')
}


function Table({className,children,onFiltersActiveChange,...props}:React.ComponentProps<'table'>&{onFiltersActiveChange?:(active:boolean)=>void}){
 const ref=React.useRef<HTMLTableElement>(null),[metadata,setMetadata]=React.useState<{labels:string[];records:TableRecord[]}>({labels:[],records:[]}),[sort,setSort]=React.useState<TableSort>(null),[filters,setFilters]=React.useState<Record<number,string>>({})
 const read=React.useCallback(()=>{const table=ref.current;if(!table)return
  const header=Array.from(table.tHead?.rows??[]).find(r=>!r.hasAttribute('data-table-controls'))
  const rows=Array.from(table.tBodies).flatMap(body=>Array.from(body.rows)).filter(row=>row.dataset.tableGroup&&!row.hasAttribute('data-table-empty'))
  const records:TableRecord[]=[],seen=new Set<string>();for(const row of rows){const key=row.dataset.tableGroup!;if(seen.has(key))continue;seen.add(key);const values=Array.from(row.cells).flatMap(c=>Array.from({length:c.colSpan},()=>cellValue(c)));records.push({key,values,treeId:row.dataset.treeId,parentId:row.dataset.treeParent,fixed:row.dataset.tableFixed})}
  const count=Math.max(header?.cells.length??0,...records.map(r=>r.values.length),0)
  const labels=Array.from({length:count},(_,i)=>(header?.cells[i]?.querySelector('[data-table-label]')?.textContent??header?.cells[i]?.textContent)?.trim().replace(/ [↑↓]$/,'')||t('table.column',{n:i+1}))
  const next={labels,records};setMetadata(old=>JSON.stringify(old)===JSON.stringify(next)?old:next)
 },[])
 React.useLayoutEffect(()=>{read()})
 React.useEffect(()=>{const table=ref.current;if(!table)return;const observer=new MutationObserver(read);observer.observe(table,{subtree:true,childList:true,characterData:true,attributes:true,attributeFilter:['data-sort-value','data-filter-value','value']});return()=>observer.disconnect()},[read])
 const active=Object.values(filters).some(Boolean)
 React.useEffect(()=>{onFiltersActiveChange?.(active)},[active,onFiltersActiveChange])
 const context=React.useMemo(()=>({...metadata,sort,filters,setSort,setFilter:(column:number,value:string)=>setFilters(old=>({...old,[column]:value}))}),[metadata,sort,filters])
 const hasHead=React.Children.toArray(children).some(c=>React.isValidElement(c)&&(c.type===TableHeader||c.type==='thead'))
 return <TableContext.Provider value={context}><div data-slot="table-container" className="relative w-full overflow-x-auto">{(sort||active)&&<button type="button" className="text-xs text-label2 flex items-center gap-1 mb-1" onClick={()=>{setSort(null);setFilters({})}}><X size={12}/>{t('table.reset')}</button>}<table ref={ref} data-slot="table" className={cn('tbl w-full caption-bottom text-sm',className)} {...props} onClickCapture={e=>{props.onClickCapture?.(e);const target=e.target as Element;if(target.closest('thead button')&&!target.closest('[data-table-column-tools]'))setSort(null)}} onChangeCapture={e=>{props.onChangeCapture?.(e);read()}}>{!hasHead&&<TableHeader/>}{children}</table></div></TableContext.Provider>
}

function ColumnTools({column,label,ctx}:{column:number;label:string;ctx:TableState}){
 return <div data-table-column-tools className="flex gap-1 items-center min-w-20 mt-1"><button type="button" className={cn('p-1 rounded hover:bg-fill',ctx.sort?.column===column&&ctx.sort.direction==='asc'&&'bg-fill text-accent')} aria-label={t('table.asc',{column:label})} aria-pressed={ctx.sort?.column===column&&ctx.sort.direction==='asc'} onClick={()=>ctx.setSort(ctx.sort?.column===column&&ctx.sort.direction==='asc'?null:{column,direction:'asc'})}><ArrowUpAZ size={13}/></button><button type="button" className={cn('p-1 rounded hover:bg-fill',ctx.sort?.column===column&&ctx.sort.direction==='desc'&&'bg-fill text-accent')} aria-label={t('table.desc',{column:label})} aria-pressed={ctx.sort?.column===column&&ctx.sort.direction==='desc'} onClick={()=>ctx.setSort(ctx.sort?.column===column&&ctx.sort.direction==='desc'?null:{column,direction:'desc'})}><ArrowDownAZ size={13}/></button><Popover><PopoverTrigger asChild><button type="button" aria-label={t('table.filter',{column:label})} className={cn('cursor-pointer p-1 rounded hover:bg-fill',ctx.filters[column]&&'bg-fill text-accent')}><Filter size={13}/></button></PopoverTrigger><PopoverContent align="start" className="w-60"><label className="text-xs block">{t('table.filter',{column:label})}<input className="block w-full border rounded p-1 font-normal mt-1" aria-label={t('table.filter_input',{column:label})} placeholder={t('table.contains')} value={ctx.filters[column]??''} onChange={e=>ctx.setFilter(column,e.target.value)}/></label><p className="font-normal text-xs text-label2">{t('table.filter_help')}</p>{ctx.filters[column]&&<button type="button" className="text-xs" onClick={()=>ctx.setFilter(column,'')}>{t('table.clear')}</button>}</PopoverContent></Popover></div>
}
function TableHeader({className,children,...props}:React.ComponentProps<'thead'>){
 const ctx=React.useContext(TableContext)
 const rows=children??(ctx&&<tr>{ctx.labels.map((label,i)=><th key={i}>{label}</th>)}</tr>)
 const decorate=(nodes:React.ReactNode):React.ReactNode=>React.Children.map(nodes,node=>{
  if(!React.isValidElement<{children?:React.ReactNode}>(node))return node
  if(node.type===React.Fragment)return React.cloneElement(node,{},decorate(node.props.children))
  if(node.type!==TableRow&&node.type!=='tr')return node
  let column=0
  return React.cloneElement(node,{},React.Children.map(node.props.children,cell=>{
   if(!React.isValidElement<React.ComponentProps<'th'>>(cell))return cell
   const index=column;column+=cell.props.colSpan??1
   return React.cloneElement(cell,{'aria-sort':ctx?.sort?(ctx.sort.column===index?(ctx.sort.direction==='asc'?'ascending':'descending'):'none'):cell.props['aria-sort']},<><span data-table-label>{cell.props.children}</span>{ctx&&<ColumnTools column={index} label={ctx.labels[index]??t('table.column',{n:index+1})} ctx={ctx}/>}</>)
  }))
 })
 return <thead data-slot="table-header" className={cn(className)} {...props}>{decorate(rows)}</thead>
}
function TableBody({className,children,...props}:React.ComponentProps<'tbody'>){
 const ctx=React.useContext(TableContext),id=React.useId(),groups=React.Children.toArray(children)
 const nodes=new Map(groups.map((node,i)=>[id+':'+(React.isValidElement(node)?node.key??i:i),node]))
 const recordMap=new Map(ctx?.records.map(r=>[r.key,r])??[])
 const records=Array.from(nodes.keys()).flatMap(key=>recordMap.has(key)?[recordMap.get(key)!]:[])
 const view=tableView(records,ctx?.sort??null,ctx?.filters??{})
 const order=[...view.order,...Array.from(nodes.keys()).filter(key=>!view.order.includes(key))]
 const filtering=Object.values(ctx?.filters??{}).some(Boolean)
 return <tbody data-slot="table-body" className={cn(className)} {...props}>{order.map(key=><RowContext.Provider key={key} value={{key,hidden:filtering&&records.some(r=>r.key===key)&&!view.visible.has(key)}}>{nodes.get(key)}</RowContext.Provider>)}{filtering&&records.length>0&&view.visible.size===0&&<tr data-table-empty><td colSpan={ctx?.labels.length||1} className="text-label2 p-3">{t('table.empty')}</td></tr>}</tbody>
}

function TableFooter({ className, ...props }: React.ComponentProps<"tfoot">) {
  return (
    <tfoot
      data-slot="table-footer"
      className={cn(
        "border-t bg-muted/50 font-medium [&>tr]:last:border-b-0",
        className
      )}
      {...props}
    />
  )
}

function TableRow({ className, style, ...props }: React.ComponentProps<"tr">) {
  const group=React.useContext(RowContext)
  return (
    <tr
      data-slot="table-row"
      data-table-group={group?.key}
      hidden={group?.hidden||props.hidden}
      style={{...style,...(group?.hidden?{display:"none"}:{})}}
      className={cn(
        "transition-colors",
        className
      )}
      {...props}
    />
  )
}

function TableHead({ className, ...props }: React.ComponentProps<"th">) {
  return (
    <th
      data-slot="table-head"
      className={cn(
        "text-left align-middle font-medium whitespace-nowrap [&:has([role=checkbox])]:pr-0",
        className
      )}
      {...props}
    />
  )
}

function TableCell({ className, ...props }: React.ComponentProps<"td">) {
  return (
    <td
      data-slot="table-cell"
      className={cn(
        "align-middle whitespace-nowrap [&:has([role=checkbox])]:pr-0",
        className
      )}
      {...props}
    />
  )
}

function TableCaption({
  className,
  ...props
}: React.ComponentProps<"caption">) {
  return (
    <caption
      data-slot="table-caption"
      className={cn("mt-3 text-xs text-label2", className)}
      {...props}
    />
  )
}

export {
  Table,
  TableHeader,
  TableBody,
  TableFooter,
  TableHead,
  TableRow,
  TableCell,
  TableCaption,
}
