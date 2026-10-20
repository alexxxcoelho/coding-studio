import {expect,it} from 'vitest'
import {compareTableValues,matchesTableFilter,tableView,type TableRecord} from './table-view'
it('compares real numbers, dates and natural text rather than lexical digits',()=>{
 expect(compareTableValues('10','2')).toBeGreaterThan(0)
 expect(compareTableValues('1 200','90')).toBeGreaterThan(0)
 expect(compareTableValues('2,5','10,1')).toBeLessThan(0)
 expect(compareTableValues('02.01.2026','31.12.2025')).toBeGreaterThan(0)
 expect(compareTableValues('Code 9','Code 12')).toBeLessThan(0)
 expect(matchesTableFilter('Électricité','electric')).toBe(true)
 expect(matchesTableFilter('12','>=10')).toBe(true)
 expect(matchesTableFilter('3','>=10')).toBe(false)
 expect(matchesTableFilter('Abc','=abc')).toBe(true)
 expect(matchesTableFilter('Abcd','=abc')).toBe(false)
 expect(matchesTableFilter('','∅')).toBe(true)
})
it('sorts sibling codes while preserving ancestry and reveals ancestors of column matches',()=>{
 const records:TableRecord[]=[{key:'z',treeId:'z',values:['Zulu','2']},{key:'child',treeId:'child',parentId:'z',values:['Alpha child','10']},{key:'a',treeId:'a',values:['Alpha','3']}]
 expect(tableView(records,{column:0,direction:'asc'},{}).order).toEqual(['a','z','child'])
 const filtered=tableView(records,null,{0:'child',1:'>=10'})
 expect([...filtered.visible]).toEqual(['child','z'])
 expect(tableView(records,null,{}).order).toEqual(['z','child','a'])
})
