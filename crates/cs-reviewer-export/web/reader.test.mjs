// SPDX-License-Identifier: LGPL-2.1-or-later
// Actual Rust encryption envelope + DOM journeys. Real-browser QA is separate.
import {readFileSync} from 'node:fs';
import {webcrypto,createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {runInNewContext} from 'node:vm';
import assert from 'node:assert/strict';
import {test} from 'node:test';
const require=createRequire(new URL('../../../web/package.json',import.meta.url));
const {JSDOM}=require('jsdom');
const html=readFileSync(process.env.READER_FIXTURE||'/private/tmp/coding-studio-reviewer-test.html','utf8');
const script=html.match(/<script>([\s\S]*?)<\/script>/)[1];
const password='synthetic test password 12345';
const modelContext={};runInNewContext(readFileSync(new URL('reader-model.js',import.meta.url),'utf8')+';globalThis.model=ReviewerData;',modelContext);const M=modelContext.model;
const pure=value=>JSON.parse(JSON.stringify(value));
function environment(content=html){
 const dom=new JSDOM(content,{url:'https://example.invalid/reviewer/index.html',runScripts:'outside-only'}),w=dom.window;
 Object.defineProperty(w,'crypto',{value:webcrypto});Object.assign(w,{TextEncoder,TextDecoder,Uint8Array,Blob});
 w.HTMLElement.prototype.scrollIntoView=function(){};w.print=()=>{};
 let submit;const form=w.document.getElementById('form'),add=form.addEventListener.bind(form);form.addEventListener=(type,fn,...rest)=>{if(type==='submit')submit=fn;add(type,fn,...rest);};
 w.eval(script);
 const get=id=>w.document.getElementById(id),text=()=>get('fields').textContent;
 const click=label=>{const b=[...w.document.querySelectorAll('button')].find(b=>b.textContent===label||b.getAttribute('aria-label')===label);assert.ok(b,`Button ${label} exists`);b.click();};
 return {dom,w,get,text,click,submit:async p=>{get('password').value=p;await submit({preventDefault(){}});},close:()=>w.close()};
}
test('CSP authenticates the combined model and UI; encrypted source hides private data',()=>{
 const hash=createHash('sha256').update(script).digest('base64');assert.ok(html.includes(`script-src 'sha256-${hash}'`));assert.ok(html.includes("connect-src 'self'"));assert.ok(!html.includes('NOM EXCLU DU TEST'));assert.ok(!html.includes(password));assert.ok(html.includes('GNU LESSER GENERAL PUBLIC LICENSE'));assert.ok(html.includes('LGPL-2.1-or-later'));
});
test('record projection respects partial fields, array positions and prototype safety',()=>{
 const m=M.build([{path:['Transcript','I1','U1','text'],value:'Only chosen text'},{path:['Arbitration','I1','2','U1','final_codes','1'],value:'C-2'},{path:['Transcript','I1','U1','__proto__','polluted'],value:true}]);
 assert.equal(m.interviews[0].id,'I1');assert.equal(m.units[0].text,'Only chosen text');assert.equal(m.units[0].speaker_raw,undefined);assert.deepEqual(pure(m.unitCodes(m.units[0])),['C-2']);assert.equal({}.polluted,undefined);assert.equal(M.text('literal\\ntext'),'literal\\ntext');
});
test('overlapping source spans preserve every Unicode character and code membership',()=>{
 const unit={text:'😀 Le suivi du stock aide.'},ps=[{text:'suivi du stock',start_offset:5,end_offset:19},{text:'stock aide',start_offset:14,end_offset:24}];
 const seg=M.segments(unit,ps);assert.equal(seg.map(s=>s.text).join(''),unit.text);assert.equal(seg.find(s=>s.text==='stock').passages.length,2);
 assert.deepEqual(pure(M.phrase('😀 À côté du stock','a cote')),{start:2,end:8});
 assert.equal(M.anchor({text:'stock stock'},{text:'stock'}),null);assert.equal(M.phrase('stockout','stock'),null);
 assert.deepEqual(pure(M.anchor({text:'Personne A parle du stock'},{text:'stock',start_offset:50,end_offset:55})),{start:20,end:25});
});
test('library → complete interview → inspector → code occurrences → source',async()=>{
 const e=environment();try{
  await e.submit(password);assert.equal(e.get('reader').hidden,false);assert.equal(e.get('password').value,'');assert.match(e.text(),/2 tours|3 tours/);e.click('TEST-01');
  assert.equal(e.w.document.querySelectorAll('.turn').length,3);assert.equal(e.w.document.querySelector('.turntext').textContent,'😀 Le suivi du stock aide à préparer les commandes.');assert.equal(e.w.document.querySelectorAll('.turn:first-child mark').length,3);
  e.click('Détail');assert.match(e.text(),/Separate the inventory phenomenon/);e.click('Notes');assert.match(e.text(),/Stock information supports order preparation/);e.click('Codebook');e.click('Stock visibility');assert.match(e.text(),/Knowing stock quantities/);assert.match(e.text(),/1 passage · 1 entretien/);
  e.click('TEST-01 · U001');assert.equal(e.w.document.querySelectorAll('.turn').length,3);
 }finally{e.close();}
});
test('lexicon frequencies exclude interviewer turns and KWIC links return to source',async()=>{
 const e=environment();try{await e.submit(password);e.click('Lexique');assert.match(e.text(),/stock/);const terms=[...e.w.document.querySelectorAll('tbody tr')].map(r=>r.textContent);assert.ok(!terms.some(t=>t.startsWith('faites')));e.click('stock');assert.match(e.text(),/2 tours dans 2 entretiens/);assert.equal(e.w.document.querySelectorAll('.kwic strong').length,2);e.click('TEST-02 · U001');assert.match(e.text(),/stock est contrôlé/);}finally{e.close();}
});
test('people, theme evidence, matrix and report remain navigable without application APIs',async()=>{
 const e=environment();try{await e.submit(password);e.click('Personnes');assert.match(e.text(),/Participant A/);assert.match(e.text(),/Stock manager/);e.click('Analyse');assert.match(e.text(),/Information is useful/);e.click('Matrice');const b=[...e.w.document.querySelectorAll('button')].find(b=>b.textContent.startsWith('Présent'));assert.ok(b);b.click();assert.match(e.text(),/suivi du stock/);e.click('Décisions documentées');assert.match(e.text(),/Order preparation/);e.click('Rapport');assert.match(e.text(),/The stock record is useful/);assert.match(e.text(),/When it supports a concrete task/);assert.match(e.text(),/Voir les sources/);}finally{e.close();}
});
test('source HTML is inert, search is local, and locking removes decrypted DOM',async()=>{
 const e=environment();try{await e.submit(password);e.get('search').value='onerror';e.get('search').dispatchEvent(new e.w.Event('input'));await new Promise(r=>setTimeout(r,220));assert.match(e.text(),/<img src=x onerror=alert\(1\)>/);assert.equal(e.w.document.querySelector('img'),null);e.click('Verrouiller');assert.equal(e.get('reader').hidden,true);assert.equal(e.get('fields').children.length,0);assert.equal(e.get('navigation').children.length,0);assert.equal(e.get('search').value,'');}finally{e.close();}
});
test('wrong password never opens or populates the reader',async()=>{
 const e=environment();try{await e.submit('wrong');assert.equal(e.get('reader').hidden,true);assert.equal(e.get('fields').children.length,0);assert.match(e.get('status').textContent,/mot de passe incorrect/);assert.equal(e.get('unlock').disabled,false);}finally{e.close();}
});

test('attribute frequencies count occurrences and full-word denominators within exported groups',()=>{
 const stats=M.termStats([{interview_id:'A',text:'Stock stock and stockout'},{interview_id:'B',text:'Stock helps'}],'stock',u=>u.interview_id==='A'?'Retail':'Hospital');
 assert.deepEqual(pure(stats.map(x=>[x.value,x.interviews,x.units,x.occurrences,x.words,x.per1000])),[['Retail',1,1,2,4,500],['Hospital',1,1,1,2,500]]);
});
