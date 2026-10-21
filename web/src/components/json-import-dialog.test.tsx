import {afterEach,beforeEach,describe,expect,it,vi} from "vitest"
import {cleanup,fireEvent,screen,waitFor} from "@testing-library/react"
import {JsonImportDialog} from "./json-import-dialog"
import {mockApi,renderWithClient,type MockApi} from "@/test-utils"
let mock:MockApi
beforeEach(()=>{mock=mockApi()})
afterEach(cleanup)
const endpoint="/api/interviews/INT-01/review/import"
const render=()=>renderWithClient(<JsonImportDialog open onOpenChange={vi.fn()} endpoint={endpoint} title="Import" description="Description"/> )
const paste=(text:string)=>fireEvent.change(screen.getByRole("textbox",{name:"Coller le JSON"}),{target:{value:text}})
const preview={digest:"digest",count:1,replacements:1,changes:[{unit_id:"INT-01-U002",excerpt:"The source passage.",before:null,after:{final_codes:["C-0001"]}}]}
describe("JSON batch import",()=>{
  it("requires a preview and explicit replacement consent before the atomic POST",async()=>{
    mock.on("POST",`${endpoint}/preview`,{body:preview})
    mock.on("POST",endpoint,{body:{count:1}})
    render();paste('{"decisions":[]}')
    expect(screen.getByRole("button",{name:"Enregistrer le lot"})).toBeDisabled()
    fireEvent.click(screen.getByRole("button",{name:"Vérifier et prévisualiser"}))
    expect(await screen.findByText("The source passage.")).toBeInTheDocument()
    expect(mock.to("POST",endpoint)).toHaveLength(0)
    expect(screen.getByRole("button",{name:"Enregistrer le lot"})).toBeDisabled()
    fireEvent.click(screen.getByRole("checkbox",{name:/Remplacer les 1 arbitrages/}))
    fireEvent.click(screen.getByRole("button",{name:"Enregistrer le lot"}))
    await waitFor(()=>expect(mock.to("POST",endpoint)).toHaveLength(1))
    expect(mock.to("POST",endpoint)[0].body).toEqual({raw:'{"decisions":[]}',expected_digest:"digest",replace_existing:true})
  })
  it("invalidates the preview when the pasted document changes",async()=>{
    mock.on("POST",`${endpoint}/preview`,{body:{...preview,replacements:0}})
    render();paste("first");fireEvent.click(screen.getByRole("button",{name:"Vérifier et prévisualiser"}))
    await screen.findByText("The source passage.")
    expect(screen.getByRole("button",{name:"Enregistrer le lot"})).toBeEnabled()
    paste("second")
    expect(screen.queryByText("The source passage.")).toBeNull()
    expect(screen.getByRole("button",{name:"Enregistrer le lot"})).toBeDisabled()
  })
  it("shows server validation errors without applying anything",async()=>{
    mock.on("POST",`${endpoint}/preview`,{status:400,body:{error:"Unknown unit",kind:"validation"}})
    render();paste("bad");fireEvent.click(screen.getByRole("button",{name:"Vérifier et prévisualiser"}))
    expect(await screen.findByRole("alert")).toHaveTextContent("Unknown unit")
    expect(mock.to("POST",endpoint)).toHaveLength(0)
  })
})
