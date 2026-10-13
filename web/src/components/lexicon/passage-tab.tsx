// The Lexique tab of the Coder inspector: lexical help for the passage in view
// (keyphrases, stems, provisional label, near passages), and the way to the
// keyword table of all the highlights.

import { PassageLexicon, SimilarPassages } from "@/components/lexicon/passage-panel"
import { shortId } from "@/components/coder/turn"
import { interviewHref } from "@/lib/router"
import type { Passage } from "@/lib/types"

export function LexiconPassageTab({ passage }: { passage: Passage | null }) {
  return (
    <div className="insp-in" aria-label="Lexique du passage">
      <div className="hs">{passage?.kind === "highlight" ? "Surlignage sélectionné" : "Passage sélectionné"}</div>
      <PassageLexicon passageId={passage?.passage_id ?? null} />
      {passage && (
        <>
          <div className="hs" style={{ marginTop: 4 }}>
            Passages proches de {shortId(passage.unit_id)}
          </div>
          <SimilarPassages passageId={passage.passage_id} />
          <a className="small" href={interviewHref(passage.interview_id, "highlights")}>
            Mots-clés de tous les surlignages
          </a>
        </>
      )}
      <a className="small" href="#/lexique">
        Recherche en contexte et fréquences par attribut
      </a>
    </div>
  )
}
