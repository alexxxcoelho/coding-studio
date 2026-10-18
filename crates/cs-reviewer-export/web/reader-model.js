/* SPDX-License-Identifier: LGPL-2.1-or-later
 * Copyright (c) 2026 Alexandre Dos Santos Coelho */
const ReviewerData = (() => {
  'use strict';
  const depths = {'Interviews':1,'Interview notes':2,'Transcript':2,'Transcript preparation':1,'Codebook':1,'Inactive codes':1,'Passages':2,'Passage coding':2,'Coding decisions':3,'Arbitration':3,'Unit status':2,'Unit marks':2,'Audio verification':2,'Themes':1,'Theme codes':2,'Theme questions':2,'Research questions':1,'Question notes':1,'Cases':1,'Case units':3,'Matrix':2,'Matrix passages':3,'Matrix views':1,'Analytical notes':3,'Quotations':2,'Evidence links':1,'People':1,'People groups':1,'People interviews':2,'People memberships':2,'AI phase 2 prompts':2,'AI phase 2 responses':2,'AI phase 2 runs':2,'AI phase 2 references':2,'AI review samples':1,'Source files':2,'Project configuration':1};
  const ordered = object => Array.isArray(object) ? object.filter(x=>x!==undefined) : object && typeof object==='object' ? Object.keys(object).sort((a,b)=>a.localeCompare(b,undefined,{numeric:true})).map(k=>object[k]) : [];
  const text = value => typeof value==='string' ? value : value==null ? '' : String(value);
  const fold = value => text(value).normalize('NFD').replace(/\p{M}/gu,'').toLocaleLowerCase();
  const stops = new Set(('a an and are as at be been but by can did do does for from has have i if in into is it its of on or so that the their them then there these they this those to was we were what when which who will with would you your et en le la les de des du un une est sont dans pour par sur au aux ce ces ca il elle ils elles que qui qu ne pas plus on se je tu nous vous mais donc aussi avec').split(' '));
  const key = (...parts) => JSON.stringify(parts);
  function build(fields) {
    const categories=new Map();
    for(const field of fields) {
      const [category,...path]=field.path, depth=depths[category]??Math.min(1,path.length);
      if(path.length<depth) continue;
      const ids=path.slice(0,depth), identity=key(...ids);
      if(!categories.has(category)) categories.set(category,new Map());
      const bucket=categories.get(category);
      if(!bucket.has(identity)) bucket.set(identity,{ids,data:Object.create(null)});
      const target=bucket.get(identity).data, tail=path.slice(depth);
      if(!tail.length) {target.value=field.value;continue;}
      if(tail.some(p=>['__proto__','prototype','constructor'].includes(p))) continue;
      let node=target;
      for(const p of tail.slice(0,-1)) {
        if(!Object.hasOwn(node,p)||!node[p]||typeof node[p]!=='object')node[p]=Object.create(null);
        node=node[p];
      }
      node[tail.at(-1)]=field.value;
    }
    const records=category=>Array.from(categories.get(category)?.values()??[]).map(r=>({...r.data,_ids:r.ids}));
    const map=(category,id)=>new Map(records(category).map(r=>[text(r[id]??r._ids.at(-1)),r]));
    const codes=map('Codebook','code_id'), inactive=map('Inactive codes','code_id');
    const units=records('Transcript').map(r=>({...r,interview_id:text(r.interview_id??r._ids[0]),unit_id:text(r.unit_id??r._ids[1])})).sort((a,b)=>(Number(a.seq)||0)-(Number(b.seq)||0)||a.unit_id.localeCompare(b.unit_id));
    const unitMap=new Map(units.map(u=>[key(u.interview_id,u.unit_id),u]));
    const passages=records('Passages').map(r=>({...r,interview_id:text(r.interview_id??r._ids[0]),passage_id:text(r.passage_id??r._ids[1]),codings:[]}));
    const passageMap=new Map(passages.map(p=>[p.passage_id,p]));
    for(const c of records('Passage coding'))passageMap.get(text(c.passage_id))?.codings.push(c);
    const unitPassages=new Map();
    for(const p of passages) {const k=key(p.interview_id,p.unit_id);if(!unitPassages.has(k))unitPassages.set(k,[]);unitPassages.get(k).push(p);}
    const initial=new Map(records('Coding decisions').map(r=>[key(text(r.interview_id??r._ids[0]),text(r.phase??r._ids[1]),text(r.unit_id??r._ids[2])),r]));
    const decisions=new Map(records('Arbitration').map(r=>[key(text(r.interview_id??r._ids[0]),text(r.unit_id??r._ids[2])),r]));
    const notes=records('Interview notes'), ids=new Set();
    for(const cat of ['Interviews','Interview notes','Transcript','Passages','Coding decisions','Arbitration','Quotations','Source files'])for(const r of records(cat))ids.add(text(r.interview_id??r._ids[0]));
    const interviews=Array.from(ids).sort().map(id=>({id,meta:records('Interviews').find(r=>(r.interview_id??r._ids[0])===id)??{},units:units.filter(u=>u.interview_id===id),passages:passages.filter(p=>p.interview_id===id),notes:notes.filter(r=>(r.interview_id??r._ids[0])===id)}));
    const interviewMap=new Map(interviews.map(i=>[i.id,i]));
    const codeName=id=>text(codes.get(id)?.name??inactive.get(id)?.name??id);
    const unitCodes=u=>{
      const decided=decisions.get(key(u.interview_id,u.unit_id));
      if(decided&&Object.hasOwn(decided,'final_codes'))return ordered(decided.final_codes).filter(c=>c!=='NA');
      const found=[...new Set((unitPassages.get(key(u.interview_id,u.unit_id))??[]).flatMap(p=>p.codings.map(c=>c.code_id).filter(Boolean)))];
      if(found.length)return found;
      return ordered(initial.get(key(u.interview_id,'2',u.unit_id))?.codes).filter(c=>c!=='NA');
    };
    const relevant=u=>!ordered(initial.get(key(u.interview_id,'1',u.unit_id))?.codes).includes('NA');
    return {fields,categories,records,codes,inactive,units,unitMap,passages,passageMap,unitPassages,initial,decisions,interviews,interviewMap,codeName,unitCodes,relevant};
  }
  function anchor(unit,passage) {
    if(typeof unit.text!=='string'||typeof passage.text!=='string'||!passage.text)return null;
    const chars=Array.from(unit.text),start=Number(passage.start_offset),end=Number(passage.end_offset);
    if(Number.isInteger(start)&&Number.isInteger(end)&&start>=0&&end>start&&end<=chars.length&&chars.slice(start,end).join('')===passage.text)return {start,end};
    // Pseudonymisation or omitted offsets: anchor only an unambiguous exact quote.
    const first=unit.text.indexOf(passage.text);
    if(first<0||unit.text.indexOf(passage.text,first+1)>=0)return null;
    const s=Array.from(unit.text.slice(0,first)).length;
    return {start:s,end:s+Array.from(passage.text).length};
  }
  function segments(unit,passages) {
    const chars=Array.from(unit.text??''), cuts=new Set([0,chars.length]), spans=[];
    for(const p of passages){const a=anchor(unit,p);if(a){spans.push({...a,passage:p});cuts.add(a.start);cuts.add(a.end);}}
    const bounds=[...cuts].sort((a,b)=>a-b);
    return bounds.slice(0,-1).map((start,i)=>({text:chars.slice(start,bounds[i+1]).join(''),passages:spans.filter(p=>p.start<=start&&p.end>=bounds[i+1]).map(p=>p.passage)}));
  }
  const tokens=value=>Array.from(fold(value).matchAll(/[\p{L}\p{N}][\p{L}\p{N}'’\-]*/gu),m=>m[0]).filter(w=>w.length>2&&!stops.has(w)&&!/^\d+$/.test(w));
  function frequencies(units) {
    const terms=new Map();
    for(const u of units.filter(u=>u.role!=='interviewer')) {
      const seen=new Set();
      for(const word of tokens(u.text)){if(!terms.has(word))terms.set(word,{word,count:0,units:0,interviews:new Set()});const r=terms.get(word);r.count++;r.interviews.add(u.interview_id);if(!seen.has(word)){r.units++;seen.add(word);}}
    }
    return [...terms.values()].sort((a,b)=>b.count-a.count||a.word.localeCompare(b.word));
  }
  function nearby(model,unit,k=8) {
    const own=new Set(tokens(unit.text));if(!own.size)return [];
    return model.units.filter(u=>u.unit_id!==unit.unit_id&&u.role!=='interviewer').map(u=>{
      const other=new Set(tokens(u.text)),shared=[...own].filter(t=>other.has(t));
      return {unit:u,shared,score:shared.length/(own.size+other.size-shared.length)};
    }).filter(r=>r.shared.length>=2).sort((a,b)=>b.score-a.score).slice(0,k);
  }
  function positions(haystack,needle) {
    if(!needle)return [];const out=[],letter=c=>/[\p{L}\p{N}]/u.test(c??'');let offset=0;
    while((offset=haystack.indexOf(needle,offset))>=0){const before=Array.from(haystack.slice(0,offset)).at(-1),after=Array.from(haystack.slice(offset+needle.length))[0];if((!letter(Array.from(needle)[0])||!letter(before))&&(!letter(Array.from(needle).at(-1))||!letter(after)))out.push(offset);offset+=Math.max(1,needle.length);}return out;
  }
  function phrase(value, query) {
    const chars=Array.from(value??''), map=[];let folded='';
    for(let i=0;i<chars.length;i++){const chunk=fold(chars[i]);folded+=chunk;for(let j=0;j<chunk.length;j++)map.push(i);}
    const needle=fold(query),index=positions(folded,needle)[0];if(index===undefined)return null;
    let end=map[index+needle.length-1]+1;while(end<chars.length&&/^\p{M}+$/u.test(chars[end]))end++;
    return {start:map[index],end};
  }
  function termStats(units, query, valueFor) {
    const groups=new Map(),needle=fold(query);
    for(const u of units){const value=text(valueFor(u))||'Non renseigné';if(!groups.has(value))groups.set(value,{value,interviews:new Set(),units:0,occurrences:0,words:0});const g=groups.get(value);g.interviews.add(u.interview_id);g.words+=(fold(u.text).match(/[\p{L}\p{N}]+/gu)??[]).length;const n=positions(fold(u.text),needle).length;if(n){g.units++;g.occurrences+=n;}}
    return [...groups.values()].map(g=>({...g,interviews:g.interviews.size,per1000:g.words?g.occurrences/g.words*1000:0}));
  }
  const parts=[['analytical_statement','Énoncé analytique'],['mechanism_evidence','Mécanisme et preuves'],['variations_divergent','Variations et cas divergents'],['competing_limits','Explications concurrentes et limites'],['mini_synthesis','Synthèse']];
  return {build,anchor,segments,frequencies,nearby,tokens,phrase,termStats,ordered,text,fold,key,parts};
})();
