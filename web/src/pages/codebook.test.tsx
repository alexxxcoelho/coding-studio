import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react"

import { CodebookPage } from "@/pages/codebook"
import { mockApi, renderWithClient, type MockApi } from "@/test-utils"
import type { CodeNode } from "@/lib/types"

function node(id: string, name: string, over: Partial<CodeNode> = {}): CodeNode {
  return {
    code_id: id, token: name.toUpperCase().replace(/ /g, "-"), name, label: name, definition: `Def ${name}`, includes: "", excludes: "",
    parent_id: null, origin: "field", toe: null, research_questions: [], active: true, system: false, in_prompt: true, merged_into: null,
    version: 1, example: null, reason: null, memo: "", origin_passage_id: null, example_passage_id: null, created_at: null, aliases: [],
    counts: { passages: 2, interviews: 1, passages_rollup: 2, interviews_rollup: 1 }, ...over,
  }
}
const CODES = [
  node("C-0001", "Partner demand", { counts: { passages: 1, interviews: 1, passages_rollup: 3, interviews_rollup: 2 } }),
  node("C-0002", "Retailer listing", { parent_id: "C-0001" }),
  node("C-0003", "Cost burden"),
]

let mock: MockApi
beforeEach(() => {
  mock = mockApi()
  mock.on("GET", "/api/codes", { body: { codebook_version: 4, codes: CODES } })
  mock.on("GET", "/api/research-questions", { body: [{ id: "SQ1", text: "Q", ord: 1 }] })
})
afterEach(() => cleanup())

describe("codebook page", () => {
  it("shows the tree with roll-up counts and the version", async () => {
    renderWithClient(<CodebookPage />)
    const row = (await screen.findByText("Partner demand")).closest("tr")!
    expect(within(row).getByText("3")).toBeInTheDocument()
    expect(screen.getByText("Version 4 · 3 codes actifs · 0 fusionné")).toBeInTheDocument()
  })

  it("collapses a code with children", async () => {
    renderWithClient(<CodebookPage />)
    await screen.findByText("Retailer listing")
    fireEvent.click(screen.getByRole("button", { name: "Replier Partner demand" }))
    expect(screen.queryByText("Retailer listing")).toBeNull()
  })

  it("filters without accents", async () => {
    renderWithClient(<CodebookPage />)
    await screen.findByText("Cost burden")
    fireEvent.change(screen.getByLabelText("Filtrer les codes"), { target: { value: "COST" } })
    expect(screen.queryByText("Partner demand")).toBeNull()
    expect(screen.getByText("Cost burden")).toBeInTheDocument()
  })

  it("renames and saves a code through PUT /api/codes/:id", async () => {
    renderWithClient(<CodebookPage />)
    fireEvent.click((await screen.findByText("Cost burden")).closest("tr")!)
    const name = await screen.findByLabelText("Nom")
    mock.on("PUT", "/api/codes/C-0003", { body: node("C-0003", "Cost") })
    fireEvent.change(name, { target: { value: "Cost" } })
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer" }))
    await waitFor(() => expect(mock.to("PUT", "/api/codes/C-0003")).toHaveLength(1))
    expect(mock.to("PUT", "/api/codes/C-0003")[0].body).toMatchObject({ name: "Cost", definition: "Def Cost burden" })
  })

  it("moves a code under another with the parent select, never under its own descendant", async () => {
    renderWithClient(<CodebookPage />)
    fireEvent.click((await screen.findByText("Partner demand")).closest("tr")!)
    const parent = (await screen.findByLabelText("Parent")) as HTMLSelectElement
    const options = within(parent).getAllByRole("option").map((o) => o.textContent)
    expect(options).toEqual(["Aucun", "Cost burden"])
  })

  it("refuses a merge without a rationale", async () => {
    renderWithClient(<CodebookPage />)
    fireEvent.click((await screen.findByText("Cost burden")).closest("tr")!)
    fireEvent.click(await screen.findByRole("button", { name: "Fusionner dans…" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.change(within(dialog).getByLabelText("Fusionner dans"), { target: { value: "C-0001" } })
    expect(within(dialog).getByRole("button", { name: "Fusionner" })).toBeDisabled()
    fireEvent.change(within(dialog).getByLabelText("Motif, obligatoire"), { target: { value: "doublon" } })
    expect(within(dialog).getByRole("button", { name: "Fusionner" })).toBeEnabled()
  })

  it("creates a code with one POST /api/codes, named as typed", async () => {
    renderWithClient(<CodebookPage />)
    await screen.findByText("Cost burden")
    mock.on("POST", "/api/codes", { body: node("C-0009", "Obligation légale") })
    fireEvent.click(screen.getByRole("button", { name: "Nouveau code" }))
    const dialog = await screen.findByRole("dialog")
    fireEvent.change(within(dialog).getByLabelText("Nom"), { target: { value: "Obligation légale" } })
    fireEvent.change(within(dialog).getByLabelText("Définition"), { target: { value: "Ce que la loi impose." } })
    fireEvent.click(within(dialog).getByRole("button", { name: "Créer le code" }))
    await waitFor(() => expect(mock.to("POST", "/api/codes")).toHaveLength(1))
    expect(mock.to("POST", "/api/codes")[0].body).toEqual({ name: "Obligation légale", definition: "Ce que la loi impose.", parent_id: null })
  })

  it("saves the memo through PUT /api/codes/:id/memo, not as a reason", async () => {
    renderWithClient(<CodebookPage />)
    fireEvent.click((await screen.findByText("Cost burden")).closest("tr")!)
    mock.on("PUT", "/api/codes/C-0003", { body: node("C-0003", "Cost burden") })
    mock.on("PUT", "/api/codes/C-0003/memo", { body: { code_id: "C-0003", memo: "À distinguer du coût d'entrée.", history: [] } })
    fireEvent.change(await screen.findByLabelText("Mémo du code"), { target: { value: "À distinguer du coût d'entrée." } })
    fireEvent.click(screen.getByRole("button", { name: "Enregistrer" }))
    await waitFor(() => expect(mock.to("PUT", "/api/codes/C-0003/memo")).toHaveLength(1))
    expect(mock.to("PUT", "/api/codes/C-0003/memo")[0].body).toEqual({ text: "À distinguer du coût d'entrée." })
    expect(mock.to("PUT", "/api/codes/C-0003")[0].body).not.toHaveProperty("reason")
  })
})

it('moves a dragged code under another and supports dropping a child back at the root',async()=>{
 mock.on('POST','/api/codes/C-0003/move',{body:node('C-0003','Cost burden',{parent_id:'C-0001'})})
 mock.on('POST','/api/codes/C-0002/move',{body:node('C-0002','Retailer listing')})
 renderWithClient(<CodebookPage/>);await screen.findByText('Cost burden')
 const data=new Map<string,string>();const transfer={setData:(k:string,v:string)=>data.set(k,v),getData:(k:string)=>data.get(k)??'',effectAllowed:'',dropEffect:''}
 fireEvent.dragStart(screen.getByText('Cost burden').closest('tr')!,{dataTransfer:transfer})
 expect(data.get('application/x-coding-studio-code')).toBe('C-0003')
 fireEvent.dragOver(screen.getByText('Partner demand').closest('tr')!,{dataTransfer:transfer})
 fireEvent.drop(screen.getByText('Partner demand').closest('tr')!,{dataTransfer:transfer})
 await waitFor(()=>expect(mock.to('POST','/api/codes/C-0003/move')).toHaveLength(1))
 expect(mock.to('POST','/api/codes/C-0003/move')[0].body).toMatchObject({parent_id:'C-0001'})
 await waitFor(()=>expect(within(screen.getByLabelText('Arbre des codes')).getByText('Cost burden').closest('tr')).toHaveAttribute('aria-selected','true'))
 fireEvent.dragStart(within(screen.getByLabelText('Arbre des codes')).getByText('Retailer listing').closest('tr')!,{dataTransfer:transfer})
 fireEvent.drop(screen.getByRole('region',{name:'Déposer ici pour déplacer à la racine'}),{dataTransfer:transfer})
 await waitFor(()=>expect(mock.to('POST','/api/codes/C-0002/move')).toHaveLength(1))
 expect(mock.to('POST','/api/codes/C-0002/move')[0].body).toMatchObject({parent_id:null})
})
it('refuses a parent dropped onto its own descendant',async()=>{
 renderWithClient(<CodebookPage/>);await screen.findByText('Retailer listing')
 const transfer={setData:()=>{},getData:()=> 'C-0001',effectAllowed:'',dropEffect:''}
 fireEvent.dragStart(screen.getByText('Partner demand').closest('tr')!,{dataTransfer:transfer})
 fireEvent.drop(screen.getByText('Retailer listing').closest('tr')!,{dataTransfer:transfer})
 expect(mock.to('POST','/api/codes/C-0001/move')).toHaveLength(0)
})
it('sorts sibling codes and filters collapsed children by column without losing ancestors',async()=>{
 renderWithClient(<CodebookPage/>);await screen.findByText('Retailer listing')
 fireEvent.click(await screen.findByRole('button',{name:'Trier Code A-Z'}))
 const tree=screen.getByLabelText('Arbre des codes')
 expect(Array.from(tree.querySelectorAll('tbody tr')).map(r=>r.getAttribute('data-tree-id'))).toEqual(['C-0003','C-0001','C-0002'])
 fireEvent.click(screen.getByRole('button',{name:'Replier Partner demand'}))
 fireEvent.click(screen.getByRole('button',{name:'Filtrer Code'}))
 fireEvent.change(await screen.findByLabelText('Filtre de colonne : Code'),{target:{value:'retailer'}})
 await waitFor(()=>expect(screen.getByText('Retailer listing')).toBeVisible())
 expect(screen.getByText('Partner demand')).toBeVisible();expect(screen.getByText('Cost burden')).not.toBeVisible()
})
