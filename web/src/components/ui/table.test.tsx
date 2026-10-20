import {afterEach,expect,it} from 'vitest'
import {cleanup,fireEvent,render,screen,waitFor} from '@testing-library/react'
import {Fragment,useState} from 'react'
import {Table,TableHeader,TableBody,TableRow,TableHead,TableCell} from './table'
afterEach(cleanup)
function Fixture(){const [expanded,setExpanded]=useState(false);return <Table aria-label="fixture"><TableHeader><TableRow><TableHead>Nom</TableHead><TableHead>Nombre</TableHead></TableRow></TableHeader><TableBody>{[{id:'z',name:'Zèbre',n:10},{id:'a',name:'Alpha',n:2}].map(r=><Fragment key={r.id}><TableRow onClick={()=>setExpanded(!expanded)}><TableCell>{r.name}</TableCell><TableCell>{r.n}</TableCell></TableRow>{expanded&&r.id==='z'&&<TableRow><TableCell colSpan={2}>Détail Zèbre</TableCell></TableRow>}</Fragment>)}</TableBody></Table>}
const visible=()=>Array.from(screen.getByRole('table').querySelectorAll('tbody > tr')).filter(r=>!(r as HTMLTableRowElement).hidden).map(r=>r.textContent)
it('sorts in both directions, combines column filters and resets the original order',async()=>{
 render(<Fixture/>);await screen.findByRole('button',{name:'Trier Nombre A-Z'})
 fireEvent.click(screen.getByRole('button',{name:'Trier Nombre A-Z'}));expect(visible()).toEqual(['Alpha2','Zèbre10'])
 fireEvent.click(screen.getByRole('button',{name:'Trier Nombre Z-A'}));expect(visible()).toEqual(['Zèbre10','Alpha2'])
 fireEvent.click(screen.getByRole('button',{name:'Filtrer Nombre'}));fireEvent.change(await screen.findByLabelText('Filtre de colonne : Nombre'),{target:{value:'>=5'}})
 await waitFor(()=>expect(visible()).toEqual(['Zèbre10']))
 fireEvent.keyDown(screen.getByLabelText('Filtre de colonne : Nombre'),{key:'Escape'})
 fireEvent.click(screen.getByRole('button',{name:'Filtrer Nom'}));fireEvent.change(await screen.findByLabelText('Filtre de colonne : Nom'),{target:{value:'alpha'}})
 await waitFor(()=>expect(visible()).toEqual(['Aucune ligne ne correspond aux filtres.']))
 const nameInput=screen.queryByLabelText('Filtre de colonne : Nom');if(nameInput)fireEvent.keyDown(nameInput,{key:'Escape'})
 fireEvent.click(screen.getByRole('button',{name:'Réinitialiser tri et filtres'}));expect(visible()).toEqual(['Zèbre10','Alpha2'])
})
it('keeps an expanded detail with its owning row when sorted',async()=>{
 render(<Fixture/>);fireEvent.click(screen.getByText('Zèbre'));await screen.findByText('Détail Zèbre')
 fireEvent.click(await screen.findByRole('button',{name:'Trier Nom A-Z'}));expect(visible()).toEqual(['Alpha2','Zèbre10','Détail Zèbre'])
})

it('keeps a filter editable when it matches no rows',async()=>{render(<Fixture/>);fireEvent.click(await screen.findByRole('button',{name:'Filtrer Nom'}));const input=await screen.findByLabelText('Filtre de colonne : Nom');fireEvent.change(input,{target:{value:'absent'}});await waitFor(()=>expect(visible()).toEqual(['Aucune ligne ne correspond aux filtres.']));expect(screen.getByLabelText('Filtre de colonne : Nom')).toBeVisible()})
