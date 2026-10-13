// Wording of the Lexique screen: what each tool does, shown while it is empty.

/** The guide section on the lexical help (docs/guide/fr/02_coder.md, action 10). */
export const GUIDE_LEXIQUE = "#/guide/coder-lexique"

export const EMPTY = {
  keywords: {
    title: "Aucun surlignage à analyser",
    text: "Le tableau des mots-clés compte, dans vos surlignages sans code, les mots (regroupés par racine) qui reviennent le plus par rapport au reste du texte des enquêtés. Il ne juge rien et n'utilise aucune IA générative. Surlignez des passages dans Coder (touche H) pour le remplir.",
  },
  passage: {
    title: "Aucun passage sélectionné",
    text: "Sélectionnez un surlignage : le panneau affiche ses expressions-clés (YAKE), ses racines et un libellé provisoire que vous pouvez utiliser ou ignorer.",
  },
  similar: {
    title: "Aucun passage proche",
    text: "Les passages proches sont les tours d'autres entretiens qui partagent le vocabulaire du passage (TF-IDF sur les racines). Aucun ne dépasse le seuil pour ce passage. Le lexique ne voit ni les synonymes ni les autres langues.",
  },
  kwic: {
    title: "Recherche en contexte",
    text: "Tapez un mot ou une expression pour voir chacune de ses occurrences au milieu de sa phrase, dans tous les entretiens. Casse et accents sont ignorés. Cochez la recherche par racine pour trouver aussi les formes voisines (rappel, rappeler). Seuls les tours des enquêtés sont fouillés.",
  },
  kwicNone: {
    title: "Aucune occurrence",
    text: "Aucun tour d'enquêté ne contient cette recherche avec ces filtres. Essayez la recherche par racine, un seul mot, ou retirez le filtre d'attribut.",
  },
  frequency: {
    title: "Fréquence par valeur d'attribut",
    text: "Choisissez un attribut des entretiens (par exemple le secteur) et, si vous voulez, des mots séparés par des virgules. Le tableau donne le nombre d'occurrences et leur fréquence pour 1000 mots dans chaque groupe d'entretiens. Sans mot, il liste les mots les plus fréquents de chaque groupe. C'est un comptage exploratoire, pas une prévalence.",
  },
  noAttribute: {
    title: "Aucun attribut défini",
    text: "Les fréquences se comptent par valeur d'un attribut des entretiens. Définissez un attribut dans les paramètres de l'étude, puis renseignez-le dans chaque entretien.",
  },
} as const
