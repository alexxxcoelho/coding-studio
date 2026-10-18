import {beforeEach,afterEach,expect,it,vi} from "vitest"
import {cleanup,fireEvent,screen,waitFor} from "@testing-library/react"
import {Anonymizer} from "./anonymizer"
import {mockApi,renderWithClient,type MockApi} from "@/test-utils"
let mock:MockApi
beforeEach(()=>{
 mock=mockApi()
 mock.on("GET","/api/anonymisation",{body:{entries:[],manual_entries:[],interviews:[{id:"INT-01",reviewed:false}],prompt:"Local reference template"}})
 mock.on("GET","/api/anonymisation/interviews/INT-01",{body:{digest:"source-version-1",reviewed:false,speaker_suggestions:["Fictional Speaker"],candidates:[{text:"Fictional",unit:"U1"}],units:[{id:"U1",speaker:"Fictional Speaker",speaker_anonymised:"[Person 1]",original:"Um, Fictional, yeah…",anonymised:"Um, [Person 1], yeah…"}]}})
})
afterEach(()=>{cleanup();vi.unstubAllGlobals()})
it("shows the local original beside its faithful copy and requires attestation",async()=>{
 renderWithClient(<Anonymizer interviewId="INT-01"/>);fireEvent.click(screen.getByRole("button",{name:"Anonymiser"}))
 expect(await screen.findByText("Um, Fictional, yeah…")).toBeInTheDocument()
 expect(screen.getByText("Um, [Person 1], yeah…")).toBeInTheDocument()
 expect(screen.getByRole("button",{name:"Valider cette copie"})).toBeDisabled()
 expect(screen.getByRole("button",{name:"Exporter la copie anonymisée"})).toBeDisabled()
 mock.on("PUT","/api/anonymisation/interviews/INT-01",{body:{reviewed:true}})
 fireEvent.click(screen.getByRole("checkbox"));fireEvent.click(screen.getByRole("button",{name:"Valider cette copie"}))
 await waitFor(()=>expect(mock.to("PUT","/api/anonymisation/interviews/INT-01")).toHaveLength(1))
 expect(mock.to("PUT","/api/anonymisation/interviews/INT-01")[0].body).toEqual({digest:"source-version-1",reviewed:true})
 expect(mock.to("POST","/api/anonymisation/interviews/INT-01/export")).toHaveLength(0)
})
it("only stores a proposed speaker replacement after an explicit local save",async()=>{
 renderWithClient(<Anonymizer interviewId="INT-01"/>);fireEvent.click(screen.getByRole("button",{name:"Anonymiser"}))
 fireEvent.click(await screen.findByRole("button",{name:"Fictional Speaker"}))
 expect(mock.to("PUT","/api/anonymisation")).toHaveLength(0)
 mock.on("PUT","/api/anonymisation",{body:{saved:true}})
 fireEvent.click(screen.getByRole("button",{name:"Enregistrer le remplacement"}))
 await waitFor(()=>expect(mock.to("PUT","/api/anonymisation")).toHaveLength(1))
 expect(mock.to("PUT","/api/anonymisation")[0].body).toEqual({entries:[{kind:"person",original:"Fictional Speaker",pseudonym:"[Person 1]"}]})
})
