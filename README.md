<p align="center"><img src="assets/logo.jpg" alt="FreeOptical" width="640"></p>

<p align="center"><a href="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml"><img src="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml/badge.svg" alt="CI"></a> <a href="https://github.com/jbandrivet/freeoptical/releases/latest"><img src="https://img.shields.io/github/v/release/jbandrivet/freeoptical" alt="Dernière version"></a> <a href="#licence"><img src="https://img.shields.io/badge/licence-MIT%20%2F%20Apache--2.0-blue" alt="Licence"></a></p>

# FreeOptical

**Français** · [English](README.en.md) · [Deutsch](README.de.md)

**Logiciel libre de conception et de simulation optique séquentielle pour Linux : interface graphique, interface terminal et serveur MCP.**

FreeOptical permet de concevoir n'importe quel système optique (objectif, lunette, télescope à miroirs, montage de test…) surface par surface, puis de l'analyser et de l'optimiser. Il est écrit en **Rust** (`egui`, `ratatui`, `nalgebra`, `rmcp`) et combine :
1. **Un moteur de lancer de rayons séquentiel 3D** : calcul paraxial ABCD, lois de Snell-Descartes vectorielles, surfaces planes, sphériques, coniques et asphériques paires, miroirs, dispersion chromatique $n_d / V_d$, spot diagram polychromatique, aberration sphérique longitudinale, différence de marche (OPD) et front d'onde avec polynômes de Zernike, rapport de Strehl, PSF et FTM de diffraction, et optimisation Damped Least Squares.
2. **Une interface graphique et une interface terminal (`--tui`) en français, en anglais ou en allemand** : Éditeur de Données de Lentilles (LDE), coupe 2D avec lancer de rayons, vue 3D orientable, spot diagram, LSA, catalogue de verres, front d'onde, PSF et FTM. On part d'un système vierge (`Fichier` → `Nouveau système vierge`) ou de l'un des exemples fournis comme point de départ.
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
# Ou en important un fichier TDESIGN / LENSDES / RAYTRACE :
freeoptical --fichier RITCHRET.TDS
# Choisir la langue de l'interface :
freeoptical --langue fr
```

#### Raccourcis clavier dans l'interface terminal :
- **`Flèches` / `Tab`** : Naviguer dans l'Éditeur de Données de Lentilles (LDE)
- **`Entrée` / `E`** : Modifier la cellule sélectionnée (Rayon, Épaisseur, Verre, Demi-diamètre, Conique, Commentaire)
- **`F10` / `M`** : Ouvrir la barre de menu supérieure (`Fichier`, `Édition`, `Analyse`, `Outils`, `Langue`)
- **`F1` / `1`** : Vue 1 — Coupe Transversale 2D & Lancer de Rayons
- **`F2` / `2`** : Vue 2 — Projection 3D Isométrique Filaire (`H`/`L` : Lacet, `J`/`K` : Tangage)
- **`F3` / `3`** : Vue 3 — Spot Diagram Polychromatique & Disque d'Airy
- **`F4` / `4`** : Vue 4 — Aberration Sphérique Longitudinale (LSA)
- **`F5` / `5`** : Vue 5 — Catalogue de Verres (`BK7`, `F2`, `SF11`, `SILICE`, `LAK9`, `SK16`, `BAF10`, `FK51A`...)
- **`F6` / `6`** : Vue 6 — Front d'onde (OPD), coefficients de Zernike et Strehl
- **`F7` / `7`** : Vue 7 — Tache image de diffraction (PSF)
- **`F8` / `8`** : Vue 8 — FTM tangentielle et sagittale de tous les champs
- **`C`** / **`W`** : Champ suivant / longueur d'onde suivante pour les vues 6 à 8
- **`I` / `Ins`** : Insérer une surface avant la ligne sélectionnée
- **`A`** : Ajouter une surface avant le Plan Image
- **`Suppr` / `X`** : Supprimer la surface sélectionnée
- **`T`** : Basculer le type de surface (`PLANE` → `SPHERIQUE` → `CONIQUE` → `ASPHERIQUE`)
- **`S`** : Définir la surface sélectionnée comme Diaphragme (`STOP`)
- **`O`** : Optimiser automatiquement le système par **Damped Least Squares (DLS)**
- **`P`** : Mise au point paraxiale automatique (ajuste le tirage sur le `BFL`)
- **`F12`** : Changer de langue (français → anglais → allemand)
- **`Q`** : Quitter

#### Langue de l'interface
FreeOptical s'affiche en français, en anglais ou en allemand : menu `Langue` (fenêtre graphique et TUI), touche `F12` dans le TUI, ou option `--langue fr|en|de`. Le choix est enregistré dans `~/.config/freeoptical/langue` ; au premier démarrage, c'est la langue du système (`LANG`) qui est prise. Les noms des outils MCP, des champs JSON et des matériaux (`AIR`, `MIROIR`) sont les mêmes dans toutes les langues ; les descriptions des outils MCP et les messages suivent la langue choisie.

Dans la fenêtre graphique, le menu `Fichier` sauvegarde et recharge `systeme_optique.json` dans le répertoire courant. On peut aussi glisser un fichier `.json`, `.TDS`, `.RLD` ou `.DES` sur la fenêtre.
Le bouton **`Mise à jour`** (en bas à droite, ou menu `Outils`) cherche la dernière version publiée sur GitHub ; si elle est plus récente, il télécharge le paquet `.deb`, vérifie son empreinte SHA-256 et l'installe avec `apt-get` (le mot de passe administrateur est demandé par `pkexec`), puis propose de redémarrer FreeOptical.
Les coefficients asphériques A4, A6, A8, A10 de la surface sélectionnée s'éditent sur la ligne au-dessus du tableau LDE (glisser : 0,1 µm de flèche au bord par pixel ; clic : saisie directe, ex. `2.1e-7`) ; modifier un coefficient passe la surface en type `ASPHERIQUE`.

#### Analyses ondulatoires : front d'onde, PSF et FTM
Les vues `F6`, `F7` et `F8` (fenêtre graphique et TUI) s'appliquent au champ et à la longueur d'onde choisis (ou à toutes les longueurs d'onde pondérées).
- **Front d'onde** : différence de marche rapportée à la sphère de référence centrée sur l'image du rayon principal et passant par la pupille de sortie paraxiale (convention de Zemax), positive quand le chemin optique du rayon est plus long ; crête-à-creux, RMS, Strehl de Maréchal et 37 coefficients de Zernike (normalisation et ordre de Noll).
- **PSF** : figure de diffraction de Fraunhofer par FFT de la pupille, polychromatique ou à une longueur d'onde, échelle linéaire ou logarithmique ; le rapport de Strehl est le pic de la PSF rapporté à celui de la même pupille sans aberration. L'échantillonnage s'adapte à la taille de la tache pour éviter le repliement.
- **FTM** : module de la transformée de Fourier de la PSF, tangentielle et sagittale pour chaque champ, avec la limite de diffraction de la pupille.
- Vérifié par les tests : parabole parfaite (Strehl 1, FTM égale à la formule de la pupille circulaire), défocalisation (Strehl exact sinc²), aberration sphérique d'un miroir sphérique et coma d'une parabole conformes aux formules du 3e ordre, centre de gravité de la PSF égal à celui du spot diagram.

#### Import des fichiers TDESIGN, LENSDES et RAYTRACE
FreeOptical lit les fichiers de conception des programmes du livre *Telescope Optics: Evaluation and Design* (H. G. J. Rutten et M. A. M. van Venrooij, Willmann-Bell, 1988) : `.TDS` (TDESIGN, télescopes à miroirs et catadioptriques), `.RLD` (LENSDES, objectifs de lunette) et `.DES` (RAYTRACE). Menu `Fichier` → `Importer TDESIGN / LENSDES / RAYTRACE`, option `--fichier`, ou outil MCP `importer_teloptik`.
- Sont repris : sphères, coniques, asphères (coefficients en r² à r¹⁰), lames de Schmidt définies par leur puissance relative et leur zone neutre (profil du 3e ordre), miroirs (changement de signe de l'indice, y compris dans le verre d'un Mangin), décalages axiaux, indices N1/N2/N3 convertis en verre `nd:Vd`, diamètres limites quand le vignettage était activé dans RAYTRACE.
- Le plan image est placé au foyer paraxial et les demi-diamètres sont calculés pour laisser passer le faisceau sur l'axe et à 0,5° de champ.
- Les obstructions centrales sont ignorées (signalées) ; les systèmes basculés ou décentrés sont refusés, FreeOptical ne gérant pas encore les surfaces non centrées.
- La focale inscrite dans l'en-tête est comparée à la focale recalculée : elles coïncident pour les fichiers enregistrés par RAYTRACE ; les avant-projets de TDESIGN gardent la focale visée, sans la puissance du correcteur.

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
- `importer_teloptik(chemin)` : Importe un fichier `.TDS`, `.RLD` ou `.DES` (TDESIGN, LENSDES, RAYTRACE) comme système courant.
- `analyser_front_onde(champ_index, longueur_onde_nm)` : Front d'onde d'un champ : crête-à-creux, RMS, Strehl (Maréchal et PSF), 37 coefficients de Zernike.
- `calculer_ftm(champ_index, longueur_onde_nm, nb_frequences)` : FTM tangentielle et sagittale, limite de diffraction et Strehl de la PSF.

---

## Contribuer

FreeOptical est un projet ouvert : chacun peut l'améliorer. Signalez un bug ou proposez une idée dans les [issues](https://github.com/jbandrivet/freeoptical/issues), posez vos questions dans les [discussions](https://github.com/jbandrivet/freeoptical/discussions), ou proposez directement du code par pull request. Le guide [CONTRIBUTING.md](CONTRIBUTING.md) explique comment compiler, tester et proposer une modification.

---

## Licence

FreeOptical est distribué sous double licence, au choix :

- Licence Apache, version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- Licence MIT ([LICENSE-MIT](LICENSE-MIT))
