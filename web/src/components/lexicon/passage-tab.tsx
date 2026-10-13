// The Lexique tab of the Coder inspector: lexical help for the passage in view
// (keyphrases, stems, provisional label, near passages), and the way to the
// keyword table of all the highlights.

import { PassageLexicon, SimilarPassages } from "@/components/lexicon/passage-panel"
import { shortId } from "@/components/coder/turn"
import { t } from "@/i18n"
import { interviewHref } from "@/lib/router"
import type { Passage } from "@/lib/types"

export function LexiconPassageTab({ passage }: { passage: Passage | null }) {
  return (
    <div className="insp-in" aria-label={t("lexicon.passage_tab.lexique_du_passage")}>
      <div className="hs">{passage?.kind === "highlight" ? t("lexicon.passage_tab.surlignage_selectionne") : t("common.passage_selectionne")}</div>
      <PassageLexicon passageId={passage?.passage_id ?? null} />
      {passage && (
        <>
          <div className="hs" style={{ marginTop: 4 }}>
            {t("lexicon.passage_tab.passages_proches_de_shortid", { shortId: shortId(passage.unit_id) })}
          </div>
          <SimilarPassages passageId={passage.passage_id} />
          <a className="small" href={interviewHref(passage.interview_id, "highlights")}>
            {t("common.mots_cles_de_tous_les")}
          </a>
        </>
      )}
      <a className="small" href="#/lexique">
        {t("common.recherche_en_contexte_et_frequences")}
      </a>
    </div>
  )
}
