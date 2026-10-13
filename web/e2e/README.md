# Contrôle visuel

`visual.py` capture chaque écran de l'application dans 6 combinaisons d'apparence (système clair ou sombre, choix de l'application Système, Clair ou Sombre) à 1440x900 et 1024x700, avec l'émulation du shell bureau (`window.__CS_VIBRANCY__ = true` et un fond de remplacement sur `html` pour la matière native : `#e6e6e9` en clair, `#2c2c2e` en sombre). Sur chaque capture :

1. axe-core (installé comme devDependency) : échec sur `color-contrast` et sur tout nom ou libellé manquant (boutons, liens, champs, listes, dialogues, barres de progression) ;
2. texte et fond tous deux sombres ou tous deux clairs (luminance de chaque nœud de texte sur son premier fond opaque) ;
3. mise en page : défilement horizontal de la page, barre d'outils qui passe à la ligne ou déborde, élément au-delà du bord droit ;
4. anglais restant dans le texte rendu, les `placeholder`, `aria-label` et `title` (liste `english-words.json`, partagée avec `src/i18n.test.tsx`) ;
5. erreurs de console et réponses HTTP 5xx.

Sortie dans `--out` : une planche par écran et par taille (`<écran>_<taille>.png`, 2 lignes système x 3 colonnes application), les captures (`shots/`), `report.md` et `report.json`. Code de sortie 1 s'il reste un problème.

## Lancer

```sh
# 1. Construire l'interface et la copier là où le binaire l'embarque
cd web
CS_GUIDE_DIR=$(realpath ../docs/guide) pnpm build
rm -rf ../crates/cs-app/web-dist && cp -r dist ../crates/cs-app/web-dist

# 2. Serveur sur une base jetable (recompiler le binaire pour embarquer le nouveau web-dist)
cd ..
cargo build -p cs-app
target/debug/coding-studio --db /tmp/cs-visual.db --library examples/library --no-open --port 7979 &

# 3. Données : balayage, segmentation de INT-01, codes, passages, surlignages, attributs,
#    modèle de mémo, codage de phase 2, prompt, passe du modèle, cas
python3 web/e2e/seed.py http://127.0.0.1:7979

# 4. Contrôle
cd web && PLAYWRIGHT_BROWSERS_PATH=/opt/pw-browsers python3 e2e/visual.py --base http://127.0.0.1:7979 --out /tmp/cs-visual
```

Sans recompiler Rust, on peut contrôler les sources : lancer `CS_API=http://127.0.0.1:7979 pnpm dev` (Vite sur 5199, `/api` relayé vers le binaire) et passer `--base http://127.0.0.1:5199`.

Options : `--only coder-selection-menu,codebook` (voir `--list`), `--viewports 1024x700`, `--combos light/light,dark/system` (système/choix de l'application). Une exécution complète (26 écrans x 12) dure une vingtaine de minutes ; `--combos light/light,dark/dark` en divise le temps par trois.

`seed.py` suppose une base vide : relancer le serveur sur un nouveau fichier avant chaque amorçage.
