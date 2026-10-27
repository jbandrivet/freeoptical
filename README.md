# FreeOptical

**Logiciel libre de conception et de simulation optique séquentielle pour Linux : interface graphique, interface terminal et serveur MCP.**

FreeOptical permet de concevoir n'importe quel système optique (objectif, lunette, télescope à miroirs, montage de test…) surface par surface, puis de l'analyser et de l'optimiser. Il est écrit en **Rust** (`egui`, `ratatui`, `nalgebra`, `rmcp`) et combine :
1. **Un moteur de lancer de rayons séquentiel 3D** : calcul paraxial ABCD, lois de Snell-Descartes vectorielles, surfaces planes, sphériques, coniques et asphériques paires, miroirs, dispersion chromatique $n_d / V_d$, spot diagram polychromatique, aberration sphérique longitudinale et optimisation Damped Least Squares.
2. **Une interface graphique et une interface terminal (`--tui`) en français** : Éditeur de Données de Lentilles (LDE), coupe 2D avec lancer de rayons, vue 3D orientable, spot diagram, LSA et catalogue de verres. On part d'un système vierge (`Fichier` → `Nouveau système vierge`) ou de l'un des exemples fournis comme point de départ.
3. **Un serveur Model Context Protocol (MCP) intégré** permettant à un agent IA de concevoir, simuler et optimiser des systèmes optiques.

---

## Installation (Debian / Ubuntu, amd64)

Télécharger le paquet `.deb` de la [dernière version](https://github.com/jbandrivet/freeoptical/releases/latest), puis :
```bash
sudo apt install ./freeoptical_*_amd64.deb
```
Les versions suivantes s'installent ensuite depuis le bouton **Mise à jour** de la fenêtre graphique.

Depuis les sources : `cargo build --release` ; le paquet se construit avec `./packaging/make-deb.sh`.

---

## Utilisation

### 1. Mode Exécutable Graphique Autonome (ou TUI `--tui`)
```bash
freeoptical
# Ou en chargeant un fichier système JSON :
freeoptical --fichier systeme_optique.json
```

#### Raccourcis clavier dans l'interface :
- **`Flèches` / `Tab`** : Naviguer dans l'Éditeur de Données de Lentilles (LDE)
- **`Entrée` / `E`** : Modifier la cellule sélectionnée (Rayon, Épaisseur, Verre, Demi-diamètre, Conique, Commentaire)
- **`F10` / `M`** : Ouvrir la barre de menu supérieure (`Fichier`, `Édition`, `Analyse`, `Outils`)
- **`F1` / `1`** : Vue 1 — Coupe Transversale 2D & Lancer de Rayons
- **`F2` / `2`** : Vue 2 — Projection 3D Isométrique Filaire (`H`/`L` : Lacet, `J`/`K` : Tangage)
- **`F3` / `3`** : Vue 3 — Spot Diagram Polychromatique & Disque d'Airy
- **`F4` / `4`** : Vue 4 — Aberration Sphérique Longitudinale (LSA)
- **`F5` / `5`** : Vue 5 — Catalogue de Verres (`BK7`, `F2`, `SF11`, `SILICE`, `LAK9`, `SK16`, `BAF10`, `FK51A`...)
- **`I` / `Ins`** : Insérer une surface avant la ligne sélectionnée
- **`A`** : Ajouter une surface avant le Plan Image
- **`Suppr` / `X`** : Supprimer la surface sélectionnée
- **`T`** : Basculer le type de surface (`PLANE` → `SPHERIQUE` → `CONIQUE` → `ASPHERIQUE`)
- **`S`** : Définir la surface sélectionnée comme Diaphragme (`STOP`)
- **`O`** : Optimiser automatiquement le système par **Damped Least Squares (DLS)**
- **`P`** : Mise au point paraxiale automatique (ajuste le tirage sur le `BFL`)
- **`Q`** : Quitter

Dans la fenêtre graphique, le menu `Fichier` sauvegarde et recharge `systeme_optique.json` dans le répertoire courant.
Le bouton **`⟳ Mise à jour`** (en bas à droite, ou menu `Outils`) cherche la dernière version publiée sur GitHub ; si elle est plus récente, il télécharge le paquet `.deb`, vérifie son empreinte SHA-256 et l'installe avec `apt-get` (le mot de passe administrateur est demandé par `pkexec`), puis propose de redémarrer FreeOptical.
Les coefficients asphériques A4, A6, A8, A10 de la surface sélectionnée s'éditent sur la ligne au-dessus du tableau LDE (glisser : 0,1 µm de flèche au bord par pixel ; clic : saisie directe, ex. `2.1e-7`) ; modifier un coefficient passe la surface en type `ASPHERIQUE`.

#### Conventions optiques
- **Diaphragme (`STOP`)** : la pupille d'entrée est l'image paraxiale du diaphragme ; les rayons hors axe sont visés au centre de cette pupille.
- **Miroirs** (verre `MIROIR`) : convention Zemax, les épaisseurs qui suivent un miroir sont négatives (la lumière repart vers -Z). La focale d'un miroir concave est positive ; le tirage (BFL) est compté le long de Z.
- **Ouverture** : `PupilleEntreeMm`, `NombreOuverture` (F/# = EFFL / EPD) ou `OuvertureNumeriqueObjet` (objet à l'infini : rapportée à l'espace image, F/# = 1 / 2NA).
- **Verres** : un nom inconnu du catalogue est traité comme de l'air (un avertissement s'affiche dans le TUI et le MCP) ; syntaxe libre `nd:Vd`, ex. `1.5168:64.17`.
- **Fichiers JSON** : seuls `nom` et `surfaces` sont obligatoires (catalogue, longueurs d'onde, champs, conique, etc. prennent des valeurs par défaut).

---

### 2. Mode Serveur MCP (Model Context Protocol)
```bash
freeoptical --mcp
```

#### Outils MCP exposés sur `stdio` :
- `creer_systeme(nom, longueur_onde_ref)` : Initialise un nouveau design optique.
- `ajouter_surface(type, rayon, epaisseur, verre, semi_diametre)` : Ajoute un dioptre ou un miroir.
- `modifier_surface(index, parametres)` : Modifie les propriétés d'une surface existante.
- `lancer_simulation()` : Exécute le lancer de rayons séquentiel et retourne les données paraxiales, les rayons 3D et la LSA.
- `obtenir_spot_diagram()` : Retourne les coordonnées des impacts au plan image, le centroïde, le rayon RMS et le rayon d'Airy.
- `optimiser_systeme(variables, cibles)` : Exécute l'optimiseur Damped Least Squares (DLS).

---

## Licence

FreeOptical est distribué sous double licence, au choix :

- Licence Apache, version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- Licence MIT ([LICENSE-MIT](LICENSE-MIT))
