import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import {cleanup,fireEvent,screen,waitFor} from '@testing-library/react'
import {mockApi,renderWithClient,type MockApi} from '@/test-utils'
import {ImportDialog} from './import-dialog'
import {PreparationWizard,PreparationOptionsEditor,DEFAULT_PREPARATION} from './local-preparation'
const options={...DEFAULT_PREPARATION,model:'fictional-model',cleanup:true,target_language:'English'}
const u={unit_id:'U1',speaker_raw:'RESP',timestamp:'00:01',text:'euh, Synthetic Person has 3 stores.'}
const anon={...u,text:'euh, [Person 1] has 3 stores.'},translated={...u,text:'[Person 1] has 3 stores.'}
const first={draft_digest:'draft-1',reviewed:false,stage:'anonymisation',anonymisation_approved:false,transformation_approved:false,options,original:{units:[u]},anonymised:{units:[{unit_id:anon.unit_id,text:anon.text,speaker_raw:anon.speaker_raw}]},prepared:{units:[{unit_id:anon.unit_id,text:anon.text,speaker_raw:anon.speaker_raw}]}}
let mock:MockApi
beforeEach(()=>{mock=mockApi();mock.on('GET','/api/interviews/I1/local-preparation',{body:null});mock.on('PUT','/api/local-preparation/config',{body:options});mock.on('POST','/api/interviews/I1/local-preparation',c=>({body:(c.body as {stage:string}).stage==='transformation'?{...first,stage:'transformation',draft_digest:'transform-1',anonymisation_approved:true,prepared:{units:[translated]}}:first}));mock.on('POST','/api/interviews/I1/local-preparation/review',(_,n)=>({body:n?{...first,stage:'transformation',draft_digest:'transform-approved',anonymisation_approved:true,transformation_approved:true,prepared:{units:[translated]}}:{...first,draft_digest:'anon-approved',anonymisation_approved:true}}));mock.on('POST','/api/interviews/I1/local-preparation/apply',{body:{applied:true}})})
afterEach(()=>{cleanup();vi.unstubAllGlobals()})
const start=async()=>{renderWithClient(<PreparationWizard ids={['I1','OLD']} initialSelected={['I1']} options={options} onClose={vi.fn()}/>);expect(screen.getByRole('checkbox',{name:'OLD'})).not.toBeChecked();fireEvent.click(screen.getByRole('button',{name:'Continuer avec la sélection'}));fireEvent.click(screen.getByRole('button',{name:'Préparer avec le modèle local'}));await screen.findByText('euh, [Person 1] has 3 stores.',{selector:'p'})}
const accept=()=>{fireEvent.click(screen.getByRole('button',{name:'Accepter toutes les différences proposées'}));fireEvent.click(screen.getByRole('checkbox',{name:/J’ai décidé les différences/}))}
it('discovers the local model without sending a transcript',async()=>{mock.on('POST','/api/local-preparation/models',{body:{models:['fictional-model','embedding']}});const change=vi.fn();renderWithClient(<PreparationOptionsEditor value={DEFAULT_PREPARATION} onChange={change}/>);fireEvent.click(screen.getByRole('button',{name:'Tester et lister les modèles'}));await screen.findByText('fictional-model · embedding');expect(mock.to('POST','/api/local-preparation/models')[0].body).toEqual({url:'http://127.0.0.1:1234'});expect(change).toHaveBeenCalledWith({...DEFAULT_PREPARATION,model:'fictional-model'})})
it('requires every diff and anonymisation approval before any cleanup or translation',async()=>{
 await start();expect(screen.getByRole('button',{name:'Valider l’anonymisation'})).toBeDisabled();expect(mock.to('POST','/api/interviews/I1/local-preparation')).toHaveLength(1);expect(mock.to('POST','/api/interviews/I1/local-preparation/apply')).toHaveLength(0);
 fireEvent.click(screen.getByRole('button',{name:'Accepter cette différence'}));fireEvent.click(screen.getByRole('checkbox',{name:/J’ai décidé les différences/}));fireEvent.click(screen.getByRole('button',{name:'Valider l’anonymisation'}));await screen.findByText(/Anonymisation validée\. Le nettoyage/);
 expect(mock.to('POST','/api/interviews/I1/local-preparation/apply')).toHaveLength(0);expect(mock.to('POST','/api/interviews/I1/local-preparation/review')[0].body).toMatchObject({pending_diffs:0,units:[{unit_id:anon.unit_id,text:anon.text,speaker_raw:anon.speaker_raw}]});
 fireEvent.click(screen.getByRole('button',{name:'Lancer le nettoyage et la traduction'}));await screen.findByText('[Person 1] has 3 stores.',{selector:'p'});accept();fireEvent.click(screen.getByRole('button',{name:'Valider cette étape et conserver la version'}));await waitFor(()=>expect(mock.to('POST','/api/interviews/I1/local-preparation/apply')).toHaveLength(1));expect(mock.to('POST','/api/interviews/I1/local-preparation/apply')[0].body).toEqual({digest:'transform-approved',reviewed:true,use_prepared:true})
})
it('allows segment feedback and re-prompts without applying a draft',async()=>{await start();fireEvent.click(screen.getByText('Signaler un oubli ou modifier ce segment'));fireEvent.change(screen.getByLabelText('Indications pour U1'),{target:{value:'Un prénom a été oublié dans ce segment.'}});fireEvent.click(screen.getByRole('button',{name:'Relancer avec mes indications'}));await waitFor(()=>expect(mock.to('POST','/api/interviews/I1/local-preparation')).toHaveLength(2));expect(mock.to('POST','/api/interviews/I1/local-preparation')[1].body).toMatchObject({stage:'anonymisation',draft_digest:'draft-1',feedback:[{unit_id:'U1',instruction:'Un prénom a été oublié dans ce segment.'}]});expect(mock.to('POST','/api/interviews/I1/local-preparation/apply')).toHaveLength(0)})
it('retains an explicitly rejected change in the reviewed unit',async()=>{await start();fireEvent.click(screen.getByRole('button',{name:'Refuser cette différence'}));fireEvent.click(screen.getByRole('checkbox',{name:/J’ai décidé les différences/}));fireEvent.click(screen.getByRole('button',{name:'Valider l’anonymisation'}));await waitFor(()=>expect(mock.to('POST','/api/interviews/I1/local-preparation/review')).toHaveLength(1));expect(mock.to('POST','/api/interviews/I1/local-preparation/review')[0].body).toMatchObject({units:[{unit_id:u.unit_id,text:u.text,speaker_raw:u.speaker_raw}]})})
it('keeps first-import model fields editable with no configured model',async()=>{mock.on('GET','/api/state',{body:{library_root:'',native_dialogs:false}});mock.on('GET','/api/interviews',{body:[]});mock.on('GET','/api/local-preparation/config',{body:DEFAULT_PREPARATION});renderWithClient(<ImportDialog open onOpenChange={vi.fn()}/>);fireEvent.click(screen.getByRole('checkbox',{name:'Préparer les transcriptions avec un modèle local après l’import'}));const model=await screen.findByRole('combobox',{name:'Modèle'});expect(model).toBeEnabled();fireEvent.change(model,{target:{value:'fictional-model'}});await waitFor(()=>expect(screen.getByRole('button',{name:'Importer'})).toBeEnabled())})
it('prepares every selected interview before exposing any result and retains per-interview decisions',async()=>{
 mock.on('POST','/api/interviews/I2/local-preparation',{body:{...first,draft_digest:'draft-2'}})
 const originalFetch=globalThis.fetch;let release!:()=>void
 const blocked=new Promise<void>(resolve=>{release=resolve})
 vi.stubGlobal('fetch',vi.fn(async(input:RequestInfo|URL,init?:RequestInit)=>{
  if(String(input)==='/api/interviews/I2/local-preparation'&&init?.method==='POST')await blocked
  return originalFetch(input,init)
 }))
 renderWithClient(<PreparationWizard ids={['I1','I2']} initialSelected={[]} options={options} onClose={vi.fn()}/>)
 fireEvent.click(screen.getByRole('button',{name:'Tout sélectionner'}))
 expect(screen.getByRole('checkbox',{name:'I1'})).toBeChecked();expect(screen.getByRole('checkbox',{name:'I2'})).toBeChecked()
 fireEvent.click(screen.getByRole('button',{name:'Continuer avec la sélection'}));fireEvent.click(screen.getByRole('button',{name:'Préparer avec le modèle local'}))
 await screen.findByText('Préparation du lot : 1/2 · I2')
 expect(screen.queryByRole('button',{name:'Accepter cette différence'})).not.toBeInTheDocument()
 expect(screen.queryByRole('tab')).not.toBeInTheDocument()
 expect(mock.to('POST','/api/interviews/I1/local-preparation/review')).toHaveLength(0)
 release();await screen.findByRole('tab',{name:'I2'})
 expect(mock.to('POST','/api/interviews/I1/local-preparation')).toHaveLength(1);expect(mock.to('POST','/api/interviews/I2/local-preparation')).toHaveLength(1)
 fireEvent.click(screen.getByRole('button',{name:'Refuser cette différence'}))
 fireEvent.click(screen.getByRole('tab',{name:'I2'}));expect(screen.getByRole('button',{name:'Refuser cette différence'})).toHaveAttribute('aria-pressed','false')
 fireEvent.click(screen.getByRole('tab',{name:'I1'}));expect(screen.getByRole('button',{name:'Refuser cette différence'})).toHaveAttribute('aria-pressed','true')
 expect(mock.to('POST',/\/apply$/)).toHaveLength(0)
})
it('continues after one failure and retries only missing results',async()=>{
 mock.on('POST','/api/interviews/I2/local-preparation',(_,n)=>n?{body:first}:{status:500,body:{error:'Synthetic failure'}})
 mock.on('POST','/api/interviews/I3/local-preparation',{body:first})
 renderWithClient(<PreparationWizard ids={['I1','I2','I3']} initialSelected={['I1','I2','I3']} options={options} onClose={vi.fn()}/>)
 fireEvent.click(screen.getByRole('button',{name:'Continuer avec la sélection'}));fireEvent.click(screen.getByRole('button',{name:'Préparer avec le modèle local'}))
 await screen.findByRole('tab',{name:'I3'});expect(screen.queryByRole('tab',{name:'I2'})).not.toBeInTheDocument()
 fireEvent.click(screen.getByRole('button',{name:'Reprendre les entretiens non traités'}));await screen.findByRole('tab',{name:'I2'})
 expect(mock.to('POST','/api/interviews/I1/local-preparation')).toHaveLength(1);expect(mock.to('POST','/api/interviews/I2/local-preparation')).toHaveLength(2);expect(mock.to('POST','/api/interviews/I3/local-preparation')).toHaveLength(1)
})

it('provides a masked Bearer field and uses it when listing models',async()=>{
 const value={...DEFAULT_PREPARATION,bearer_token:'synthetic-secret'};const change=vi.fn()
 mock.on('POST','/api/local-preparation/models',{body:{models:['fictional-model']}})
 renderWithClient(<PreparationOptionsEditor value={value} onChange={change}/>)
 expect(screen.getByLabelText('Jeton Bearer (facultatif)')).toHaveAttribute('type','password')
 fireEvent.click(screen.getByRole('button',{name:'Tester et lister les modèles'}));await screen.findByText('fictional-model',{selector:'p'})
 expect(mock.to('POST','/api/local-preparation/models')[0].body).toEqual({url:value.url,bearer_token:'synthetic-secret'})
})
it('generates all manual prompts and imports pasted responses before opening the grouped review',async()=>{
 const {ManualPreparationWizard}=await import('./manual-preparation')
 for(const id of ['I1','I2'])mock.on('POST',`/api/interviews/${id}/local-preparation`,c=>({body:(c.body as {prompt_only?:boolean}).prompt_only?{prompt:`Complete fictional prompt ${id}`,prompt_digest:`p-${id}`}:{...first,draft_digest:`d-${id}`}}))
 renderWithClient(<ManualPreparationWizard ids={['I1','I2']} initialSelected={['I1','I2']} options={options} onClose={vi.fn()}/>)
 fireEvent.click(screen.getByRole('button',{name:'Générer les prompts'}))
 await screen.findByLabelText('Réponse JSON pour I2')
 expect(mock.to('PUT','/api/local-preparation/config')).toHaveLength(0)
 expect(mock.to('POST',/\/local-preparation$/)).toHaveLength(2)
 expect(mock.to('POST',/\/local-preparation$/).every(c=>(c.body as {prompt_only?:boolean}).prompt_only)).toBe(true)
 expect(screen.queryByRole('tab')).not.toBeInTheDocument()
 fireEvent.change(screen.getByLabelText('Réponse JSON pour I1'),{target:{value:'{"entities":[]}'}})
 expect(screen.getByRole('button',{name:'Afficher tous les résultats'})).toBeDisabled()
 fireEvent.change(screen.getByLabelText('Réponse JSON pour I2'),{target:{value:'{"entities":[]}'}})
 fireEvent.click(screen.getByRole('button',{name:'Afficher tous les résultats'}));await screen.findByRole('tab',{name:'I2'})
 expect(mock.to('POST','/api/interviews/I1/local-preparation')[1].body).toMatchObject({pasted_response:'{"entities":[]}',prompt_digest:'p-I1'})
 expect(mock.to('POST',/\/apply$/)).toHaveLength(0)
})
it('uses manual prompts for cleanup only after anonymisation is approved',async()=>{
 const {ManualPreparationWizard}=await import('./manual-preparation')
 mock.on('POST','/api/interviews/I1/local-preparation',c=>{const b=c.body as {stage:string;prompt_only?:boolean};return {body:b.prompt_only?{prompt:`Prompt ${b.stage}`,prompt_digest:'manual-prompt'}:b.stage==='transformation'?{...first,stage:'transformation',draft_digest:'transform-1',anonymisation_approved:true,prepared:{units:[translated]}}:first}})
 renderWithClient(<ManualPreparationWizard ids={['I1']} initialSelected={['I1']} options={options} onClose={vi.fn()}/>)
 fireEvent.click(screen.getByRole('button',{name:'Générer les prompts'}));fireEvent.change(await screen.findByLabelText('Réponse JSON pour I1'),{target:{value:'{"entities":[]}'}});fireEvent.click(screen.getByRole('button',{name:'Afficher tous les résultats'}));await screen.findByRole('tab',{name:'I1'})
 accept();fireEvent.click(screen.getByRole('button',{name:'Valider l’anonymisation'}));await screen.findByText(/Anonymisation validée/)
 fireEvent.click(screen.getByRole('button',{name:'Générer le prompt de nettoyage et traduction'}));await screen.findByDisplayValue('Prompt transformation')
 expect(mock.to('POST','/api/interviews/I1/local-preparation').at(-1)?.body).toMatchObject({stage:'transformation',prompt_only:true,draft_digest:'anon-approved'})
 fireEvent.change(screen.getByLabelText('Réponse JSON pour I1'),{target:{value:'{"units":[]}'}});fireEvent.click(screen.getByRole('button',{name:'Afficher les différences'}));await screen.findByText('[Person 1] has 3 stores.',{selector:'p'})
 expect(mock.to('POST',/\/apply$/)).toHaveLength(0)
})
it('exposes an independent output budget and OpenAI thinking effort',()=>{
 const change=vi.fn();const value={...DEFAULT_PREPARATION,model:'fictional-model',reasoning:'on',reasoning_effort:'medium'}
 renderWithClient(<PreparationOptionsEditor value={value} onChange={change}/>)
 expect(screen.getByLabelText('Effort de thinking')).toBeEnabled()
 fireEvent.change(screen.getByLabelText('Effort de thinking'),{target:{value:'high'}})
 expect(change).toHaveBeenLastCalledWith({...value,reasoning_effort:'high'})
 fireEvent.change(screen.getByLabelText('Thinking (API OpenAI compatible)'),{target:{value:'off'}})
 expect(change).toHaveBeenLastCalledWith({...value,reasoning:'off'})
 fireEvent.change(screen.getByLabelText('Budget de sortie initial (tokens)'),{target:{value:'65536'}})
 expect(change).toHaveBeenLastCalledWith({...value,output_tokens:65536})
})
