# Guide de Coding Studio {#sommaire}

## Ce que couvre ce guide {#sommaire-portee}

Coding Studio est un outil local d'analyse qualitative d'entretiens semi-directifs. Il vous aide à passer de transcriptions à des résultats que vous pouvez défendre, chaque affirmation pouvant être suivie jusqu'aux passages exacts. Il reprend l'essentiel d'un logiciel comme NVivo (codage de passages, arbre de codes, mémos, matrices, exports) et ajoute deux options : un second codage par un modèle de langage, fait à l'aveugle, et un atelier lexical sans IA générative.

Le guide décrit l'outil pour n'importe quelle étude. Questions, attributs, mémos et cadre d'interprétation se définissent dans les Réglages de votre projet. Les exemples portent sur une étude fictive : l'adoption d'un nouveau système de prêt par de petites bibliothèques (entretiens BIB-01, BIB-02, ...).

## Le parcours en une liste {#sommaire-parcours}

Le travail suit six temps. Le parcours n'est pas une cascade : revenir aux entretiens, rouvrir un code ou réécrire un thème est normal.

1. Préparer (une fois par étude). Vous créez le projet, vous écrivez vos questions de recherche, vous définissez les attributs d'entretien, les modèles de mémo et les listes de contrôle, puis vous importez les transcriptions.
2. Coder (entretien par entretien). Vous écrivez un mémo de familiarisation, puis vous lisez les tours du répondant et vous posez des codes sur des passages. Le codebook se construit en codant.
3. Revoir (facultatif, entretien par entretien). Vous harmonisez le codebook après chaque changement important, et vous pouvez faire relire un entretien par un modèle de langage, à l'aveugle, pour confronter les deux lectures.
4. Analyser (sur l'ensemble des entretiens). Vous regroupez les codes en thèmes, vous testez chaque thème, vous comparez les entretiens dans une matrice et vous explorez les mots et les passages proches.
5. Rédiger (sur l'ensemble des entretiens). Vous écrivez le texte des résultats thème par thème, vous reliez chaque passage cité à la partie qu'il soutient, puis vous répondez à chaque question de recherche.
6. Exporter et auditer. Vous produisez les paquets d'export, vous vérifiez le journal d'audit, vous contrôlez trois affirmations au hasard et vous constituez vos annexes.

Dans l'application, les étapes 2 à 5 forment la barre numérotée de l'écran d'un entretien. Les étapes 1 et 6 se font depuis la Bibliothèque et les Réglages.

## Les écrans de l'application {#sommaire-ecrans}

La navigation du haut offre la Bibliothèque, l'Analyser, Rédiger, le Codebook, les Réglages et le Guide, plus l'entretien ouvert. Chaque écran porte en haut à droite un bouton « ? » qui ouvre la section correspondante du guide dans un tiroir latéral, sans quitter l'écran.

- Bibliothèque : la liste des entretiens, l'import et la fiche. Voir [Préparer](01_preparer.md#preparer-bibliotheque).
- Coder : le texte d'un entretien, la liste des codes, le mémo. Voir [Coder](02_coder.md#coder-parcourir).
- Surlignages et lexique : le surlignage sans code et l'atelier de mots-clés. Voir [Coder](02_coder.md#coder-surlignages).
- Détail : le tiroir d'un passage que vous allez citer. Voir [Coder](02_coder.md#coder-detail).
- Revoir, volet IA : la préparation du prompt et l'import de la réponse. Voir [Revoir](03_revoir.md#revoir-prompt).
- Revoir, volet décisions : les désaccords et les concepts proposés. Voir [Revoir](03_revoir.md#revoir-arbitrer).
- Analyser : les thèmes, la matrice, l'exploration. Voir [Analyser](04_analyser.md#analyser-themes).
- Rédiger : cinq onglets (par thème, synthèse par question, lecture par le cadre, audit final, décisions documentées). Voir [Rédiger](05_rediger.md#rediger-theme).
- Codebook : l'arbre des codes. Voir [Coder](02_coder.md#coder-codebook).
- Réglages : tout ce qui se définit une fois par étude. Voir [Préparer](01_preparer.md#preparer-reglages).
- Guide : ce texte, avec son sommaire et le bouton « ? » de chaque écran. Voir [Sommaire](00_sommaire.md#sommaire-lire).

## Comment lire ce guide {#sommaire-lire}

Chaque étape suit le même plan.

1. Pourquoi cette étape : trois à six phrases sur la méthode.
2. Ce que vous faites : des actions numérotées. Sous chacune, un bloc « Dans Coding Studio » dit où cliquer, ce qui s'affiche et ce qui est enregistré.
3. Pièges fréquents : les erreurs courantes.
4. Ce qui sort de cette étape : ce que vous avez en main à la fin.

Trois principes traversent tout le guide.

Un code décrit, un thème interprète. Les codes que vous posez en codant restent proches de ce que dit le répondant. Les thèmes, construits plus tard, sont des propositions que vous formulez et que vous défendez.

Le cadre d'interprétation est facultatif et tardif. Si vous en choisissez un, il n'apparaît qu'aux étapes Analyser et Rédiger. Il n'apparaît jamais pendant le codage et n'est jamais un champ des codes.

Vous restez l'auteur. L'outil enregistre, compare, compte et propose, mais ne décide jamais à votre place. Il refuse aussi de produire un chiffre que vous ne pourriez pas retrouver dans les données.

## Ce qui est personnalisable {#sommaire-personnalisable}

Ces éléments sont des listes modifiables, avec des valeurs par défaut : modèles de mémo, attributs d'entretien, liste de contrôle de fin d'entretien, cadre d'interprétation, contrôles de thème (sept par défaut), natures de preuve, types de lien et dimensions de matrice. Un modèle de projet JSON exporte ces choix pour une nouvelle étude (voir [Préparer](01_preparer.md#preparer-modele)). Seules les valeurs de cellule de la matrice (présent, non évoqué, contredit, incertain) sont fixes.
