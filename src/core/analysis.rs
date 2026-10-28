use crate::core::paraxial::calculer_paraxial;
use crate::core::raytracer::{VisePupille, coordonnees_image, tracer_grille_hexapolaire, tracer_rayon_unique};
use crate::core::types::OpticalSystem;
use serde::{Deserialize, Serialize};

/// Un impact individuel dans le Spot Diagram au plan image
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotPoint {
    /// Abscisse X sur le plan image (mm)
    pub x_mm: f64,
    /// Ordonnée Y sur le plan image (mm)
    pub y_mm: f64,
    /// Longueur d'onde associée (nm)
    pub longueur_onde_nm: f64,
    /// Angle de champ associé (hx_deg, hy_deg)
    pub champ_deg: (f64, f64),
}

/// Résultat complet de l'analyse Spot Diagram
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotDiagramResult {
    /// Liste de tous les impacts sur le plan image
    pub points: Vec<SpotPoint>,
    /// Coordonnées du centroïde (x_bar, y_bar) en mm
    pub centroide_mm: (f64, f64),
    /// Rayon RMS du Spot Diagram en millimètres
    pub rayon_rms_mm: f64,
    /// Rayon RMS du Spot Diagram en micromètres (µm)
    pub rayon_rms_um: f64,
    /// Rayon géométrique maximal (GEO radius) en micromètres (µm)
    pub rayon_geo_um: f64,
    /// Rayon de la tache de diffraction d'Airy (1.22 * λ * F/#) en micromètres (µm)
    pub rayon_airy_um: f64,
    /// Nombre total de rayons transmis jusqu'au plan image
    pub nb_rayons_valides: usize,
}

/// Calcule le Spot Diagram polychromatique sur le plan image pour un champ donné (par défaut sur l'axe)
pub fn calculer_spot_diagram(
    systeme: &OpticalSystem,
    champ_deg: (f64, f64),
    nb_anneaux: usize,
) -> SpotDiagramResult {
    let mut points = Vec::new();
    let longueurs_onde = if systeme.longueurs_onde.is_empty() {
        vec![systeme.longueur_onde_ref_nm]
    } else {
        systeme.longueurs_onde.iter().map(|w| w.valeur_nm).collect()
    };

    for &lambda in &longueurs_onde {
        let rayons = tracer_grille_hexapolaire(systeme, champ_deg.0, champ_deg.1, nb_anneaux, lambda);
        for tr in rayons {
            if let Some(hit) = tr.impact_plan_image() {
                points.push(SpotPoint {
                    x_mm: coordonnees_image(systeme, hit).0,
                    y_mm: coordonnees_image(systeme, hit).1,
                    longueur_onde_nm: lambda,
                    champ_deg,
                });
            }
        }
    }

    let n = points.len();
    if n == 0 {
        return SpotDiagramResult {
            points,
            centroide_mm: (0.0, 0.0),
            rayon_rms_mm: 0.0,
            rayon_rms_um: 0.0,
            rayon_geo_um: 0.0,
            rayon_airy_um: 0.0,
            nb_rayons_valides: 0,
        };
    }

    let sum_x: f64 = points.iter().map(|p| p.x_mm).sum();
    let sum_y: f64 = points.iter().map(|p| p.y_mm).sum();
    let cx = sum_x / (n as f64);
    let cy = sum_y / (n as f64);

    let mut sum_r2 = 0.0;
    let mut max_r = 0.0_f64;

    for p in &points {
        let dx = p.x_mm - cx;
        let dy = p.y_mm - cy;
        let r2 = dx * dx + dy * dy;
        sum_r2 += r2;
        max_r = max_r.max(r2.sqrt());
    }

    let rms_mm = (sum_r2 / (n as f64)).sqrt();
    let rms_um = rms_mm * 1000.0;
    let geo_um = max_r * 1000.0;

    let paraxial = calculer_paraxial(systeme);
    // r_Airy (µm) = 1.22 * λ(µm) * F/#
    let lambda_um = systeme.longueur_onde_ref_nm * 1e-3;
    // En afocal : rayon angulaire 1,22 λ / D exprimé en µrad (sur une lentille idéale de 1000 mm)
    let rayon_airy_um = if systeme.afocal {
        1.22 * lambda_um * 1000.0 / paraxial.diametre_pupille_sortie_mm.max(1e-9)
    } else {
        1.22 * lambda_um * paraxial.nombre_ouverture_fnum
    };

    SpotDiagramResult {
        points,
        centroide_mm: (cx, cy),
        rayon_rms_mm: rms_mm,
        rayon_rms_um: rms_um,
        rayon_geo_um: geo_um,
        rayon_airy_um,
        nb_rayons_valides: n,
    }
}

/// Point de courbe d'aberration sphérique longitudinale (LSA)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LsaSample {
    /// Coordonnée pupillaire normalisée py dans [0, 1]
    pub pupille_py: f64,
    /// Écart longitudinal de focalisation ΔZ (en mm) par rapport au plan image
    pub delta_z_mm: f64,
    /// Longueur d'onde associée (nm)
    pub longueur_onde_nm: f64,
}

/// Résultat de l'analyse d'Aberration Sphérique Longitudinale (LSA)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongitudinalAberrationResult {
    /// Échantillons (py, delta_z_mm) pour chaque longueur d'onde
    pub echantillons: Vec<LsaSample>,
    /// Aberration sphérique longitudinale maximale (écart crête-à-crête à λ_ref en mm)
    pub lsa_max_mm: f64,
}

/// Calcule la courbe d'aberration sphérique longitudinale (LSA) sur l'axe
pub fn calculer_aberration_spherique_longitudinale(
    systeme: &OpticalSystem,
    nb_pas: usize,
) -> LongitudinalAberrationResult {
    let pas = nb_pas.max(5);
    let mut echantillons = Vec::new();

    let longueurs_onde = if systeme.longueurs_onde.is_empty() {
        vec![systeme.longueur_onde_ref_nm]
    } else {
        systeme.longueurs_onde.iter().map(|w| w.valeur_nm).collect()
    };

    let vise = VisePupille::calculer(systeme);
    let mut lsa_ref_min = f64::INFINITY;
    let mut lsa_ref_max = f64::NEG_INFINITY;

    for &lambda in &longueurs_onde {
        for i in 1..=pas {
            let py = (i as f64) / (pas as f64);
            let ray = vise.rayon(0.0, py, 0.0, 0.0, lambda);
            let tr = tracer_rayon_unique(systeme, ray, (0.0, 0.0), (0.0, py));
            if let Some(hit) = tr.impact_plan_image() {
                // Au niveau du plan image (situé à z_image), le rayon est en y = hit.point_local.y
                // avec la direction sortante (dy = direction_sortante.y, dz = direction_sortante.z).
                // Il coupe l'axe optique (Y = 0) à z_cross = z_image - y * (dz / dy)
                let dy = hit.direction_sortante.y;
                let dz = hit.direction_sortante.z;
                if dy.abs() > 1e-10 {
                    let delta_z = -hit.point_local.y * (dz / dy);
                    echantillons.push(LsaSample {
                        pupille_py: py,
                        delta_z_mm: delta_z,
                        longueur_onde_nm: lambda,
                    });

                    if (lambda - systeme.longueur_onde_ref_nm).abs() < 1.0 {
                        lsa_ref_min = lsa_ref_min.min(delta_z);
                        lsa_ref_max = lsa_ref_max.max(delta_z);
                    }
                }
            }
        }
    }

    let lsa_max_mm = if lsa_ref_max.is_finite() && lsa_ref_min.is_finite() {
        (lsa_ref_max - lsa_ref_min).abs()
    } else {
        0.0
    };

    LongitudinalAberrationResult {
        echantillons,
        lsa_max_mm,
    }
}
