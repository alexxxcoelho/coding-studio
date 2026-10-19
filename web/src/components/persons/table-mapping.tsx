import {useEffect,useMemo,useState} from 'react'
import {t} from '@/i18n'
import {PERSON_COLUMNS,suggestColumns,tableCsv,type WorkbookPreview} from '@/lib/persons-table'
export function TableMapping({book,onCsv}:{book:WorkbookPreview;onCsv:(v:string)=>void}) {
 const labels={name:t("persons.column.name"),first_name:t("persons.column.first_name"),last_name:t("persons.column.last_name"),kind:t("persons.column.kind"),organisation:t("persons.column.organisation"),function:t("persons.column.function"),email:t("persons.column.email"),phone:t("persons.column.phone"),notes:t("persons.column.notes"),groups:t("persons.column.groups"),anonymise:t("persons.column.anonymise"),pseudonym:t("persons.column.pseudonym"),interviews:t("persons.column.interviews"),consent_status:t("persons.column.consent_status"),consent_date:t("persons.column.consent_date"),consent_document:t("persons.column.consent_document"),person_id:t("persons.column.person_id")};
 const [sheet,setSheet]=useState(0);const rows=book.sheets[sheet]?.rows??[];const [mapping,setMapping]=useState(()=>suggestColumns(rows[0]??[]));
 const csv=useMemo(()=>tableCsv(rows,mapping),[rows,mapping]);useEffect(()=>onCsv(csv),[csv,onCsv]);
 return <section className="space-y-2 rounded border border-sep p-3 text-sm">
 <label>{t('persons.excel_sheet')}<select className="block w-full" value={sheet} onChange={e=>{const i=Number(e.target.value);setSheet(i);setMapping(suggestColumns(book.sheets[i].rows[0]??[]))}}>{book.sheets.map((s,i)=><option key={i} value={i}>{s.name}</option>)}</select></label>
 <p>{t('persons.excel_map_help')}</p><div className="max-h-44 overflow-auto space-y-1">{(rows[0]??[]).map((header,i)=><label className="grid grid-cols-2 gap-2" key={i}><span>{header||t('persons.excel_empty_column',{n:i+1})}</span><select aria-label={t('persons.excel_map_column',{name:header||String(i+1)})} value={mapping[i]??''} onChange={e=>setMapping(m=>m.map((v,j)=>j===i?e.target.value:v))}><option value="">{t('persons.excel_ignore')}</option>{PERSON_COLUMNS.map(k=><option value={k} key={k}>{labels[k]}</option>)}</select></label>)}</div>
 {book.warnings.map(w=><p className="text-label2" key={w}>{w}</p>)}
 <details><summary>{t('persons.excel_preview')}</summary><div className="max-h-40 overflow-auto"><table className="text-xs"><thead><tr>{(rows[0]??[]).map((v,i)=><th key={i}>{v}</th>)}</tr></thead><tbody>{rows.slice(1,6).map((row,i)=><tr key={i}>{row.map((v,j)=><td className="px-2" key={j}>{v}</td>)}</tr>)}</tbody></table></div></details>
 </section>
}
