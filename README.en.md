<p align="center"><img src="assets/logo.jpg" alt="FreeOptical" width="640"></p>

<p align="center"><a href="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml"><img src="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml/badge.svg" alt="CI"></a> <a href="https://github.com/jbandrivet/freeoptical/releases/latest"><img src="https://img.shields.io/github/v/release/jbandrivet/freeoptical" alt="Latest release"></a> <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="License"></a></p>

# FreeOptical

[Français](README.md) · **English** · [Deutsch](README.de.md) · **Website: [jbandrivet.github.io/freeoptical](https://jbandrivet.github.io/freeoptical/en/)**

**Free sequential optical design and simulation software for Linux: graphical interface, terminal interface and MCP server.**

FreeOptical lets you design any optical system (objective, refractor, mirror telescope, test setup…) surface by surface, then analyse and optimize it. It is written in **Rust** (`egui`, `ratatui`, `nalgebra`, `rmcp`) and combines:
1. **A 3D sequential ray tracing engine**: ABCD paraxial calculation, vector Snell's law, flat, spherical, conic and even aspheric surfaces, mirrors, tilted and decentered surfaces (folded systems), central obstructions, diffraction gratings, real glasses (Sellmeier formulas, Zemax `.agf` catalogues: Schott, Ohara, CDGM…), polychromatic spot diagram, longitudinal spherical aberration, optical path difference (OPD) and wavefront with Zernike polynomials, Strehl ratio, diffraction PSF and MTF, ray fans, field curvature, distortion, Seidel coefficients, and Damped Least Squares optimization over all fields and colours.
2. **A graphical interface and a terminal interface (`--tui`) in English, French or German**: Lens Data Editor (LDE), 2D cross section with ray trace, rotatable 3D view, spot diagram, LSA, glass catalogue, wavefront, PSF and MTF. Start from an empty system (`File` → `New empty system`) or from one of the bundled examples.
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
- **`F6` / `6`**: view 6 — wavefront (OPD), Zernike coefficients and Strehl
- **`F7` / `7`**: view 7 — diffraction point spread function (PSF)
- **`F8` / `8`**: view 8 — tangential and sagittal MTF of all fields
- **`9`**: ray fans (transverse aberrations); **`0`**: field curvature and distortion; Seidel coefficients from the `Analysis` menu
- **`C`** / **`W`**: next field / next wavelength for the analysis views
- **`I` / `Ins`**: insert a surface before the selected row
- **`A`**: add a surface before the image plane
- **`Del` / `X`**: delete the selected surface
- **`T`**: cycle the surface type (`FLAT` → `SPHERICAL` → `CONIC` → `ASPHERIC`)
- **`S`**: set the selected surface as aperture stop (`STOP`)
- **`O`**: optimize the system with **Damped Least Squares (DLS)**
- **`P`**: automatic paraxial focus (sets the back focus to the `BFL`)
- **`F12`**: switch language (French → English → German)
- **`Q`**: quit

#### Interface language
FreeOptical is available in French, English and German: `Language` menu (graphical window and TUI), `F12` key in the TUI, or `--langue fr|en|de` option. The choice is saved in `~/.config/freeoptical/langue`; on first start the system language (`LANG`) is used. MCP tool names, JSON field names and material names (`AIR`, `MIROIR`) are the same in every language; MCP tool descriptions and messages follow the chosen language.

In the graphical window, the `File` menu saves and reloads `systeme_optique.json` in the current directory. You can also drop a `.json`, `.TDS`, `.RLD` or `.DES` file onto the window.
The **`Update`** button (bottom right, or `Tools` menu) looks for the latest version published on GitHub; if it is newer, it downloads the `.deb` package, checks its SHA-256 digest and installs it with `apt-get` (the administrator password is asked by `pkexec`), then offers to restart FreeOptical.
The A4, A6, A8, A10 aspheric coefficients of the selected surface are edited on the line above the LDE table (drag: 0.1 µm of edge sag per pixel; click: type a value, e.g. `2.1e-7`); changing a coefficient switches the surface to the `ASPHERIC` type.

#### Graphical window: views and settings
- Tabs **F1** to **F11** below the editor: 2D section, 3D view, spot, LSA, glasses, wavefront, PSF, MTF, **ray fans** (transverse aberrations and optical path difference along the tangential and sagittal sections), **field curvature and distortion**, **Seidel coefficients** per surface.
- Below the aspheric coefficients, the line of the selected surface sets its **decenter** (X, Y), its **tilt** (about X then Y, in degrees, about the vertex), its **central obstruction** radius and a **diffraction grating** (lines per mm, order).
- The **Optimize DLS** button opens the merit function: variables (radii, lens thicknesses, air spaces, back focus, conics, aspherics A4/A6), criterion (RMS spot or RMS wavefront over all fields and wavelengths), target focal length and constraints (edge and centre thicknesses, total length, distortion).

#### Glasses
The built-in glasses (N-BK7, N-K5, F2, SF2, N-SF5, N-SF6, N-SF11, N-LAK9, N-SK16, N-BAF10, N-FK51A, N-PK52A, fused silica, CaF2 fluorite) follow their Sellmeier formulas; their old names (BK7, SF11…) are still accepted. Zemax **`.agf`** catalogues (Schott, Ohara, CDGM, Sumita…, ANSI or UTF-16) are imported from the `File` menu, by drag and drop, with `--catalogue file.agf` or with the `importer_catalogue_agf` MCP tool; they are kept in `~/.config/freeoptical/verres` and reloaded at every start. The `nd:Vd` syntax is still available for a model glass.

#### Tilted surfaces, obstructions and gratings
- Each surface is placed at the axial position given by the thicknesses, then decentered and tilted about its vertex; the ray trace is exact in 3D (Newtonian folded by a 45° flat, Schiefspiegler, spectrograph…). Paraxial calculations (focal length, pupils, Seidel) ignore tilts and gratings.
- A central obstruction stops the rays passing closer than its radius to the surface axis; spot, wavefront, PSF and MTF take it into account (the reference chief ray is traced without obstruction).
- Diffraction grating: vector grating equation, lines parallel to the local X axis of the surface, a positive order deviates light towards local +Y; in reflection on a mirror, in transmission otherwise.

#### Wave analyses: wavefront, PSF and MTF
Views `F6`, `F7` and `F8` (graphical window and TUI) apply to the chosen field and wavelength (or to all weighted wavelengths).
- **Wavefront**: optical path difference referred to the reference sphere centred on the chief-ray image and passing through the paraxial exit pupil (Zemax convention), positive when the ray's optical path is longer; peak-to-valley, RMS, Maréchal Strehl and 37 Zernike coefficients (Noll normalization and ordering).
- **PSF**: Fraunhofer diffraction pattern by FFT of the pupil, polychromatic or at one wavelength, linear or logarithmic scale; the Strehl ratio is the PSF peak relative to the same pupil without aberration. Sampling adapts to the spot size to avoid aliasing.
- **MTF**: modulus of the Fourier transform of the PSF, tangential and sagittal for each field, with the diffraction limit of the pupil.
- Checked by the tests: perfect paraboloid (Strehl 1, MTF equal to the circular pupil formula), defocus (exact sinc² Strehl), spherical aberration of a spherical mirror and coma of a paraboloid matching the third-order formulas, PSF centroid equal to the spot diagram centroid.

#### Importing TDESIGN, LENSDES and RAYTRACE files
FreeOptical reads the design files of the programs from the book *Telescope Optics: Evaluation and Design* (H. G. J. Rutten and M. A. M. van Venrooij, Willmann-Bell, 1988): `.TDS` (TDESIGN, mirror and catadioptric telescopes), `.RLD` (LENSDES, refractor objectives) and `.DES` (RAYTRACE). `File` menu → `Import TDESIGN / LENSDES / RAYTRACE`, `--fichier` option, or `importer_teloptik` MCP tool.
- Imported: spheres, conics, aspheres (r² to r¹⁰ coefficients), Schmidt plates defined by their relative power and neutral zone (third-order profile), mirrors (sign change of the index, including inside the glass of a Mangin), axial shifts, N1/N2/N3 indices converted to `nd:Vd` glasses, limiting diameters when vignetting was enabled in RAYTRACE.
- The image plane is placed at the paraxial focus and the semi-diameters are computed to pass the beam on axis and at 0.5° field.
- Central obstructions are imported when vignetting was enabled in RAYTRACE; tilted or decentered files are refused, as RAYTRACE's placement convention is not known for certain.
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
- `analyser_front_onde(champ_index, longueur_onde_nm)`: wavefront of a field: peak-to-valley, RMS, Strehl (Maréchal and PSF), 37 Zernike coefficients.
- `calculer_ftm(champ_index, longueur_onde_nm, nb_frequences)`: tangential and sagittal MTF, diffraction limit and PSF Strehl.
- `analyser_aberrations(champ_index, longueur_onde_nm)`: Seidel coefficients per surface, field curvature and distortion, ray fans.
- `importer_catalogue_agf(chemin)`: imports a Zemax `.agf` glass catalogue.
- `modifier_surface` also accepts `decentrement`, `inclinaison_deg`, `rayon_obstruction`, `reseau_traits_mm` and `reseau_ordre`; `optimiser_systeme` accepts the variables `a4` to `a10` and the targets `spot_rms_champs`, `opd_rms_champs`, `longueur_totale_max`, `distorsion_max`, `epaisseur_centre_min`, `epaisseur_bord_min`.

---

## Contributing

FreeOptical is an open project: anyone can improve it. Report a bug or suggest an idea in the [issues](https://github.com/jbandrivet/freeoptical/issues), ask questions in the [discussions](https://github.com/jbandrivet/freeoptical/discussions), or send code as a pull request. The [CONTRIBUTING.md](CONTRIBUTING.md) guide (in French) explains how to build, test and propose a change; discussions can be held in English.

---

## License

FreeOptical is dual-licensed, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))
