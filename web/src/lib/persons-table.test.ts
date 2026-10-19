import {expect,it} from 'vitest'
import {suggestColumns,tableCsv} from './persons-table'
it('maps French headers, joins first and last names and preserves interview links',()=>{
 const rows=[['Prénom','Nom','Entreprise','Entretien','Extra'],['Sophie','Martin','Fictional SA','INT-01;INT-02:interviewer','ignore']];const mapping=suggestColumns(rows[0]);expect(mapping).toEqual(['first_name','last_name','organisation','interviews','']);expect(tableCsv(rows,mapping)).toBe('"name","organisation","interviews"\n"Sophie Martin","Fictional SA","INT-01;INT-02:interviewer"\n')
})
it('quotes commas, quotes and newlines without interpreting cells',()=>{expect(tableCsv([['name'],['A, "B"\nC']],['name'])).toContain('"A, ""B""\nC"')})
