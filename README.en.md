<p align="center"><img src="assets/logo.jpg" alt="FreeOptical" width="640"></p>

<p align="center"><a href="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml"><img src="https://github.com/jbandrivet/freeoptical/actions/workflows/ci.yml/badge.svg" alt="CI"></a> <a href="https://github.com/jbandrivet/freeoptical/releases/latest"><img src="https://img.shields.io/github/v/release/jbandrivet/freeoptical" alt="Latest release"></a> <a href="#license"><img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue" alt="License"></a></p>

# FreeOptical

[Français](README.md) · **English** · [Deutsch](README.de.md) · **Website: [jbandrivet.github.io/freeoptical](https://jbandrivet.github.io/freeoptical/en/)**

**Free sequential optical design and simulation software for Linux: graphical interface, terminal interface and MCP server.**

FreeOptical lets you design any optical system (objective, refractor, mirror telescope, test setup…) surface by surface, then analyse and optimize it. It is written in **Rust** (`egui`, `ratatui`, `nalgebra`, `rmcp`) and combines:
1. **A 3D sequential ray tracing engine**: ABCD paraxial calculation, vector Snell's law, flat, spherical, conic and even aspheric surfaces, mirrors, tilted and decentered surfaces, coordinate breaks for folded systems, solves and pickups (automatic focus, imposed F-number, aplanatic surface, copied values), central obstructions, diffraction gratings, real glasses (Sellmeier formulas, Zemax `.agf` catalogues: Schott, Ohara, CDGM…), polychromatic spot diagram, longitudinal spherical aberration, optical path difference (OPD) and wavefront with Zernike polynomials, Strehl ratio, diffraction PSF and MTF, ray fans, field curvature, distortion, Seidel coefficients, encircled energy, chromatic focal shift and lateral colour, beam footprints and required semi-diameters, import of Zemax `.zmx` files, tolerancing (sensitivities and Monte Carlo), and Damped Least Squares optimization over all fields and colours; mirror testing with the Bath interferometer (simulation, reduction, analysis of interferogram photos).
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
freeoptical --fichier lens.zmx
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
- **`Shift+F1`**: encircled energy; **`Shift+F2`**: chromatic focal shift and lateral colour; **`Shift+F3`**: beam footprint (`[` / `]`: previous / next surface) — also `F13` to `F15`, and from the `Analysis` menu
- **`B`**: Bath interferometer for the system's mirror (`X`/`x`, `Y`/`y`: tilt, `D`/`d`: defocus, `F`: straight fringes, `E`/`e`: separation, `K`/`k` and `V`/`v`: actual and target conics, `A`: astigmatism, `Esc`: close)
- **`C`** / **`W`**: next field / next wavelength for the analysis views
- **`I` / `Ins`**: insert a surface before the selected row
- **`A`**: add a surface before the image plane
- **`Del` / `X`**: delete the selected surface
- **`T`**: cycle the surface type (`FLAT` → `SPHERICAL` → `CONIC` → `ASPHERIC` → `COORDBRK`); a value computed by a solve is followed by `S`
- **`S`**: set the selected surface as aperture stop (`STOP`)
- **`O`**: optimize the system with **Damped Least Squares (DLS)**
- **`P`**: automatic paraxial focus (sets the back focus to the `BFL`)
- **`F12`**: switch language (French → English → German)
- **`Q`**: quit

#### Interface language
FreeOptical is available in French, English and German: `Language` menu (graphical window and TUI), `F12` key in the TUI, or `--langue fr|en|de` option. The choice is saved in `~/.config/freeoptical/langue`; on first start the system language (`LANG`) is used. MCP tool names, JSON field names and material names (`AIR`, `MIROIR`) are the same in every language; MCP tool descriptions and messages follow the chosen language.

In the graphical window, the `File` menu offers **Open…** (`Ctrl+O`), **Save** (`Ctrl+S`) and **Save as…** (`Ctrl+Shift+S`): a folder browser with shortcuts to the home folder, Desktop, Downloads and Documents, which opens `.json` systems, Zemax `.zmx` files, `.TDS`, `.RLD`, `.DES` files and `.agf` catalogues, and remembers the last folder used. When quitting (window close button or `File` → `Quit`), FreeOptical offers to save a modified system. The bundled examples are under `File` → `Examples`. You can also drop a file onto the window.
The **`Update`** button (bottom right, or `Tools` menu) looks for the latest version published on GitHub; if it is newer, it downloads the `.deb` package, checks its SHA-256 digest and installs it with `apt-get` (the administrator password is asked by `pkexec`), then offers to restart FreeOptical.
The A4, A6, A8, A10 aspheric coefficients of the selected surface are edited on the line above the LDE table (drag: 0.1 µm of edge sag per pixel; click: type a value, e.g. `2.1e-7`); changing a coefficient switches the surface to the `ASPHERIC` type.

#### Graphical window: views and settings
- Tabs **Shift+F1** to **Shift+F3**: **encircled energy** (geometric, diffraction and Airy pattern; 50% and 80% radii), **chromatic** (paraxial focus shift versus wavelength, compared with the ±2λN² depth of focus, and lateral colour across the field, compared with the Airy radius), **footprint** of the beams of all fields on the chosen surface, with the required semi-diameter and the **Fit all semi-diameters** button.
- **Solves** row of the selected surface: drop-down lists for radius, thickness and conic; the computed value is shown in orange followed by `S` in the table.
- Tabs **F1** to **F11** below the editor: 2D section, 3D view, spot, LSA, glasses, wavefront, PSF, MTF, **ray fans** (transverse aberrations and optical path difference along the tangential and sagittal sections), **field curvature and distortion**, **Seidel coefficients** per surface.
- Below the aspheric coefficients, the line of the selected surface sets its **decenter** (X, Y), its **tilt** (about X then Y, in degrees, about the vertex), its **central obstruction** radius and a **diffraction grating** (lines per mm, order).
- The **Optimize DLS** button opens the merit function: variables (radii, lens thicknesses, air spaces, back focus, conics, aspherics A4/A6), criterion (RMS spot or RMS wavefront over all fields and wavelengths), target focal length and constraints (edge and centre thicknesses, total length, distortion).

#### Glasses
The built-in glasses (N-BK7, N-K5, F2, SF2, N-SF5, N-SF6, N-SF11, N-LAK9, N-SK16, N-BAF10, N-FK51A, N-PK52A, fused silica, CaF2 fluorite) follow their Sellmeier formulas; their old names (BK7, SF11…) are still accepted. Zemax **`.agf`** catalogues (Schott, Ohara, CDGM, Sumita…, ANSI or UTF-16) are imported from the `File` menu, by drag and drop, with `--catalogue file.agf` or with the `importer_catalogue_agf` MCP tool; they are kept in `~/.config/freeoptical/verres` and reloaded at every start. The `nd:Vd` syntax is still available for a model glass.

#### Tilted surfaces, obstructions and gratings
- Each surface is placed at the position given by the thicknesses along the current axis, then decentered and tilted about its vertex (for itself only); the ray trace is exact in 3D. Paraxial calculations (focal length, pupils, Seidel) follow the unfolded axis and ignore tilts and gratings.
- **Coordinate break** (type `COORDBRK`, as in Zemax): a dummy surface that decenters (X, Y) then tilts (about X, the new Y, the new Z) the frame of **all following surfaces**; its thickness is measured along the new axis. The “tilt before decenter” box is Zemax order 1 (rotations Z, Y, X then decenter): with opposite values it exactly undoes a previous break. A Newtonian is written as in Zemax: primary (negative thickness), 45° break, flat mirror, 45° break, back focus, image plane — which ends up perpendicular to the beam without any adjustment.
- A central obstruction stops the rays passing closer than its radius to the surface axis; spot, wavefront, PSF and MTF take it into account (the reference chief ray is traced without obstruction).
- Diffraction grating: vector grating equation, lines parallel to the local X axis of the surface, a positive order deviates light towards local +Y; in reflection on a mirror, in transmission otherwise.

#### Near objects, afocal systems and spectrographs
- **System settings** (`Edit` menu): object at infinity (fields in degrees) or at a finite distance (fields as object heights in mm: microscopes, relays, projectors, spectrograph slit), aperture (entrance pupil, F/# or object NA), fields and wavelengths with weights and reference. The paraxial back focus becomes the image distance and the summary shows the magnification.
- **Afocal image space** (eyepieces, sighting telescopes, binoculars, beam expanders): spot and PSF in µrad, MTF in cycles/mrad, wavefront referred to a plane wave, field curvature in dioptres, angular magnification.
- **Ideal lens** (`IDEAL` type, adjustable focal length): perfect lens for all conjugates, to simulate an ideal collimator or camera, or to look at the image of an afocal system.
- **Spectrograph** (view **F12**): position of each wavelength on the detector, inverse linear dispersion (nm/mm), spot per wavelength, slit image and resolving power R = λ/Δλ, for a slit width in µm (near object) or arcseconds (object at infinity); the spot diagram can centre each wavelength on its own centroid. Example: `File` → `Load a grating spectrograph`.

#### Wave analyses: wavefront, PSF and MTF
Views `F6`, `F7` and `F8` (graphical window and TUI) apply to the chosen field and wavelength (or to all weighted wavelengths).
- **Wavefront**: optical path difference referred to the reference sphere centred on the chief-ray image and passing through the paraxial exit pupil (Zemax convention), positive when the ray's optical path is longer; peak-to-valley, RMS, Maréchal Strehl and 37 Zernike coefficients (Noll normalization and ordering).
- **PSF**: Fraunhofer diffraction pattern by FFT of the pupil, polychromatic or at one wavelength, linear or logarithmic scale; the Strehl ratio is the PSF peak relative to the same pupil without aberration. Sampling adapts to the spot size to avoid aliasing.
- **MTF**: modulus of the Fourier transform of the PSF, tangential and sagittal for each field, with the diffraction limit of the pupil.
- Checked by the tests: perfect paraboloid (Strehl 1, MTF equal to the circular pupil formula), defocus (exact sinc² Strehl), spherical aberration of a spherical mirror and coma of a paraboloid matching the third-order formulas, PSF centroid equal to the spot diagram centroid.

#### Bath interferometer (testing mirrors at the centre of curvature)
- `Tools` → `Bath interferometer` (key `B` in the TUI). **Simulation**: mirror (diameter, radius of curvature, actual and target conic, obstruction), set-up (wavelength, source – reference separation and its direction), adjustments in fringes (X/Y tilt, defocus; "straightest fringes" button) and surface errors (Zernike, zone, turned-down or turned-up edge). Paths are computed exactly (`|SM| + |MQ|` on the exact conic sag): the Bath astigmatism caused by the separation `s` (s²D²/16R³ peak-to-valley) appears by itself.
- **Reduction**: the map is compared with that of the perfect mirror of the target conic in the same set-up (spherical aberration at the centre of curvature and Bath astigmatism subtracted), piston, tilts and defocus removed (astigmatism on request): measured conic, wavefront peak-to-valley and RMS (at the laser wavelength and at 550 nm) and surface (nm), Strehl at 550 nm, error map and Zernike of the deviation.
- **Photo analysis** (PNG, JPEG, BMP, TIFF, which can be dropped onto the window): the mirror disc is found automatically (adjustable by hand or with the mouse), the wavefront is extracted with Takeda's method (FFT, Gerchberg extrapolation, quality-guided unwrapping, local refinement then global fit of the image model on 37 Zernike terms) and reduced as above. About ten to twenty tilt fringes are needed, with focus adjusted for fringes as straight as possible. A single photo cannot tell a bump from a hollow: the sign is chosen automatically (closest to the target conic) or forced.
- Checked by the tests: sphere without separation = null fringe, exact fringe adjustments, Bath astigmatism and paraboloid spherical aberration matching the formulas, conic and errors recovered; on simulated photos (noise, Gaussian illumination, central obstruction), conic recovered within ±0.005 and RMS within ±0.003 λ, disc located to better than one pixel.
- The example `File` → `Load Bath Interferometer` is the matching sequential set-up: ideal Bath lens (its aberrations cancel in the real set-up) and mirror as the pupil; its wavefront (F6) shows the mirror's departure from the sphere.

#### Tolerancing
`Tools` → `Tolerancing…` (TUI `Tools` menu, MCP tool `tolerancer`). Tolerances are saved with the system; “Default tolerances” suggests a full, editable set: 2 fringes of power, ±0.05 mm thickness, ±0.01 conic, lenses decentered ±0.02 mm and tilted ±1', faces tilted ±1' (wedge), mirrors decentered ±0.05 mm and tilted ±1', index ±0.0005, Abbe number ±0.5%.
- **Tolerances**: radius (mm), power in fringes (Newton rings at 546.1 nm over the clear aperture), thickness, conic, surface decenter and tilt, decenter and tilt of a whole **element** (lens, group: a pair of coordinate breaks about the vertex of its first surface, as in Zemax), glass index and Abbe number (the perturbed glass is a copy of the catalogue glass, dispersion formula included).
- **Criterion**: RMS spot (µm), RMS wavefront (λ, all colours referred to the same image point) or polychromatic MTF at a frequency, averaged over the fields. Pupil sampled by Gaussian quadrature (exact for third-order aberrations).
- **Compensator**: best focus (default), paraxial focus or none; the system's solves stay active (a pickup follows its surface).
- **Sensitivities**: each tolerance alone at its minimum and maximum, from worst to least harmful, with the refocus; **RSS estimate** of the overall change.
- **Monte Carlo**: all tolerances drawn together (normal law truncated at ±2σ or uniform, reproducible seed), computed on all cores: mean, standard deviation, percentiles (“90% of trials do better than…”), histogram; the worst trial can be inspected in the editor, then the nominal system restored.
- Checked by the tests: ±0.01 conic on a paraboloid matching W040/√180, exact perturbed glasses (Δn, ΔV), element put back in place at zero perturbation, reproducible trials.

#### Solves and pickups
Automatic calculations redone before every analysis and at every optimizer step (a computed parameter cannot be a variable), surface by surface, radius before thickness:
- **Thickness**: *marginal ray height* (the marginal ray reaches height `h` on the next surface; `h = 0` = focus; `zone` ≠ 0: real ray at that pupil fraction, 0.7071 for best focus with spherical aberration), *chief ray height* (`h = 0`: next surface in a pupil).
- **Radius**: imposed *marginal* or *chief ray angle* after the surface, *F-number* (the last curvature sets the F/#), *aplanatic* (no spherical aberration nor coma for the marginal ray).
- **Pickup** (radius, thickness or conic): another surface's value × scale + offset (symmetric lens: R2 = −1 × R1).
- An impossible solve (parallel ray, missing surface…) is reported and the previous value kept.

#### Importing Zemax `.zmx` files
Sequential OpticStudio lens files (UTF-16 or ASCII): `File` → `Open…`, drag and drop, `--fichier` option or MCP tool `importer_zemax`.
- Imported: `STANDARD`, `EVENASPH` (r⁴ to r¹⁰), `COORDBRK`, `PARAXIAL`, `DGRATING` surfaces, curvatures, thicknesses, conics, glasses (by name when known — built-in or imported `.agf` catalogues — otherwise by their nd/Vd model), `MIRROR`, semi-diameters (computed when zero), `CLAP` and `OBSC`, stop, units (mm, cm, m, inches), pupil (`ENPD`, `FNUM`, `OBNA`, `FLOA`), fields as angles, object heights or image heights (converted), wavelengths and primary wavelength.
- Zemax solves are imported with their current values, frozen; other surface types are imported as standard surfaces and everything not imported is listed in the import report.

#### Importing TDESIGN, LENSDES and RAYTRACE files
FreeOptical reads the design files of the programs from the book *Telescope Optics: Evaluation and Design* (H. G. J. Rutten and M. A. M. van Venrooij, Willmann-Bell, 1988): `.TDS` (TDESIGN, mirror and catadioptric telescopes), `.RLD` (LENSDES, refractor objectives) and `.DES` (RAYTRACE). `File` menu → `Open…`, `--fichier` option, or `importer_teloptik` MCP tool.
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
- `importer_zemax(chemin)`: imports a sequential Zemax `.zmx` file as the current system.
- `tolerancer(tolerances, par_defaut, enregistrer_dans_systeme, reglages, valeurs_monte_carlo)`: sensitivities, RSS estimate and Monte Carlo.
- `analyser_energie_encerclee(champ_index)`: geometric, diffraction and Airy encircled energy, 50% and 80% radii.
- `analyser_chromatisme()`: chromatic focal shift and lateral colour.
- `analyser_empreinte(index_surface, ajuster_demi_diametres, points)`: beam footprint on a surface and required semi-diameters (applied on request).
- `analyser_front_onde(champ_index, longueur_onde_nm)`: wavefront of a field: peak-to-valley, RMS, Strehl (Maréchal and PSF), 37 Zernike coefficients.
- `calculer_ftm(champ_index, longueur_onde_nm, nb_frequences)`: tangential and sagittal MTF, diffraction limit and PSF Strehl.
- `analyser_aberrations(champ_index, longueur_onde_nm)`: Seidel coefficients per surface, field curvature and distortion, ray fans.
- `importer_catalogue_agf(chemin)`: imports a Zemax `.agf` glass catalogue.
- `definir_systeme(...)`: object (infinity or distance), afocal, aperture, fields, wavelengths.
- `analyser_spectre(champ_index, lambda_min_nm, lambda_max_nm, nb, largeur_fente)`: positions, dispersion, spot, slit image and resolution of a spectrograph.
- `simuler_bath(parametres, chemin_png)`: interferogram of a mirror at its centre of curvature and its reduction (measured conic, PV/RMS, Strehl, Zernike), optional PNG.
- `analyser_interferogramme(chemin, parametres, disque, marge_pct, signe)`: analysis and reduction of a Bath interferogram photo.
- `modifier_surface` also accepts `solve_rayon`, `solve_epaisseur`, `solve_conique` (e.g. `{"type":"HauteurMarginale","hauteur_mm":0,"zone":0}`, `{"type":"NombreOuverture","f_nombre":8}`, `{"type":"Reprise","surface":1,"facteur":-1,"decalage":0}`), `retirer_solves`, `inclinaison_z_deg`, `inclinaison_avant_decentrement` (`COORDBRK` coordinate breaks), `decentrement`, `inclinaison_deg`, `rayon_obstruction`, `reseau_traits_mm` and `reseau_ordre`; `optimiser_systeme` accepts the variables `a4` to `a10` and the targets `spot_rms_champs`, `opd_rms_champs`, `longueur_totale_max`, `distorsion_max`, `epaisseur_centre_min`, `epaisseur_bord_min`.

---

## Contributing

FreeOptical is an open project: anyone can improve it. Report a bug or suggest an idea in the [issues](https://github.com/jbandrivet/freeoptical/issues), ask questions in the [discussions](https://github.com/jbandrivet/freeoptical/discussions), or send code as a pull request. The [CONTRIBUTING.md](CONTRIBUTING.md) guide (in French) explains how to build, test and propose a change; discussions can be held in English.

---

## License

FreeOptical is dual-licensed, at your option:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))
