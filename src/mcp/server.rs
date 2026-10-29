use crate::core::{
    analysis::{
        LongitudinalAberrationResult, SpotDiagramResult,
        calculer_aberration_spherique_longitudinale, calculer_spot_diagram,
    },
    optimizer::{OptimizationReport, OptimizationTarget, OptimizationVariable, optimiser_systeme_dls},
    paraxial::{ParaxialData, calculer_paraxial},
    raytracer::{TracedRay, tracer_eventail_meridional},
    onde::{calculer_front_onde, calculer_psf_ftm, nom_zernike},
    teloptik::importer_fichier_teloptik,
    types::{OpticalSystem, Surface, SurfaceType},
};
use rmcp::{
    handler::server::wrapper::Parameters,
    schemars::{self, JsonSchema},
    serde::{Deserialize, Serialize},
    tool, tool_handler, tool_router,
};
use std::sync::{Arc, Mutex};

/// Paramètres de l'outil MCP `creer_systeme(nom, longueur_onde_ref)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreerSystemeParams {
    /// Nom du nouveau design optique
    pub nom: String,
    /// Longueur d'onde de référence en nanomètres (ex: 587.56)
    pub longueur_onde_ref: f64,
}

/// Paramètres de l'outil MCP `ajouter_surface(type, rayon, epaisseur, verre, semi_diametre)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AjouterSurfaceParams {
    /// Type géométrique de la surface : `"PLANE"`, `"SPHERIQUE"`, `"CONIQUE"`, `"ASPHERIQUE"`,
    /// `"IDEALE"` ou `"COORDBRK"` (rupture de repère : décentre et incline le repère de toutes
    /// les surfaces suivantes, réglée ensuite par `modifier_surface`)
    #[serde(rename = "type", alias = "type_surface")]
    pub r#type: String,
    /// Rayon de courbure en mm (0.0 ou très grand pour une surface plane)
    pub rayon: f64,
    /// Épaisseur axiale jusqu'à la surface suivante en mm
    pub epaisseur: f64,
    /// Matériau après la surface (`"AIR"`, `"BK7"`, `"F2"`, `"SF11"`, `"SILICE"`, `"MIROIR"`, ou `"nd:Vd"`)
    pub verre: String,
    /// Demi-diamètre mécanique utile en mm
    pub semi_diametre: f64,
}

/// Sous-structure des propriétés modifiables dans `modifier_surface(index, parametres)`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SurfacePatchParams {
    /// Nouveau type de surface (`"PLANE"`, `"SPHERIQUE"`, `"CONIQUE"`, `"ASPHERIQUE"`, `"IDEALE"`,
    /// `"COORDBRK"`)
    #[serde(rename = "type", alias = "type_surface")]
    pub r#type: Option<String>,
    /// Nouveau rayon de courbure en mm
    pub rayon: Option<f64>,
    /// Nouvelle épaisseur en mm
    pub epaisseur: Option<f64>,
    /// Nouveau matériau optique (`"AIR"`, `"BK7"`, `"F2"`, `"MIROIR"`...)
    pub verre: Option<String>,
    /// Nouveau demi-diamètre en mm
    pub semi_diametre: Option<f64>,
    /// Nouvelle constante conique k
    pub conique: Option<f64>,
    /// Nouveaux coefficients asphériques pairs [A4, A6, A8, A10]
    pub aspheriques: Option<[f64; 4]>,
    /// Nouveau commentaire descriptif
    pub commentaire: Option<String>,
    /// Définit cette surface comme diaphragme (STOP)
    pub est_diaphragme: Option<bool>,
    /// Décentrement du sommet [x, y] en mm
    pub decentrement: Option<[f64; 2]>,
    /// Inclinaison [autour de X, puis autour de Y] en degrés ; sur une rupture de repère
    /// (`"COORDBRK"`), elle s'applique à toutes les surfaces suivantes
    pub inclinaison_deg: Option<[f64; 2]>,
    /// Rayon de l'obstruction centrale en mm (0 = aucune)
    pub rayon_obstruction: Option<f64>,
    /// Réseau de diffraction : traits par mm (0 = aucun), traits parallèles à X local
    pub reseau_traits_mm: Option<f64>,
    /// Ordre de diffraction du réseau
    pub reseau_ordre: Option<i32>,
    /// Focale d'une lentille idéale (type `"IDEALE"`), en mm
    pub focale_ideale_mm: Option<f64>,
    /// Rupture de repère (type `"COORDBRK"`) : inclinaison appliquée avant le décentrement
    /// (ordre 1 de Zemax, pour défaire une rupture précédente)
    pub inclinaison_avant_decentrement: Option<bool>,
    /// Rupture de repère : rotation autour du nouvel axe Z (degrés), après X et Y
    pub inclinaison_z_deg: Option<f64>,
    /// Solve du rayon : `{"type":"NombreOuverture","f_nombre":8}`, `{"type":"AngleMarginal","angle":-0.05}`,
    /// `{"type":"AnglePrincipal","angle":0}`, `{"type":"Aplanetique"}` ou
    /// `{"type":"Reprise","surface":2,"facteur":-1,"decalage":0}` (surface numérotée à partir de 1)
    pub solve_rayon: Option<crate::core::types::Solve>,
    /// Solve de l'épaisseur : `{"type":"HauteurMarginale","hauteur_mm":0,"zone":0}` (mise au point ;
    /// zone ≠ 0 : rayon réel à cette fraction de la pupille), `{"type":"HauteurPrincipale","hauteur_mm":0}`
    /// (surface suivante dans une pupille) ou une reprise
    pub solve_epaisseur: Option<crate::core::types::Solve>,
    /// Reprise de la constante conique : `{"type":"Reprise","surface":1,"facteur":1,"decalage":0}`
    pub solve_conique: Option<crate::core::types::Solve>,
    /// Vrai pour retirer les solves existants de la surface (avant d'appliquer ceux fournis)
    pub retirer_solves: Option<bool>,
}

/// Paramètres de l'outil MCP `modifier_surface(index, parametres)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ModifierSurfaceParams {
    /// Index de la surface à modifier (0..N-1)
    pub index: usize,
    /// Dictionnaire des propriétés à mettre à jour sur cette surface
    pub parametres: SurfacePatchParams,
}

/// Paramètres de l'outil MCP `optimiser_systeme(variables, cibles)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct OptimiserSystemeParams {
    /// Liste des variables d'optimisation (index_surface et parametre : "rayon", "epaisseur", "conique")
    pub variables: Vec<OptimizationVariable>,
    /// Liste des cibles de la fonction de mérite ("focale", "spot_rms", "aberration_spherique", "tirage_bfl")
    pub cibles: Vec<OptimizationTarget>,
}

/// Paramètres de l'outil MCP `importer_teloptik(chemin)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ImporterTeloptikParams {
    /// Chemin absolu d'un fichier `.TDS` (TDESIGN), `.RLD` (LENSDES) ou `.DES` (RAYTRACE)
    pub chemin: String,
}

/// Paramètres de l'outil MCP `analyser_empreinte`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AnalyserEmpreinteParams {
    /// Surface étudiée (0..N-1, 0 par défaut)
    pub index_surface: Option<usize>,
    /// Vrai pour donner d'abord à chaque surface le demi-diamètre nécessaire
    pub ajuster_demi_diametres: Option<bool>,
    /// Vrai pour retourner aussi la liste des impacts (x, y, champ, longueur d'onde, arrêté)
    pub points: Option<bool>,
}

/// Paramètres de l'outil MCP `definir_systeme`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct DefinirSystemeParams {
    /// Distance de l'objet à la première surface (mm) ; 0 ou absente avec `objet_infini` : objet à
    /// l'infini. Avec un objet à distance finie, les champs sont des hauteurs d'objet en mm
    pub distance_objet_mm: Option<f64>,
    /// Vrai pour remettre l'objet à l'infini (champs en degrés)
    pub objet_infini: Option<bool>,
    /// Espace image afocal (oculaire, jumelles, élargisseur de faisceau) : analyses en angles
    pub afocal: Option<bool>,
    /// Type d'ouverture : `"pupille_entree_mm"`, `"nombre_ouverture"` ou `"ouverture_numerique_objet"`
    pub ouverture_type: Option<String>,
    /// Valeur de l'ouverture
    pub ouverture_valeur: Option<f64>,
    /// Champs [x, y] : degrés (objet à l'infini) ou mm (objet à distance finie)
    pub champs: Option<Vec<[f64; 2]>>,
    /// Longueurs d'onde en nm
    pub longueurs_onde_nm: Option<Vec<f64>>,
    /// Poids des longueurs d'onde (1 par défaut)
    pub poids: Option<Vec<f64>>,
    /// Longueur d'onde de référence en nm
    pub longueur_onde_ref_nm: Option<f64>,
}

/// Paramètres de l'outil MCP `analyser_spectre`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AnalyserSpectreParams {
    /// Index du champ (0 par défaut)
    pub champ_index: Option<usize>,
    /// Domaine de longueurs d'onde (nm), par défaut celui du système
    pub lambda_min_nm: Option<f64>,
    pub lambda_max_nm: Option<f64>,
    /// Nombre de longueurs d'onde calculées (21 par défaut)
    pub nb: Option<usize>,
    /// Largeur de fente : µm (objet à distance finie) ou secondes d'arc (objet à l'infini)
    pub largeur_fente: Option<f64>,
}

/// Paramètres de l'outil MCP `simuler_bath`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SimulerBathParams {
    /// Miroir, montage, réglages (franges), défauts de surface et options de réduction ; les
    /// champs absents prennent leur valeur par défaut (200 mm F/6 parabolique, He-Ne 632,8 nm,
    /// écart de 5 mm, 10 franges de basculement)
    #[serde(default)]
    pub parametres: crate::core::bath::ParametresBath,
    /// Chemin d'un PNG où enregistrer l'interférogramme simulé (facultatif)
    pub chemin_png: Option<String>,
}

/// Paramètres de l'outil MCP `analyser_interferogramme`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AnalyserInterferogrammeParams {
    /// Chemin absolu de la photo de l'interférogramme (PNG, JPEG, BMP, TIFF)
    pub chemin: String,
    /// Miroir (diamètre, rayon de courbure, conique visée, obstruction) et montage (longueur
    /// d'onde, écart source – référence et sa direction)
    #[serde(default)]
    pub parametres: crate::core::bath::ParametresBath,
    /// Disque du miroir dans la photo (pixels) ; absent : repéré automatiquement
    pub disque: Option<crate::core::franges::Disque>,
    /// Couronne exclue au bord du miroir, en % du rayon (3 par défaut)
    pub marge_pct: Option<f64>,
    /// Signe de la carte : `Auto` (par défaut), `Normal` ou `Inverse`
    pub signe: Option<crate::core::bath::SigneCarte>,
}

/// Résumé d'une réduction de Bath, sans les cartes
fn resume_reduction_bath(r: &crate::core::bath::ReductionBath) -> serde_json::Value {
    let ecart: Vec<_> = r
        .zernike_residu_ondes
        .iter()
        .enumerate()
        .filter(|(j, c)| *j >= 4 && c.abs() >= 1e-3)
        .map(|(j, c)| serde_json::json!({ "noll": j + 1, "nom": nom_zernike(j + 1), "ondes": c }))
        .collect();
    serde_json::json!({
        "conique_visee": r.conique_visee,
        "conique_mesuree": r.conique_mesuree,
        "front_onde_pv_ondes": r.pv_ondes,
        "front_onde_rms_ondes": r.rms_ondes,
        "front_onde_pv_ondes_550nm": r.pv_ondes_550,
        "front_onde_rms_ondes_550nm": r.rms_ondes_550,
        "strehl_550nm": r.strehl_550,
        "surface_pv_nm": r.pv_surface_nm,
        "surface_rms_nm": r.rms_surface_nm,
        "astigmatisme_bath_deduit_pv_ondes": r.astigmatisme_bath_ondes,
        "spherique_attendue_z11_ondes": r.spherique_visee_ondes,
        "signe_carte": r.signe,
        "zernike_ecart_ondes": ecart,
        "zernike_mesure_ondes": r.zernike_mesure_ondes,
        "nb_points": r.nb_points,
    })
}

/// Paramètres de l'outil MCP `importer_catalogue_agf(chemin)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ImporterCatalogueParams {
    /// Chemin absolu d'un catalogue de verres Zemax `.agf`
    pub chemin: String,
}

/// Paramètres des outils MCP `analyser_front_onde` et `calculer_ftm`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct AnalyseOndeParams {
    /// Index du champ dans `champs_deg` (0 = sur l'axe par défaut)
    pub champ_index: Option<usize>,
    /// Longueur d'onde en nm ; absente : longueur d'onde de référence (front d'onde) ou
    /// polychromatique pondérée (FTM)
    pub longueur_onde_nm: Option<f64>,
    /// Nombre de fréquences retournées pour la FTM (11 par défaut, de 0 à la coupure)
    pub nb_frequences: Option<usize>,
}

/// Données brutes retournées par `lancer_simulation()`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationOutput {
    pub systeme: OpticalSystem,
    pub paraxial: ParaxialData,
    pub rayons_meridiens: Vec<TracedRay>,
    pub aberration_spherique: LongitudinalAberrationResult,
}

/// Résultat retourné par `optimiser_systeme(variables, cibles)`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimisationOutput {
    pub rapport: OptimizationReport,
    pub systeme_optimise: OpticalSystem,
}

fn avertissement_verre(verre: &str) -> String {
    trf!(
        "Verre '{}' inconnu : il est traité comme de l'AIR. Utiliser un nom du catalogue (BK7, F2, SF11, SILICE...), MIROIR, ou la syntaxe 'nd:Vd'.", "Unknown glass '{}': it is treated as AIR. Use a catalogue name (BK7, F2, SF11, SILICE...), MIROIR, or the 'nd:Vd' syntax.", "Unbekanntes Glas '{}': es wird als Luft (AIR) behandelt. Einen Katalognamen (BK7, F2, SF11, SILICE...), MIROIR oder die Syntax 'nd:Vd' verwenden.",
        verre
    )
}

/// Serveur MCP de conception et simulation optique FreeOptical
#[derive(Clone)]
pub struct FreeOpticalMcpServer {
    pub systeme: Arc<Mutex<OpticalSystem>>,
}

impl FreeOpticalMcpServer {
    pub fn nouveau(systeme_initial: OpticalSystem) -> Self {
        Self {
            systeme: Arc::new(Mutex::new(systeme_initial)),
        }
    }
}

/// Descriptions des outils en anglais et en allemand (le français est celui des attributs `#[tool]`)
const DESCRIPTIONS_OUTILS: [(&str, &str, &str); 19] = [
    (
        "importer_zemax",
        "Imports a sequential Zemax OpticStudio lens file (.zmx, UTF-16 or ASCII) and makes it the current system: standard, even-asphere, coordinate-break, paraxial and grating surfaces, glasses (catalogue name or nd/Vd model), apertures and obscurations, pupil, fields (angles, object or image heights), wavelengths. Solves are imported as frozen values. Returns the system, its paraxial data and the items not imported.",
        "Importiert eine sequentielle Zemax-OpticStudio-Linsendatei (.zmx, UTF-16 oder ASCII) als aktuelles System: Standard-, gerade asphärische, Koordinatenbruch-, paraxiale und Gitterflächen, Gläser (Katalogname oder nd/Vd-Modell), Aperturen und Abschattungen, Pupille, Felder (Winkel, Objekt- oder Bildhöhen), Wellenlängen. Solves werden als feste Werte übernommen. Gibt das System, seine paraxialen Daten und die nicht übernommenen Elemente zurück.",
    ),
    (
        "analyser_energie_encerclee",
        "Encircled energy of a field around the spot centroid: geometric curve (all wavelengths weighted), diffraction curve from the polychromatic PSF, Airy curve of an aberration-free pupil, radii containing 50% and 80% of the energy (µm).",
        "Eingeschlossene Energie eines Feldes um den Schwerpunkt: geometrische Kurve (alle Wellenlängen gewichtet), Beugungskurve aus der polychromatischen PSF, Airy-Kurve einer fehlerfreien Pupille, Radien mit 50 % und 80 % der Energie (µm).",
    ),
    (
        "analyser_chromatisme",
        "Chromatic aberrations: paraxial focal shift versus wavelength over the system band (range, diffraction depth of focus ±2λN²) and lateral colour (chief ray image height spread between wavelengths versus field, compared with the Airy radius).",
        "Farbfehler: paraxiale Fokusverschiebung über die Wellenlänge im Band des Systems (Bereich, Beugungs-Schärfentiefe ±2λN²) und Farbquerfehler (Abweichung der Hauptstrahl-Bildhöhe zwischen den Wellenlängen über dem Feld, verglichen mit dem Airy-Radius).",
    ),
    (
        "analyser_empreinte",
        "Beam footprint on a surface (local frame): ray hits of all fields and wavelengths, radius used, number of stopped rays, and the semi-diameter each surface needs to pass every beam (like Zemax automatic semi-diameters); can apply these semi-diameters.",
        "Strahlen-Footprint auf einer Fläche (lokales System): Auftreffpunkte aller Felder und Wellenlängen, genutzter Radius, Zahl gestoppter Strahlen und der Halbdurchmesser, den jede Fläche für alle Bündel braucht (wie die automatischen Halbdurchmesser von Zemax); kann diese Halbdurchmesser übernehmen.",
    ),
    (
        "simuler_bath",
        "Bath interferometer: simulates the interferogram of a concave mirror tested at its centre of curvature (exact paths, source/reference separation and Bath astigmatism, tilt and defocus in fringes, surface errors: Zernike, zone, turned edge) and reduces it against the target conic: measured conic, wavefront and surface PV/RMS, Strehl at 550 nm, Zernike of the deviation. Can save the interferogram as PNG.",
        "Bath-Interferometer: simuliert das Interferogramm eines im Krümmungsmittelpunkt geprüften Hohlspiegels (exakte Wege, Abstand Quelle/Referenz und Bath-Astigmatismus, Kippung und Defokus in Streifen, Oberflächenfehler: Zernike, Zone, abfallender Rand) und wertet es gegen die Ziel-Konik aus: gemessene Konik, PV/RMS von Wellenfront und Oberfläche, Strehl bei 550 nm, Zernike der Abweichung. Kann das Interferogramm als PNG speichern.",
    ),
    (
        "analyser_interferogramme",
        "Analyses a photo of a Bath interferogram: finds the mirror disc (or uses the given one), extracts the wavefront by Fourier transform (Takeda) with Gerchberg extrapolation, quality-guided unwrapping and a global fit of the image model, then reduces it like simuler_bath (measured conic, PV/RMS, Strehl, Zernike). Needs about 10 to 20 tilt fringes.",
        "Wertet ein Foto eines Bath-Interferogramms aus: findet die Spiegelscheibe (oder nimmt die angegebene), gewinnt die Wellenfront per Fourier-Transformation (Takeda) mit Gerchberg-Extrapolation, qualitätsgeführter Phasenentfaltung und globaler Anpassung des Bildmodells und wertet sie wie simuler_bath aus (gemessene Konik, PV/RMS, Strehl, Zernike). Benötigt etwa 10 bis 20 Kippstreifen.",
    ),
    (
        "definir_systeme",
        "Sets the system parameters: object at infinity (fields in degrees) or at a finite distance (fields as object heights in mm), afocal image space, aperture (entrance pupil, F-number or object NA), fields and wavelengths with weights and reference.",
        "Legt die Systemparameter fest: Objekt im Unendlichen (Felder in Grad) oder in endlicher Entfernung (Felder als Objekthöhen in mm), afokaler Bildraum, Öffnung (Eintrittspupille, Blendenzahl oder objektseitige NA), Felder und Wellenlängen mit Gewichten und Referenz.",
    ),
    (
        "analyser_spectre",
        "Spectrograph analysis over a wavelength range: chief-ray position of each wavelength on the detector, inverse linear dispersion (nm/mm), spot per wavelength, slit image width and resolving power R = λ/Δλ.",
        "Spektrographenanalyse über einen Wellenlängenbereich: Position jeder Wellenlänge auf dem Detektor (Hauptstrahl), reziproke Lineardispersion (nm/mm), Spot je Wellenlänge, Spaltbildbreite und Auflösungsvermögen R = λ/Δλ.",
    ),
    (
        "importer_catalogue_agf",
        "Imports a Zemax .agf glass catalogue (Schott, Ohara, CDGM…): its glasses become usable by name in every system and are kept for the next starts.",
        "Importiert einen Zemax-Glaskatalog .agf (Schott, Ohara, CDGM…): seine Gläser sind in jedem System per Name verwendbar und bleiben für spätere Starts erhalten.",
    ),
    (
        "analyser_aberrations",
        "Geometric aberrations: Seidel coefficients per surface (spherical, coma, astigmatism, Petzval, distortion, axial and lateral colour) with equivalent wave coefficients, field curvature (tangential and sagittal foci) and distortion versus field angle, tangential and sagittal ray fans of a field.",
        "Geometrische Aberrationen: Seidel-Koeffizienten je Fläche (sphärisch, Koma, Astigmatismus, Petzval, Verzeichnung, Farblängs- und Farbquerfehler) mit äquivalenten Wellenkoeffizienten, Bildfeldwölbung (tangentialer und sagittaler Fokus) und Verzeichnung über dem Feldwinkel, tangentiale und sagittale Strahlfächer eines Feldes.",
    ),
    (
        "analyser_front_onde",
        "Computes the wavefront (OPD referred to the chief-ray reference sphere at the exit pupil) for a field and a wavelength: peak-to-valley and RMS in waves, Maréchal Strehl and diffraction PSF Strehl, and the 37 Noll Zernike coefficients.",
        "Berechnet die Wellenfront (OPD bezogen auf die Referenzkugel des Hauptstrahls in der Austrittspupille) für ein Feld und eine Wellenlänge: Spitze-Tal und RMS in Wellen, Strehl nach Maréchal und aus der Beugungs-PSF sowie die 37 Zernike-Koeffizienten nach Noll.",
    ),
    (
        "calculer_ftm",
        "Computes the diffraction MTF (FFT of the PSF) of a field, tangential and sagittal, with the diffraction limit of the same pupil, polychromatic by default; also returns the PSF Strehl ratio.",
        "Berechnet die Beugungs-MTF (FFT der PSF) eines Feldes, tangential und sagittal, mit der Beugungsgrenze derselben Pupille, standardmäßig polychromatisch; liefert auch das Strehl-Verhältnis der PSF.",
    ),
    (
        "creer_systeme",
        "Creates a new sequential optical system with its name and reference wavelength (in nm).",
        "Legt ein neues sequentielles optisches System mit Namen und Referenzwellenlänge (in nm) an.",
    ),
    (
        "ajouter_surface",
        "Adds an optical surface (PLANE, SPHERIQUE, CONIQUE, ASPHERIQUE, IDEALE, or COORDBRK coordinate break) with its radius, thickness, glass and semi-diameter.",
        "Fügt eine optische Fläche (PLANE, SPHERIQUE, CONIQUE, ASPHERIQUE, IDEALE oder Koordinatenbruch COORDBRK) mit Radius, Dicke, Glas und Halbdurchmesser hinzu.",
    ),
    (
        "modifier_surface",
        "Modifies the properties (type, rayon, epaisseur, verre, semi_diametre, conique, aspheriques, decentrement, inclinaison_deg…) of an existing surface, and its solves and pickups (solve_rayon, solve_epaisseur, solve_conique: focus by the marginal ray, surface in a pupil, imposed F-number, aplanatic surface, value copied from another surface).",
        "Ändert die Eigenschaften (type, rayon, epaisseur, verre, semi_diametre, conique, aspheriques, decentrement, inclinaison_deg…) einer vorhandenen Fläche sowie ihre Solves und Übernahmen (solve_rayon, solve_epaisseur, solve_conique: Fokus über den Randstrahl, Fläche in einer Pupille, vorgegebene Blendenzahl, aplanatische Fläche, von einer anderen Fläche übernommener Wert).",
    ),
    (
        "importer_teloptik",
        "Imports a .TDS (TDESIGN), .RLD (LENSDES) or .DES (RAYTRACE) file from the programs of the book \"Telescope Optics\" by Rutten and van Venrooij and makes it the current system. Returns the system, its paraxial data, the focal length written in the file and the items not imported (obstructions, extra colours…). Tilted or decentered systems are refused.",
        "Importiert eine .TDS- (TDESIGN), .RLD- (LENSDES) oder .DES-Datei (RAYTRACE) der Programme aus dem Buch „Telescope Optics“ von Rutten und van Venrooij als aktuelles System. Gibt das System, seine paraxialen Daten, die in der Datei stehende Brennweite und die nicht übernommenen Elemente (Obstruktionen, zusätzliche Farben…) zurück. Gekippte oder dezentrierte Systeme werden abgelehnt.",
    ),
    (
        "lancer_simulation",
        "Runs the sequential ray trace and returns the raw data: paraxial parameters, 3D ray paths and longitudinal spherical aberration.",
        "Führt die sequentielle Strahlverfolgung aus und gibt die Rohdaten zurück: paraxiale Parameter, 3D-Strahlwege und longitudinale sphärische Aberration.",
    ),
    (
        "obtenir_spot_diagram",
        "Returns the image plane hit coordinates, the centroid, the RMS radius (µm and mm), the maximum geometric radius and the Airy radius.",
        "Gibt die Auftreffpunkte in der Bildebene, den Schwerpunkt, den RMS-Radius (µm und mm), den maximalen geometrischen Radius und den Airy-Radius zurück.",
    ),
    (
        "optimiser_systeme",
        "Runs the Damped Least Squares optimizer (DLS / Levenberg-Marquardt) on the selected variables to reach a focal length or minimize the RMS spot / spherical aberration.",
        "Startet die Optimierung nach Damped Least Squares (DLS / Levenberg-Marquardt) mit den gewählten Variablen, um eine Brennweite zu erreichen oder den RMS-Spot / die sphärische Aberration zu minimieren.",
    ),
];

impl FreeOpticalMcpServer {
    /// Outils MCP avec leurs descriptions dans la langue courante
    pub fn routeur_traduit() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
        let mut routeur = Self::routeur_outils();
        for (nom, en, de) in DESCRIPTIONS_OUTILS {
            if let Some(route) = routeur.map.get_mut(nom) {
                if let Some(fr) = route.attr.description.clone() {
                    let texte = match crate::i18n::langue() {
                        crate::i18n::Langue::Fr => fr,
                        crate::i18n::Langue::En => en.into(),
                        crate::i18n::Langue::De => de.into(),
                    };
                    route.attr.description = Some(texte);
                }
            }
        }
        routeur
    }
}

#[tool_handler(router = Self::routeur_traduit())]
impl rmcp::ServerHandler for FreeOpticalMcpServer {}

#[tool_router(router = routeur_outils)]
impl FreeOpticalMcpServer {
    /// Initialise un nouveau design optique avec son nom et sa longueur d'onde de référence (en nm).
    #[tool(
        name = "creer_systeme",
        description = "Initialise un nouveau système optique séquentiel avec son nom et sa longueur d'onde de référence (en nm)."
    )]
    pub async fn creer_systeme(&self, Parameters(params): Parameters<CreerSystemeParams>) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let lambda = if params.longueur_onde_ref > 100.0 {
            params.longueur_onde_ref
        } else {
            587.5618
        };
        // Crée un système doté uniquement d'un plan image initial afin que `ajouter_surface`
        // empile les dioptres dans l'ordre exact de création avant le plan image
        let mut nouveau = OpticalSystem::nouveau(&params.nom, lambda);
        nouveau.surfaces = vec![Surface::plan_image(15.0)];
        *sys = nouveau;

        serde_json::to_string_pretty(&*sys).unwrap_or_default()
    }

    /// Ajoute une surface optique (lentille ou miroir) avant le plan image du système.
    #[tool(
        name = "ajouter_surface",
        description = "Ajoute une surface optique (PLANE, SPHERIQUE, CONIQUE, ASPHERIQUE, IDEALE, ou rupture de repère COORDBRK) avec son rayon, épaisseur, verre et demi-diamètre."
    )]
    pub async fn ajouter_surface(
        &self,
        Parameters(params): Parameters<AjouterSurfaceParams>,
    ) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let type_surf = SurfaceType::depuis_chaine(&params.r#type);
        let mut surf = Surface::nouvelle(
            type_surf,
            params.rayon,
            params.epaisseur,
            &params.verre,
            params.semi_diametre,
        );
        if sys.surfaces.len() <= 1 {
            surf.est_diaphragme = true;
        }
        surf.commentaire = trf!("Surface {}", "Surface {}", "Fläche {}", sys.surfaces.len());

        // Insère juste avant le Plan Image final
        let pos_insertion = sys.surfaces.len().saturating_sub(1);
        let verre_connu = sys.catalogue.est_connu(&surf.verre);
        let verre = surf.verre.clone();
        sys.surfaces.insert(pos_insertion, surf);
        crate::core::solves::appliquer_solves(&mut sys);

        let paraxial = calculer_paraxial(&sys);
        let mut reponse = serde_json::json!({
            "statut": tr!("Surface ajoutée avec succès", "Surface added", "Fläche hinzugefügt"),
            "index_insere": pos_insertion,
            "nb_surfaces": sys.surfaces.len(),
            "paraxial": paraxial,
        });
        if !verre_connu {
            reponse["avertissement"] = avertissement_verre(&verre).into();
        }
        serde_json::to_string_pretty(&reponse).unwrap_or_default()
    }

    /// Modifie les propriétés d'une surface existante à partir de son index (0..N-1).
    #[tool(
        name = "modifier_surface",
        description = "Modifie les propriétés (type, rayon, epaisseur, verre, semi_diametre, conique, aspheriques, decentrement, inclinaison_deg…) d'une surface existante, ainsi que ses solves et reprises (solve_rayon, solve_epaisseur, solve_conique : mise au point par le rayon marginal, surface dans une pupille, nombre d'ouverture imposé, surface aplanétique, valeur recopiée d'une autre surface)."
    )]
    pub async fn modifier_surface(
        &self,
        Parameters(params): Parameters<ModifierSurfaceParams>,
    ) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let nb = sys.surfaces.len();
        let Some(surf) = sys.surfaces.get_mut(params.index) else {
            return serde_json::json!({
                "erreur": trf!("Index de surface {} hors limites (0..{})", "Surface index {} out of range (0..{})", "Flächenindex {} außerhalb des Bereichs (0..{})", params.index, nb.saturating_sub(1))
            })
            .to_string();
        };

        let p = params.parametres;
        let type_explicite = p.r#type.is_some();
        if let Some(t) = p.r#type {
            surf.type_surface = SurfaceType::depuis_chaine(&t);
            if surf.type_surface == SurfaceType::Plane {
                surf.rayon = f64::INFINITY;
            }
        }
        if let Some(r) = p.rayon {
            surf.rayon = if r.abs() < 1e-9 { f64::INFINITY } else { r };
            // Un rayon fini sur une surface plane la rend sphérique (sinon il serait ignoré)
            if !type_explicite && surf.rayon.is_finite() && surf.type_surface == SurfaceType::Plane {
                surf.type_surface = SurfaceType::Spherique;
            }
        }
        if let Some(ep) = p.epaisseur {
            surf.epaisseur = ep;
        }
        if let Some(v) = p.verre {
            surf.verre = v.to_uppercase();
        }
        if let Some(sd) = p.semi_diametre {
            surf.semi_diametre = sd.max(0.1);
        }
        if let Some(k) = p.conique {
            surf.conique = k;
            // Une sphère ignore la constante conique
            if !type_explicite && k.abs() > 1e-9 && surf.type_surface == SurfaceType::Spherique {
                surf.type_surface = SurfaceType::Conique;
            }
        }
        if let Some(asph) = p.aspheriques {
            surf.aspheriques = asph;
        }
        if let Some(comm) = p.commentaire {
            surf.commentaire = comm;
        }
        if let Some(v) = p.decentrement {
            surf.decentrement = v;
        }
        if let Some(v) = p.inclinaison_deg {
            surf.inclinaison_deg = v;
        }
        if let Some(v) = p.rayon_obstruction {
            surf.rayon_obstruction = v.max(0.0);
        }
        if let Some(v) = p.reseau_traits_mm {
            surf.reseau_traits_mm = v;
        }
        if let Some(v) = p.reseau_ordre {
            surf.reseau_ordre = v;
        }
        if let Some(v) = p.focale_ideale_mm {
            surf.focale_ideale_mm = v;
        }
        if let Some(stop) = p.est_diaphragme {
            surf.est_diaphragme = stop;
        }
        if let Some(v) = p.inclinaison_avant_decentrement {
            surf.inclinaison_avant_decentrement = v;
        }
        if let Some(v) = p.inclinaison_z_deg {
            surf.inclinaison_z_deg = v;
        }
        if p.retirer_solves == Some(true) {
            surf.solve_rayon = None;
            surf.solve_epaisseur = None;
            surf.solve_conique = None;
        }
        if p.solve_rayon.is_some() {
            surf.solve_rayon = p.solve_rayon;
        }
        if p.solve_epaisseur.is_some() {
            surf.solve_epaisseur = p.solve_epaisseur;
        }
        if p.solve_conique.is_some() {
            surf.solve_conique = p.solve_conique;
        }

        let alertes = crate::core::solves::appliquer_solves(&mut sys);
        let surf_maj = sys.surfaces[params.index].clone();
        let paraxial = calculer_paraxial(&sys);
        let mut reponse = serde_json::json!({
            "statut": tr!("Surface modifiée avec succès", "Surface modified", "Fläche geändert"),
            "index": params.index,
            "surface": surf_maj,
            "paraxial": paraxial,
        });
        if !sys.catalogue.est_connu(&surf_maj.verre) && !surf_maj.est_coupure() {
            reponse["avertissement"] = avertissement_verre(&surf_maj.verre).into();
        }
        if !alertes.is_empty() {
            reponse["alertes_solves"] = alertes.into();
        }
        serde_json::to_string_pretty(&reponse).unwrap_or_default()
    }

    /// Remplace le système courant par un fichier TDESIGN / LENSDES / RAYTRACE importé.
    #[tool(
        name = "importer_teloptik",
        description = "Importe un fichier .TDS (TDESIGN), .RLD (LENSDES) ou .DES (RAYTRACE) des programmes du livre « Telescope Optics » de Rutten et van Venrooij, et en fait le système courant. Retourne le système, ses données paraxiales, la focale inscrite dans le fichier et les éléments non repris (obstructions, couleurs supplémentaires…). Les systèmes basculés ou décentrés sont refusés."
    )]
    pub async fn importer_teloptik(
        &self,
        Parameters(params): Parameters<ImporterTeloptikParams>,
    ) -> String {
        match importer_fichier_teloptik(std::path::Path::new(&params.chemin)) {
            Ok(import) => {
                let mut sys = self.systeme.lock().unwrap();
                *sys = import.systeme;
                let reponse = serde_json::json!({
                    "systeme": &*sys,
                    "paraxial": calculer_paraxial(&sys),
                    "focale_fichier_mm": import.focale_fichier_mm,
                    "avertissements": import.avertissements,
                });
                serde_json::to_string_pretty(&reponse).unwrap_or_default()
            }
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Remplace le système courant par un fichier Zemax `.zmx` importé.
    #[tool(
        name = "importer_zemax",
        description = "Importe un fichier de lentilles Zemax OpticStudio séquentiel (.zmx, UTF-16 ou ASCII) et en fait le système courant : surfaces standard, asphériques paires, ruptures de repère, paraxiales et réseaux, verres (nom de catalogue ou modèle nd/Vd), ouvertures et obstructions, pupille, champs (angles, hauteurs objet ou image), longueurs d'onde. Les solves sont repris comme valeurs figées. Retourne le système, ses données paraxiales et les éléments non repris."
    )]
    pub async fn importer_zemax(&self, Parameters(params): Parameters<ImporterTeloptikParams>) -> String {
        match crate::core::zemax::importer_fichier_zemax(std::path::Path::new(&params.chemin)) {
            Ok(import) => {
                let mut sys = self.systeme.lock().unwrap();
                *sys = import.systeme;
                serde_json::to_string_pretty(&serde_json::json!({
                    "systeme": &*sys,
                    "paraxial": calculer_paraxial(&sys),
                    "avertissements": import.avertissements,
                }))
                .unwrap_or_default()
            }
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Énergie encerclée d'un champ.
    #[tool(
        name = "analyser_energie_encerclee",
        description = "Énergie encerclée d'un champ autour du centroïde de la tache : courbe géométrique (toutes longueurs d'onde pondérées), courbe de diffraction tirée de la PSF polychromatique, courbe d'Airy d'une pupille sans aberration, rayons contenant 50 % et 80 % de l'énergie (µm)."
    )]
    pub async fn analyser_energie_encerclee(&self, Parameters(params): Parameters<AnalyseOndeParams>) -> String {
        let sys = self.systeme.lock().unwrap();
        let champ = sys.champs_deg.get(params.champ_index.unwrap_or(0)).copied().unwrap_or((0.0, 0.0));
        match crate::core::energie::calculer_energie_encerclee(&sys, champ, true) {
            Ok(e) => {
                // Courbes réduites à 41 points
                let pas = (e.rayons_um.len() / 40).max(1);
                let reduire = |v: &[f64]| v.iter().step_by(pas).copied().collect::<Vec<f64>>();
                serde_json::json!({
                    "champ": e.champ_deg,
                    "r50_geometrique_um": e.r50_geo_um,
                    "r80_geometrique_um": e.r80_geo_um,
                    "r50_diffraction_um": e.r50_diffraction_um,
                    "r80_diffraction_um": e.r80_diffraction_um,
                    "rayon_airy_um": e.rayon_airy_um,
                    "rayons_um": reduire(&e.rayons_um),
                    "geometrique": reduire(&e.geometrique),
                    "diffraction": e.diffraction.as_deref().map(reduire),
                    "airy": reduire(&e.airy),
                })
                .to_string()
            }
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Décalage focal chromatique et couleur latérale.
    #[tool(
        name = "analyser_chromatisme",
        description = "Aberrations chromatiques : décalage du foyer paraxial en fonction de la longueur d'onde sur le domaine du système (étendue, profondeur de foyer de diffraction ±2λN²) et couleur latérale (écart de hauteur image du rayon principal entre longueurs d'onde en fonction du champ, comparé au rayon d'Airy)."
    )]
    pub async fn analyser_chromatisme(&self) -> String {
        let sys = self.systeme.lock().unwrap();
        let ou_erreur = |r: Result<serde_json::Value, String>| r.unwrap_or_else(|e| serde_json::json!({ "erreur": e }));
        serde_json::json!({
            "decalage_focal": ou_erreur(crate::core::chromatique::calculer_decalage_focal(&sys, 21).map(|d| serde_json::to_value(d).unwrap_or_default())),
            "couleur_laterale": ou_erreur(crate::core::chromatique::calculer_couleur_laterale(&sys, 11).map(|c| serde_json::to_value(c).unwrap_or_default())),
        })
        .to_string()
    }

    /// Empreinte des faisceaux et demi-diamètres nécessaires.
    #[tool(
        name = "analyser_empreinte",
        description = "Empreinte des faisceaux sur une surface (repère local) : impacts de tous les champs et longueurs d'onde, rayon utilisé, nombre de rayons arrêtés, et demi-diamètre nécessaire à chaque surface pour transmettre tous les faisceaux (comme le demi-diamètre automatique de Zemax) ; peut appliquer ces demi-diamètres."
    )]
    pub async fn analyser_empreinte(&self, Parameters(params): Parameters<AnalyserEmpreinteParams>) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let requis = crate::core::empreinte::demi_diametres_requis(&sys);
        if params.ajuster_demi_diametres == Some(true) {
            let n = sys.surfaces.len();
            for (i, (s, &r)) in sys.surfaces.iter_mut().zip(&requis).enumerate() {
                if !s.est_coupure() && r > 0.0 && (i + 1 < n || r > s.semi_diametre) {
                    s.semi_diametre = (r * 1000.0).ceil() / 1000.0;
                }
            }
        }
        let surface = params.index_surface.unwrap_or(0);
        let empreinte = crate::core::empreinte::calculer_empreinte(&sys, surface, 6).map(|e| {
            serde_json::json!({
                "surface": e.surface,
                "semi_diametre_mm": e.semi_diametre_mm,
                "rayon_utile_mm": e.rayon_utile_mm,
                "rayon_requis_mm": e.rayon_requis_mm,
                "nb_rayons": e.nb_rayons,
                "nb_vignettes": e.nb_vignettes,
                "points": if params.points == Some(true) { serde_json::to_value(&e.points).unwrap_or_default() } else { serde_json::Value::Null },
            })
        });
        serde_json::json!({
            "empreinte": empreinte.unwrap_or_else(|e| serde_json::json!({ "erreur": e })),
            "demi_diametres_requis_mm": requis,
            "demi_diametres_mm": sys.surfaces.iter().map(|s| s.semi_diametre).collect::<Vec<f64>>(),
        })
        .to_string()
    }

    /// Paramètres du système : objet, afocal, ouverture, champs, longueurs d'onde.
    #[tool(
        name = "definir_systeme",
        description = "Règle les paramètres du système : objet à l'infini (champs en degrés) ou à distance finie (champs en hauteurs d'objet, mm), espace image afocal, ouverture (pupille d'entrée, nombre d'ouverture ou ouverture numérique objet), champs et longueurs d'onde avec poids et référence."
    )]
    pub async fn definir_systeme(&self, Parameters(p): Parameters<DefinirSystemeParams>) -> String {
        use crate::core::types::{ApertureSpec, Wavelength};
        let mut sys = self.systeme.lock().unwrap();
        if p.objet_infini == Some(true) {
            sys.distance_objet_mm = None;
        }
        if let Some(d) = p.distance_objet_mm {
            sys.distance_objet_mm = (d > 0.0 && d.is_finite()).then_some(d);
        }
        if let Some(a) = p.afocal {
            sys.afocal = a;
        }
        if let Some(v) = p.ouverture_valeur {
            let genre = p.ouverture_type.as_deref().unwrap_or(match sys.ouverture {
                ApertureSpec::PupilleEntreeMm(_) => "pupille_entree_mm",
                ApertureSpec::NombreOuverture(_) => "nombre_ouverture",
                ApertureSpec::OuvertureNumeriqueObjet(_) => "ouverture_numerique_objet",
            });
            sys.ouverture = match genre {
                "nombre_ouverture" => ApertureSpec::NombreOuverture(v),
                "ouverture_numerique_objet" => ApertureSpec::OuvertureNumeriqueObjet(v.min(0.99)),
                _ => ApertureSpec::PupilleEntreeMm(v),
            };
        }
        if let Some(c) = p.champs {
            sys.champs_deg = c.into_iter().map(|c| (c[0], c[1])).collect();
        }
        if let Some(l) = p.longueurs_onde_nm {
            let poids = p.poids.unwrap_or_default();
            sys.longueurs_onde = l
                .iter()
                .enumerate()
                .map(|(i, &v)| Wavelength::new(v, poids.get(i).copied().unwrap_or(1.0), format!("{v:.1} nm")))
                .collect();
        }
        if let Some(r) = p.longueur_onde_ref_nm {
            sys.longueur_onde_ref_nm = r;
        }
        crate::core::solves::appliquer_solves(&mut sys);
        serde_json::json!({
            "distance_objet_mm": sys.distance_objet_mm,
            "afocal": sys.afocal,
            "ouverture": sys.ouverture,
            "champs": sys.champs_deg,
            "unite_champs": sys.unite_champ(),
            "longueurs_onde": sys.longueurs_onde,
            "longueur_onde_ref_nm": sys.longueur_onde_ref_nm,
            "paraxial": calculer_paraxial(&sys),
        })
        .to_string()
    }

    /// Analyse de spectrographe.
    #[tool(
        name = "analyser_spectre",
        description = "Analyse de spectrographe sur un domaine de longueurs d'onde : position de chaque longueur d'onde sur le détecteur (rayon principal), dispersion linéaire inverse (nm/mm), tache par longueur d'onde, largeur de l'image de la fente et pouvoir de résolution R = λ/Δλ."
    )]
    pub async fn analyser_spectre(&self, Parameters(p): Parameters<AnalyserSpectreParams>) -> String {
        let sys = self.systeme.lock().unwrap();
        let champ = sys.champs_deg.get(p.champ_index.unwrap_or(0)).copied().unwrap_or((0.0, 0.0));
        let cache = crate::core::onde::AnalysesOndes::default();
        let (a, b) = cache.domaine_spectral(&sys);
        let fente = p.largeur_fente.unwrap_or(if sys.objet_fini().is_some() { 20.0 } else { 2.0 });
        match crate::core::spectre::calculer_spectre(
            &sys,
            champ,
            p.lambda_min_nm.unwrap_or(a),
            p.lambda_max_nm.unwrap_or(b),
            p.nb.unwrap_or(21),
            fente,
        ) {
            Ok(s) => serde_json::to_string(&s).unwrap_or_default(),
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Interféromètre de Bath : simulation et réduction.
    #[tool(
        name = "simuler_bath",
        description = "Interféromètre de Bath : simule l'interférogramme d'un miroir concave testé à son centre de courbure (chemins exacts, écart source/référence et astigmatisme de Bath, basculement et défocalisation en franges, défauts de surface : Zernike, zone, bord rabattu) et le réduit par rapport à la conique visée : conique mesurée, PV/RMS du front d'onde et de la surface, Strehl à 550 nm, Zernike de l'écart. Peut enregistrer l'interférogramme en PNG."
    )]
    pub async fn simuler_bath(&self, Parameters(params): Parameters<SimulerBathParams>) -> String {
        let p = &params.parametres;
        match crate::core::bath::simuler_et_reduire(p) {
            Ok((sim, red)) => {
                let mut png = serde_json::Value::Null;
                if let Some(chemin) = &params.chemin_png {
                    let image = crate::core::franges::ImageGris::depuis_grille(&sim.intensite, sim.taille);
                    png = match image.enregistrer_png(std::path::Path::new(chemin)) {
                        Ok(()) => serde_json::json!(chemin),
                        Err(e) => serde_json::json!({ "erreur": e }),
                    };
                }
                serde_json::json!({
                    "rapport_ouverture": p.rapport_ouverture(),
                    "decalage_lateral_mm": [sim.decalage_mm.0, sim.decalage_mm.1],
                    "defocalisation_mm": sim.defocalisation_mm,
                    "astigmatisme_bath_theorique_pv_ondes": p.astigmatisme_bath_theorique_ondes(),
                    "reduction": resume_reduction_bath(&red),
                    "png": png,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Analyse d'une photo d'interférogramme de Bath.
    #[tool(
        name = "analyser_interferogramme",
        description = "Analyse la photo d'un interférogramme de Bath : repère le disque du miroir (ou prend celui donné), extrait le front d'onde par transformée de Fourier (Takeda) avec extrapolation de Gerchberg, déroulement guidé par la qualité et ajustement global du modèle d'image, puis le réduit comme simuler_bath (conique mesurée, PV/RMS, Strehl, Zernike). Il faut une dizaine à une vingtaine de franges de basculement."
    )]
    pub async fn analyser_interferogramme(&self, Parameters(params): Parameters<AnalyserInterferogrammeParams>) -> String {
        use crate::core::franges::{ImageGris, OptionsFranges, detecter_disque, extraire_phase};
        let resultat = (|| -> Result<serde_json::Value, String> {
            let p = &params.parametres;
            p.valider()?;
            let img = ImageGris::charger(std::path::Path::new(&params.chemin))?;
            let disque = match params.disque {
                Some(d) => d,
                None => detecter_disque(&img).ok_or_else(|| {
                    tr!(
                        "miroir non repéré dans la photo : donnez le disque (centre et rayon en pixels)",
                        "mirror not found in the photo: give the disc (centre and radius in pixels)",
                        "Spiegel im Foto nicht gefunden: Scheibe angeben (Mitte und Radius in Pixeln)"
                    )
                    .to_string()
                })?,
            };
            let opt = OptionsFranges {
                disque,
                marge_pct: params.marge_pct.unwrap_or(3.0),
                rho_obstruction: p.rho_obstruction(),
                taille: p.taille,
            };
            let carte = extraire_phase(&img, &opt)?;
            let red = crate::core::bath::reduire_carte(&carte.opd_ondes, p, params.signe.unwrap_or_default())?;
            Ok(serde_json::json!({
                "image_px": [img.largeur, img.hauteur],
                "disque": disque,
                "porteuse_franges": [carte.porteuse_franges.0, carte.porteuse_franges.1],
                "contraste": carte.contraste,
                "reduction": resume_reduction_bath(&red),
            }))
        })();
        match resultat {
            Ok(v) => v.to_string(),
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Importe un catalogue de verres .agf.
    #[tool(
        name = "importer_catalogue_agf",
        description = "Importe un catalogue de verres Zemax .agf (Schott, Ohara, CDGM…) : ses verres deviennent utilisables par leur nom dans tous les systèmes et sont conservés pour les prochains démarrages."
    )]
    pub async fn importer_catalogue_agf(&self, Parameters(params): Parameters<ImporterCatalogueParams>) -> String {
        match crate::core::glass::importer_catalogue_agf(std::path::Path::new(&params.chemin), true) {
            Ok(nb) => serde_json::json!({ "verres_importes": nb }).to_string(),
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Seidel, courbure de champ, distorsion et courbes de rayons.
    #[tool(
        name = "analyser_aberrations",
        description = "Aberrations géométriques : coefficients de Seidel par surface (sphérique, coma, astigmatisme, Petzval, distorsion, chromatismes axial et latéral) et coefficients d'onde équivalents, courbure de champ (foyers tangentiel et sagittal) et distorsion en fonction de l'angle de champ, courbes de rayons tangentielle et sagittale d'un champ."
    )]
    pub async fn analyser_aberrations(&self, Parameters(params): Parameters<AnalyseOndeParams>) -> String {
        use crate::core::aberrations::{calculer_courbes_rayons, calculer_courbure_distorsion, calculer_seidel};
        let sys = self.systeme.lock().unwrap();
        let champ = sys.champs_deg.get(params.champ_index.unwrap_or(0)).copied().unwrap_or((0.0, 0.0));
        let lambda = params.longueur_onde_nm.unwrap_or(sys.longueur_onde_ref_nm);
        let ou_erreur = |r: Result<serde_json::Value, String>| r.unwrap_or_else(|e| serde_json::json!({ "erreur": e }));
        serde_json::json!({
            "seidel": ou_erreur(calculer_seidel(&sys).map(|s| serde_json::to_value(s).unwrap_or_default())),
            "courbure_distorsion": ou_erreur(calculer_courbure_distorsion(&sys, lambda, 11).map(|c| serde_json::to_value(c).unwrap_or_default())),
            "courbes_rayons": ou_erreur(calculer_courbes_rayons(&sys, champ, 11).map(|c| serde_json::to_value(c).unwrap_or_default())),
        })
        .to_string()
    }

    /// Front d'onde, Strehl et coefficients de Zernike d'un champ.
    #[tool(
        name = "analyser_front_onde",
        description = "Calcule le front d'onde (OPD rapportée à la sphère de référence du rayon principal en pupille de sortie) d'un champ à une longueur d'onde : crête-à-creux et RMS en ondes, Strehl de Maréchal et Strehl de la PSF de diffraction, 37 coefficients de Zernike (Noll)."
    )]
    pub async fn analyser_front_onde(&self, Parameters(params): Parameters<AnalyseOndeParams>) -> String {
        let sys = self.systeme.lock().unwrap();
        let champ = sys.champs_deg.get(params.champ_index.unwrap_or(0)).copied().unwrap_or((0.0, 0.0));
        let lambda = params.longueur_onde_nm.unwrap_or(sys.longueur_onde_ref_nm);
        let resultat = calculer_front_onde(&sys, champ, lambda, 64).and_then(|carte| {
            let (psf, _) = calculer_psf_ftm(&sys, champ, Some(lambda))?;
            Ok((carte, psf.strehl))
        });
        match resultat {
            Ok((carte, strehl_psf)) => serde_json::json!({
                "champ_deg": carte.champ_deg,
                "longueur_onde_nm": carte.longueur_onde_nm,
                "pv_ondes": carte.pv_ondes,
                "rms_ondes": carte.rms_ondes,
                "strehl_marechal": carte.strehl_marechal,
                "strehl_psf": strehl_psf,
                "nb_rayons": carte.nb_points,
                "residu_zernike_ondes": carte.residu_zernike_ondes,
                "zernike_noll_ondes": carte.zernike_ondes.iter().enumerate().map(|(k, c)| serde_json::json!({
                    "j": k + 1,
                    "nom": nom_zernike(k + 1),
                    "ondes": c,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// FTM tangentielle et sagittale d'un champ.
    #[tool(
        name = "calculer_ftm",
        description = "Calcule la FTM de diffraction (FFT de la PSF) d'un champ, tangentielle et sagittale, avec la limite de diffraction de la même pupille, polychromatique par défaut ; retourne aussi le rapport de Strehl de la PSF."
    )]
    pub async fn calculer_ftm(&self, Parameters(params): Parameters<AnalyseOndeParams>) -> String {
        let sys = self.systeme.lock().unwrap();
        let champ = sys.champs_deg.get(params.champ_index.unwrap_or(0)).copied().unwrap_or((0.0, 0.0));
        match calculer_psf_ftm(&sys, champ, params.longueur_onde_nm) {
            Ok((psf, ftm)) => {
                let n = params.nb_frequences.unwrap_or(11).clamp(2, ftm.frequences_cyc_mm.len());
                let dernier = ftm.frequences_cyc_mm.len() - 1;
                let points: Vec<_> = (0..n)
                    .map(|k| {
                        let i = (k * dernier + (n - 1) / 2) / (n - 1);
                        serde_json::json!({
                            "frequence_cyc_mm": ftm.frequences_cyc_mm[i],
                            "tangentielle": ftm.tangentielle[i],
                            "sagittale": ftm.sagittale[i],
                            "limite_diffraction": ftm.limite_diffraction[i],
                        })
                    })
                    .collect();
                serde_json::json!({
                    "champ_deg": ftm.champ_deg,
                    "longueurs_onde_nm": psf.longueurs_onde_nm,
                    "strehl": psf.strehl,
                    "frequence_coupure_cyc_mm": ftm.frequence_coupure_cyc_mm,
                    "ftm": points,
                })
                .to_string()
            }
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Exécute le lancer de rayons séquentiel et retourne les données brutes (paraxial, rayons, LSA).
    #[tool(
        name = "lancer_simulation",
        description = "Exécute le ray tracing séquentiel et retourne les données brutes : paramètres paraxiaux, trajectoires 3D des rayons et aberration sphérique longitudinale."
    )]
    pub async fn lancer_simulation(&self) -> String {
        let sys = self.systeme.lock().unwrap();
        let paraxial = calculer_paraxial(&sys);
        let rayons_meridiens =
            tracer_eventail_meridional(&sys, 0.0, 9, sys.longueur_onde_ref_nm);
        let aberration_spherique = calculer_aberration_spherique_longitudinale(&sys, 12);

        let out = SimulationOutput {
            systeme: sys.clone(),
            paraxial,
            rayons_meridiens,
            aberration_spherique,
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }

    /// Calcule et retourne les coordonnées des points d'impact au plan image et le rayon RMS du Spot Diagram.
    #[tool(
        name = "obtenir_spot_diagram",
        description = "Retourne les coordonnées des points d'impact au plan image, le centroïde, le rayon RMS (µm et mm), le rayon géométrique maximal et le rayon d'Airy."
    )]
    pub async fn obtenir_spot_diagram(&self) -> String {
        let sys = self.systeme.lock().unwrap();
        let spot: SpotDiagramResult = calculer_spot_diagram(&sys, (0.0, 0.0), 4);
        serde_json::to_string_pretty(&spot).unwrap_or_default()
    }

    /// Lance l'algorithme d'optimisation Damped Least Squares (DLS) selon les variables et cibles fournies.
    #[tool(
        name = "optimiser_systeme",
        description = "Lance l'algorithme d'optimisation Damped Least Squares (DLS / Levenberg-Marquardt) sur les variables sélectionnées pour atteindre une focale ou minimiser le Spot RMS / l'aberration sphérique."
    )]
    pub async fn optimiser_systeme(
        &self,
        Parameters(params): Parameters<OptimiserSystemeParams>,
    ) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let rapport = optimiser_systeme_dls(&mut sys, &params.variables, &params.cibles, 20);
        let out = OptimisationOutput {
            rapport,
            systeme_optimise: sys.clone(),
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptions_traduites_pour_chaque_outil() {
        let routeur = FreeOpticalMcpServer::routeur_outils();
        assert_eq!(routeur.map.len(), DESCRIPTIONS_OUTILS.len());
        for (nom, _, _) in DESCRIPTIONS_OUTILS {
            assert!(routeur.map.contains_key(nom), "outil {nom} absent");
        }
    }

    #[tokio::test]
    async fn test_outils_mcp_workflow_complet() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());

        // 1. creer_systeme
        let res_creer = srv
            .creer_systeme(Parameters(CreerSystemeParams {
                nom: "Singlet Test MCP".to_string(),
                longueur_onde_ref: 587.56,
            }))
            .await;
        assert!(res_creer.contains("Singlet Test MCP"));

        // 2. ajouter_surface (Entrée et Sortie de la lentille BK7)
        srv.ajouter_surface(Parameters(AjouterSurfaceParams {
            r#type: "SPHERIQUE".to_string(),
            rayon: 60.0,
            epaisseur: 5.0,
            verre: "BK7".to_string(),
            semi_diametre: 15.0,
        }))
        .await;

        srv.ajouter_surface(Parameters(AjouterSurfaceParams {
            r#type: "SPHERIQUE".to_string(),
            rayon: -180.0,
            epaisseur: 85.0,
            verre: "AIR".to_string(),
            semi_diametre: 15.0,
        }))
        .await;

        // 3. modifier_surface
        let res_mod = srv
            .modifier_surface(Parameters(ModifierSurfaceParams {
                index: 0,
                parametres: SurfacePatchParams {
                    rayon: Some(65.0),
                    commentaire: Some("Entrée Singlet Modifiée".to_string()),
                    ..Default::default()
                },
            }))
            .await;
        assert!(res_mod.contains("Surface modifiée avec succès"));

        // 4. lancer_simulation
        let res_sim = srv.lancer_simulation().await;
        assert!(res_sim.contains("focale_effl_mm"));

        // 5. obtenir_spot_diagram
        let res_spot = srv.obtenir_spot_diagram().await;
        assert!(res_spot.contains("rayon_rms_um"));

        // 6. optimiser_systeme
        let res_opt = srv
            .optimiser_systeme(Parameters(OptimiserSystemeParams {
                variables: vec![
                    OptimizationVariable {
                        index_surface: 1,
                        parametre: "rayon".to_string(),
                    },
                    OptimizationVariable {
                        index_surface: 1,
                        parametre: "epaisseur".to_string(),
                    },
                ],
                cibles: vec![
                    OptimizationTarget {
                        type_cible: "focale".to_string(),
                        valeur_cible: 100.0,
                        poids: 1.0,
                    },
                    OptimizationTarget {
                        type_cible: "spot_rms".to_string(),
                        valeur_cible: 0.0,
                        poids: 1.0,
                    },
                ],
            }))
            .await;
        assert!(res_opt.contains("DLS terminé"));
    }

    #[tokio::test]
    async fn test_outils_mcp_systeme_et_spectre() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        let res = srv
            .definir_systeme(Parameters(DefinirSystemeParams {
                distance_objet_mm: Some(1000.0),
                champs: Some(vec![[0.0, 0.0], [0.0, 10.0]]),
                ..Default::default()
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(json["unite_champs"], "mm");
        assert!(json["paraxial"]["grandissement"].as_f64().unwrap() < 0.0);
        let res = srv.analyser_spectre(Parameters(AnalyserSpectreParams::default())).await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(json["longueurs_onde_nm"].as_array().unwrap().len(), 21, "{res}");
    }

    #[tokio::test]
    async fn test_outil_mcp_aberrations() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        let res = srv
            .analyser_aberrations(Parameters(AnalyseOndeParams { champ_index: Some(1), ..Default::default() }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(json["seidel"]["lignes"].as_array().unwrap().len(), 3);
        assert!(json["courbure_distorsion"]["distorsion_pct"].is_array(), "{res}");
        assert!(json["courbes_rayons"]["transverse_t_um"].is_array());
        let erreur = srv
            .importer_catalogue_agf(Parameters(ImporterCatalogueParams { chemin: "/inexistant.agf".into() }))
            .await;
        assert!(erreur.contains("erreur"));
    }

    #[tokio::test]
    async fn test_outils_mcp_v09_solves_ruptures_analyses_zemax() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        // Solve de mise au point : l'épaisseur suit un changement de rayon
        let res = srv
            .modifier_surface(Parameters(ModifierSurfaceParams {
                index: 2,
                parametres: SurfacePatchParams {
                    solve_epaisseur: Some(crate::core::types::Solve::HauteurMarginale { hauteur_mm: 0.0, zone: 0.0 }),
                    ..Default::default()
                },
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        let bfl = json["paraxial"]["tirage_bfl_mm"].as_f64().unwrap();
        assert!((json["surface"]["epaisseur"].as_f64().unwrap() - bfl).abs() < 1e-9, "{res}");
        let res = srv
            .modifier_surface(Parameters(ModifierSurfaceParams {
                index: 0,
                parametres: SurfacePatchParams { rayon: Some(50.0), ..Default::default() },
            }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        let sys = srv.systeme.lock().unwrap().clone();
        assert!((sys.surfaces[2].epaisseur - json["paraxial"]["tirage_bfl_mm"].as_f64().unwrap()).abs() < 1e-9);
        drop(sys);
        // Rupture de repère par JSON
        let schema = serde_json::from_str::<SurfacePatchParams>(r#"{"type":"COORDBRK","inclinaison_deg":[10,0],"inclinaison_z_deg":5}"#).unwrap();
        assert_eq!(schema.inclinaison_z_deg, Some(5.0));
        // Analyses
        let e: serde_json::Value = serde_json::from_str(&srv.analyser_energie_encerclee(Parameters(AnalyseOndeParams::default())).await).unwrap();
        assert!(e["r80_geometrique_um"].as_f64().unwrap() > 0.0 && e["diffraction"].is_array(), "{e}");
        let c: serde_json::Value = serde_json::from_str(&srv.analyser_chromatisme().await).unwrap();
        assert!(c["decalage_focal"]["etendue_um"].as_f64().unwrap() > 0.0 && c["couleur_laterale"]["ecart_um"].is_array(), "{c}");
        let f: serde_json::Value = serde_json::from_str(
            &srv.analyser_empreinte(Parameters(AnalyserEmpreinteParams { index_surface: Some(2), ajuster_demi_diametres: Some(true), points: None })).await,
        )
        .unwrap();
        assert_eq!(f["empreinte"]["nb_vignettes"].as_u64(), Some(0), "{f}");
        let requis = f["demi_diametres_requis_mm"][0].as_f64().unwrap();
        assert!((f["demi_diametres_mm"][0].as_f64().unwrap() - requis).abs() < 1e-3);
        // Import Zemax : erreur propre sur un fichier absent
        let z = srv.importer_zemax(Parameters(ImporterTeloptikParams { chemin: "/inexistant.zmx".into() })).await;
        assert!(z.contains("erreur"));
    }

    #[tokio::test]
    async fn test_outils_mcp_front_onde_et_ftm() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        let res = srv.analyser_front_onde(Parameters(AnalyseOndeParams::default())).await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        let strehl = json["strehl_psf"].as_f64().unwrap();
        assert!(strehl > 0.0 && strehl <= 1.0, "{res}");
        assert_eq!(json["zernike_noll_ondes"].as_array().unwrap().len(), 37);

        let res = srv
            .calculer_ftm(Parameters(AnalyseOndeParams { champ_index: Some(1), nb_frequences: Some(5), ..Default::default() }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        let ftm = json["ftm"].as_array().unwrap();
        assert_eq!(ftm.len(), 5);
        assert!((ftm[0]["tangentielle"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        let coupure = json["frequence_coupure_cyc_mm"].as_f64().unwrap();
        assert!((ftm[4]["frequence_cyc_mm"].as_f64().unwrap() - coupure).abs() < 1e-6 * coupure);
    }

    #[tokio::test]
    async fn test_outils_mcp_bath() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        let png = std::env::temp_dir().join(format!("freeoptical_bath_{}.png", std::process::id()));
        let parametres: crate::core::bath::ParametresBath =
            serde_json::from_value(serde_json::json!({ "conique": -0.9, "basculement_y_franges": 12.0, "defocalisation_franges": -2.0 })).unwrap();
        let res = srv
            .simuler_bath(Parameters(SimulerBathParams { parametres: parametres.clone(), chemin_png: Some(png.display().to_string()) }))
            .await;
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        let k = json["reduction"]["conique_mesuree"].as_f64().unwrap();
        assert!((k + 0.9).abs() < 0.005, "{res}");

        // L'interférogramme enregistré, relu comme une photo, redonne la même conique
        let res = srv
            .analyser_interferogramme(Parameters(AnalyserInterferogrammeParams {
                chemin: png.display().to_string(),
                parametres,
                ..Default::default()
            }))
            .await;
        std::fs::remove_file(&png).ok();
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        let k = json["reduction"]["conique_mesuree"].as_f64().unwrap_or(f64::NAN);
        assert!((k + 0.9).abs() < 0.02, "{res}");
        let erreur = srv
            .analyser_interferogramme(Parameters(AnalyserInterferogrammeParams { chemin: "/inexistant.png".into(), ..Default::default() }))
            .await;
        assert!(erreur.contains("erreur"));
    }

    #[tokio::test]
    async fn test_outil_mcp_import_teloptik() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        let chemin = std::env::temp_dir().join(format!("freeoptical_test_{}.TDS", std::process::id()));
        std::fs::write(
            &chemin,
            "\"Parabole\"\r\n1,1,200,-1000,0\r\n-2000,0,-1\r\n0,0,0,0,0\r\n-1\r\n-1\r\n\"N\",\"N\"\r\n",
        )
        .unwrap();
        let res = srv
            .importer_teloptik(Parameters(ImporterTeloptikParams {
                chemin: chemin.display().to_string(),
            }))
            .await;
        std::fs::remove_file(&chemin).ok();
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(json["systeme"]["nom"], "Parabole");
        let effl = json["paraxial"]["focale_effl_mm"].as_f64().unwrap();
        assert!((effl - 1000.0).abs() < 1e-6, "EFFL = {effl}");
        assert_eq!(srv.systeme.lock().unwrap().surfaces.len(), 2);

        let erreur = srv
            .importer_teloptik(Parameters(ImporterTeloptikParams {
                chemin: "/inexistant.TDS".to_string(),
            }))
            .await;
        assert!(erreur.contains("erreur"));
    }
}

