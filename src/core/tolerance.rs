//! Tolérancement, à la manière de Zemax : chaque tolérance perturbe un paramètre (rayon,
//! franges de puissance, épaisseur, conique, décentrement et inclinaison d'une surface ou d'un
//! élément entier, indice et constringence d'un verre) entre un minimum et un maximum.
//!
//! * **Sensibilités** : le critère (tache RMS, front d'onde RMS ou FTM à une fréquence) est
//!   calculé pour chaque tolérance seule à son minimum et à son maximum, le compensateur
//!   (mise au point) étant réajusté ; l'écart quadratique (RSS) des pires écarts estime la
//!   dégradation de l'ensemble.
//! * **Monte-Carlo** : toutes les tolérances sont tirées au hasard ensemble (loi uniforme ou
//!   normale tronquée à ±2σ), le compensateur réajusté ; statistiques et pire tirage.
//!
//! Les éléments (lentilles, groupes) sont décentrés et inclinés autour du sommet de leur
//! première surface par une paire de ruptures de repère, comme dans Zemax. Les solves du
//! système restent actifs pendant l'analyse (une reprise suit sa surface, une mise au point par
//! solve agit comme un compensateur).

use crate::core::glass::MediumKind;
use crate::core::onde::{calculer_psf_ftm, opd_pupille};
use crate::core::paraxial::calculer_paraxial;
use crate::core::raytracer::{VisePupille, coordonnees_image, tracer_rayon_unique};
use crate::core::solves::appliquer_solves;
use crate::core::types::{OpticalSystem, Solve, Surface, SurfaceType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Paramètre perturbé par une tolérance
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum TypeTolerance {
    /// Rayon de courbure (± mm)
    Rayon,
    /// Puissance en franges (anneaux de Newton à 546,1 nm sur le diamètre utile, ± franges)
    Franges,
    /// Épaisseur après la surface (± mm)
    Epaisseur,
    /// Constante conique (± Δk)
    Conique,
    /// Décentrement de la surface seule (± mm)
    DecentrementX,
    DecentrementY,
    /// Inclinaison de la surface seule autour de son sommet (± degrés)
    InclinaisonX,
    InclinaisonY,
    /// Décentrement d'un élément (surfaces `surface` à `derniere`, ± mm)
    DecentrementElementX,
    DecentrementElementY,
    /// Inclinaison d'un élément autour du sommet de sa première surface (± degrés)
    InclinaisonElementX,
    InclinaisonElementY,
    /// Indice du verre après la surface (± Δn)
    Indice,
    /// Constringence du verre après la surface (± %)
    Abbe,
}

impl TypeTolerance {
    pub const TOUS: [TypeTolerance; 14] = [
        TypeTolerance::Franges,
        TypeTolerance::Rayon,
        TypeTolerance::Epaisseur,
        TypeTolerance::Conique,
        TypeTolerance::DecentrementX,
        TypeTolerance::DecentrementY,
        TypeTolerance::InclinaisonX,
        TypeTolerance::InclinaisonY,
        TypeTolerance::DecentrementElementX,
        TypeTolerance::DecentrementElementY,
        TypeTolerance::InclinaisonElementX,
        TypeTolerance::InclinaisonElementY,
        TypeTolerance::Indice,
        TypeTolerance::Abbe,
    ];

    /// Code court à la Zemax
    pub fn code(self) -> &'static str {
        match self {
            TypeTolerance::Rayon => "TRAD",
            TypeTolerance::Franges => "TFRN",
            TypeTolerance::Epaisseur => "TTHI",
            TypeTolerance::Conique => "TCON",
            TypeTolerance::DecentrementX => "TSDX",
            TypeTolerance::DecentrementY => "TSDY",
            TypeTolerance::InclinaisonX => "TSTX",
            TypeTolerance::InclinaisonY => "TSTY",
            TypeTolerance::DecentrementElementX => "TEDX",
            TypeTolerance::DecentrementElementY => "TEDY",
            TypeTolerance::InclinaisonElementX => "TETX",
            TypeTolerance::InclinaisonElementY => "TETY",
            TypeTolerance::Indice => "TIND",
            TypeTolerance::Abbe => "TABB",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            TypeTolerance::Rayon => tr!("rayon", "radius", "Radius"),
            TypeTolerance::Franges => tr!("puissance (franges)", "power (fringes)", "Passe (Ringe)"),
            TypeTolerance::Epaisseur => tr!("épaisseur", "thickness", "Dicke"),
            TypeTolerance::Conique => tr!("conique", "conic", "Konik"),
            TypeTolerance::DecentrementX => tr!("décentr. surface X", "surface decenter X", "Flächendezentr. X"),
            TypeTolerance::DecentrementY => tr!("décentr. surface Y", "surface decenter Y", "Flächendezentr. Y"),
            TypeTolerance::InclinaisonX => tr!("inclin. surface X", "surface tilt X", "Flächenkippung X"),
            TypeTolerance::InclinaisonY => tr!("inclin. surface Y", "surface tilt Y", "Flächenkippung Y"),
            TypeTolerance::DecentrementElementX => tr!("décentr. élément X", "element decenter X", "Elementdezentr. X"),
            TypeTolerance::DecentrementElementY => tr!("décentr. élément Y", "element decenter Y", "Elementdezentr. Y"),
            TypeTolerance::InclinaisonElementX => tr!("inclin. élément X", "element tilt X", "Elementkippung X"),
            TypeTolerance::InclinaisonElementY => tr!("inclin. élément Y", "element tilt Y", "Elementkippung Y"),
            TypeTolerance::Indice => tr!("indice", "index", "Brechzahl"),
            TypeTolerance::Abbe => tr!("constringence", "Abbe number", "Abbe-Zahl"),
        }
    }

    pub fn unite(self) -> &'static str {
        match self {
            TypeTolerance::Rayon | TypeTolerance::Epaisseur => "mm",
            TypeTolerance::DecentrementX | TypeTolerance::DecentrementY => "mm",
            TypeTolerance::DecentrementElementX | TypeTolerance::DecentrementElementY => "mm",
            TypeTolerance::InclinaisonX | TypeTolerance::InclinaisonY => "°",
            TypeTolerance::InclinaisonElementX | TypeTolerance::InclinaisonElementY => "°",
            TypeTolerance::Franges => tr!("fr.", "fr.", "R."),
            TypeTolerance::Abbe => "%",
            TypeTolerance::Conique | TypeTolerance::Indice => "",
        }
    }

    /// Vrai pour les tolérances portant sur un élément (plage de surfaces)
    pub fn est_element(self) -> bool {
        matches!(
            self,
            TypeTolerance::DecentrementElementX
                | TypeTolerance::DecentrementElementY
                | TypeTolerance::InclinaisonElementX
                | TypeTolerance::InclinaisonElementY
        )
    }
}

fn est_nul_usize(v: &usize) -> bool {
    *v == 0
}

/// Une tolérance : paramètre, surface (numéro affiché, à partir de 1), dernière surface de
/// l'élément pour les tolérances d'élément, et bornes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tolerance {
    #[serde(rename = "type")]
    pub type_tolerance: TypeTolerance,
    /// Numéro de la surface (à partir de 1) ; première surface de l'élément
    pub surface: usize,
    /// Dernière surface de l'élément (tolérances d'élément ; 0 = la même que `surface`)
    #[serde(default, skip_serializing_if = "est_nul_usize")]
    pub derniere: usize,
    pub min: f64,
    pub max: f64,
}

impl Tolerance {
    pub fn nouvelle(type_tolerance: TypeTolerance, surface: usize, derniere: usize, ecart: f64) -> Self {
        Self { type_tolerance, surface, derniere, min: -ecart, max: ecart }
    }

    /// Surfaces concernées (indices 0..N-1)
    fn plage(&self) -> (usize, usize) {
        let a = self.surface.saturating_sub(1);
        let b = if self.type_tolerance.est_element() && self.derniere > 0 { self.derniere - 1 } else { a };
        (a.min(b), a.max(b))
    }

    pub fn libelle(&self) -> String {
        let (a, b) = self.plage();
        let surfaces = if b > a { format!("{}-{}", a + 1, b + 1) } else { format!("{}", a + 1) };
        format!("{} {} ({})", self.type_tolerance.code(), surfaces, self.type_tolerance.libelle())
    }
}

/// Critère de qualité
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "type")]
pub enum Critere {
    /// Rayon RMS de la tache (µm ; µrad en afocal), moyenne quadratique des champs
    #[default]
    SpotRms,
    /// Front d'onde RMS (ondes), toutes couleurs, moyenne quadratique des champs
    FrontOndeRms,
    /// FTM polychromatique à une fréquence (moyenne tangentielle / sagittale et des champs)
    Ftm { frequence: f64 },
}

impl Critere {
    /// Vrai si une valeur plus grande est meilleure (FTM)
    pub fn plus_grand_meilleur(self) -> bool {
        matches!(self, Critere::Ftm { .. })
    }

    pub fn libelle(self) -> String {
        match self {
            Critere::SpotRms => tr!("tache RMS (µm)", "RMS spot (µm)", "RMS-Fleck (µm)").to_string(),
            Critere::FrontOndeRms => tr!("front d'onde RMS (λ)", "RMS wavefront (λ)", "RMS-Wellenfront (λ)").to_string(),
            Critere::Ftm { frequence } => trf!("FTM à {} c/mm", "MTF at {} c/mm", "MTF bei {} P/mm", frequence),
        }
    }
}

/// Compensateur réajusté pour chaque système perturbé
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub enum Compensateur {
    /// Aucun : le plan image reste en place
    Aucun,
    /// Plan image au foyer paraxial
    MiseAuPointParaxiale,
    /// Plan image à la meilleure mise au point pour le critère
    #[default]
    MeilleureMiseAuPoint,
}

/// Loi des tirages Monte-Carlo
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub enum Distribution {
    /// Uniforme entre le minimum et le maximum
    Uniforme,
    /// Normale centrée, écart type = quart de l'intervalle, tronquée aux bornes (±2σ)
    #[default]
    Normale,
}

/// Réglages de l'analyse
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReglagesTolerance {
    #[serde(default)]
    pub critere: Critere,
    #[serde(default)]
    pub compensateur: Compensateur,
    /// Nombre de tirages Monte-Carlo (0 = sensibilités seules)
    #[serde(default = "tirages_defaut")]
    pub nb_tirages: usize,
    #[serde(default)]
    pub distribution: Distribution,
    /// Graine du générateur aléatoire (mêmes tirages pour une même graine)
    #[serde(default = "graine_defaut")]
    pub graine: u64,
}

fn tirages_defaut() -> usize {
    100
}
fn graine_defaut() -> u64 {
    1
}

impl Default for ReglagesTolerance {
    fn default() -> Self {
        Self {
            critere: Critere::default(),
            compensateur: Compensateur::default(),
            nb_tirages: tirages_defaut(),
            distribution: Distribution::default(),
            graine: graine_defaut(),
        }
    }
}

/// Résultat d'une tolérance seule à ses deux bornes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LigneSensibilite {
    pub tolerance: Tolerance,
    pub libelle: String,
    pub critere_min: f64,
    pub critere_max: f64,
    /// Écarts au nominal (positifs = dégradation, quel que soit le critère)
    pub degradation_min: f64,
    pub degradation_max: f64,
    /// Déplacement du compensateur (mm) aux deux bornes
    pub compensation_min: f64,
    pub compensation_max: f64,
}

impl LigneSensibilite {
    pub fn pire(&self) -> f64 {
        self.degradation_min.max(self.degradation_max)
    }
}

/// Statistiques des tirages Monte-Carlo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarlo {
    /// Valeur du critère pour chaque tirage (NaN : système inutilisable)
    pub valeurs: Vec<f64>,
    pub moyenne: f64,
    pub ecart_type: f64,
    pub meilleur: f64,
    pub pire: f64,
    /// (pourcentage de tirages, valeur atteinte au moins par ce pourcentage), du plus
    /// exigeant au moins exigeant : « 90 % des tirages font mieux que x »
    pub centiles: Vec<(f64, f64)>,
    /// Déplacement du compensateur : moyenne et plus grand écart (mm)
    pub compensation_moyenne: f64,
    pub compensation_max: f64,
    pub index_pire: usize,
    pub nb_echecs: usize,
}

/// Résultat complet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultatTolerance {
    pub critere: Critere,
    pub nominal: f64,
    pub lignes: Vec<LigneSensibilite>,
    /// Dégradation estimée : racine de la somme des carrés des pires dégradations
    pub degradation_rss: f64,
    /// Critère estimé (nominal dégradé du RSS)
    pub critere_estime: f64,
    pub monte_carlo: Option<MonteCarlo>,
    pub avertissements: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Critères
// ---------------------------------------------------------------------------------------------

fn champs(sys: &OpticalSystem) -> Vec<(f64, f64)> {
    if sys.champs_deg.is_empty() { vec![(0.0, 0.0)] } else { sys.champs_deg.clone() }
}

fn couleurs(sys: &OpticalSystem) -> Vec<(f64, f64)> {
    if sys.longueurs_onde.is_empty() {
        vec![(sys.longueur_onde_ref_nm, 1.0)]
    } else {
        sys.longueurs_onde.iter().map(|w| (w.valeur_nm, w.poids.max(0.0))).collect()
    }
}

/// Points pondérés de la pupille (px, py, poids ; poids de somme 1) : quadrature de Gauss-Legendre
/// en ρ² sur 4 anneaux de 12 points, exacte pour les polynômes radiaux jusqu'à ρ¹⁴ (la variance
/// d'une aberration sphérique du 3e ordre est en ρ⁸) ; une grille régulière surpondérerait le bord
fn pupille_gauss() -> Vec<(f64, f64, f64)> {
    const NOEUDS: [(f64, f64); 4] = [
        (-0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
        (-0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
        (0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
        (0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
    ];
    let mut pts = Vec::with_capacity(48);
    for (k, &(x, w)) in NOEUDS.iter().enumerate() {
        let rho = (0.5 * (x + 1.0)).sqrt();
        for j in 0..12 {
            let t = std::f64::consts::TAU * (j as f64 + 0.25 + 0.5 * (k % 2) as f64) / 12.0;
            pts.push((rho * t.cos(), rho * t.sin(), 0.5 * w / 12.0));
        }
    }
    pts
}

/// Valeur donnée à un système dont les rayons ne passent plus
const ECHEC: f64 = f64::NAN;

/// Tache RMS (µm), moyenne quadratique des champs, couleurs pondérées
pub fn tache_rms_um(sys: &OpticalSystem) -> f64 {
    let vise = VisePupille::calculer(sys);
    let pts = pupille_gauss();
    let mut somme = 0.0;
    let ch = champs(sys);
    for &c in &ch {
        let mut impacts = Vec::new();
        for &(l, wl) in &couleurs(sys) {
            for &(px, py, wp) in &pts {
                let tr = tracer_rayon_unique(sys, vise.rayon(px, py, c.0, c.1, l), c, (px, py));
                if let Some(h) = tr.impact_plan_image() {
                    let (x, y) = coordonnees_image(sys, h);
                    if x.is_finite() && y.is_finite() {
                        impacts.push((x, y, wl * wp));
                    }
                }
            }
        }
        let poids: f64 = impacts.iter().map(|p| p.2).sum();
        if impacts.len() < pts.len() / 2 || poids <= 0.0 {
            return ECHEC;
        }
        let cx = impacts.iter().map(|p| p.0 * p.2).sum::<f64>() / poids;
        let cy = impacts.iter().map(|p| p.1 * p.2).sum::<f64>() / poids;
        somme += impacts.iter().map(|p| p.2 * ((p.0 - cx).powi(2) + (p.1 - cy).powi(2))).sum::<f64>() / poids;
    }
    1000.0 * (somme / ch.len() as f64).sqrt()
}

/// Front d'onde RMS (ondes), toutes couleurs (rapportées au point image de la longueur d'onde
/// de référence), moyenne quadratique des champs
pub fn front_onde_rms(sys: &OpticalSystem) -> f64 {
    let pts = pupille_gauss();
    let xy: Vec<(f64, f64)> = pts.iter().map(|p| (p.0, p.1)).collect();
    let ch = champs(sys);
    let mut somme = 0.0;
    for &c in &ch {
        let (mut var, mut poids) = (0.0, 0.0);
        for &(l, wl) in &couleurs(sys) {
            let Ok(opd) = opd_pupille(sys, c, l, &xy) else { return ECHEC };
            let v: Vec<(f64, f64)> = opd.iter().zip(&pts).filter_map(|(o, p)| o.map(|o| (o, p.2))).collect();
            let w: f64 = v.iter().map(|x| x.1).sum();
            if v.len() < pts.len() / 2 || w <= 0.0 {
                return ECHEC;
            }
            let m = v.iter().map(|x| x.0 * x.1).sum::<f64>() / w;
            var += wl * v.iter().map(|x| x.1 * (x.0 - m).powi(2)).sum::<f64>() / w;
            poids += wl;
        }
        somme += var / poids.max(1e-12);
    }
    (somme / ch.len() as f64).sqrt()
}

/// FTM polychromatique moyenne (tangentielle et sagittale, tous champs) à une fréquence
pub fn ftm_moyenne(sys: &OpticalSystem, frequence: f64) -> f64 {
    let ch = champs(sys);
    let mut somme = 0.0;
    for &c in &ch {
        let Ok((_, ftm)) = calculer_psf_ftm(sys, c, None) else { return ECHEC };
        let interp = |v: &[f64]| -> f64 {
            let f = &ftm.frequences_cyc_mm;
            if f.len() < 2 || frequence >= *f.last().unwrap() {
                return 0.0;
            }
            let k = f.iter().position(|&x| x > frequence).unwrap_or(1).max(1);
            let t = (frequence - f[k - 1]) / (f[k] - f[k - 1]);
            v[k - 1] + t * (v[k] - v[k - 1])
        };
        somme += 0.5 * (interp(&ftm.tangentielle) + interp(&ftm.sagittale));
    }
    somme / ch.len() as f64
}

pub fn evaluer_critere(sys: &OpticalSystem, critere: Critere) -> f64 {
    match critere {
        Critere::SpotRms => tache_rms_um(sys),
        Critere::FrontOndeRms => front_onde_rms(sys),
        Critere::Ftm { frequence } => ftm_moyenne(sys, frequence),
    }
}

/// Dégradation (positive = moins bon) d'une valeur par rapport au nominal
fn degradation(critere: Critere, valeur: f64, nominal: f64) -> f64 {
    if !valeur.is_finite() {
        return f64::INFINITY;
    }
    if critere.plus_grand_meilleur() { nominal - valeur } else { valeur - nominal }
}

// ---------------------------------------------------------------------------------------------
// Compensateur
// ---------------------------------------------------------------------------------------------

/// Applique les solves et le compensateur ; retourne la valeur du critère et le déplacement du
/// plan image imposé par le compensateur (mm), mesuré dans ce système (des ruptures de repère
/// insérées pour un élément peuvent changer la surface qui porte la dernière épaisseur)
pub fn compenser_et_evaluer(sys: &mut OpticalSystem, reglages: &ReglagesTolerance) -> (f64, f64) {
    appliquer_solves(sys);
    let n = sys.surfaces.len();
    if n < 2 {
        return (ECHEC, 0.0);
    }
    let k = n - 2;
    let depart = sys.surfaces[k].epaisseur;
    if reglages.compensateur != Compensateur::Aucun && !sys.afocal {
        // Foyer paraxial d'abord
        // (le compensateur remplace un éventuel solve de mise au point du système)
        sys.surfaces[k].solve_epaisseur = Some(Solve::HauteurMarginale { hauteur_mm: 0.0, zone: 0.0 });
        appliquer_solves(sys);
        sys.surfaces[k].solve_epaisseur = None;
        if reglages.compensateur == Compensateur::MeilleureMiseAuPoint {
            // Puis recherche de la meilleure mise au point (section dorée) sur un critère rapide
            let mesure = |s: &OpticalSystem| -> f64 {
                let v = match reglages.critere {
                    Critere::SpotRms => tache_rms_um(s),
                    _ => front_onde_rms(s),
                };
                if v.is_finite() { v } else { f64::MAX }
            };
            let p = calculer_paraxial(sys);
            let lambda_mm = sys.longueur_onde_ref_nm * 1e-6;
            let nf = p.nombre_ouverture_fnum.max(0.5);
            let t0 = sys.surfaces[k].epaisseur;
            let mut demi = (12.0 * lambda_mm * nf * nf).max(0.005);
            let mut centre = t0;
            for _ in 0..3 {
                let (mut a, mut b) = (centre - demi, centre + demi);
                let g = 0.5 * (5f64.sqrt() - 1.0);
                let mut x1 = b - g * (b - a);
                let mut x2 = a + g * (b - a);
                let essai = |t: f64| {
                    let mut s = sys.clone();
                    s.surfaces[k].epaisseur = t;
                    mesure(&s)
                };
                let (mut f1, mut f2) = (essai(x1), essai(x2));
                for _ in 0..16 {
                    if f1 < f2 {
                        b = x2;
                        x2 = x1;
                        f2 = f1;
                        x1 = b - g * (b - a);
                        f1 = essai(x1);
                    } else {
                        a = x1;
                        x1 = x2;
                        f1 = f2;
                        x2 = a + g * (b - a);
                        f2 = essai(x2);
                    }
                }
                let t = 0.5 * (a + b);
                // Optimum en bord d'intervalle : on recentre et on élargit
                let au_bord = (t - centre).abs() > 0.9 * demi;
                centre = t;
                if !au_bord {
                    break;
                }
                demi *= 3.0;
            }
            sys.surfaces[k].epaisseur = centre;
        }
    }
    (evaluer_critere(sys, reglages.critere), sys.surfaces[k].epaisseur - depart)
}

// ---------------------------------------------------------------------------------------------
// Perturbations
// ---------------------------------------------------------------------------------------------

/// Insère une surface à l'indice `pos` en renumérotant les reprises (pickups)
fn inserer(sys: &mut OpticalSystem, pos: usize, s: Surface) {
    for surf in &mut sys.surfaces {
        for solve in [&mut surf.solve_rayon, &mut surf.solve_epaisseur, &mut surf.solve_conique].into_iter().flatten() {
            if let Solve::Reprise { surface, .. } = solve {
                if *surface > pos {
                    *surface += 1;
                }
            }
        }
    }
    sys.surfaces.insert(pos, s);
}

/// Décentre et incline l'élément `a..=b` autour du sommet de sa surface `a` : rupture de repère
/// avant, dernière épaisseur ramenée au pivot, rupture inverse après (comme Zemax)
fn perturber_element(sys: &mut OpticalSystem, a: usize, b: usize, dec: [f64; 2], incl: [f64; 2]) {
    let longueur: f64 = sys.surfaces[a..b].iter().map(|s| s.epaisseur).sum();
    let t_b = sys.surfaces[b].epaisseur;
    let solve_b = sys.surfaces[b].solve_epaisseur.take();
    let mut entree = Surface::nouvelle(SurfaceType::CoupureRepere, f64::INFINITY, 0.0, "AIR", 1.0);
    entree.commentaire = tr!("Tolérance : élément décentré/incliné", "Tolerance: element decenter/tilt", "Toleranz: Element dezentriert/gekippt").to_string();
    entree.decentrement = dec;
    entree.inclinaison_deg = incl;
    let mut sortie = entree.clone();
    sortie.commentaire = tr!("Tolérance : retour", "Tolerance: return", "Toleranz: Rückkehr").to_string();
    sortie.decentrement = [-dec[0], -dec[1]];
    sortie.inclinaison_deg = [-incl[0], -incl[1]];
    sortie.inclinaison_avant_decentrement = true;
    sortie.epaisseur = longueur + t_b;
    sortie.solve_epaisseur = solve_b;
    sys.surfaces[b].epaisseur = -longueur;
    inserer(sys, b + 1, sortie);
    inserer(sys, a, entree);
}

/// Remplace le verre après la surface `i` par une copie perturbée (indice, constringence)
fn perturber_verre(sys: &mut OpticalSystem, i: usize, dn: f64, dv_pct: f64) {
    let mat = sys.catalogue.resoudre(&sys.surfaces[i].verre);
    let MediumKind::Verre { nd, vd, formule, .. } = mat.kind else { return };
    let nd2 = nd + dn;
    let vd2 = vd * (1.0 + dv_pct / 100.0);
    let nom = format!("{}~T{}", mat.nom, i + 1);
    let formule = formule.map(|mut f| {
        // Facteur de dispersion tel que V' = V (1 + p) avec le nouvel indice
        let facteur = if (nd - 1.0).abs() > 1e-9 && vd2.abs() > 1e-9 { (nd2 - 1.0) * vd / ((nd - 1.0) * vd2) } else { 1.0 };
        f.decalage += dn;
        f.facteur_dispersion *= facteur;
        f
    });
    let m = crate::core::glass::Material {
        nom: nom.clone(),
        kind: MediumKind::Verre { nom: nom.clone(), nd: nd2, vd: vd2, formule },
        catalogue: mat.catalogue.clone(),
    };
    sys.catalogue.ajouter_materiau(m);
    sys.surfaces[i].verre = nom;
}

/// Système perturbé par les tolérances `ops` prises aux valeurs `valeurs`
pub fn perturber(sys: &OpticalSystem, ops: &[Tolerance], valeurs: &[f64]) -> OpticalSystem {
    let mut s = sys.clone();
    let n = s.surfaces.len();
    let mut verres: std::collections::BTreeMap<usize, (f64, f64)> = Default::default();
    let mut elements: std::collections::BTreeMap<(usize, usize), ([f64; 2], [f64; 2])> = Default::default();
    for (op, &v) in ops.iter().zip(valeurs) {
        let (a, b) = op.plage();
        if a >= n {
            continue;
        }
        let surf = &mut s.surfaces[a];
        match op.type_tolerance {
            TypeTolerance::Rayon => {
                if surf.rayon.is_finite() {
                    surf.rayon += v;
                }
            }
            TypeTolerance::Franges => {
                // Écart de flèche N·λ/2 sur le diamètre utile (λ = 546,1 nm) : Δc = 4 N λ / D²
                let d = 2.0 * surf.semi_diametre.max(1e-3);
                let c = surf.courbure() + 4.0 * v * 546.1e-6 / (d * d);
                surf.rayon = if c.abs() > 1e-12 { 1.0 / c } else { f64::INFINITY };
                if surf.rayon.is_finite() && surf.type_surface == SurfaceType::Plane {
                    surf.type_surface = SurfaceType::Spherique;
                }
            }
            TypeTolerance::Epaisseur => surf.epaisseur += v,
            TypeTolerance::Conique => {
                surf.conique += v;
                if surf.type_surface == SurfaceType::Spherique {
                    surf.type_surface = SurfaceType::Conique;
                }
            }
            TypeTolerance::DecentrementX => surf.decentrement[0] += v,
            TypeTolerance::DecentrementY => surf.decentrement[1] += v,
            TypeTolerance::InclinaisonX => surf.inclinaison_deg[0] += v,
            TypeTolerance::InclinaisonY => surf.inclinaison_deg[1] += v,
            TypeTolerance::Indice => verres.entry(a).or_default().0 += v,
            TypeTolerance::Abbe => verres.entry(a).or_default().1 += v,
            TypeTolerance::DecentrementElementX => elements.entry((a, b)).or_default().0[0] += v,
            TypeTolerance::DecentrementElementY => elements.entry((a, b)).or_default().0[1] += v,
            TypeTolerance::InclinaisonElementX => elements.entry((a, b)).or_default().1[0] += v,
            TypeTolerance::InclinaisonElementY => elements.entry((a, b)).or_default().1[1] += v,
        }
    }
    for (i, (dn, dv)) in verres {
        perturber_verre(&mut s, i, dn, dv);
    }
    // Éléments du dernier au premier : les indices des précédents restent valables
    for ((a, b), (dec, incl)) in elements.into_iter().rev() {
        let nul = dec == [0.0, 0.0] && incl == [0.0, 0.0];
        if b + 1 < s.surfaces.len() && !nul {
            perturber_element(&mut s, a, b, dec, incl);
        }
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Tolérances par défaut
// ---------------------------------------------------------------------------------------------

/// Jeu de tolérances usuel pour un atelier soigné : 2 franges de puissance, ±0,05 mm sur les
/// épaisseurs, ±0,01 sur les coniques, lentilles décentrées de ±0,02 mm et inclinées de ±1',
/// surfaces de lentille inclinées de ±1' (prisme), miroirs décentrés de ±0,05 mm et inclinés de
/// ±1', indice ±0,0005 et constringence ±0,5 %. L'épaisseur avant le plan image (mise au point)
/// et les paramètres imposés par un solve ne sont pas tolérancés.
pub fn tolerances_par_defaut(sys: &OpticalSystem) -> Vec<Tolerance> {
    let n = sys.surfaces.len();
    let mut t = Vec::new();
    let est_verre = |i: usize| matches!(sys.catalogue.resoudre(&sys.surfaces[i].verre).kind, MediumKind::Verre { .. });
    let est_miroir = |i: usize| sys.catalogue.resoudre(&sys.surfaces[i].verre).est_miroir();
    let minute = 1.0 / 60.0;
    let mut i = 0;
    while i + 1 < n {
        let s = &sys.surfaces[i];
        if s.est_coupure() || s.type_surface == SurfaceType::Ideale {
            if s.est_coupure() && i + 2 < n && s.solve_epaisseur.is_none() {
                t.push(Tolerance::nouvelle(TypeTolerance::Epaisseur, i + 1, 0, 0.05));
            }
            i += 1;
            continue;
        }
        if s.solve_rayon.is_none() {
            t.push(Tolerance::nouvelle(TypeTolerance::Franges, i + 1, 0, 2.0));
        }
        if matches!(s.type_surface, SurfaceType::Conique | SurfaceType::Aspherique) && s.solve_conique.is_none() {
            t.push(Tolerance::nouvelle(TypeTolerance::Conique, i + 1, 0, 0.01));
        }
        if i + 2 < n && s.solve_epaisseur.is_none() {
            t.push(Tolerance::nouvelle(TypeTolerance::Epaisseur, i + 1, 0, 0.05));
        }
        if est_miroir(i) {
            for ty in [TypeTolerance::DecentrementX, TypeTolerance::DecentrementY] {
                t.push(Tolerance::nouvelle(ty, i + 1, 0, 0.05));
            }
            for ty in [TypeTolerance::InclinaisonX, TypeTolerance::InclinaisonY] {
                t.push(Tolerance::nouvelle(ty, i + 1, 0, minute));
            }
        } else if est_verre(i) {
            // Lentille ou groupe collé : de la surface i jusqu'à la première suivie d'air
            let mut j = i;
            while j + 1 < n && est_verre(j) && !sys.surfaces[j + 1].est_coupure() {
                t.push(Tolerance::nouvelle(TypeTolerance::Indice, j + 1, 0, 0.0005));
                t.push(Tolerance::nouvelle(TypeTolerance::Abbe, j + 1, 0, 0.5));
                j += 1;
            }
            if j + 1 < n {
                for ty in [TypeTolerance::DecentrementElementX, TypeTolerance::DecentrementElementY] {
                    t.push(Tolerance::nouvelle(ty, i + 1, j + 1, 0.02));
                }
                for ty in [TypeTolerance::InclinaisonElementX, TypeTolerance::InclinaisonElementY] {
                    t.push(Tolerance::nouvelle(ty, i + 1, j + 1, minute));
                }
                // Prisme des faces (surface inclinée seule)
                for k in i..=j {
                    for ty in [TypeTolerance::InclinaisonX, TypeTolerance::InclinaisonY] {
                        t.push(Tolerance::nouvelle(ty, k + 1, 0, minute));
                    }
                    let sk = &sys.surfaces[k];
                    if k > i {
                        if sk.solve_rayon.is_none() {
                            t.push(Tolerance::nouvelle(TypeTolerance::Franges, k + 1, 0, 2.0));
                        }
                        if matches!(sk.type_surface, SurfaceType::Conique | SurfaceType::Aspherique) && sk.solve_conique.is_none() {
                            t.push(Tolerance::nouvelle(TypeTolerance::Conique, k + 1, 0, 0.01));
                        }
                        if k + 2 < n && sk.solve_epaisseur.is_none() {
                            t.push(Tolerance::nouvelle(TypeTolerance::Epaisseur, k + 1, 0, 0.05));
                        }
                    }
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    t
}

/// Vérifie les tolérances ; retourne les avertissements (surfaces inexistantes, paramètre imposé
/// par un solve, compensateur tolérancé…)
pub fn verifier(sys: &OpticalSystem, ops: &[Tolerance], reglages: &ReglagesTolerance) -> Vec<String> {
    let n = sys.surfaces.len();
    let mut a = Vec::new();
    for op in ops {
        let (i, j) = op.plage();
        if j + 1 >= n {
            a.push(trf!("{} : surface inexistante ou plan image", "{}: missing surface or image plane", "{}: Fläche fehlt oder Bildebene", op.libelle()));
            continue;
        }
        let s = &sys.surfaces[i];
        let impose = match op.type_tolerance {
            TypeTolerance::Rayon | TypeTolerance::Franges => s.solve_rayon.is_some(),
            TypeTolerance::Epaisseur => s.solve_epaisseur.is_some(),
            TypeTolerance::Conique => s.solve_conique.is_some(),
            _ => false,
        };
        if impose {
            a.push(trf!("{} : paramètre imposé par un solve, sans effet", "{}: parameter set by a solve, no effect", "{}: Parameter durch Solve festgelegt, ohne Wirkung", op.libelle()));
        }
        if op.type_tolerance == TypeTolerance::Epaisseur && i + 2 == n && reglages.compensateur != Compensateur::Aucun {
            a.push(trf!("{} : c'est le compensateur (mise au point), sans effet", "{}: this is the compensator (focus), no effect", "{}: das ist der Kompensator (Fokus), ohne Wirkung", op.libelle()));
        }
        if op.min > op.max {
            a.push(trf!("{} : minimum supérieur au maximum", "{}: minimum above maximum", "{}: Minimum über Maximum", op.libelle()));
        }
    }
    a
}

// ---------------------------------------------------------------------------------------------
// Calcul parallèle et générateur aléatoire
// ---------------------------------------------------------------------------------------------

/// Évalue `f(k)` pour k = 0..n sur tous les cœurs, dans l'ordre ; s'arrête si `arret` passe à vrai
fn en_parallele<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync, progression: &AtomicUsize, arret: &AtomicBool) -> Vec<Option<T>> {
    let suivant = AtomicUsize::new(0);
    let fils = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4).min(n.max(1));
    let mut resultats: Vec<Option<T>> = (0..n).map(|_| None).collect();
    let morceaux: Vec<Vec<(usize, T)>> = std::thread::scope(|portee| {
        let taches: Vec<_> = (0..fils)
            .map(|_| {
                portee.spawn(|| {
                    let mut faits = Vec::new();
                    loop {
                        if arret.load(Ordering::Relaxed) {
                            break;
                        }
                        let k = suivant.fetch_add(1, Ordering::Relaxed);
                        if k >= n {
                            break;
                        }
                        faits.push((k, f(k)));
                        progression.fetch_add(1, Ordering::Relaxed);
                    }
                    faits
                })
            })
            .collect();
        taches.into_iter().map(|t| t.join().unwrap_or_default()).collect()
    });
    for (k, v) in morceaux.into_iter().flatten() {
        resultats[k] = Some(v);
    }
    resultats
}

/// SplitMix64 : générateur rapide et reproductible
struct Hasard(u64);

impl Hasard {
    fn suivant(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniforme dans [0, 1)
    fn uniforme(&mut self) -> f64 {
        (self.suivant() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn normale(&mut self) -> f64 {
        let u1 = self.uniforme().max(1e-300);
        let u2 = self.uniforme();
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

/// Valeurs tirées pour le tirage `k` (même graine et même k : mêmes valeurs)
fn tirage(ops: &[Tolerance], reglages: &ReglagesTolerance, k: usize) -> Vec<f64> {
    let mut h = Hasard(reglages.graine.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ (k as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    ops.iter()
        .map(|op| {
            let (c, d) = (0.5 * (op.min + op.max), 0.5 * (op.max - op.min));
            match reglages.distribution {
                Distribution::Uniforme => op.min + h.uniforme() * (op.max - op.min),
                Distribution::Normale => loop {
                    let z = h.normale();
                    if z.abs() <= 2.0 {
                        break c + d * z / 2.0;
                    }
                },
            }
        })
        .collect()
}

/// Système du tirage Monte-Carlo `k`, compensateur réglé (pour l'examiner)
pub fn systeme_du_tirage(sys: &OpticalSystem, ops: &[Tolerance], reglages: &ReglagesTolerance, k: usize) -> OpticalSystem {
    let mut s = perturber(sys, ops, &tirage(ops, reglages, k));
    s.nom = trf!("{} — tirage Monte-Carlo n° {}", "{} — Monte Carlo trial #{}", "{} — Monte-Carlo-Lauf Nr. {}", sys.nom, k + 1);
    compenser_et_evaluer(&mut s, reglages);
    s
}

/// Nombre total de calculs d'une analyse (pour la barre de progression)
pub fn nb_calculs(ops: &[Tolerance], reglages: &ReglagesTolerance) -> usize {
    1 + 2 * ops.len() + reglages.nb_tirages
}

/// Analyse de tolérancement complète. `progression` compte les calculs faits, `arret` permet
/// d'interrompre (le résultat est alors une erreur).
pub fn tolerancer(
    sys: &OpticalSystem,
    ops: &[Tolerance],
    reglages: &ReglagesTolerance,
    progression: &AtomicUsize,
    arret: &AtomicBool,
) -> Result<ResultatTolerance, String> {
    let avertissements = verifier(sys, ops, reglages);
    let ops: Vec<Tolerance> = ops.iter().filter(|op| op.plage().1 + 1 < sys.surfaces.len()).cloned().collect();
    let mut base = sys.clone();
    let (nominal, comp_nominal) = compenser_et_evaluer(&mut base, reglages);
    progression.fetch_add(1, Ordering::Relaxed);
    if !nominal.is_finite() {
        return Err(tr!(
            "le critère n'est pas calculable pour le système nominal (rayons vignettés ?)",
            "the criterion cannot be computed for the nominal system (vignetted rays?)",
            "das Kriterium ist für das Nennsystem nicht berechenbar (vignettierte Strahlen?)"
        )
        .to_string());
    }
    let critere = reglages.critere;
    // Sensibilités : chaque tolérance seule, à son minimum puis à son maximum
    let evals = en_parallele(
        2 * ops.len(),
        |k| {
            let op = &ops[k / 2];
            let mut v = vec![0.0; ops.len()];
            v[k / 2] = if k % 2 == 0 { op.min } else { op.max };
            let mut s = perturber(sys, &ops, &v);
            compenser_et_evaluer(&mut s, reglages)
        },
        progression,
        arret,
    );
    if arret.load(Ordering::Relaxed) {
        return Err(tr!("analyse interrompue", "analysis stopped", "Analyse abgebrochen").to_string());
    }
    let mut lignes: Vec<LigneSensibilite> = ops
        .iter()
        .enumerate()
        .map(|(i, op)| {
            let (cmin, tmin) = evals[2 * i].unwrap_or((ECHEC, comp_nominal));
            let (cmax, tmax) = evals[2 * i + 1].unwrap_or((ECHEC, comp_nominal));
            LigneSensibilite {
                tolerance: op.clone(),
                libelle: op.libelle(),
                critere_min: cmin,
                critere_max: cmax,
                degradation_min: degradation(critere, cmin, nominal),
                degradation_max: degradation(critere, cmax, nominal),
                compensation_min: tmin - comp_nominal,
                compensation_max: tmax - comp_nominal,
            }
        })
        .collect();
    let degradation_rss = lignes.iter().map(|l| l.pire().max(0.0).powi(2)).sum::<f64>().sqrt();
    lignes.sort_by(|a, b| b.pire().total_cmp(&a.pire()));
    let critere_estime = if critere.plus_grand_meilleur() { nominal - degradation_rss } else { nominal + degradation_rss };

    // Monte-Carlo
    let monte_carlo = if reglages.nb_tirages > 0 && !ops.is_empty() {
        let res = en_parallele(
            reglages.nb_tirages,
            |k| {
                let mut s = perturber(sys, &ops, &tirage(&ops, reglages, k));
                compenser_et_evaluer(&mut s, reglages)
            },
            progression,
            arret,
        );
        if arret.load(Ordering::Relaxed) {
            return Err(tr!("analyse interrompue", "analysis stopped", "Analyse abgebrochen").to_string());
        }
        let valeurs: Vec<f64> = res.iter().map(|r| r.map(|r| r.0).unwrap_or(ECHEC)).collect();
        let comps: Vec<f64> = res.iter().filter_map(|r| r.map(|r| r.1 - comp_nominal)).collect();
        let bons: Vec<(usize, f64)> = valeurs.iter().copied().enumerate().filter(|v| v.1.is_finite()).collect();
        let nb = bons.len().max(1) as f64;
        let moyenne = bons.iter().map(|v| v.1).sum::<f64>() / nb;
        let ecart_type = (bons.iter().map(|v| (v.1 - moyenne).powi(2)).sum::<f64>() / nb).sqrt();
        // Tri du meilleur au pire
        let mut tries: Vec<(usize, f64)> = bons.clone();
        tries.sort_by(|a, b| if critere.plus_grand_meilleur() { b.1.total_cmp(&a.1) } else { a.1.total_cmp(&b.1) });
        let nb_echecs = valeurs.len() - bons.len();
        let total = valeurs.len() as f64;
        // « p % des tirages font au moins aussi bien que x » (les échecs comptent comme les pires)
        let centiles = [98.0, 90.0, 80.0, 50.0, 20.0, 10.0, 2.0]
            .into_iter()
            .filter_map(|p: f64| {
                let rang = ((p / 100.0 * total).ceil() as usize).max(1);
                tries.get(rang - 1).map(|v| (p, v.1))
            })
            .collect();
        let (index_pire, pire) = if nb_echecs > 0 {
            (valeurs.iter().position(|v| !v.is_finite()).unwrap_or(0), ECHEC)
        } else {
            tries.last().copied().unwrap_or((0, ECHEC))
        };
        Some(MonteCarlo {
            meilleur: tries.first().map(|v| v.1).unwrap_or(ECHEC),
            pire,
            moyenne,
            ecart_type,
            centiles,
            compensation_moyenne: comps.iter().sum::<f64>() / comps.len().max(1) as f64,
            compensation_max: comps.iter().fold(0.0, |m: f64, c| m.max(c.abs())),
            index_pire,
            nb_echecs,
            valeurs,
        })
    } else {
        None
    };
    Ok(ResultatTolerance { critere, nominal, lignes, degradation_rss, critere_estime, monte_carlo, avertissements })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lancer(sys: &OpticalSystem, ops: &[Tolerance], r: &ReglagesTolerance) -> ResultatTolerance {
        tolerancer(sys, ops, r, &AtomicUsize::new(0), &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn element_decentre_puis_ramene() {
        // Valeur nulle : rien n'est inséré ; perturbation infime : ruptures insérées, système
        // équivalent au nominal
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let op = [Tolerance::nouvelle(TypeTolerance::InclinaisonElementX, 1, 3, 0.1)];
        assert_eq!(perturber(&sys, &op, &[0.0]).surfaces.len(), 4);
        let s0 = perturber(&sys, &op, &[1e-12]);
        assert_eq!(s0.surfaces.len(), 6);
        let r0 = sys.reperes();
        let r1 = s0.reperes();
        assert!((r0[3].origine - r1[5].origine).norm() < 1e-9, "plan image déplacé");
        let (a, b) = (tache_rms_um(&sys), tache_rms_um(&s0));
        assert!((a - b).abs() < 1e-6, "{a} / {b}");
        // Le compensateur ne voit pas de déplacement artificiel dû aux ruptures insérées
        let r = ReglagesTolerance::default();
        let (ca, da) = compenser_et_evaluer(&mut sys.clone(), &r);
        let (cb, db) = compenser_et_evaluer(&mut s0.clone(), &r);
        assert!((ca - cb).abs() < 1e-6 && (da - db).abs() < 1e-6, "{da} / {db}");
        // Incliné de 0,1° : l'image se déplace (tache dégradée), le plan image reste en place
        let s1 = perturber(&sys, &op, &[0.1]);
        assert!((s1.reperes()[5].origine - r0[3].origine).norm() < 1e-9);
        assert!(tache_rms_um(&s1) > a);
    }

    #[test]
    fn verre_perturbe() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let ops = [Tolerance::nouvelle(TypeTolerance::Indice, 1, 0, 0.001), Tolerance::nouvelle(TypeTolerance::Abbe, 1, 0, 1.0)];
        let s = perturber(&sys, &ops, &[0.001, 1.0]);
        let m0 = sys.catalogue.resoudre(&sys.surfaces[0].verre);
        let m1 = s.catalogue.resoudre(&s.surfaces[0].verre);
        let d = 587.5618;
        assert!((m1.indice_refraction(d) - m0.indice_refraction(d) - 0.001).abs() < 1e-9);
        let v = |m: &crate::core::glass::Material| (m.indice_refraction(d) - 1.0) / (m.indice_refraction(486.1327) - m.indice_refraction(656.2725));
        assert!((v(&m1) / v(&m0) - 1.01).abs() < 1e-6, "{} / {}", v(&m1), v(&m0));
        // Le système perturbé se sérialise et se relit (verre embarqué dans son catalogue)
        let json = serde_json::to_string(&s).unwrap();
        let relu: OpticalSystem = serde_json::from_str(&json).unwrap();
        assert!((relu.catalogue.resoudre(&relu.surfaces[0].verre).indice_refraction(d) - m1.indice_refraction(d)).abs() < 1e-12);
    }

    #[test]
    fn franges_et_mise_au_point() {
        // 2 franges sur une face de 30 mm : ΔR ≈ R² · 4 N λ / D²
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let s = perturber(&sys, &[Tolerance::nouvelle(TypeTolerance::Franges, 1, 0, 2.0)], &[2.0]);
        let r = sys.surfaces[0].rayon;
        let attendu = -r * r * 4.0 * 2.0 * 546.1e-6 / 900.0;
        assert!((s.surfaces[0].rayon - r - attendu).abs() < 1e-3 * attendu.abs(), "{}", s.surfaces[0].rayon - r);
        // La meilleure mise au point vaut au moins la mise au point paraxiale
        let mut a = s.clone();
        let mut b = s.clone();
        let reg = |c| ReglagesTolerance { compensateur: c, ..Default::default() };
        let (pa, _) = compenser_et_evaluer(&mut a, &reg(Compensateur::MiseAuPointParaxiale));
        let (pb, _) = compenser_et_evaluer(&mut b, &reg(Compensateur::MeilleureMiseAuPoint));
        assert!(pb <= pa + 1e-9, "{pb} > {pa}");
    }

    #[test]
    fn analyse_complete_du_doublet() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let ops = tolerances_par_defaut(&sys);
        let codes: Vec<&str> = ops.iter().map(|o| o.type_tolerance.code()).collect();
        for c in ["TFRN", "TTHI", "TEDX", "TETY", "TIND", "TABB", "TSTX"] {
            assert!(codes.contains(&c), "{c} absent : {codes:?}");
        }
        // Pas de tolérance sur l'épaisseur avant le plan image (compensateur)
        assert!(!ops.iter().any(|o| o.type_tolerance == TypeTolerance::Epaisseur && o.surface == 3));
        assert!(verifier(&sys, &ops, &ReglagesTolerance::default()).is_empty());
        let r = lancer(&sys, &ops, &ReglagesTolerance { nb_tirages: 40, ..Default::default() });
        assert_eq!(r.lignes.len(), ops.len());
        assert!(r.lignes.windows(2).all(|w| w[0].pire() >= w[1].pire()));
        assert!(r.degradation_rss > 0.0 && r.critere_estime > r.nominal);
        let mc = r.monte_carlo.as_ref().unwrap();
        assert_eq!(mc.valeurs.len(), 40);
        assert_eq!(mc.nb_echecs, 0);
        assert!(mc.moyenne > r.nominal && mc.pire >= mc.moyenne && mc.meilleur <= mc.moyenne);
        // Centiles croissants du plus exigeant au moins exigeant
        assert!(mc.centiles.windows(2).all(|w| w[0].1 >= w[1].1), "{:?}", mc.centiles);
        // Reproductible : même graine, mêmes tirages ; le pire tirage se reconstruit
        let r2 = lancer(&sys, &ops, &ReglagesTolerance { nb_tirages: 40, ..Default::default() });
        assert_eq!(r2.monte_carlo.unwrap().valeurs, mc.valeurs);
        let pire = systeme_du_tirage(&sys, &ops, &ReglagesTolerance { nb_tirages: 40, ..Default::default() }, mc.index_pire);
        assert!((tache_rms_um(&pire) - mc.pire).abs() < 1e-9);
    }

    #[test]
    fn criteres_onde_et_ftm() {
        // Parabole parfaite : front d'onde nul, FTM égale à celle de la pupille circulaire
        let mut m = Surface::nouvelle(SurfaceType::Conique, -2000.0, -1000.0, "MIROIR", 101.0);
        m.conique = -1.0;
        m.est_diaphragme = true;
        let sys = OpticalSystem {
            surfaces: vec![m, Surface::plan_image(10.0)],
            ouverture: crate::core::types::ApertureSpec::PupilleEntreeMm(200.0),
            champs_deg: vec![(0.0, 0.0)],
            longueurs_onde: vec![crate::core::types::Wavelength::new(587.5618, 1.0, "d")],
            ..OpticalSystem::nouveau("parabole", 587.5618)
        };
        assert!(front_onde_rms(&sys) < 1e-6);
        let nu = 100.0 / (1.0 / (587.5618e-6 * 5.0));
        let attendu = crate::core::onde::ftm_ideale_circulaire(nu);
        assert!((ftm_moyenne(&sys, 100.0) - attendu).abs() < 0.01, "{}", ftm_moyenne(&sys, 100.0));
        // Inclinaison du miroir de 1' : coma et astigmatisme, front d'onde dégradé ; conique
        // ±0,01 : sphérique
        let ops = tolerances_par_defaut(&sys);
        assert!(ops.iter().any(|o| o.type_tolerance == TypeTolerance::Conique));
        let r = lancer(&sys, &ops, &ReglagesTolerance { critere: Critere::FrontOndeRms, nb_tirages: 0, ..Default::default() });
        assert!(r.monte_carlo.is_none());
        let conique = r.lignes.iter().find(|l| l.tolerance.type_tolerance == TypeTolerance::Conique).unwrap();
        // Δk = 0,01 sur une parabole de 200 mm F/5 : flèche Δk·r⁴/(8R³), doublée à la réflexion,
        // W040 = Δk·D⁴/(64·R³·λ) ondes, RMS = W040/√180 à la meilleure mise au point
        let w040 = 0.01 * 200f64.powi(4) / (64.0 * 2000f64.powi(3) * 587.5618e-6);
        assert!((conique.critere_max - w040 / 180f64.sqrt()).abs() < 0.02 * w040 / 180f64.sqrt(), "{} / {}", conique.critere_max, w040 / 180f64.sqrt());
    }
}

#[cfg(test)]
mod duree {
    use super::*;

    /// Durées et résultats types (cargo test --release duree_analyses -- --ignored --nocapture)
    #[test]
    #[ignore]
    fn duree_analyses() {
        let newton = {
            let mut m = Surface::nouvelle(SurfaceType::Conique, -2000.0, -800.0, "MIROIR", 101.0);
            m.conique = -1.0;
            m.est_diaphragme = true;
            m.rayon_obstruction = 25.0;
            let mut cb = Surface::nouvelle(SurfaceType::CoupureRepere, f64::INFINITY, 0.0, "AIR", 1.0);
            cb.inclinaison_deg = [45.0, 0.0];
            let plan = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 0.0, "MIROIR", 45.0);
            let mut cb2 = cb.clone();
            cb2.epaisseur = 200.0;
            OpticalSystem {
                surfaces: vec![m, cb, plan, cb2, Surface::plan_image(10.0)],
                ouverture: crate::core::types::ApertureSpec::PupilleEntreeMm(200.0),
                champs_deg: vec![(0.0, 0.0), (0.0, 0.25), (0.0, 0.35)],
                ..OpticalSystem::nouveau("Newton 200/1000", 587.5618)
            }
        };
        for (nom, sys) in [("doublet", OpticalSystem::doublet_achromatique_defaut()), ("newton", newton)] {
            let ops = tolerances_par_defaut(&sys);
            for critere in [Critere::SpotRms, Critere::FrontOndeRms, Critere::Ftm { frequence: 50.0 }] {
                let r = ReglagesTolerance { critere, nb_tirages: 100, ..Default::default() };
                let t0 = std::time::Instant::now();
                let res = tolerancer(&sys, &ops, &r, &AtomicUsize::new(0), &AtomicBool::new(false)).unwrap();
                let mc = res.monte_carlo.as_ref().unwrap();
                eprintln!(
                    "{nom} {:?} : {} tolérances, {:.1} s ; nominal {:.4}, RSS → {:.4}, MC moyenne {:.4} ±{:.4}, 90 % ≤ {:.4}, pire {:.4} ; pire tolérance {}",
                    critere,
                    ops.len(),
                    t0.elapsed().as_secs_f64(),
                    res.nominal,
                    res.critere_estime,
                    mc.moyenne,
                    mc.ecart_type,
                    mc.centiles.iter().find(|c| c.0 == 90.0).map(|c| c.1).unwrap_or(f64::NAN),
                    mc.pire,
                    res.lignes[0].libelle
                );
            }
        }
    }
}
