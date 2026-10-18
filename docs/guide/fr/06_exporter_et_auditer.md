# Exporter et auditer {#exporter}

## Pourquoi cette étape {#exporter-pourquoi}

Une analyse qualitative qui ne peut pas être revue demande à être crue sur parole. Coding Studio garde tout dans une base unique et produit des exports dont vos tableaux et annexes sont tirés, de sorte qu'aucun chiffre du rapport n'est saisi à la main. Le journal d'audit enregistre chaque opération dans l'ordre, chaque entrée étant liée à la précédente par une empreinte : on ne peut en supprimer ni en modifier une sans que la chaîne casse. C'est ce qui permet de prouver que votre codage précédait celui du modèle. Braun et Clarke (2021) demandent un rapport transparent sur les choix faits ; le paquet d'export en est la forme concrète. Exportez en cours de route pour sauvegarder, puis une dernière fois avant la remise.

## Ce que vous faites {#exporter-actions}

### 1. Exporter un entretien {#exporter-entretien}

L'export d'un entretien rassemble tout ce qui le concerne dans un dossier daté.

**Dans Coding Studio.** Dans Réglages, bloc Exports et vérification, cliquez sur Exporter un entretien et choisissez l'entretien. L'outil écrit un dossier horodaté (par exemple `exports/2026-10-13_BIB-01`) sans jamais écraser un export précédent. L'écran affiche ensuite le nombre de fichiers et indique que les empreintes sont vérifiées. Le dossier contient notamment la liste des unités, le codage humain, le codage et la comparaison de chaque revue IA (si elle a eu lieu) avec le texte envoyé et la réponse brute, les décisions, le codebook, les citations avec leur vérification, le journal d'audit et le manifeste.

### 2. Exporter le transversal {#exporter-transversal}

L'export transversal couvre l'ensemble du corpus.

**Dans Coding Studio.** Dans le même bloc, cliquez sur Exporter le transversal. Le paquet contient le fichier des réponses (le texte de l'étape Rédiger), la matrice, la table de couverture des entretiens, la liste des cas, un manifeste et un fichier README qui décrit le contenu. Vous n'avez pas à construire de tableau de saturation vous-même : c'est un fichier produit par l'export à partir des codes posés.

Pour un document Word rassemblant les résultats et leurs preuves, utilisez aussi [Rapport analytique Word dans Analyser](04_analyser.md#analyser-rapport).

### 3. Lire le manifeste {#exporter-manifeste}

Le manifeste est la table des matières vérifiable du paquet.

**Dans Coding Studio.** Chaque paquet comprend un manifeste qui liste tous les fichiers avec leur taille et leur empreinte SHA-256, un nombre calculé sur le contenu qui change si le fichier change d'un caractère. Pour contrôler un paquet archivé, recalculez l'empreinte d'un fichier avec l'outil de votre système (sha256sum, Get-FileHash) et comparez-la au manifeste. Gardez la base et les exports ensemble.

### 4. Vérifier la chaîne du journal d'audit {#exporter-journal}

La vérification confirme que le journal n'a pas été altéré.

**Dans Coding Studio.** Dans Réglages, bloc Exports et vérification, cliquez sur Journal d'audit pour parcourir les opérations : date, auteur, type d'action, ancienne et nouvelle valeur, raison. Cliquez sur Vérifier la chaîne : l'outil recalcule les empreintes de bout en bout et affiche le résultat. Une chaîne valide ne prouve pas que vos choix sont justes, mais que l'ordre et le contenu des opérations enregistrées n'ont pas été modifiés après coup. Les mêmes vues, avec la table de saturation et la matrice transversale, restent consultables dans ce bloc ; ce sont des sorties, pas des étapes.

### 5. Rejouer l'audit final {#exporter-audit-final}

L'audit final de trois affirmations (onglet Audit final de Rédiger, voir [Rédiger](05_rediger.md#rediger-audit)) est le contrôle de remise.

**Dans Coding Studio.** Après le dernier export, relancez le tirage dans l'onglet Audit final de Rédiger. Pour chaque affirmation, le résultat (étayée, à nuancer ou à retirer) va dans l'export : joignez-le à votre dossier de remise. Si un chemin est cassé, corrigez, exportez de nouveau et recommencez.

### 6. Choisir ce qui va en annexe {#exporter-annexes}

Tout n'a pas à aller en annexe : choisissez ce qui permet au lecteur de juger la rigueur sans le noyer.

**Dans Coding Studio.** À partir des paquets, prenez en règle générale :

- la liste des codes, avec définition, inclusions, exclusions et nombre de passages et d'entretiens (fichier codebook) ;
- le tableau des codes par question de recherche ;
- la couverture de vérification : la part des unités vérifiées à l'audio, à côté de chaque décompte ;
- les distributions des natures de preuve, des origines et des rôles des citations ;
- les fiches de thème et leurs mémos de décision ;
- les décisions documentées ;
- le fichier des réponses, qui est le texte final ;
- si vous avez fait des revues IA, la déclaration des entretiens concernés, le fournisseur et le lieu d'exécution, le prompt et la réponse brute.

Le journal d'audit complet et les copies de la base se gardent dans votre archive, avec la transcription originale. Ne publiez aucun fichier non pseudonymisé sans contrôle manuel du contenu libre (mémos, notes), que la liste de pseudonymisation ne couvre pas.

## Pièges fréquents {#exporter-pieges}

- Déplacer ou renommer un fichier d'un paquet : son empreinte ne correspond plus au manifeste.
- Déclarer une saturation automatique. Un tableau n'est pas une preuve de suffisance : dites quels signaux justifient que les données suffisent pour vos questions et quelles zones restent moins éclairées.
- Oublier que le contenu libre (mémos, notes) peut contenir des noms.

## Ce qui sort de cette étape {#exporter-sortie}

- Un paquet par entretien et un paquet transversal, chacun avec son manifeste et ses empreintes.
- Un journal d'audit dont la chaîne a été vérifiée.
- Le texte des résultats, les tableaux et les annexes tirés directement des exports.
- Le résultat de l'audit final de trois affirmations.
- Une archive cohérente de la base, des sources et des exports, prête à remettre.


## Export final pour les correcteurs

Ouvrir **Export final pour les correcteurs** depuis Analyser ou depuis le menu Exporter de la Bibliothèque. L’outil prépare un dossier local de consultation en lecture seule. Il ne publie rien.

1. **Contenu.** Déplier les rubriques jusqu’au champ voulu. Cocher un dossier sélectionne tous ses champs ; décocher une feuille crée une exception. Cliquer sur un champ affiche son aperçu. Les textes complets sont exportés, même si l’aperçu est limité. Les fichiers originaux, fiches de personnes, métadonnées d’entretien et passes IA sont décochés au départ. Pour une transcription non segmentée, sélectionner son fichier source. Une référence ne réintroduit jamais le contenu d’un objet exclu.
2. **Accès.** Choisir et conserver une phrase de passe unique d’au moins 16 caractères. Elle ne sera pas enregistrée dans l’export et ne pourra pas être récupérée. La transmettre séparément. L’adresse HTTPS est facultative : sans hébergement défini, laisser ce champ vide. Aucun lien public ni QR ne sera créé. Avec une adresse réelle, URL.txt et QR.svg correspondent à cette adresse ; aucune mise en ligne n’est effectuée.
3. **Vérification.** Relire la sélection, son volume et les options de pseudonymisation, puis créer l’export protégé. Le fichier upload.zip contient uniquement le lecteur et ses données chiffrées, jamais la base de travail ni le mot de passe. Héberger son contenu complet sur un serveur HTTPS de confiance et tester ensuite le lien réel.

Le lecteur recherche dans les champs inclus. Il s’ouvre avec le mot de passe dans un navigateur récent autorisant WebCrypto. Pour les pièces jointes depuis le disque, choisir le sous-dossier data dans le lecteur. En ligne, elles sont chargées depuis le même hébergement. Les fichiers sont chiffrés par morceaux ; un fichier de plus de 1 Gio est refusé avec une erreur explicite, sans troncature. Les noms de téléchargement sont neutres ; le nom original reste un champ sélectionnable séparément.

La liste de pseudonymisation ne remplace que les formes de noms configurées, sur une copie. Zéro remplacement ne signifie pas anonyme. Les fichiers originaux ne sont pas pseudonymisés : ils peuvent révéler des voix ou des identités. Le mot de passe protège le contenu chiffré, mais il ne constitue pas une gestion de comptes et ne permet pas de révoquer une copie déjà téléchargée. Un projet modifié depuis l’ouverture du catalogue impose de recommencer la sélection. Un fichier source modifié ou absent fait échouer la génération, sans dossier final incomplet.

## Style des notes

Utiliser des notes courtes, en puces si utile : idée, passage utile, contraste, question ouverte, limite. Les mémos d’entretien, fiches de thèmes et zones de rédaction servent de prise de notes du chercheur. Une définition de code tient en une phrase simple. Une note ou un statut de traitement ne remplace pas la vérification ciblée d’une citation contre l’enregistrement.

## Copie destinée aux correcteurs

L’assistant **Export correcteurs** ne propose que la copie anonymisée. Tous les entretiens doivent avoir été relus dans **Anonymiser**. Relisez aussi les champs sélectionnés dans l’arbre, notamment notes, analyse, rapport et anciennes passes IA : des noms inconnus peuvent s’y trouver. Confirmez cette relecture avant de générer le dossier chiffré. Les fichiers originaux, coordonnées et documents de consentement sont exclus, ainsi que la table privée des identités. Un export ancien ne devient pas anonyme rétroactivement : régénérez-le après cette vérification.
