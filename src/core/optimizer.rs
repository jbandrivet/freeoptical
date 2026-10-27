use crate::core::analysis::{calculer_aberration_spherique_longitudinale, calculer_spot_diagram};
use crate::core::paraxial::calculer_paraxial;
use crate::core::glass::MediumKind;
use crate::core::types::{OpticalSystem, SurfaceType};
use nalgebra::{DMatrix, DVector};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Paramètre d'une surface déclaré comme variable d'optimisation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationVariable {
    /// Index de la surface dans le système optique (0..N-1)
    pub index_surface: usize,
    /// Nom du paramètre à faire varier : `"rayon"`, `"epaisseur"`, ou `"conique"`
    pub parametre: String,
}

/// Cible (opérande de la fonction de mérite) pour l'optimiseur Damped Least Squares
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationTarget {
    /// Type de cible : `"focale"` (EFFL en mm), `"spot_rms"` (minimiser le rayon RMS),
    /// `"aberration_spherique"` (minimiser la LSA), ou `"tirage_bfl"` (focaliser au plan image)
    pub type_cible: String,
    /// Valeur cible souhaitée (ex: 100.0 pour une focale de 100 mm, 0.0 pour minimiser une aberration)
    pub valeur_cible: f64,
    /// Poids relatif de l'opérande dans la fonction de mérite (par défaut 1.0)
    pub poids: f64,
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
                _ => vals.push(0.0),
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
                _ => {}
            }
        }
    }
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

    if residus.is_empty() {
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
            let min_bord = 1.8;
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

    let cibles_effectives: Vec<OptimizationTarget> = if cibles.is_empty() {
        vec![OptimizationTarget {
            type_cible: "spot_rms".to_string(),
            valeur_cible: 0.0,
            poids: 1.0,
        }]
    } else {
        cibles.to_vec()
    };

    let mut x = extraire_vecteur_variables(systeme, &vars_effectives);
    let mut r = evaluer_residus(systeme, &cibles_effectives);
    let merite_initial = r.norm_squared();
    let mut merite_courant = merite_initial;

    let mut damping = 1e-2_f64;
    let n_vars = x.len();
    let mut iter_realisees = 0;

    for iter in 0..max_iterations.max(1) {
        iter_realisees = iter + 1;
        let n_res = r.len();
        if merite_courant < 1e-12 || n_res == 0 {
            break;
        }

        // Calcul de la matrice Jacobienne J par différences finies
        let mut j_mat = DMatrix::<f64>::zeros(n_res, n_vars);
        for j in 0..n_vars {
            let delta = match vars_effectives[j].parametre.to_lowercase().as_str() {
                "rayon" | "r" | "courbure" => 1e-5,
                _ => 1e-3,
            };
            let mut x_plus = x.clone();
            x_plus[j] += delta;

            let mut sys_temp = systeme.clone();
            appliquer_vecteur_variables(&mut sys_temp, &vars_effectives, &x_plus);
            let r_plus = evaluer_residus(&sys_temp, &cibles_effectives);

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
        let mut sys_candidat = systeme.clone();
        appliquer_vecteur_variables(&mut sys_candidat, &vars_effectives, &x_candidat);
        let r_candidat = evaluer_residus(&sys_candidat, &cibles_effectives);
        let merite_candidat = r_candidat.norm_squared();

        if merite_candidat < merite_courant {
            x = x_candidat;
            r = r_candidat;
            merite_courant = merite_candidat;
            *systeme = sys_candidat;
            damping = (damping * 0.3).max(1e-7);
        } else {
            damping = (damping * 5.0).min(1e6);
        }
    }

    let paraxial_final = calculer_paraxial(systeme);
    let spot_final = calculer_spot_diagram(systeme, (0.0, 0.0), 3);

    OptimizationReport {
        merite_initial,
        merite_final: merite_courant,
        iterations: iter_realisees,
        focale_finale_mm: paraxial_final.focale_effl_mm,
        spot_rms_final_um: spot_final.rayon_rms_um,
        resume: format!(
            "DLS terminé en {} itérations : Mérite {:.4e} -> {:.4e} | EFFL = {:.2} mm | Spot RMS = {:.2} µm",
            iter_realisees,
            merite_initial,
            merite_courant,
            paraxial_final.focale_effl_mm,
            spot_final.rayon_rms_um
        ),
    }
}
