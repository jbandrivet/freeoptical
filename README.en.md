<p align="center"><img src="assets/logo.jpg" alt="FreeOptical" width="640"></p>

<p align="center"><a href="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml"><img src="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml/badge.svg" alt="CI"></a> <a href="https://github.com/jbandrivet/freeoptical/releases/latest"><img src="https://img.shields.io/github/v/release/jbandrivet/freeoptical" alt="Latest release"></a> <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="License"></a></p>

# FreeOptical

[Français](README.md) · **English** · [Deutsch](README.de.md)

**Free sequential optical design and simulation software for Linux: graphical interface, terminal interface and MCP server.**

FreeOptical lets you design any optical system (objective, refractor, mirror telescope, test setup…) surface by surface, then analyse and optimize it. It is written in **Rust** (`egui`, `ratatui`, `nalgebra`, `rmcp`) and combines:
1. **A 3D sequential ray tracing engine**: ABCD paraxial calculation, vector Snell's law, flat, spherical, conic and even aspheric surfaces, mirrors, $n_d / V_d$ chromatic dispersion, polychromatic spot diagram, longitudinal spherical aberration and Damped Least Squares optimization.
2. **A graphical interface and a terminal interface (`--tui`) in English, French or German**: Lens Data Editor (LDE), 2D cross section with ray trace, rotatable 3D view, spot diagram, LSA and glass catalogue. Start from an empty system (`File` → `New empty system`) or from one of the bundled examples.
3. **A built-in Model Context Protocol (MCP) server** letting an AI agent design, simulate and optimize optical systems.

---

## Installation (Debian / Ubuntu, amd64)

Download the `.deb` package of the [latest release](https://github.com/jbandrivet/freeoptical/releases/latest), then:
```bash
sudo apt install ./freeoptical_*_amd64.deb
```
Later versions install from the **Update** button of the graphical window.

From source: `cargo build --release`; the package is built with `./packaging/make-deb.sh`.

---

## Usage

### 1. Graphical application (or TUI with `--tui`)
```bash
freeoptical
# Or load a JSON system file:
freeoptical --fichier systeme_optique.json
# Or import a TDESIGN / LENSDES / RAYTRACE file:
freeoptical --fichier RITCHRET.TDS
# Choose the interface language:
freeoptical --langue en
```

#### Keyboard shortcuts in the terminal interface:
- **`Arrows` / `Tab`**: move in the Lens Data Editor (LDE)
- **`Enter` / `E`**: edit the selected cell (radius, thickness, glass, semi-diameter, conic, comment)
- **`F10` / `M`**: open the top menu bar (`File`, `Edit`, `Analysis`, `Tools`, `Language`)
- **`F1` / `1`**: view 1 — 2D cross section & ray trace
- **`F2` / `2`**: view 2 — 3D isometric wireframe (`H`/`L`: yaw, `J`/`K`: pitch)
- **`F3` / `3`**: view 3 — polychromatic spot diagram & Airy disk
- **`F4` / `4`**: view 4 — longitudinal spherical aberration (LSA)
- **`F5` / `5`**: view 5 — glass catalogue (`BK7`, `F2`, `SF11`, `SILICE`, `LAK9`, `SK16`, `BAF10`, `FK51A`...)
- **`I` / `Ins`**: insert a surface before the selected row
- **`A`**: add a surface before the image plane
- **`Del` / `X`**: delete the selected surface
- **`T`**: cycle the surface type (`FLAT` → `SPHERICAL` → `CONIC` → `ASPHERIC`)
- **`S`**: set the selected surface as aperture stop (`STOP`)
- **`O`**: optimize the system with **Damped Least Squares (DLS)**
- **`P`**: automatic paraxial focus (sets the back focus to the `BFL`)
- **`F6`**: switch language (French → English → German)
- **`Q`**: quit

#### Interface language
FreeOptical is available in French, English and German: `Language` menu (graphical window and TUI), `F6` key in the TUI, or `--langue fr|en|de` option. The choice is saved in `~/.config/freeoptical/langue`; on first start the system language (`LANG`) is used. MCP tool names, JSON field names and material names (`AIR`, `MIROIR`) are the same in every language; MCP tool descriptions and messages follow the chosen language.

In the graphical window, the `File` menu saves and reloads `systeme_optique.json` in the current directory. You can also drop a `.json`, `.TDS`, `.RLD` or `.DES` file onto the window.
The **`Update`** button (bottom right, or `Tools` menu) looks for the latest version published on GitHub; if it is newer, it downloads the `.deb` package, checks its SHA-256 digest and installs it with `apt-get` (the administrator password is asked by `pkexec`), then offers to restart FreeOptical.
The A4, A6, A8, A10 aspheric coefficients of the selected surface are edited on the line above the LDE table (drag: 0.1 µm of edge sag per pixel; click: type a value, e.g. `2.1e-7`); changing a coefficient switches the surface to the `ASPHERIC` type.

#### Importing TDESIGN, LENSDES and RAYTRACE files
FreeOptical reads the design files of the programs from the book *Telescope Optics: Evaluation and Design* (H. G. J. Rutten and M. A. M. van Venrooij, Willmann-Bell, 1988): `.TDS` (TDESIGN, mirror and catadioptric telescopes), `.RLD` (LENSDES, refractor objectives) and `.DES` (RAYTRACE). `File` menu → `Import TDESIGN / LENSDES / RAYTRACE`, `--fichier` option, or `importer_teloptik` MCP tool.
- Imported: spheres, conics, aspheres (r² to r¹⁰ coefficients), Schmidt plates defined by their relative power and neutral zone (third-order profile), mirrors (sign change of the index, including inside the glass of a Mangin), axial shifts, N1/N2/N3 indices converted to `nd:Vd` glasses, limiting diameters when vignetting was enabled in RAYTRACE.
- The image plane is placed at the paraxial focus and the semi-diameters are computed to pass the beam on axis and at 0.5° field.
- Central obstructions are ignored (and reported); tilted or decentered systems are refused, as FreeOptical does not handle non-centered surfaces yet.
- The focal length in the header is compared with the recomputed one: they match for files saved by RAYTRACE; TDESIGN pre-designs keep the target focal length, without the corrector power.

#### Optical conventions
- **Aperture stop (`STOP`)**: the entrance pupil is the paraxial image of the stop; off-axis rays are aimed at the centre of this pupil.
- **Mirrors** (glass `MIROIR`): Zemax convention, thicknesses after a mirror are negative (light travels back towards -Z). A concave mirror has a positive focal length; the back focus (BFL) is measured along Z.
- **Aperture**: `PupilleEntreeMm`, `NombreOuverture` (F/# = EFFL / EPD) or `OuvertureNumeriqueObjet` (object at infinity: referred to image space, F/# = 1 / 2NA).
- **Glasses**: a name unknown to the catalogue is treated as air (a warning is shown in the TUI and the MCP); free `nd:Vd` syntax, e.g. `1.5168:64.17`.
- **JSON files**: only `nom` and `surfaces` are required (catalogue, wavelengths, fields, conic, etc. take default values).

---

### 2. MCP server (Model Context Protocol)
```bash
freeoptical --mcp
```

#### MCP tools exposed on `stdio`:
- `creer_systeme(nom, longueur_onde_ref)`: creates a new optical design.
- `ajouter_surface(type, rayon, epaisseur, verre, semi_diametre)`: adds a refracting surface or a mirror.
- `modifier_surface(index, parametres)`: modifies the properties of an existing surface.
- `lancer_simulation()`: runs the sequential ray trace and returns the paraxial data, the 3D rays and the LSA.
- `obtenir_spot_diagram()`: returns the image plane hits, the centroid, the RMS radius and the Airy radius.
- `optimiser_systeme(variables, cibles)`: runs the Damped Least Squares (DLS) optimizer.
- `importer_teloptik(chemin)`: imports a `.TDS`, `.RLD` or `.DES` file (TDESIGN, LENSDES, RAYTRACE) as the current system.

---

## Contributing

FreeOptical is an open project: anyone can improve it. Report a bug or suggest an idea in the [issues](https://github.com/jbandrivet/freeoptical/issues), ask questions in the [discussions](https://github.com/jbandrivet/freeoptical/discussions), or send code as a pull request. The [CONTRIBUTING.md](CONTRIBUTING.md) guide (in French) explains how to build, test and propose a change; discussions can be held in English.

---

## License

FreeOptical is dual-licensed, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))
