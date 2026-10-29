//! Empreinte des faisceaux (footprint) : points d'impact sur une surface, dans son repère local,
//! pour tous les champs et toutes les longueurs d'onde ; demi-diamètres nécessaires pour ne
//! vignetter aucun rayon (équivalent du demi-diamètre automatique de Zemax).

use crate::core::raytracer::tracer_grille_hexapolaire;
use crate::core::types::OpticalSystem;
use serde::{Deserialize, Serialize};

/// Impact d'un rayon sur la surface étudiée
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PointEmpreinte {
    pub x_mm: f64,
    pub y_mm: f64,
    /// Index du champ dans `champs_deg`
    pub champ: usize,
    pub longueur_onde_nm: f64,
    /// Rayon arrêté sur cette surface ou une précédente
    pub vignette: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Empreinte {
    /// Index de la surface (0..N-1)
    pub surface: usize,
    pub points: Vec<PointEmpreinte>,
    pub semi_diametre_mm: f64,
    pub rayon_obstruction_mm: f64,
    /// Plus grande distance à l'axe local des rayons transmis jusqu'ici
    pub rayon_utile_mm: f64,
    /// Demi-diamètre nécessaire pour ne vignetter aucun rayon (surfaces précédentes ouvertes)
    pub rayon_requis_mm: f64,
    /// Rayons arrêtés sur cette surface ou avant, sur le nombre de rayons lancés
    pub nb_vignettes: usize,
    pub nb_rayons: usize,
}

/// Copie du système sans aucune ouverture mécanique (demi-diamètres 20 fois plus grands que la
/// plus grande ouverture, pas d'obstruction). Pas davantage : le départ des rayons est calé sur
/// la flèche au bord de la première surface.
fn sans_ouvertures(sys: &OpticalSystem) -> OpticalSystem {
    let mut libre = sys.clone();
    let ouverture = sys
        .surfaces
        .iter()
        .map(|s| s.semi_diametre)
        .fold(crate::core::raytracer::rayon_pupille_entree(sys), f64::max);
    for s in &mut libre.surfaces {
        s.semi_diametre = 20.0 * ouverture;
        s.rayon_obstruction = 0.0;
    }
    libre
}

fn champs(sys: &OpticalSystem) -> Vec<(f64, f64)> {
    if sys.champs_deg.is_empty() { vec![(0.0, 0.0)] } else { sys.champs_deg.clone() }
}

fn longueurs_onde(sys: &OpticalSystem) -> Vec<f64> {
    if sys.longueurs_onde.is_empty() {
        vec![sys.longueur_onde_ref_nm]
    } else {
        sys.longueurs_onde.iter().map(|w| w.valeur_nm).collect()
    }
}

/// Demi-diamètre nécessaire de chaque surface pour transmettre tous les rayons de tous les
/// champs et toutes les longueurs d'onde (0 pour une rupture de repère ou une surface non atteinte)
pub fn demi_diametres_requis(sys: &OpticalSystem) -> Vec<f64> {
    let libre = sans_ouvertures(sys);
    let mut requis = vec![0.0_f64; sys.surfaces.len()];
    for champ in champs(sys) {
        for &l in &longueurs_onde(sys) {
            for tr in tracer_grille_hexapolaire(&libre, champ.0, champ.1, 8, l) {
                for h in &tr.impacts {
                    if !sys.surfaces[h.index_surface].est_coupure() {
                        let r = h.point_local.x.hypot(h.point_local.y);
                        requis[h.index_surface] = requis[h.index_surface].max(r);
                    }
                }
            }
        }
    }
    requis
}

/// Empreinte des faisceaux sur la surface `surface`
pub fn calculer_empreinte(sys: &OpticalSystem, surface: usize, nb_anneaux: usize) -> Result<Empreinte, String> {
    let s = sys.surfaces.get(surface).ok_or_else(|| {
        trf!("surface #{} inexistante", "surface #{} does not exist", "Fläche #{} existiert nicht", surface + 1)
    })?;
    if s.est_coupure() {
        return Err(tr!(
            "une rupture de repère ne reçoit aucun rayon",
            "a coordinate break receives no ray",
            "ein Koordinatenbruch empfängt keine Strahlen"
        )
        .to_string());
    }
    let mut points = Vec::new();
    let (mut nb_rayons, mut nb_vignettes) = (0, 0);
    for (ic, champ) in champs(sys).into_iter().enumerate() {
        for &l in &longueurs_onde(sys) {
            for tr in tracer_grille_hexapolaire(sys, champ.0, champ.1, nb_anneaux.max(2), l) {
                nb_rayons += 1;
                match tr.impacts.get(surface) {
                    Some(h) => {
                        let vignette = tr.impacts[..=surface].iter().any(|h| h.vignette);
                        nb_vignettes += vignette as usize;
                        points.push(PointEmpreinte {
                            x_mm: h.point_local.x,
                            y_mm: h.point_local.y,
                            champ: ic,
                            longueur_onde_nm: l,
                            vignette,
                        });
                    }
                    None => nb_vignettes += 1,
                }
            }
        }
    }
    let rayon_utile_mm = points.iter().filter(|p| !p.vignette).map(|p| p.x_mm.hypot(p.y_mm)).fold(0.0, f64::max);
    Ok(Empreinte {
        surface,
        semi_diametre_mm: s.semi_diametre,
        rayon_obstruction_mm: s.rayon_obstruction,
        rayon_utile_mm,
        rayon_requis_mm: demi_diametres_requis(sys)[surface],
        nb_vignettes,
        nb_rayons,
        points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doublet_ouvert_puis_vignette() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        // Pupille de 24 mm sur la première surface (diaphragme) : demi-diamètre requis un peu
        // plus de 12 mm, les faisceaux de champ frappant la face courbe en avant du plan pupille
        let requis = demi_diametres_requis(&sys);
        assert!(requis[0] > 12.0 && requis[0] < 12.1, "{requis:?}");
        assert!(requis[2] > 11.0 && requis[2] < 12.5);
        let e = calculer_empreinte(&sys, 2, 6).unwrap();
        assert_eq!(e.nb_vignettes, 0);
        assert!((e.rayon_utile_mm - e.rayon_requis_mm).abs() < 0.05);
        // Dernière face réduite à 10 mm : une partie du faisceau de champ est arrêtée
        sys.surfaces[2].semi_diametre = 10.0;
        let e = calculer_empreinte(&sys, 2, 6).unwrap();
        assert!(e.nb_vignettes > 0 && e.rayon_utile_mm <= 10.0 + 1e-6);
        assert!(e.points.iter().filter(|p| p.vignette).all(|p| p.x_mm.hypot(p.y_mm) > 10.0 - 1e-6));
        // Plan image : la tache du champ 2,5° est centrée vers 4,4 mm
        let img = calculer_empreinte(&sys, 3, 4).unwrap();
        let champ2: Vec<_> = img.points.iter().filter(|p| p.champ == 1 && !p.vignette).collect();
        let y = champ2.iter().map(|p| p.y_mm).sum::<f64>() / champ2.len() as f64;
        assert!((y.abs() - 100.0 * 2.5f64.to_radians().tan()).abs() < 0.2, "y = {y}");
    }
}
