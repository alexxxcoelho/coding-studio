export type WorkbookPreview={sheets:{name:string;rows:string[][]}[];warnings:string[]}
export const PERSON_COLUMNS=["name","first_name","last_name","kind","organisation","function","email","phone","notes","groups","anonymise","pseudonym","interviews","consent_status","consent_date","consent_document","person_id"] as const
const normal=(s:string)=>s.normalize('NFD').replace(/\p{M}/gu,'').toLowerCase().replace(/[^a-z0-9]/g,'')
export function suggestColumns(headers:string[]):string[]{
 const aliases:Record<string,string>={nom:'last_name',prenom:'first_name',firstname:'first_name',lastname:'last_name',surname:'last_name',nomcomplet:'name',fullname:'name',nomprenom:'name',intervenant:'name',personne:'name',respondent:'name',entreprise:'organisation',company:'organisation',organization:'organisation',societe:'organisation',fonction:'function',poste:'function',role:'function',courriel:'email',mail:'email',telephone:'phone',tel:'phone',mobile:'phone',entretien:'interviews',entretiens:'interviews',interview:'interviews',interviewid:'interviews',interviewcode:'interviews',codeentretien:'interviews',idinterview:'interviews',identifiantentretien:'interviews',groupes:'groups',anonymiser:'anonymise',pseudonyme:'pseudonym'}
 const result=headers.map(h=>PERSON_COLUMNS.find(k=>normal(k)===normal(h))??aliases[normal(h)]??'')
 // A lone surname column is the usable name; separate first/last columns are joined.
 if(!result.includes('name')&&result.includes('last_name')&&!result.includes('first_name'))return result.map(v=>v==='last_name'?'name':v)
 return result
}
export function tableCsv(rows:string[][],mapping:string[]):string {
 const columns=[...new Set(mapping.filter(v=>v&&v!=='first_name'&&v!=='last_name'))]
 if(!columns.includes('name')&&(mapping.includes('first_name')||mapping.includes('last_name')))columns.unshift('name')
 const quote=(s:string)=>'"'+s.replaceAll('"','""')+'"'
 return [columns,...rows.slice(1).filter(row=>row.some(v=>v.trim())).map(row=>columns.map(k=>{
  const values=mapping.flatMap((v,i)=>v===k?[row[i]??'']:[])
  if(k==='name'&&!mapping.includes('name'))return [row[mapping.indexOf('first_name')]??'',row[mapping.indexOf('last_name')]??''].filter(Boolean).join(' ').trim()
  return values.join(' ').trim()
 }))].map(r=>r.map(quote).join(',')).join('\n')+'\n'
}
