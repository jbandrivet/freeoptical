<p align="center"><img src="assets/logo.jpg" alt="FreeOptical" width="640"></p>

<p align="center"><a href="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml"><img src="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml/badge.svg" alt="CI"></a> <a href="https://github.com/jbandrivet/freeoptical/releases/latest"><img src="https://img.shields.io/github/v/release/jbandrivet/freeoptical" alt="Neueste Version"></a> <a href="#lizenz"><img src="https://img.shields.io/badge/Lizenz-MIT%20%2F%20Apache--2.0-blue" alt="Lizenz"></a></p>

# FreeOptical

[Français](README.md) · [English](README.en.md) · **Deutsch** · **Webseite: [jbandrivet.github.io/freeoptical](https://jbandrivet.github.io/freeoptical/de/)**

**Freie Software für sequentielles Optikdesign und Simulation unter Linux: grafische Oberfläche, Terminal-Oberfläche und MCP-Server.**

Mit FreeOptical lässt sich jedes optische System (Objektiv, Refraktor, Spiegelteleskop, Prüfaufbau…) Fläche für Fläche entwerfen, analysieren und optimieren. Es ist in **Rust** geschrieben (`egui`, `ratatui`, `nalgebra`, `rmcp`) und vereint:
1. **Eine sequentielle 3D-Strahlverfolgung**: paraxiale ABCD-Rechnung, vektorielles Brechungsgesetz, plane, sphärische, konische und gerade asphärische Flächen, Spiegel, gekippte und dezentrierte Flächen (gefaltete Systeme), zentrale Obstruktionen, Beugungsgitter, echte Gläser (Sellmeier-Formeln, Zemax-Kataloge `.agf`: Schott, Ohara, CDGM…), polychromatisches Spotdiagramm, longitudinale sphärische Aberration, optische Wegdifferenz (OPD) und Wellenfront mit Zernike-Polynomen, Strehl-Verhältnis, Beugungs-PSF und MTF, Strahlfächer, Bildfeldwölbung, Verzeichnung, Seidel-Koeffizienten sowie Optimierung nach Damped Least Squares über alle Felder und Farben.
2. **Eine grafische Oberfläche und eine Terminal-Oberfläche (`--tui`) auf Deutsch, Englisch oder Französisch**: Linsendaten-Editor (LDE), 2D-Schnitt mit Strahlverfolgung, drehbare 3D-Ansicht, Spotdiagramm, LSA, Glaskatalog, Wellenfront, PSF und MTF. Man beginnt mit einem leeren System (`Datei` → `Neues leeres System`) oder mit einem der mitgelieferten Beispiele.
3. **Einen eingebauten Model-Context-Protocol-Server (MCP)**, mit dem ein KI-Agent optische Systeme entwerfen, simulieren und optimieren kann.

---

## Installation (Debian / Ubuntu, amd64)

Das `.deb`-Paket der [neuesten Version](https://github.com/jbandrivet/freeoptical/releases/latest) herunterladen, dann:
```bash
sudo apt install ./freeoptical_*_amd64.deb
```
Spätere Versionen werden über die Schaltfläche **Update** im grafischen Fenster installiert.

Aus den Quellen: `cargo build --release`; das Paket wird mit `./packaging/make-deb.sh` gebaut.

---

## Verwendung

### 1. Grafische Anwendung (oder TUI mit `--tui`)
```bash
freeoptical
# Oder eine JSON-Systemdatei laden:
freeoptical --fichier systeme_optique.json
# Oder eine TDESIGN- / LENSDES- / RAYTRACE-Datei importieren:
freeoptical --fichier RITCHRET.TDS
# Sprache der Oberfläche wählen:
freeoptical --langue de
```

#### Tastenkürzel in der Terminal-Oberfläche:
- **`Pfeile` / `Tab`**: im Linsendaten-Editor (LDE) bewegen
- **`Eingabe` / `E`**: ausgewählte Zelle bearbeiten (Radius, Dicke, Glas, Halbdurchmesser, Konik, Kommentar)
- **`F10` / `M`**: obere Menüleiste öffnen (`Datei`, `Bearbeiten`, `Analyse`, `Werkzeuge`, `Sprache`)
- **`F1` / `1`**: Ansicht 1 — 2D-Schnitt & Strahlverfolgung
- **`F2` / `2`**: Ansicht 2 — isometrische 3D-Drahtansicht (`H`/`L`: Gieren, `J`/`K`: Nicken)
- **`F3` / `3`**: Ansicht 3 — polychromatisches Spotdiagramm & Airy-Scheibchen
- **`F4` / `4`**: Ansicht 4 — longitudinale sphärische Aberration (LSA)
- **`F5` / `5`**: Ansicht 5 — Glaskatalog (`BK7`, `F2`, `SF11`, `SILICE`, `LAK9`, `SK16`, `BAF10`, `FK51A`...)
- **`F6` / `6`**: Ansicht 6 — Wellenfront (OPD), Zernike-Koeffizienten und Strehl
- **`F7` / `7`**: Ansicht 7 — Beugungs-Punktbildfunktion (PSF)
- **`F8` / `8`**: Ansicht 8 — tangentiale und sagittale MTF aller Felder
- **`9`**: Strahlfächer (Queraberrationen); **`0`**: Bildfeldwölbung und Verzeichnung; Seidel-Koeffizienten über das Menü `Analyse`
- **`C`** / **`W`**: nächstes Feld / nächste Wellenlänge für die Analyseansichten
- **`I` / `Einfg`**: Fläche vor der ausgewählten Zeile einfügen
- **`A`**: Fläche vor der Bildebene hinzufügen
- **`Entf` / `X`**: ausgewählte Fläche löschen
- **`T`**: Flächentyp wechseln (`PLAN` → `SPHÄRISCH` → `KONISCH` → `ASPHÄRISCH`)
- **`S`**: ausgewählte Fläche als Aperturblende setzen (`STOP`)
- **`O`**: System mit **Damped Least Squares (DLS)** optimieren
- **`P`**: automatische paraxiale Fokussierung (setzt die Schnittweite auf die `BFL`)
- **`F12`**: Sprache wechseln (Französisch → Englisch → Deutsch)
- **`Q`**: beenden

#### Sprache der Oberfläche
FreeOptical gibt es auf Französisch, Englisch und Deutsch: Menü `Sprache` (grafisches Fenster und TUI), Taste `F12` in der TUI oder Option `--langue fr|en|de`. Die Wahl wird in `~/.config/freeoptical/langue` gespeichert; beim ersten Start gilt die Systemsprache (`LANG`). Die Namen der MCP-Werkzeuge, der JSON-Felder und der Materialien (`AIR`, `MIROIR`) sind in allen Sprachen gleich; die Beschreibungen der MCP-Werkzeuge und die Meldungen folgen der gewählten Sprache.

Im grafischen Fenster speichert und lädt das Menü `Datei` die Datei `systeme_optique.json` im aktuellen Verzeichnis. Eine `.json`-, `.TDS`-, `.RLD`- oder `.DES`-Datei kann auch auf das Fenster gezogen werden.
Die Schaltfläche **`Update`** (unten rechts oder Menü `Werkzeuge`) sucht die neueste auf GitHub veröffentlichte Version; ist sie neuer, lädt sie das `.deb`-Paket herunter, prüft dessen SHA-256-Prüfsumme, installiert es mit `apt-get` (das Administratorpasswort wird von `pkexec` abgefragt) und bietet einen Neustart von FreeOptical an.
Die Asphärenkoeffizienten A4, A6, A8, A10 der ausgewählten Fläche werden in der Zeile über der LDE-Tabelle bearbeitet (Ziehen: 0,1 µm Pfeilhöhe am Rand pro Pixel; Klicken: direkte Eingabe, z. B. `2.1e-7`); das Ändern eines Koeffizienten schaltet die Fläche auf den Typ `ASPHÄRISCH`.

#### Grafisches Fenster: Ansichten und Einstellungen
- Reiter **F1** bis **F11** unter dem Editor: 2D-Schnitt, 3D-Ansicht, Spot, LSA, Gläser, Wellenfront, PSF, MTF, **Strahlfächer** (Queraberrationen und optische Wegdifferenz im tangentialen und sagittalen Schnitt), **Bildfeldwölbung und Verzeichnung**, **Seidel-Koeffizienten** je Fläche.
- Unter den Asphärenkoeffizienten stellt die Zeile der ausgewählten Fläche ihre **Dezentrierung** (X, Y), ihre **Kippung** (um X, dann um Y, in Grad, um den Scheitel), den Radius ihrer **zentralen Obstruktion** und ein **Beugungsgitter** (Linien pro mm, Ordnung) ein.
- Die Schaltfläche **DLS optimieren** öffnet die Gütefunktion: Variablen (Radien, Linsendicken, Luftabstände, Schnittweite, Kegelschnitte, Asphären A4/A6), Kriterium (RMS-Spot oder RMS-Wellenfront über alle Felder und Wellenlängen), Zielbrennweite und Nebenbedingungen (Rand- und Mittendicken, Baulänge, Verzeichnung).

#### Gläser
Die eingebauten Gläser (N-BK7, N-K5, F2, SF2, N-SF5, N-SF6, N-SF11, N-LAK9, N-SK16, N-BAF10, N-FK51A, N-PK52A, Quarzglas, Flussspat CaF2) folgen ihren Sellmeier-Formeln; die alten Namen (BK7, SF11…) bleiben gültig. Zemax-Kataloge **`.agf`** (Schott, Ohara, CDGM, Sumita…, ANSI oder UTF-16) werden über das Menü `Datei`, per Drag-and-drop, mit `--catalogue datei.agf` oder mit dem MCP-Werkzeug `importer_catalogue_agf` importiert; sie werden in `~/.config/freeoptical/verres` aufbewahrt und bei jedem Start geladen. Die Syntax `nd:Vd` bleibt für ein Modellglas verfügbar.

#### Gekippte Flächen, Obstruktionen und Gitter
- Jede Fläche wird an der durch die Dicken gegebenen axialen Position platziert, dann um ihren Scheitel dezentriert und gekippt; die Strahlverfolgung ist in 3D exakt (Newton mit 45°-Fangspiegel, Schiefspiegler, Spektrograph…). Die paraxialen Rechnungen (Brennweite, Pupillen, Seidel) ignorieren Kippungen und Gitter.
- Eine zentrale Obstruktion hält die Strahlen auf, die näher als ihr Radius an der Flächenachse vorbeigehen; Spot, Wellenfront, PSF und MTF berücksichtigen sie (der Referenz-Hauptstrahl wird ohne Obstruktion verfolgt).
- Beugungsgitter: vektorielle Gittergleichung, Linien parallel zur lokalen X-Achse der Fläche, eine positive Ordnung lenkt das Licht nach lokal +Y ab; in Reflexion auf einem Spiegel, sonst in Transmission.

#### Wellenoptische Analysen: Wellenfront, PSF und MTF
Die Ansichten `F6`, `F7` und `F8` (grafisches Fenster und TUI) gelten für das gewählte Feld und die gewählte Wellenlänge (oder alle gewichteten Wellenlängen).
- **Wellenfront**: optische Wegdifferenz bezogen auf die Referenzkugel um das Bild des Hauptstrahls durch die paraxiale Austrittspupille (Zemax-Konvention), positiv, wenn der optische Weg des Strahls länger ist; Spitze-Tal, RMS, Strehl nach Maréchal und 37 Zernike-Koeffizienten (Normierung und Reihenfolge nach Noll).
- **PSF**: Fraunhofer-Beugungsbild per FFT der Pupille, polychromatisch oder bei einer Wellenlänge, lineare oder logarithmische Skala; das Strehl-Verhältnis ist das PSF-Maximum bezogen auf dieselbe Pupille ohne Aberration. Die Abtastung passt sich der Spotgröße an, um Aliasing zu vermeiden.
- **MTF**: Betrag der Fourier-Transformierten der PSF, tangential und sagittal für jedes Feld, mit der Beugungsgrenze der Pupille.
- Durch Tests geprüft: perfekter Paraboloid (Strehl 1, MTF gleich der Formel der Kreispupille), Defokus (exakter sinc²-Strehl), sphärische Aberration eines Kugelspiegels und Koma eines Paraboloids gemäß den Formeln dritter Ordnung, Schwerpunkt der PSF gleich dem des Spotdiagramms.

#### Import von TDESIGN-, LENSDES- und RAYTRACE-Dateien
FreeOptical liest die Designdateien der Programme aus dem Buch *Telescope Optics: Evaluation and Design* (H. G. J. Rutten und M. A. M. van Venrooij, Willmann-Bell, 1988): `.TDS` (TDESIGN, Spiegel- und katadioptrische Teleskope), `.RLD` (LENSDES, Refraktorobjektive) und `.DES` (RAYTRACE). Menü `Datei` → `TDESIGN / LENSDES / RAYTRACE importieren`, Option `--fichier` oder MCP-Werkzeug `importer_teloptik`.
- Übernommen werden: Sphären, Kegelschnitte, Asphären (Koeffizienten r² bis r¹⁰), Schmidtplatten aus relativer Stärke und neutraler Zone (Profil dritter Ordnung), Spiegel (Vorzeichenwechsel des Index, auch im Glas eines Mangin-Spiegels), axiale Verschiebungen, Indizes N1/N2/N3 als Gläser `nd:Vd`, Grenzdurchmesser, wenn in RAYTRACE die Vignettierung aktiv war.
- Die Bildebene liegt im paraxialen Fokus; die Halbdurchmesser werden so berechnet, dass das Bündel auf der Achse und bei 0,5° Feld durchgeht.
- Zentrale Obstruktionen werden übernommen, wenn in RAYTRACE die Vignettierung aktiv war; gekippte oder dezentrierte Dateien werden abgelehnt, da die Platzierungskonvention von RAYTRACE nicht sicher bekannt ist.
- Die Brennweite im Dateikopf wird mit der neu berechneten verglichen: bei von RAYTRACE gespeicherten Dateien stimmen sie überein; TDESIGN-Vorentwürfe enthalten die Zielbrennweite ohne die Stärke des Korrektors.

#### Optische Konventionen
- **Aperturblende (`STOP`)**: die Eintrittspupille ist das paraxiale Bild der Blende; außeraxiale Strahlen zielen auf die Mitte dieser Pupille.
- **Spiegel** (Glas `MIROIR`): Zemax-Konvention, Dicken nach einem Spiegel sind negativ (das Licht läuft zurück nach -Z). Ein Hohlspiegel hat eine positive Brennweite; die Schnittweite (BFL) wird entlang Z gezählt.
- **Öffnung**: `PupilleEntreeMm`, `NombreOuverture` (F/# = EFFL / EPD) oder `OuvertureNumeriqueObjet` (Objekt im Unendlichen: auf den Bildraum bezogen, F/# = 1 / 2NA).
- **Gläser**: ein im Katalog unbekannter Name wird als Luft behandelt (Warnung in der TUI und im MCP); freie Syntax `nd:Vd`, z. B. `1.5168:64.17`.
- **JSON-Dateien**: nur `nom` und `surfaces` sind Pflicht (Katalog, Wellenlängen, Felder, Konik usw. erhalten Standardwerte).

---

### 2. MCP-Server (Model Context Protocol)
```bash
freeoptical --mcp
```

#### Über `stdio` bereitgestellte MCP-Werkzeuge:
- `creer_systeme(nom, longueur_onde_ref)`: legt ein neues optisches Design an.
- `ajouter_surface(type, rayon, epaisseur, verre, semi_diametre)`: fügt eine brechende Fläche oder einen Spiegel hinzu.
- `modifier_surface(index, parametres)`: ändert die Eigenschaften einer vorhandenen Fläche.
- `lancer_simulation()`: führt die sequentielle Strahlverfolgung aus und liefert paraxiale Daten, 3D-Strahlen und LSA.
- `obtenir_spot_diagram()`: liefert die Auftreffpunkte in der Bildebene, den Schwerpunkt, den RMS-Radius und den Airy-Radius.
- `optimiser_systeme(variables, cibles)`: startet die Optimierung nach Damped Least Squares (DLS).
- `importer_teloptik(chemin)`: importiert eine `.TDS`-, `.RLD`- oder `.DES`-Datei (TDESIGN, LENSDES, RAYTRACE) als aktuelles System.
- `analyser_front_onde(champ_index, longueur_onde_nm)`: Wellenfront eines Feldes: Spitze-Tal, RMS, Strehl (Maréchal und PSF), 37 Zernike-Koeffizienten.
- `calculer_ftm(champ_index, longueur_onde_nm, nb_frequences)`: tangentiale und sagittale MTF, Beugungsgrenze und Strehl der PSF.
- `analyser_aberrations(champ_index, longueur_onde_nm)`: Seidel-Koeffizienten je Fläche, Bildfeldwölbung und Verzeichnung, Strahlfächer.
- `importer_catalogue_agf(chemin)`: importiert einen Zemax-Glaskatalog `.agf`.
- `modifier_surface` akzeptiert auch `decentrement`, `inclinaison_deg`, `rayon_obstruction`, `reseau_traits_mm` und `reseau_ordre`; `optimiser_systeme` akzeptiert die Variablen `a4` bis `a10` und die Ziele `spot_rms_champs`, `opd_rms_champs`, `longueur_totale_max`, `distorsion_max`, `epaisseur_centre_min`, `epaisseur_bord_min`.

---

## Mitmachen

FreeOptical ist ein offenes Projekt: jeder kann es verbessern. Fehler melden oder Ideen vorschlagen in den [Issues](https://github.com/jbandrivet/freeoptical/issues), Fragen stellen in den [Discussions](https://github.com/jbandrivet/freeoptical/discussions) oder Code per Pull-Request einreichen. Der Leitfaden [CONTRIBUTING.md](CONTRIBUTING.md) (auf Französisch) erklärt Bauen, Testen und Einreichen; Diskussionen sind auch auf Englisch möglich.

---

## Lizenz

FreeOptical steht wahlweise unter einer der beiden Lizenzen:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))
