# Revoir (facultatif) {#revoir}

## Pourquoi cette étape {#revoir-pourquoi}

Revoir sert à contrôler votre propre lecture, pas à la remplacer. Le premier volet n'utilise aucune IA : après une fusion ou une redéfinition de code, vous relisez les unités des entretiens précédents concernées. Le second est une revue par un modèle de langage, que vous lancez si elle vous semble utile pour un entretien. Le modèle passe après vous et à l'aveugle : s'il passait avant, son codage orienterait le vôtre ; s'il voyait vos codes, il aurait tendance à les confirmer. Il reçoit donc les tours et le codebook, jamais votre codage. Le résultat est un contrôle de lecture documenté, pas une mesure de fiabilité entre codeurs, que l'analyse thématique réflexive de Braun et Clarke (2021) n'exige pas. Un entretien peut être codé sans cette revue ; quand elle est faite, déclarez dans votre rapport sur quels entretiens.

## Ce que vous faites {#revoir-actions}

### 1. Harmoniser sans IA {#revoir-harmoniser}

Quand deux codes sont fusionnés ou qu'une définition change, des unités déjà codées peuvent ne plus correspondre au nouveau sens.

**Dans Coding Studio.** Après une fusion, une scission ou une redéfinition dans le Codebook, la liste des unités à reverifier apparaît dans l'écran Codebook et dans le volet décisions de Revoir. Chaque ligne affiche le texte de l'unité et ses codes actuels. Vous la relisez, puis vous confirmez le codage ou vous le corrigez. Quand toute la liste est traitée, la Bibliothèque marque l'entretien comme revu.

### 2. Décider si une revue IA est utile {#revoir-decider}

La revue IA coûte de l'attention et expose un texte à un tiers si le modèle est hébergé. Elle se justifie surtout sur les premiers entretiens, ou sur un entretien dont vous doutez.

**Dans Coding Studio.** La barre des étapes indique l'état de la revue pour l'entretien : « pas de revue », « revue IA en attente » ou « revue IA faite, 9 sur 12 décidées ». La revue n'est jamais un verrou : vous pouvez passer à l'Analyser sans l'avoir faite.

### 3. Configurer un fournisseur, ou y renoncer {#revoir-fournisseurs}

Le copier-coller vers le modèle de votre choix reste le parcours de référence, après validation de l’anonymisation. Un fournisseur configuré fait seulement l'aller-retour à votre place.

**Dans Coding Studio.** Dans Réglages, bloc Fournisseurs IA, la liste montre les fournisseurs déclarés (commande locale, API compatible OpenAI, connexion par compte). Cliquez sur Ajouter un fournisseur et renseignez son type. Une clé n'est jamais stockée : elle se nomme par sa variable d'environnement. Chaque fournisseur déclare ce qu'il envoie et où il s'exécute (local ou hébergé). Pour une commande locale, Tester l'isolation vérifie son isolation et date le contrôle. L'envoi est toujours un geste explicite et journalisé. Voir aussi [la pseudonymisation](01_preparer.md#preparer-pseudo).

### 4. Générer le prompt {#revoir-prompt}

Générer le prompt fige votre codage de l'entretien et prépare le texte à envoyer.

**Dans Coding Studio.** Ouvrez l'entretien, puis l'étape Revoir. Le volet IA commence par Générer le prompt, désactivé tant que les tours pertinents ne sont pas codés. Au clic, votre codage est enregistré tel quel : c'est la référence de la comparaison, même si vous changez d'avis ensuite. Le prompt contient la description de l'étude, les questions, les règles de codage, le codebook (arbre, définitions, exemples), l'attribut du répondant utile au contexte et, pour chaque tour, la question de l'intervieweur qui le précède. Les noms de personnes et d'organisations sont pseudonymisés avant envoi. Vos codes n'y figurent jamais. Le prompt est découpé en parties si nécessaire ; une ligne indique la date, la version du codebook, le nombre d'unités et de parties. Cliquez sur Copier la partie 1 sur 2, puis sur la partie 2 sur 2. Le modèle peut signaler un « concept absent du codebook » : cette proposition arrivera comme candidat dans le volet décisions.

Avant de copier ou lancer un fournisseur :

1. Ouvrez **Anonymiser**, relisez et validez la copie de l’entretien.
2. Affichez toutes les parties du prompt. Vérifiez également le codebook, les exemples, les attributs, les questions et la description de l’étude.
3. Cochez la confirmation puis cliquez sur **Valider ce prompt**. La copie passe par le même contrôle que les fournisseurs. Une modification du texte, des règles, du codebook ou du contexte invalide l’autorisation correspondante. Les anciennes passes IA restent conservées ; leur affichage pour copie est remasqué avec les règles actuelles.

### 5. Importer la réponse du modèle {#revoir-reponse}

Une fois la réponse obtenue, elle entre dans l'outil, qui la contrôle avant de l'accepter.

**Dans Coding Studio.** Parcours de référence (2a) : collez le prompt dans le modèle de votre choix, puis, dans le champ Modèle utilisé, nommez-le. Collez sa réponse dans le champ Réponse à la partie 1 sur 2, puis cliquez sur Importer la réponse. Répétez pour chaque partie. Une réponse mal formée est refusée en bloc, avec la liste des problèmes ; rien n'est gardé à moitié. Avec un fournisseur configuré (2b), choisissez-le et cliquez sur Lancer : l'outil fait l'aller-retour, puis applique la même validation. Le journal enregistre le fournisseur, le lieu d'exécution, le texte envoyé et la réponse brute. Les runs apparaissent dans la liste Runs enregistrés, par exemple « R1, 13.10 09:40, modèle X, 2 parties sur 2, complet. Arbitrer ». Un run complet suffit. Un second run n'est utile que pour comparer deux modèles.

### 6. Arbitrer les désaccords {#revoir-arbitrer}

L'outil liste les unités où le modèle et vous n'avez pas posé les mêmes codes. Ce sont elles qui méritent votre décision.

**Dans Coding Studio.** Dans le volet décisions, la section Désaccords affiche une carte par unité, avec un compteur (par exemple 7 sur 9 décidés). Une carte contient le texte du tour avec la question qui le précède, vos codes figés, ceux du modèle et trois choix : Garder le mien, Prendre celui du modèle, Autre (choisir les codes). Une justification est obligatoire. Elle se saisit en une phrase, par exemple « le modèle a lu une contrainte là où le répondant décrit un choix ». Cliquez sur Enregistrer. Votre décision est finale. L'outil conserve séparément trois objets, sans jamais les écraser : votre lecture figée, celle du modèle et votre décision.

### 7. Contrôler un échantillon d'accords {#revoir-accords}

Un accord n'est pas une preuve : vous et le modèle pouvez vous tromper de la même façon.

**Dans Coding Studio.** La section Échantillon d'accords présente quelques unités tirées au sort parmi celles où vous et le modèle êtes d'accord (le tirage est enregistré avec sa graine). Pour chacune, cliquez sur Confirmer ou sur Corriger. Une justification est facultative.

### 8. Traiter les concepts absents du codebook {#revoir-concepts}

Le modèle peut signaler un concept que votre codebook ne couvre pas : c'est un candidat, jamais un code appliqué seul.

**Dans Coding Studio.** La section Concepts absents du codebook liste chaque proposition avec les unités qui la motivent. Pour chacune, choisissez Créer un code, Rattacher à un code existant ou Écarter. La décision est motivée et datée, comme pour un désaccord.

### 9. Réviser le codebook après la revue {#revoir-codebook}

Les désaccords révèlent souvent un code mal défini.

**Dans Coding Studio.** Retournez dans le Codebook pour préciser une définition, fusionner ou scinder ; la liste des unités à reverifier réapparaît (action 1). Relancer une revue après un changement important n'a de sens que si vous le jugez utile.

## Pièges fréquents {#revoir-pieges}

- Traiter la revue IA comme un second codeur dont l'accord prouverait la fiabilité. Elle contrôle votre lecture, rien de plus.
- Ne regarder que les désaccords. Un accord peut cacher une erreur commune.
- Envoyer un prompt à un fournisseur hébergé sans avoir relu la liste de pseudonymisation.

## Ce qui sort de cette étape {#revoir-sortie}

- Pour chaque entretien revu, trois objets distincts : votre lecture figée, celle du modèle et vos décisions motivées.
- Des désaccords tranchés, un échantillon d'accords vérifié.
- Des codes créés, rattachés ou écartés à partir des concepts proposés.
- Un codebook harmonisé, avec la liste des unités revérifiées.
- Un journal qui prouve l'ordre des opérations : votre codage précède celui du modèle.
