// The Lexique inspector of the Surlignages screen (design/macos/Lexique.dc.html):
// the selected highlight, the keyword table of all highlights, the passages
// close to the selected one. The wider tools (keyword in context, frequency per
// attribute) live on the Lexique screen, linked at the end.

import { KeywordTable } from "@/components/lexicon/keyword-table"
import { PassageLexicon, SimilarPassages } from "@/components/lexicon/passage-panel"

export function LexiconInspector({ passageId, unitLabel, onUseLabel }: { passageId: string | null; unitLabel: string | null; onUseLabel?: (label: string) => void }) {
  return (
    <div className="insp-in">
      <div className="hs">Lexique</div>
      <PassageLexicon passageId={passageId} onUseLabel={onUseLabel} />
      <div className="hs" style={{ marginTop: 4 }}>
        Mots-clés de tous les surlignages
      </div>
      <KeywordTable compact />
      {passageId !== null && (
        <>
          <div className="hs" style={{ marginTop: 4 }}>
            Passages proches{unitLabel ? ` de ${unitLabel}` : ""}
          </div>
          <SimilarPassages passageId={passageId} />
        </>
      )}
      <a href="#/lexique" className="small" style={{ marginTop: 4 }}>
        Recherche en contexte et fréquences par attribut
      </a>
    </div>
  )
}
