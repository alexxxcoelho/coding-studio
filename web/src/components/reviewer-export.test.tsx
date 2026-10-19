import { beforeEach, expect, it, vi } from "vitest"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"
import { ReviewerExport } from "./reviewer-export"
import { request } from "@/lib/api"
vi.mock("@/lib/api", () => ({ request: vi.fn() }))

const catalogue = {snapshot:"snapshot-1",file_ids:{},pseudonym_rules:0,fields:[
  {id:"text",path:["Transcript","I01","text"],value:"Source quote",selected_by_default:true},
  {id:"name",path:["People","P01","name"],value:"PRIVATE NAME",selected_by_default:false},
]}
beforeEach(() => { vi.mocked(request).mockReset(); localStorage.clear() })

it("takes the exact field selection through the wizard without inventing a URL", async () => {
  vi.mocked(request).mockImplementation(async method => method === "GET" ? catalogue : {dir:"/exports/reviewer",zip:"/exports/reviewer/upload.zip",fields:1,files:0,pseudonym_replacements:0,url:null,published:false})
  render(<ReviewerExport open onOpenChange={vi.fn()} />)
  await screen.findByRole("button",{name:/Transcript/})
  expect(screen.getByRole("checkbox",{name:"People"})).not.toBeChecked()
  fireEvent.click(screen.getByRole("button",{name:"Continuer"}))
  fireEvent.change(screen.getByLabelText("Mot de passe du dossier"),{target:{value:"synthetic test only 12345"}})
  fireEvent.change(screen.getByLabelText("Confirmer le mot de passe"),{target:{value:"synthetic test only 12345"}})
  fireEvent.click(screen.getByRole("button",{name:"Continuer"}))
  fireEvent.click(screen.getByRole("checkbox",{name:/J’ai vérifié/}))
  fireEvent.click(screen.getByRole("button",{name:"Créer l’export protégé"}))
  await screen.findByText("Le dossier local est prêt")
  const call=vi.mocked(request).mock.calls.find(([method])=>method==="POST")!
  expect(call[1]).toBe("/api/export/reviewer")
  expect(call[2]).toMatchObject({selected:["text"],snapshot:"snapshot-1",url:"",pseudonymise:true})
  expect(screen.queryByDisplayValue("synthetic test only 12345")).not.toBeInTheDocument()
})

it("requires a new password after a failed generation", async () => {
  vi.mocked(request).mockImplementation(async method => {if(method==="GET")return catalogue; throw new Error("Source changed")})
  render(<ReviewerExport open onOpenChange={vi.fn()} />)
  await screen.findByRole("button",{name:/Transcript/})
  fireEvent.click(screen.getByRole("button",{name:"Continuer"}))
  for(const label of ["Mot de passe du dossier","Confirmer le mot de passe"])fireEvent.change(screen.getByLabelText(label),{target:{value:"synthetic test only 12345"}})
  fireEvent.click(screen.getByRole("button",{name:"Continuer"}))
  fireEvent.click(screen.getByRole("checkbox",{name:/J’ai vérifié/}))
  fireEvent.click(screen.getByRole("button",{name:"Créer l’export protégé"}))
  await waitFor(()=>expect(screen.getByRole("alert")).toHaveTextContent("Source changed"))
  expect(screen.getByLabelText("Mot de passe du dossier")).toHaveValue("")
  expect(screen.getByRole("button",{name:"Continuer"})).toBeDisabled()
})

it("requires a password for identified copies and reloads their field selection",async()=>{
 vi.mocked(request).mockImplementation(async method=>method==="GET"?catalogue:{});
 render(<ReviewerExport open onOpenChange={vi.fn()}/>);await screen.findByRole("button",{name:"Continuer"});
 fireEvent.change(screen.getByRole("combobox",{name:"Version du dossier"}),{target:{value:"identified"}});
 await waitFor(()=>expect(request).toHaveBeenCalledWith("GET","/api/export/reviewer/catalogue?pseudonymise=false"));
 fireEvent.click(screen.getByRole("button",{name:"Continuer"}));expect(screen.getByRole("button",{name:"Continuer"})).toBeDisabled();
})
