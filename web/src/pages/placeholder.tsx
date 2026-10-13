// Analyser and Rédiger are new screens of a later lot. Until they ship, these
// pages say so and link to the pages that carry the same work today.

import type { ReactNode } from "react"

import { EmptyState, PageTitle } from "@/components/bits"
import { Button } from "@/components/ui/button"

function Placeholder({ title, sub, lot, children, links }: { title: string; sub: string; lot: string; children: ReactNode; links: { href: string; label: string }[] }) {
  return (
    <>
      <PageTitle title={title} sub={sub} />
      <EmptyState
        title={`Cet écran arrive avec le ${lot}`}
        action={
          <div className="flex flex-wrap gap-2">
            {links.map((l) => (
              <Button key={l.href} asChild variant="outline">
                <a href={l.href}>{l.label}</a>
              </Button>
            ))}
          </div>
        }
      >
        {children}
      </EmptyState>
    </>
  )
}

export function AnalyserPage() {
  return (
    <Placeholder
      title="Analyser"
      sub="Des codes aux thèmes, matrice et exploration"
      lot="lot 5"
      links={[{ href: "#/cross-case", label: "Ouvrir le transversal" }]}
    >
      Les thèmes, la matrice et l'exploration des passages y seront réunis. En attendant, le tableau transversal montre, pour chaque question de recherche, les codes
      portés par chaque entretien avec les passages qui les soutiennent.
    </Placeholder>
  )
}

export function RedigerPage() {
  return (
    <Placeholder
      title="Rédiger"
      sub="Les résultats, thème par thème"
      lot="lot 5"
      links={[{ href: "#/answers", label: "Ouvrir les réponses" }]}
    >
      La rédaction par thème, la synthèse par question et l'audit final y seront réunis. En attendant, la page des réponses porte la synthèse par question de
      recherche, avec les passages pour et contre.
    </Placeholder>
  )
}
