import {beforeEach,afterEach,expect,it,vi} from "vitest"
import {cleanup,fireEvent,screen,waitFor} from "@testing-library/react"
import {mockApi,renderWithClient,type MockApi} from "@/test-utils"
import {ImportDialog} from "./import-dialog"
import {PreparationWizard,PreparationOptionsEditor,DEFAULT_PREPARATION} from "./local-preparation"
const options={...DEFAULT_PREPARATION,model:"fictional-model",cleanup:true,target_language:"English"}
const u={unit_id:"U1",speaker_raw:"RESP",timestamp:"00:01",text:"euh, Synthetic Person has 3 stores."}
const draft={draft_digest:"draft-1",reviewed:false,options,entries:[{original:"Synthetic Person",pseudonym:"[Person 1]"}],original:{units:[u]},anonymised:{units:[{...u,text:"euh, [Person 1] has 3 stores."}]},prepared:{units:[{...u,text:"[Person 1] has 3 stores."}]}}
let mock:MockApi
beforeEach(()=>{mock=mockApi();mock.on("GET","/api/interviews/I1/local-preparation",{body:null});mock.on("PUT","/api/local-preparation/config",{body:options});mock.on("POST","/api/interviews/I1/local-preparation",{body:draft})})
afterEach(()=>{cleanup();vi.unstubAllGlobals()})
it("discovers the local model without sending a transcript",async()=>{
 mock.on("POST","/api/local-preparation/models",{body:{models:["fictional-model","embedding"]}});const change=vi.fn();renderWithClient(<PreparationOptionsEditor value={DEFAULT_PREPARATION} onChange={change}/>);
 fireEvent.click(screen.getByRole("button",{name:"Tester et lister les modèles"}));await screen.findByText("fictional-model · embedding");expect(mock.to("POST","/api/local-preparation/models")[0].body).toEqual({url:"http://127.0.0.1:1234"});expect(change).toHaveBeenCalledWith({...DEFAULT_PREPARATION,model:"fictional-model"})
})
it("selects only new interviews and never applies a draft before explicit validation",async()=>{
 renderWithClient(<PreparationWizard ids={["I1","OLD"]} initialSelected={["I1"]} options={options} onClose={vi.fn()}/>);
 expect(screen.getByRole("checkbox",{name:"I1"})).toBeChecked();expect(screen.getByRole("checkbox",{name:"OLD"})).not.toBeChecked();fireEvent.click(screen.getByRole("button",{name:"Continuer avec la sélection"}));fireEvent.click(screen.getByRole("button",{name:"Préparer avec le modèle local"}));
 expect(await screen.findByText("[Person 1] has 3 stores.")).toBeInTheDocument();expect(screen.getByText(u.text)).toBeInTheDocument();expect(screen.getByRole("button",{name:"Valider et utiliser cette version pour le codage"})).toBeDisabled();expect(mock.to("POST","/api/interviews/I1/local-preparation/apply")).toHaveLength(0);
 mock.on("POST","/api/interviews/I1/local-preparation/apply",{body:{applied:true}});fireEvent.click(screen.getByRole("checkbox",{name:/J’ai relu tous les tours/}));fireEvent.click(screen.getByRole("button",{name:"Valider et utiliser cette version pour le codage"}));await waitFor(()=>expect(mock.to("POST","/api/interviews/I1/local-preparation/apply")).toHaveLength(1));expect(mock.to("POST","/api/interviews/I1/local-preparation/apply")[0].body).toEqual({digest:"draft-1",reviewed:true,use_prepared:true})
})
it("allows the faithful anonymised copy instead of the cleaned translation",async()=>{
 renderWithClient(<PreparationWizard ids={["I1"]} initialSelected={["I1"]} options={options} onClose={vi.fn()}/>);fireEvent.click(screen.getByRole("button",{name:"Continuer avec la sélection"}));fireEvent.click(screen.getByRole("button",{name:"Préparer avec le modèle local"}));await screen.findByText("[Person 1] has 3 stores.");fireEvent.click(screen.getByRole("radio",{name:"Copie fidèle (anonymisée si demandé)"}));expect(screen.getByText("euh, [Person 1] has 3 stores.")).toBeInTheDocument();mock.on("POST","/api/interviews/I1/local-preparation/apply",{body:{applied:true}});fireEvent.click(screen.getByRole("checkbox",{name:/J’ai relu tous les tours/}));fireEvent.click(screen.getByRole("button",{name:"Valider et utiliser cette version pour le codage"}));await waitFor(()=>expect(mock.to("POST","/api/interviews/I1/local-preparation/apply")).toHaveLength(1));expect(mock.to("POST","/api/interviews/I1/local-preparation/apply")[0].body).toMatchObject({use_prepared:false})
})

it("keeps first-import model fields editable when no model has been configured",async()=>{
 mock.on("GET","/api/state",{body:{library_root:"",native_dialogs:false}});mock.on("GET","/api/interviews",{body:[]});mock.on("GET","/api/local-preparation/config",{body:DEFAULT_PREPARATION});
 renderWithClient(<ImportDialog open onOpenChange={vi.fn()}/>);
 fireEvent.click(screen.getByRole("checkbox",{name:"Préparer les transcriptions avec un modèle local après l’import"}));
 const model=await screen.findByRole("combobox",{name:"Modèle"});expect(model).toBeEnabled();expect(screen.getByRole("button",{name:"Tester et lister les modèles"})).toBeEnabled();expect(screen.getByRole("button",{name:"Importer"})).toBeDisabled();
 fireEvent.change(model,{target:{value:"fictional-model"}});await waitFor(()=>expect(screen.getByRole("button",{name:"Importer"})).toBeEnabled());
})
