use crate::core::analysis::{calculer_aberration_spherique_longitudinale, calculer_spot_diagram};
use crate::core::paraxial::calculer_paraxial;
use crate::core::glass::MediumKind;
use crate::core::aberrations::calculer_courbure_distorsion;
use crate::core::onde::opd_pupille;
use crate::core::raytracer::{VisePupille, tracer_rayon_unique};
use crate::core::configurations::{TypeOperande, ValeurConfig};
use crate::core::types::{OpticalSystem, SurfaceType};
use nalgebra::{DMatrix, DVector};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Paramètre d'une surface déclaré comme variable d'optimisation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationVariable {
    /// Index de la surface dans le système optique (0..N-1)
    pub index_surface: usize,
    /// Nom du paramètre à faire varier : `"rayon"`, `"epaisseur"`, `"conique"`, ou un coefficient
    /// asphérique `"a4"`, `"a6"`, `"a8"`, `"a10"`
    pub parametre: String,
}

/// Cible (opérande de la fonction de mérite) pour l'optimiseur Damped Least Squares
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationTarget {
    /// Type de cible : `"focale"` (EFFL en mm), `"spot_rms"` (rayon RMS sur l'axe),
    /// `"spot_rms_champs"` (écarts au centroïde, tous champs et toutes couleurs),
    /// `"opd_rms_champs"` (front d'onde RMS, tous champs et toutes couleurs),
    /// `"aberration_spherique"` (LSA), `"tirage_bfl"` (mise au point), contraintes
    /// `"longueur_totale_max"` (mm), `"distorsion_max"` (%), `"epaisseur_centre_min"` et
    /// `"epaisseur_bord_min"` (mm, 1,8 par défaut)
    pub type_cible: String,
    /// Valeur cible souhaitée (ex: 100.0 pour une focale de 100 mm, 0.0 pour minimiser une aberration)
    pub valeur_cible: f64,
    /// Poids relatif de l'opérande dans la fonction de mérite (par défaut 1.0)
    pub poids: f64,
    /// Système à plusieurs configurations : numéro de la configuration où la cible s'applique
    /// (0 = première) ; absent : dans chacune des configurations
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<usize>,
}

/// Rapport d'exécution de l'optimisation Damped Least Squares (DLS)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationReport {
    /// Valeur initiale de la fonction de mérite
    pub merite_initial: f64,
    /// Valeur finale de la fonction de mérite après optimisation
    pub merite_final: f64,
    /// Nombre d'itérations DLS effectuées
    pub iterations: usize,
    /// Focale effective finale (EFFL en mm)
    pub focale_finale_mm: f64,
    /// Rayon RMS final du Spot Diagram (en µm)
    pub spot_rms_final_um: f64,
    /// Résumé textuel des paramètres optimisés
    pub resume: String,
}

/// Lit le vecteur des variables courantes à partir du système optique.
/// Pour le rayon de courbure, on travaille sur la courbure `c = 1/R` afin d'éviter la discontinuité à l'infini.
fn extraire_vecteur_variables(systeme: &OpticalSystem, variables: &[OptimizationVariable]) -> DVector<f64> {
    let mut vals = Vec::with_capacity(variables.len());
    for v in variables {
        if let Some(s) = systeme.surfaces.get(v.index_surface) {
            match v.parametre.to_lowercase().as_str() {
                "rayon" | "r" | "courbure" => vals.push(s.courbure()),
                "epaisseur" | "t" | "thickness" => vals.push(s.epaisseur),
                "conique" | "k" | "conic" => vals.push(s.conique),
                p => match indice_asphere(p) {
                    Some(k) => vals.push(s.aspheriques[k] * echelle_asphere(s.semi_diametre, k)),
                    None => vals.push(0.0),
                },
            }
        } else {
            vals.push(0.0);
        }
    }
    DVector::from_vec(vals)
}

/// Applique un vecteur de variables au système optique
fn appliquer_vecteur_variables(
    systeme: &mut OpticalSystem,
    variables: &[OptimizationVariable],
    x: &DVector<f64>,
) {
    for (i, v) in variables.iter().enumerate() {
        if let Some(s) = systeme.surfaces.get_mut(v.index_surface) {
            let val = x[i];
            match v.parametre.to_lowercase().as_str() {
                "rayon" | "r" | "courbure" => {
                    let c_clamped = val.clamp(-0.2, 0.2);
                    s.rayon = if c_clamped.abs() < 1e-9 {
                        f64::INFINITY
                    } else {
                        if s.type_surface == SurfaceType::Plane {
                            s.type_surface = SurfaceType::Spherique;
                        }
                        1.0 / c_clamped
                    };
                }
                "epaisseur" | "t" | "thickness" => {
                    // Conserve le sens de parcours : épaisseurs négatives après un miroir
                    let signe = if s.epaisseur < 0.0 { -1.0 } else { 1.0 };
                    s.epaisseur = signe * (signe * val).clamp(0.2, 2000.0);
                }
                "conique" | "k" | "conic" => {
                    s.conique = val.clamp(-10.0, 10.0);
                }
                p => {
                    if let Some(k) = indice_asphere(p) {
                        s.aspheriques[k] = val / echelle_asphere(s.semi_diametre, k);
                        if s.type_surface != SurfaceType::Aspherique {
                            s.type_surface = SurfaceType::Aspherique;
                        }
                    }
                }
            }
        }
    }
    // Les paramètres calculés (mise au point, reprises…) suivent les variables
    crate::core::solves::appliquer_solves(systeme);
}

/// Vrai si le paramètre est imposé par un solve (il ne peut donc pas être variable)
fn est_impose_par_solve(systeme: &OpticalSystem, v: &OptimizationVariable) -> bool {
    let Some(s) = systeme.surfaces.get(v.index_surface) else { return false };
    match v.parametre.to_lowercase().as_str() {
        "rayon" | "r" | "courbure" => s.solve_rayon.is_some(),
        "epaisseur" | "t" | "thickness" => s.solve_epaisseur.is_some(),
        "conique" | "k" | "conic" => s.solve_conique.is_some(),
        _ => false,
    }
}

/// Vrai si un opérande de configuration pilote le paramètre (variable par configuration)
fn est_pilote_par_configuration(systeme: &OpticalSystem, v: &OptimizationVariable) -> bool {
    let t = match v.parametre.to_lowercase().as_str() {
        "rayon" | "r" | "courbure" => TypeOperande::Rayon,
        "epaisseur" | "t" | "thickness" => TypeOperande::Epaisseur,
        "conique" | "k" | "conic" => TypeOperande::Conique,
        _ => return false,
    };
    systeme.est_pilote_par_configuration(t, v.index_surface)
}

/// Variable propre à une configuration : valeur `config` de l'opérande `operande`
#[derive(Debug, Clone, Copy)]
struct VariableConfig {
    operande: usize,
    config: usize,
    type_op: TypeOperande,
}

/// Valeur optimisée (courbure pour un rayon) d'une valeur d'opérande
fn variable_depuis_valeur(t: TypeOperande, v: f64) -> f64 {
    match t {
        TypeOperande::Rayon => {
            if v.is_finite() && v != 0.0 { 1.0 / v } else { 0.0 }
        }
        _ => v,
    }
}

/// Valeur d'opérande correspondant à une variable, avec les mêmes bornes que les variables communes
fn valeur_depuis_variable(t: TypeOperande, x: f64, ancienne: f64) -> f64 {
    match t {
        TypeOperande::Rayon => {
            let c = x.clamp(-0.2, 0.2);
            if c.abs() < 1e-9 { f64::INFINITY } else { 1.0 / c }
        }
        TypeOperande::Epaisseur => {
            let signe = if ancienne < 0.0 { -1.0 } else { 1.0 };
            signe * (signe * x).clamp(0.2, 2000.0)
        }
        TypeOperande::Conique => x.clamp(-10.0, 10.0),
        _ => x,
    }
}

/// Problème d'optimisation : variables communes (LDE), variables par configuration et cibles
struct Probleme<'a> {
    base: OpticalSystem,
    variables: Vec<OptimizationVariable>,
    variables_config: Vec<VariableConfig>,
    cibles: &'a [OptimizationTarget],
    multi: bool,
}

impl Probleme<'_> {
    fn nb_variables(&self) -> usize {
        self.variables.len() + self.variables_config.len()
    }

    fn vecteur_initial(&self) -> DVector<f64> {
        let mut x = extraire_vecteur_variables(&self.base, &self.variables).as_slice().to_vec();
        for v in &self.variables_config {
            let val = self.base.configurations.operandes[v.operande].valeurs[v.config].nombre().unwrap_or(0.0);
            x.push(variable_depuis_valeur(v.type_op, val));
        }
        DVector::from_vec(x)
    }

    fn pas_difference(&self, j: usize) -> f64 {
        let rayon = if j < self.variables.len() {
            matches!(self.variables[j].parametre.to_lowercase().as_str(), "rayon" | "r" | "courbure")
        } else {
            self.variables_config[j - self.variables.len()].type_op == TypeOperande::Rayon
        };
        if rayon { 1e-5 } else { 1e-3 }
    }

    /// Système (configuration active) correspondant au vecteur `x`
    fn systeme(&self, x: &DVector<f64>) -> OpticalSystem {
        let n = self.variables.len();
        let mut s = self.base.clone();
        appliquer_vecteur_variables(&mut s, &self.variables, &DVector::from_column_slice(&x.as_slice()[..n]));
        for (i, v) in self.variables_config.iter().enumerate() {
            let cellule = &mut s.configurations.operandes[v.operande].valeurs[v.config];
            let ancienne = cellule.nombre().unwrap_or(0.0);
            *cellule = ValeurConfig::depuis_nombre(valeur_depuis_variable(v.type_op, x[n + i], ancienne));
        }
        if self.multi {
            let active = s.configurations.active;
            s.appliquer_configuration(active);
            crate::core::solves::appliquer_solves(&mut s);
        }
        s
    }

    /// Résidus de toutes les configurations concernées
    fn residus(&self, x: &DVector<f64>) -> (DVector<f64>, OpticalSystem) {
        let s = self.systeme(x);
        if !self.multi {
            let r = evaluer_residus(&s, self.cibles);
            return (r, s);
        }
        let mut tous = Vec::new();
        for k in 0..s.nb_configurations() {
            let mut cibles_k: Vec<OptimizationTarget> =
                self.cibles.iter().filter(|c| c.configuration.is_none_or(|n| n == k)).cloned().collect();
            if cibles_k.is_empty() {
                // Configuration sans cible : seules les contraintes d'épaisseur au bord comptent
                cibles_k.push(OptimizationTarget { type_cible: "epaisseur_bord_min".into(), valeur_cible: 1.8, poids: 1.0, configuration: None });
            }
            let sk = s.systeme_de_configuration(k);
            tous.extend(evaluer_residus(&sk, &cibles_k).iter().copied());
        }
        (DVector::from_vec(tous), s)
    }
}

/// Index du coefficient asphérique ("a4" → 0, "a6" → 1, "a8" → 2, "a10" → 3)
fn indice_asphere(p: &str) -> Option<usize> {
    match p {
        "a4" => Some(0),
        "a6" => Some(1),
        "a8" => Some(2),
        "a10" => Some(3),
        _ => None,
    }
}

/// Les coefficients asphériques sont optimisés sous forme de flèche ajoutée au bord (mm),
/// grandeur de l'ordre de celles des autres variables
fn echelle_asphere(semi_diametre: f64, k: usize) -> f64 {
    semi_diametre.max(1.0).powi(4 + 2 * k as i32)
}

/// Écarts au centroïde (mm) des rayons d'une grille hexapolaire de 3 anneaux, pour tous les
/// champs et toutes les longueurs d'onde (pondérées) : critère « spot RMS » du système entier
fn residus_spot_champs(systeme: &OpticalSystem, poids: f64, residus: &mut Vec<f64>) {
    let vise = VisePupille::calculer(systeme);
    let champs = if systeme.champs_deg.is_empty() { vec![(0.0, 0.0)] } else { systeme.champs_deg.clone() };
    let lambdas: Vec<(f64, f64)> = if systeme.longueurs_onde.is_empty() {
        vec![(systeme.longueur_onde_ref_nm, 1.0)]
    } else {
        systeme.longueurs_onde.iter().map(|w| (w.valeur_nm, w.poids.max(0.0))).collect()
    };
    let total_l: f64 = lambdas.iter().map(|l| l.1).sum::<f64>().max(1e-12);
    let points = grille_hexapolaire(3);
    for &champ in &champs {
        let mut impacts = Vec::new();
        for &(lambda, wl) in &lambdas {
            for &(px, py) in &points {
                let tr = tracer_rayon_unique(systeme, vise.rayon(px, py, champ.0, champ.1, lambda), champ, (px, py));
                impacts.push((tr.impact_plan_image().map(|h| crate::core::raytracer::coordonnees_image(systeme, h)), (wl / total_l).sqrt()));
            }
        }
        let valides: Vec<(f64, f64)> = impacts.iter().filter_map(|i| i.0).collect();
        let n = valides.len().max(1) as f64;
        let cx = valides.iter().map(|v| v.0).sum::<f64>() / n;
        let cy = valides.iter().map(|v| v.1).sum::<f64>() / n;
        let norme = poids * 50.0 / (points.len() as f64).sqrt();
        for (impact, w) in impacts {
            match impact {
                Some((x, y)) => {
                    residus.push(norme * w * (x - cx));
                    residus.push(norme * w * (y - cy));
                }
                None => {
                    residus.push(norme * w);
                    residus.push(norme * w);
                }
            }
        }
    }
}

/// Différence de marche (ondes, piston retiré) sur une grille hexapolaire de 3 anneaux, pour
/// tous les champs et toutes les longueurs d'onde : critère « front d'onde RMS »
fn residus_opd_champs(systeme: &OpticalSystem, poids: f64, residus: &mut Vec<f64>) {
    let champs = if systeme.champs_deg.is_empty() { vec![(0.0, 0.0)] } else { systeme.champs_deg.clone() };
    let lambdas: Vec<(f64, f64)> = if systeme.longueurs_onde.is_empty() {
        vec![(systeme.longueur_onde_ref_nm, 1.0)]
    } else {
        systeme.longueurs_onde.iter().map(|w| (w.valeur_nm, w.poids.max(0.0))).collect()
    };
    let total_l: f64 = lambdas.iter().map(|l| l.1).sum::<f64>().max(1e-12);
    let points = grille_hexapolaire(3);
    let norme = poids * 0.5 / (points.len() as f64).sqrt();
    for &champ in &champs {
        for &(lambda, wl) in &lambdas {
            let w = norme * (wl / total_l).sqrt();
            match opd_pupille(systeme, champ, lambda, &points) {
                Ok(opd) => {
                    let valides: Vec<f64> = opd.iter().flatten().copied().collect();
                    let moyenne = valides.iter().sum::<f64>() / valides.len().max(1) as f64;
                    for v in opd {
                        residus.push(w * v.map(|v| v - moyenne).unwrap_or(1.0));
                    }
                }
                Err(_) => residus.extend(std::iter::repeat_n(w * 10.0, points.len())),
            }
        }
    }
}

/// Points (px, py) d'une grille hexapolaire : centre puis anneaux de 6, 12, 18… points
fn grille_hexapolaire(anneaux: usize) -> Vec<(f64, f64)> {
    let mut pts = vec![(0.0, 0.0)];
    for k in 1..=anneaux {
        let rho = k as f64 / anneaux as f64;
        for j in 0..6 * k {
            let t = std::f64::consts::TAU * j as f64 / (6 * k) as f64;
            pts.push((rho * t.cos(), rho * t.sin()));
        }
    }
    pts
}

/// Nombre de hauteurs pupillaires échantillonnées pour l'opérande d'aberration sphérique
const NB_PAS_LSA: usize = 8;

/// Évalue le vecteur de résidus pondérés `r(x)` pour le système optique donné
fn evaluer_residus(systeme: &OpticalSystem, cibles: &[OptimizationTarget]) -> DVector<f64> {
    let mut residus = Vec::new();
    let paraxial = calculer_paraxial(systeme);
    let spot = calculer_spot_diagram(systeme, (0.0, 0.0), 3);
    let lsa = calculer_aberration_spherique_longitudinale(systeme, NB_PAS_LSA);

    for c in cibles {
        let w = c.poids.max(1e-6);
        match c.type_cible.to_lowercase().as_str() {
            "focale" | "effl" => {
                let err = if paraxial.focale_effl_mm.is_finite() {
                    paraxial.focale_effl_mm - c.valeur_cible
                } else {
                    1e4
                };
                residus.push(w * err);
            }
            "tirage" | "tirage_bfl" | "bfl" => {
                // Écart entre l'épaisseur avant le plan image et le tirage paraxial BFL
                let nb = systeme.surfaces.len();
                let ep_image = if nb >= 2 {
                    systeme.surfaces[nb - 2].epaisseur
                } else {
                    0.0
                };
                let cible_bfl = if c.valeur_cible.abs() > 1e-6 {
                    c.valeur_cible
                } else {
                    paraxial.tirage_bfl_mm
                };
                residus.push(w * (ep_image - cible_bfl));
            }
            "aberration_spherique" | "lsa" | "spha" => {
                residus.push(w * (lsa.lsa_max_mm - c.valeur_cible) * 10.0);
                // Échantillons à la longueur d'onde de référence uniquement, à hauteurs pupillaires
                // fixes : le vecteur de résidus garde la même taille même si un rayon est vignetté
                let lambda_ref = systeme.longueur_onde_ref_nm;
                for k in 1..=NB_PAS_LSA {
                    let py = (k as f64) / (NB_PAS_LSA as f64);
                    let val = lsa
                        .echantillons
                        .iter()
                        .find(|e| {
                            (e.longueur_onde_nm - lambda_ref).abs() < 1.0
                                && (e.pupille_py - py).abs() < 1e-9
                        })
                        .map(|e| e.delta_z_mm - c.valeur_cible)
                        .unwrap_or(10.0);
                    residus.push(w * val);
                }
            }
            "spot_rms_champs" | "spot_champs" => residus_spot_champs(systeme, w, &mut residus),
            "opd_rms_champs" | "front_onde_rms" | "opd" => residus_opd_champs(systeme, w, &mut residus),
            "longueur_totale_max" | "longueur_totale" => {
                let l = paraxial.longueur_totale_mm.abs();
                residus.push(w * 10.0 * (l - c.valeur_cible).max(0.0));
            }
            "distorsion_max" => {
                let d = calculer_courbure_distorsion(systeme, systeme.longueur_onde_ref_nm, 2)
                    .ok()
                    .and_then(|r| r.distorsion_pct.last().copied().flatten())
                    .unwrap_or(0.0);
                residus.push(w * (d.abs() - c.valeur_cible.abs()).max(0.0));
            }
            "epaisseur_centre_min" => {
                for (i, s1) in systeme.surfaces.iter().enumerate().take(systeme.surfaces.len().saturating_sub(1)) {
                    let _ = i;
                    if matches!(systeme.catalogue.resoudre(&s1.verre).kind, MediumKind::Verre { .. }) {
                        residus.push(w * 500.0 * (c.valeur_cible - s1.epaisseur.abs()).max(0.0));
                    }
                }
            }
            // Réglage de la contrainte d'épaisseur au bord (appliquée plus bas)
            "epaisseur_bord_min" => {}
            // Par défaut : "spot_rms" / minimisation du rayon RMS et GEO du Spot Diagram
            _ => {
                if spot.nb_rayons_valides == 0 {
                    residus.push(w * 1000.0);
                    residus.push(w * 1000.0);
                } else {
                    residus.push(w * (spot.rayon_rms_mm - c.valeur_cible) * 50.0);
                    residus.push(w * (spot.rayon_geo_um * 1e-3 - c.valeur_cible) * 20.0);
                }
            }
        }
    }

    if residus.is_empty() && cibles.is_empty() {
        residus.push(spot.rayon_rms_mm);
    }

    // Contrainte physique d'épaisseur au bord (empêche deux surfaces d'une lentille de se croiser)
    for i in 0..systeme.surfaces.len().saturating_sub(1) {
        let s1 = &systeme.surfaces[i];
        let s2 = &systeme.surfaces[i + 1];
        if matches!(systeme.catalogue.resoudre(&s1.verre).kind, MediumKind::Verre { .. }) {
            let sd = s1.semi_diametre.min(s2.semi_diametre);
            let f1 = s1.fleche(sd).unwrap_or(100.0);
            let f2 = s2.fleche(sd).unwrap_or(-100.0);
            let ep_bord = s1.epaisseur + f2 - f1;
            let min_bord = cibles
                .iter()
                .find(|c| c.type_cible.eq_ignore_ascii_case("epaisseur_bord_min"))
                .map(|c| c.valeur_cible)
                .unwrap_or(1.8);
            if ep_bord < min_bord {
                residus.push((min_bord - ep_bord) * 500.0);
            } else {
                residus.push(0.0);
            }
        }
    }

    DVector::from_vec(residus)
}

/// Exécute l'algorithme Damped Least Squares (DLS / Levenberg-Marquardt) sur le système optique
pub fn optimiser_systeme_dls(
    systeme: &mut OpticalSystem,
    variables: &[OptimizationVariable],
    cibles: &[OptimizationTarget],
    max_iterations: usize,
) -> OptimizationReport {
    let vars_effectives: Vec<OptimizationVariable> = if variables.is_empty() {
        // Par défaut, si aucune variable n'est fournie, optimise les rayons des surfaces courbes et le tirage image
        let mut def = Vec::new();
        let n = systeme.surfaces.len();
        for idx in 0..n.saturating_sub(1) {
            if systeme.surfaces[idx].type_surface == SurfaceType::Plane {
                continue;
            }
            def.push(OptimizationVariable {
                index_surface: idx,
                parametre: "rayon".to_string(),
            });
        }
        if n >= 2 {
            def.push(OptimizationVariable {
                index_surface: n - 2,
                parametre: "epaisseur".to_string(),
            });
        }
        def
    } else {
        variables.to_vec()
    };
    crate::core::solves::appliquer_solves(systeme);
    systeme.capturer_configuration();
    let vars_effectives: Vec<OptimizationVariable> = vars_effectives
        .into_iter()
        .filter(|v| !est_impose_par_solve(systeme, v) && !est_pilote_par_configuration(systeme, v))
        .collect();

    let cibles_effectives: Vec<OptimizationTarget> = if cibles.is_empty() {
        vec![OptimizationTarget {
            type_cible: "spot_rms".to_string(),
            valeur_cible: 0.0,
            poids: 1.0,
            configuration: None,
        }]
    } else {
        cibles.to_vec()
    };

    // Variables propres à chaque configuration (opérandes marqués « variable »)
    let multi = systeme.nb_configurations() > 1;
    let mut variables_config = Vec::new();
    if multi {
        for (i, op) in systeme.configurations.operandes.iter().enumerate() {
            if op.variable && op.type_op.peut_varier() {
                for k in 0..systeme.nb_configurations() {
                    variables_config.push(VariableConfig { operande: i, config: k, type_op: op.type_op });
                }
            }
        }
    }
    let probleme = Probleme {
        base: systeme.clone(),
        variables: vars_effectives,
        variables_config,
        cibles: &cibles_effectives,
        multi,
    };

    let mut x = probleme.vecteur_initial();
    let (mut r, _) = probleme.residus(&x);
    let merite_initial = r.norm_squared();
    let mut merite_courant = merite_initial;

    let mut damping = 1e-2_f64;
    let n_vars = probleme.nb_variables();
    let mut iter_realisees = 0;

    for iter in 0..max_iterations.max(1) {
        iter_realisees = iter + 1;
        let n_res = r.len();
        if merite_courant < 1e-12 || n_res == 0 || n_vars == 0 {
            break;
        }

        // Calcul de la matrice Jacobienne J par différences finies
        let mut j_mat = DMatrix::<f64>::zeros(n_res, n_vars);
        for j in 0..n_vars {
            let delta = probleme.pas_difference(j);
            let mut x_plus = x.clone();
            x_plus[j] += delta;
            let (r_plus, _) = probleme.residus(&x_plus);
            if r_plus.len() == n_res {
                let col = (&r_plus - &r) / delta;
                j_mat.set_column(j, &col);
            }
        }

        let jtj = j_mat.transpose() * &j_mat;
        let jtr = j_mat.transpose() * &r;

        // Matrice amortie : (J^T * J + λ * diag(J^T * J + I)) * dx = -J^T * r
        let mut a_mat = jtj.clone();
        for d in 0..n_vars {
            a_mat[(d, d)] += damping * (jtj[(d, d)].abs() + 1.0);
        }

        let Some(dx) = a_mat.lu().solve(&(-jtr)) else {
            damping *= 10.0;
            continue;
        };

        if dx.norm() < 1e-10 {
            break;
        }

        let x_candidat = &x + &dx;
        let (r_candidat, sys_candidat) = probleme.residus(&x_candidat);
        let merite_candidat = r_candidat.norm_squared();

        if merite_candidat < merite_courant && r_candidat.len() == n_res {
            x = x_candidat;
            r = r_candidat;
            merite_courant = merite_candidat;
            *systeme = sys_candidat;
            damping = (damping * 0.3).max(1e-7);
        } else {
            damping = (damping * 5.0).min(1e6);
        }
    }
    systeme.capturer_configuration();
    let nb_configs = systeme.nb_configurations();

    let paraxial_final = calculer_paraxial(systeme);
    let spot_final = calculer_spot_diagram(systeme, (0.0, 0.0), 3);

    OptimizationReport {
        merite_initial,
        merite_final: merite_courant,
        iterations: iter_realisees,
        focale_finale_mm: paraxial_final.focale_effl_mm,
        spot_rms_final_um: spot_final.rayon_rms_um,
        resume: trf!(
            "DLS terminé en {} itérations : Mérite {:.4e} -> {:.4e} | EFFL = {:.2} mm | Spot RMS = {:.2} µm", "DLS finished in {} iterations: merit {:.4e} -> {:.4e} | EFFL = {:.2} mm | Spot RMS = {:.2} µm", "DLS nach {} Iterationen beendet: Gütefunktion {:.4e} -> {:.4e} | EFFL = {:.2} mm | Spot RMS = {:.2} µm",
            iter_realisees,
            merite_initial,
            merite_courant,
            paraxial_final.focale_effl_mm,
            spot_final.rayon_rms_um
        ) + &if nb_configs > 1 {
            trf!(" | {} configurations", " | {} configurations", " | {} Konfigurationen", nb_configs)
        } else {
            String::new()
        },
    }
}

#[cfg(test)]
mod tests_merite {
    use super::*;
    use crate::core::onde::calculer_front_onde;
    use crate::core::types::{ApertureSpec, Surface, Wavelength};

    fn cible(t: &str, v: f64) -> OptimizationTarget {
        OptimizationTarget { type_cible: t.into(), valeur_cible: v, poids: 1.0, configuration: None }
    }

    #[test]
    fn front_onde_rms_rend_parabolique_un_miroir_spherique() {
        let mut m = Surface::nouvelle(SurfaceType::Conique, -1000.0, -500.0, "MIROIR", 50.0);
        m.est_diaphragme = true;
        let mut sys = OpticalSystem {
            nom: "miroir".into(),
            longueur_onde_ref_nm: 587.5618,
            longueurs_onde: vec![Wavelength::new(587.5618, 1.0, "d")],
            ouverture: ApertureSpec::PupilleEntreeMm(100.0),
            champs_deg: vec![(0.0, 0.0)],
            surfaces: vec![m, Surface::plan_image(10.0)],
            catalogue: Default::default(),
            distance_objet_mm: None,
            afocal: false,
            tolerances: Vec::new(),
            configurations: Default::default(),
        };
        let vars = vec![OptimizationVariable { index_surface: 0, parametre: "conique".into() }];
        optimiser_systeme_dls(&mut sys, &vars, &[cible("opd_rms_champs", 0.0)], 30);
        assert!((sys.surfaces[0].conique + 1.0).abs() < 0.01, "k = {}", sys.surfaces[0].conique);
        let carte = calculer_front_onde(&sys, (0.0, 0.0), 587.5618, 32).unwrap();
        assert!(carte.rms_ondes < 0.01, "RMS {}", carte.rms_ondes);
    }

    #[test]
    fn asphere_et_spot_tous_champs() {
        // Miroir sphérique : un terme A4 corrige aussi l'aberration sphérique
        let mut m = Surface::nouvelle(SurfaceType::Aspherique, -1000.0, -500.0, "MIROIR", 50.0);
        m.est_diaphragme = true;
        let mut sys = OpticalSystem {
            nom: "miroir".into(),
            longueur_onde_ref_nm: 587.5618,
            longueurs_onde: vec![Wavelength::new(587.5618, 1.0, "d")],
            ouverture: ApertureSpec::PupilleEntreeMm(100.0),
            champs_deg: vec![(0.0, 0.0)],
            surfaces: vec![m, Surface::plan_image(10.0)],
            catalogue: Default::default(),
            distance_objet_mm: None,
            afocal: false,
            tolerances: Vec::new(),
            configurations: Default::default(),
        };
        let avant = calculer_spot_diagram(&sys, (0.0, 0.0), 6).rayon_rms_um;
        let vars = vec![OptimizationVariable { index_surface: 0, parametre: "a4".into() }];
        optimiser_systeme_dls(&mut sys, &vars, &[cible("spot_rms_champs", 0.0)], 30);
        let apres = calculer_spot_diagram(&sys, (0.0, 0.0), 6).rayon_rms_um;
        assert!(apres < 0.05 * avant, "spot {avant} → {apres} µm");
        // Flèche sphère - parabole = c³ r⁴ / 8 + … : le terme A4 trouvé vaut ≈ -c³/8
        let c: f64 = -1.0 / 1000.0;
        let attendu = -c.powi(3) / 8.0;
        let a4 = sys.surfaces[0].aspheriques[0];
        assert!((a4 - attendu).abs() < 0.05 * attendu.abs(), "A4 = {a4}, attendu {attendu}");
    }

    #[test]
    fn contraintes_longueur_et_epaisseur() {
        let mut d = OpticalSystem::doublet_achromatique_defaut();
        let vars = vec![OptimizationVariable { index_surface: 2, parametre: "epaisseur".into() }];
        optimiser_systeme_dls(&mut d, &vars, &[cible("longueur_totale_max", 95.0)], 30);
        assert!(calculer_paraxial(&d).longueur_totale_mm < 95.5);
    }

    #[test]
    fn mise_au_point_propre_a_chaque_configuration() {
        use crate::core::configurations::{TypeOperande, ValeurConfig};
        // Lunette : à l'infini et sur un objet à 5 m ; seule la mise au point change
        let mut s = OpticalSystem::doublet_achromatique_defaut();
        s.ajouter_operande(TypeOperande::DistanceObjet, 0).unwrap();
        let op_ep = s.ajouter_operande(TypeOperande::Epaisseur, 2).unwrap();
        s.configurations.operandes[op_ep].variable = true;
        let k = s.ajouter_configuration(0, Some("5 m".into()));
        s.configurations.operandes[0].valeurs[k] = ValeurConfig::Nombre(5000.0);
        // Hauteur d'objet équivalente au champ en degrés
        s.ajouter_operande(TypeOperande::ChampY, 1).unwrap();
        let h = 5000.0 * s.champs_deg[1].1.to_radians().tan();
        s.configurations.operandes[2].valeurs[k] = ValeurConfig::Nombre(h);
        // Le rayon de la 1re surface, piloté par rien, reste commun et fixe ; l'épaisseur
        // pilotée par configuration n'est pas une variable commune
        let vars = vec![OptimizationVariable { index_surface: 2, parametre: "epaisseur".into() }];
        let rapport = optimiser_systeme_dls(&mut s, &vars, &[cible("spot_rms_champs", 0.0)], 30);
        assert!(rapport.merite_final < rapport.merite_initial);
        let ep: Vec<f64> = (0..2).map(|k| s.configurations.operandes[op_ep].valeurs[k].nombre().unwrap()).collect();
        // Chaque mise au point est celle qu'on trouve en optimisant la configuration seule
        for k in 0..2 {
            let mut seule = s.systeme_de_configuration(k);
            seule.configurations = Default::default();
            seule.surfaces[2].epaisseur = 93.0;
            optimiser_systeme_dls(&mut seule, &vars, &[cible("spot_rms_champs", 0.0)], 30);
            assert!((seule.surfaces[2].epaisseur - ep[k]).abs() < 2e-3, "config {k} : {} seule, {} ensemble", seule.surfaces[2].epaisseur, ep[k]);
        }
        // Le tirage s'allonge d'environ f²/(5000 - f) ≈ 2 mm (formule de Newton)
        let ecart = ep[1] - ep[0];
        assert!((ecart - 2.05).abs() < 0.15, "écart {ecart}");
        // Le système affiché est la configuration active (la première), à jour
        assert_eq!(s.configurations.active, 0);
        assert_eq!(s.surfaces[2].epaisseur, ep[0]);
        assert!(rapport.resume.contains("2 configurations"));
    }

    #[test]
    fn cible_limitee_a_une_configuration() {
        use crate::core::configurations::{TypeOperande, ValeurConfig};
        // Deux configurations qui ne diffèrent que par l'épaisseur du verre flint :
        // la focale visée n'est imposée qu'à la seconde (plus longue : le bord de la lentille
        // crown est déjà à la limite de 1,8 mm)
        let mut s = OpticalSystem::doublet_achromatique_defaut();
        s.ajouter_operande(TypeOperande::Epaisseur, 1).unwrap();
        let k = s.ajouter_configuration(0, None);
        s.configurations.operandes[0].valeurs[k] = ValeurConfig::Nombre(4.0);
        let vars = vec![OptimizationVariable { index_surface: 0, parametre: "rayon".into() }];
        let mut c = cible("focale", 102.0);
        c.configuration = Some(1);
        optimiser_systeme_dls(&mut s, &vars, &[c], 30);
        let f1 = calculer_paraxial(&s.systeme_de_configuration(1)).focale_effl_mm;
        assert!((f1 - 102.0).abs() < 0.01, "f config 2 = {f1}");
        let f0 = calculer_paraxial(&s.systeme_de_configuration(0)).focale_effl_mm;
        assert!((f0 - 102.0).abs() > 0.05, "f config 1 = {f0}");
    }
}
