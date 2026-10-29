//! Aberrations chromatiques : décalage focal paraxial en fonction de la longueur d'onde
//! (chromatisme longitudinal) et écart de hauteur image du rayon principal réel en fonction du
//! champ (couleur latérale), comparés à la profondeur de foyer et à la tache d'Airy.

use crate::core::paraxial::calculer_paraxial;
use crate::core::raytracer::{VisePupille, coordonnees_image, tracer_rayon_unique};
use crate::core::types::OpticalSystem;
use serde::{Deserialize, Serialize};

/// Décalage du foyer paraxial avec la longueur d'onde
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecalageFocal {
    pub longueurs_onde_nm: Vec<f64>,
    /// Position du foyer paraxial par rapport à celui de la longueur d'onde de référence (µm,
    /// positif dans le sens de propagation de la lumière)
    pub decalage_um: Vec<f64>,
    /// Écart entre les foyers extrêmes sur le domaine (µm)
    pub etendue_um: f64,
    /// Profondeur de foyer limitée par la diffraction, ±2 λ N² (µm)
    pub profondeur_diffraction_um: f64,
    /// Longueur d'onde de foyer le plus court et le plus long
    pub lambda_foyer_min_nm: f64,
    pub lambda_foyer_max_nm: f64,
    pub longueur_onde_ref_nm: f64,
}

/// Couleur latérale : écart de hauteur image du rayon principal par rapport à la longueur d'onde
/// de référence, le long du champ maximal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouleurLaterale {
    /// Fraction du champ maximal (0 à 1)
    pub fractions_champ: Vec<f64>,
    /// Champ maximal (degrés ou mm)
    pub champ_max: (f64, f64),
    pub longueurs_onde_nm: Vec<f64>,
    /// Écart (µm ; µrad en afocal) par longueur d'onde puis par fraction de champ
    pub ecart_um: Vec<Vec<Option<f64>>>,
    /// Plus grand écart entre deux longueurs d'onde (µm)
    pub ecart_max_um: f64,
    pub rayon_airy_um: f64,
}

/// Domaine spectral du système (longueurs d'onde extrêmes ; raies F et C à défaut)
fn domaine(sys: &OpticalSystem) -> (f64, f64) {
    let (mut a, mut b) = (f64::INFINITY, f64::NEG_INFINITY);
    for w in &sys.longueurs_onde {
        a = a.min(w.valeur_nm);
        b = b.max(w.valeur_nm);
    }
    if !(b > a) { (486.1327, 656.2725) } else { (a, b) }
}

pub fn calculer_decalage_focal(sys: &OpticalSystem, nb: usize) -> Result<DecalageFocal, String> {
    if sys.afocal {
        return Err(tr!(
            "système afocal : pas de foyer image",
            "afocal system: no image focus",
            "afokales System: kein Bildfokus"
        )
        .to_string());
    }
    let tirage = |lambda: f64| {
        let mut s = sys.clone();
        s.longueur_onde_ref_nm = lambda;
        calculer_paraxial(&s).tirage_bfl_mm
    };
    let reference = tirage(sys.longueur_onde_ref_nm);
    if !reference.is_finite() {
        return Err(tr!("foyer paraxial à l'infini", "paraxial focus at infinity", "paraxialer Fokus im Unendlichen").to_string());
    }
    let sens = sys.sens_propagation_image();
    let (a, b) = domaine(sys);
    let nb = nb.max(3);
    let longueurs_onde_nm: Vec<f64> = (0..nb).map(|k| a + (b - a) * k as f64 / (nb - 1) as f64).collect();
    let decalage_um: Vec<f64> = longueurs_onde_nm.iter().map(|&l| sens * (tirage(l) - reference) * 1000.0).collect();
    let (mut imin, mut imax) = (0, 0);
    for (k, d) in decalage_um.iter().enumerate() {
        if *d < decalage_um[imin] {
            imin = k;
        }
        if *d > decalage_um[imax] {
            imax = k;
        }
    }
    let n = calculer_paraxial(sys).nombre_ouverture_fnum;
    Ok(DecalageFocal {
        etendue_um: decalage_um[imax] - decalage_um[imin],
        profondeur_diffraction_um: 2.0 * sys.longueur_onde_ref_nm * 1e-3 * n * n,
        lambda_foyer_min_nm: longueurs_onde_nm[imin],
        lambda_foyer_max_nm: longueurs_onde_nm[imax],
        longueur_onde_ref_nm: sys.longueur_onde_ref_nm,
        longueurs_onde_nm,
        decalage_um,
    })
}

pub fn calculer_couleur_laterale(sys: &OpticalSystem, nb: usize) -> Result<CouleurLaterale, String> {
    let champ_max = sys
        .champs_deg
        .iter()
        .copied()
        .max_by(|a, b| a.0.hypot(a.1).total_cmp(&b.0.hypot(b.1)))
        .filter(|c| c.0.hypot(c.1) > 0.0)
        .ok_or_else(|| tr!("aucun champ hors axe défini", "no off-axis field defined", "kein außeraxiales Feld definiert").to_string())?;
    let longueurs_onde_nm: Vec<f64> = if sys.longueurs_onde.len() >= 2 {
        sys.longueurs_onde.iter().map(|w| w.valeur_nm).collect()
    } else {
        let (a, b) = domaine(sys);
        vec![a, sys.longueur_onde_ref_nm, b]
    };
    let nb = nb.max(3);
    let fractions_champ: Vec<f64> = (0..nb).map(|k| k as f64 / (nb - 1) as f64).collect();
    let norme = champ_max.0.hypot(champ_max.1);
    let direction = (champ_max.0 / norme, champ_max.1 / norme);
    let vise = VisePupille::calculer(sys);
    // Hauteur image du rayon principal projetée sur la direction du champ
    let hauteur = |f: f64, lambda: f64| -> Option<f64> {
        let champ = (champ_max.0 * f, champ_max.1 * f);
        let tr = tracer_rayon_unique(sys, vise.rayon(0.0, 0.0, champ.0, champ.1, lambda), champ, (0.0, 0.0));
        let (x, y) = coordonnees_image(sys, tr.impact_plan_image()?);
        Some(1000.0 * (x * direction.0 + y * direction.1))
    };
    let reference: Vec<Option<f64>> = fractions_champ.iter().map(|&f| hauteur(f, sys.longueur_onde_ref_nm)).collect();
    let ecart_um: Vec<Vec<Option<f64>>> = longueurs_onde_nm
        .iter()
        .map(|&l| {
            fractions_champ
                .iter()
                .zip(&reference)
                .map(|(&f, r)| Some(hauteur(f, l)? - (*r)?))
                .collect()
        })
        .collect();
    let mut ecart_max_um = 0.0_f64;
    for k in 0..nb {
        let vals: Vec<f64> = ecart_um.iter().filter_map(|v| v[k]).collect();
        if let (Some(a), Some(b)) = (vals.iter().copied().reduce(f64::min), vals.iter().copied().reduce(f64::max)) {
            ecart_max_um = ecart_max_um.max(b - a);
        }
    }
    let rayon_airy_um = crate::core::analysis::calculer_spot_diagram(sys, (0.0, 0.0), 1).rayon_airy_um;
    Ok(CouleurLaterale { fractions_champ, champ_max, longueurs_onde_nm, ecart_um, ecart_max_um, rayon_airy_um })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::glass::{LAMBDA_C_NM, LAMBDA_F_NM};
    use crate::core::types::{Surface, SurfaceType};

    #[test]
    fn singlet_et_doublet() {
        // Singlet BK7 de 100 mm : chromatisme longitudinal f / Vd entre F et C (≈ 1,56 mm)
        let mut sys = OpticalSystem::nouveau("singlet", 587.5618);
        let mut s1 = Surface::nouvelle(SurfaceType::Spherique, 51.68, 3.0, "BK7", 15.0);
        s1.est_diaphragme = true;
        let s2 = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 98.0, "AIR", 15.0);
        sys.surfaces = vec![s1, s2, Surface::plan_image(10.0)];
        let d = calculer_decalage_focal(&sys, 21).unwrap();
        let f = calculer_paraxial(&sys).focale_effl_mm;
        assert!((d.etendue_um - 1000.0 * f / 64.17).abs() < 0.02 * d.etendue_um, "étendue {} µm", d.etendue_um);
        // Le bleu converge avant le rouge
        assert!(d.lambda_foyer_min_nm < 500.0 && d.lambda_foyer_max_nm > 640.0);
        let k = d.longueurs_onde_nm.iter().position(|&l| (l - 587.5618).abs() < 10.0).unwrap();
        assert!(d.decalage_um[k].abs() < 100.0);

        // Doublet achromatique : courbe en U (spectre secondaire), foyer le plus court à
        // l'intérieur du domaine, étendue plus de 10 fois plus faible que celle du singlet
        let doublet = OpticalSystem::doublet_achromatique_defaut();
        let dd = calculer_decalage_focal(&doublet, 41).unwrap();
        assert!((dd.longueurs_onde_nm[0] - LAMBDA_F_NM).abs() < 1e-6 && (dd.longueurs_onde_nm[40] - LAMBDA_C_NM).abs() < 1e-6);
        assert!(dd.lambda_foyer_min_nm > 500.0 && dd.lambda_foyer_min_nm < 560.0, "{}", dd.lambda_foyer_min_nm);
        assert!(dd.etendue_um < d.etendue_um / 10.0, "{} µm", dd.etendue_um);
        let k = dd.longueurs_onde_nm.iter().position(|&l| (l - 587.5618).abs() < 3.0).unwrap();
        assert!(dd.decalage_um[k].abs() < 5.0);
    }

    #[test]
    fn couleur_laterale_d_un_singlet_avec_diaphragme_en_avant() {
        // Diaphragme 20 mm devant un singlet : le rayon principal traverse la lentille hors
        // de son centre, la couleur latérale croît avec le champ ; nulle au centre
        let mut stop = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 20.0, "AIR", 5.0);
        stop.est_diaphragme = true;
        let s1 = Surface::nouvelle(SurfaceType::Spherique, 51.68, 4.0, "BK7", 15.0);
        let s2 = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 96.0, "AIR", 15.0);
        let mut sys = OpticalSystem::nouveau("singlet", 587.5618);
        sys.surfaces = vec![stop, s1, s2, Surface::plan_image(20.0)];
        sys.ouverture = crate::core::types::ApertureSpec::PupilleEntreeMm(8.0);
        sys.champs_deg = vec![(0.0, 0.0), (0.0, 5.0)];
        let c = calculer_couleur_laterale(&sys, 11).unwrap();
        assert_eq!(c.longueurs_onde_nm.len(), 3);
        assert!(c.ecart_um.iter().all(|v| v[0].unwrap().abs() < 1e-9));
        let iref = c.longueurs_onde_nm.iter().position(|&l| (l - 587.5618).abs() < 1e-6).unwrap();
        assert!(c.ecart_um[iref].iter().all(|v| v.unwrap().abs() < 1e-9));
        let bord: Vec<f64> = c.ecart_um.iter().map(|v| v[10].unwrap()).collect();
        assert!(c.ecart_max_um > 5.0 && (bord.iter().cloned().fold(f64::MIN, f64::max) - bord.iter().cloned().fold(f64::MAX, f64::min) - c.ecart_max_um).abs() < 1e-9);
        assert!(c.ecart_um[0][5].unwrap().abs() < c.ecart_um[0][10].unwrap().abs());
    }
}
