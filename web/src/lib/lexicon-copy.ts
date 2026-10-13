// Wording of the Lexique screen: what each tool does, shown while it is empty.

import { t } from "@/i18n"
/** The guide section on the lexical help (docs/guide/fr/02_coder.md, action 10). */
export const GUIDE_LEXIQUE = "#/guide/coder-lexique"

export const EMPTY = {
  keywords: {
    get title() { return t("lexicon_copy.aucun_surlignage_a_analyser") },
    get text() { return t("lexicon_copy.le_tableau_des_mots_cles") },
  },
  passage: {
    get title() { return t("lexicon_copy.aucun_passage_selectionne") },
    get text() { return t("lexicon_copy.selectionnez_un_surlignage_le_pannea") },
  },
  similar: {
    get title() { return t("lexicon_copy.aucun_passage_proche") },
    get text() { return t("lexicon_copy.les_passages_proches_sont_les") },
  },
  kwic: {
    get title() { return t("common.recherche_en_contexte") },
    get text() { return t("lexicon_copy.tapez_un_mot_ou_une") },
  },
  kwicNone: {
    get title() { return t("lexicon_copy.aucune_occurrence") },
    get text() { return t("lexicon_copy.aucun_tour_d_enquete_ne") },
  },
  frequency: {
    get title() { return t("lexicon_copy.frequence_par_valeur_d_attribut") },
    get text() { return t("lexicon_copy.choisissez_un_attribut_des_entretien") },
  },
  noAttribute: {
    get title() { return t("common.aucun_attribut_defini") },
    get text() { return t("lexicon_copy.les_frequences_se_comptent_par") },
  },
} as const
