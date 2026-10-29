//! Énergie encerclée : fraction de la lumière contenue dans un cercle de rayon donné centré sur
//! le centroïde de la tache, géométrique (lancer de rayons) et par diffraction (PSF), comparée à
//! la tache d'Airy d'une pupille circulaire sans aberration.

use crate::core::analysis::calculer_spot_diagram;
use crate::core::onde::calculer_psf_ftm;
use crate::core::raytracer::{coordonnees_image, tracer_grille_hexapolaire};
use crate::core::types::OpticalSystem;
use serde::{Deserialize, Serialize};

/// Courbes d'énergie encerclée d'un champ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergieEncerclee {
    /// Rayons des cercles (µm ; µrad en afocal)
    pub rayons_um: Vec<f64>,
    /// Fraction géométrique (0 à 1), toutes longueurs d'onde pondérées
    pub geometrique: Vec<f64>,
    /// Fraction par diffraction, d'après la PSF polychromatique
    pub diffraction: Option<Vec<f64>>,
    /// Tache d'Airy d'une pupille pleine sans aberration au même nombre d'ouverture
    pub airy: Vec<f64>,
    /// Rayons contenant 50 % et 80 % de l'énergie (géométrique)
    pub r50_geo_um: f64,
    pub r80_geo_um: f64,
    /// Rayons contenant 50 % et 80 % de l'énergie (diffraction)
    pub r50_diffraction_um: Option<f64>,
    pub r80_diffraction_um: Option<f64>,
    /// Rayon du premier anneau noir d'Airy (µm)
    pub rayon_airy_um: f64,
    pub champ_deg: (f64, f64),
}

/// Fonction de Bessel J0 (approximations rationnelles de Numerical Recipes, erreur < 1e-8)
pub fn bessel_j0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let a = 57568490574.0 + y * (-13362590354.0 + y * (651619640.7 + y * (-11214424.18 + y * (77392.33017 + y * -184.9052456))));
        let b = 57568490411.0 + y * (1029532985.0 + y * (9494680.718 + y * (59272.64853 + y * (267.8532712 + y))));
        a / b
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - 0.785398164;
        let a = 1.0 + y * (-0.1098628627e-2 + y * (0.2734510407e-4 + y * (-0.2073370639e-5 + y * 0.2093887211e-6)));
        let b = -0.1562499995e-1 + y * (0.1430488765e-3 + y * (-0.6911147651e-5 + y * (0.7621095161e-6 - y * 0.934935152e-7)));
        (0.636619772 / ax).sqrt() * (xx.cos() * a - z * xx.sin() * b)
    }
}

/// Fonction de Bessel J1
pub fn bessel_j1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let a = x * (72362614232.0 + y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * -30.16036606)))));
        let b = 144725228442.0 + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y))));
        a / b
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - 2.356194491;
        let a = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * -0.240337019e-6)));
        let b = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
        let r = (0.636619772 / ax).sqrt() * (xx.cos() * a - z * xx.sin() * b);
        if x < 0.0 { -r } else { r }
    }
}

/// Énergie encerclée de la tache d'Airy (pupille circulaire pleine) dans un rayon `r_um`
pub fn energie_airy(r_um: f64, lambda_um: f64, f_nombre: f64) -> f64 {
    let v = std::f64::consts::PI * r_um / (lambda_um * f_nombre);
    1.0 - bessel_j0(v).powi(2) - bessel_j1(v).powi(2)
}

/// Rayon où une courbe croissante atteint `niveau` (interpolation linéaire)
pub fn rayon_pour(rayons: &[f64], courbe: &[f64], niveau: f64) -> f64 {
    for k in 1..courbe.len() {
        if courbe[k] >= niveau {
            let (a, b) = (courbe[k - 1], courbe[k]);
            let t = if b > a { (niveau - a) / (b - a) } else { 0.0 };
            return rayons[k - 1] + t * (rayons[k] - rayons[k - 1]);
        }
    }
    f64::NAN
}

/// Énergie encerclée d'une PSF échantillonnée (grille `n` × `n`, pas `pas_um`), autour de son
/// centroïde. Calcul exact pour une PSF à bande limitée : énergie dans un disque de rayon r =
/// Σ FTO(ν) · r J1(2π r ν) / ν (transformée du disque), la FTO étant obtenue par FFT de la PSF.
pub fn energie_psf(intensite: &[f64], n: usize, pas_um: f64, rayons_um: &[f64]) -> Vec<f64> {
    use rustfft::{FftPlanner, num_complex::Complex};
    let somme: f64 = intensite.iter().sum();
    if somme <= 0.0 || n == 0 {
        return vec![0.0; rayons_um.len()];
    }
    let (mut mx, mut my) = (0.0, 0.0);
    for (k, &v) in intensite.iter().enumerate() {
        mx += v * (k % n) as f64;
        my += v * (k / n) as f64;
    }
    let (mx, my) = (mx / somme, my / somme);
    let mut fto: Vec<Complex<f64>> = intensite.iter().map(|&v| Complex::new(v, 0.0)).collect();
    crate::core::onde::fft2(&mut FftPlanner::new(), &mut fto, n);
    let zero = fto[0].re;
    let dnu = 1.0 / (n as f64 * pas_um);
    // Fréquences utiles : FTO non négligeable, recentrée sur le centroïde (théorème du retard)
    let signe = |k: usize| if k < n / 2 { k as f64 } else { k as f64 - n as f64 };
    let mut termes = Vec::new();
    for (k, c) in fto.iter().enumerate() {
        if c.norm() < 1e-10 * zero.abs() {
            continue;
        }
        let (kx, ky) = (signe(k % n), signe(k / n));
        let phase = 2.0 * std::f64::consts::PI * (kx * mx + ky * my) / n as f64;
        let reel = (c * Complex::new(phase.cos(), phase.sin())).re / zero;
        termes.push(((kx.hypot(ky)) * dnu, reel * dnu * dnu));
    }
    rayons_um
        .iter()
        .map(|&r| {
            let e: f64 = termes
                .iter()
                .map(|&(nu, a)| {
                    let noyau = if nu == 0.0 {
                        std::f64::consts::PI * r * r
                    } else {
                        r * bessel_j1(2.0 * std::f64::consts::PI * r * nu) / nu
                    };
                    a * noyau
                })
                .sum();
            e.clamp(0.0, 1.0)
        })
        .collect()
}

/// Calcule l'énergie encerclée du champ `champ` ; avec `diffraction`, ajoute la courbe tirée
/// de la PSF (plus lent)
pub fn calculer_energie_encerclee(sys: &OpticalSystem, champ: (f64, f64), diffraction: bool) -> Result<EnergieEncerclee, String> {
    let longueurs: Vec<(f64, f64)> = if sys.longueurs_onde.is_empty() {
        vec![(sys.longueur_onde_ref_nm, 1.0)]
    } else {
        sys.longueurs_onde.iter().map(|w| (w.valeur_nm, w.poids.max(0.0))).collect()
    };
    // Impacts pondérés : la grille hexapolaire échantillonne la pupille à densité uniforme
    let mut impacts = Vec::new();
    for &(lambda, poids) in &longueurs {
        let rayons = tracer_grille_hexapolaire(sys, champ.0, champ.1, 16, lambda);
        let n = rayons.len() as f64;
        for tr in &rayons {
            if let Some(h) = tr.impact_plan_image() {
                let (x, y) = coordonnees_image(sys, h);
                if x.is_finite() && y.is_finite() {
                    impacts.push((x * 1000.0, y * 1000.0, poids / n));
                }
            }
        }
    }
    let total: f64 = impacts.iter().map(|p| p.2).sum();
    if impacts.is_empty() || total <= 0.0 {
        return Err(tr!("aucun rayon n'atteint le plan image", "no ray reaches the image plane", "kein Strahl erreicht die Bildebene").to_string());
    }
    let cx = impacts.iter().map(|p| p.0 * p.2).sum::<f64>() / total;
    let cy = impacts.iter().map(|p| p.1 * p.2).sum::<f64>() / total;
    let mut distances: Vec<(f64, f64)> = impacts.iter().map(|p| ((p.0 - cx).hypot(p.1 - cy), p.2 / total)).collect();
    distances.sort_by(|a, b| a.0.total_cmp(&b.0));

    let spot = calculer_spot_diagram(sys, champ, 2);
    let rayon_airy_um = spot.rayon_airy_um;
    let r_max = (distances.last().map(|d| d.0).unwrap_or(0.0) * 1.05).max(3.0 * rayon_airy_um).max(1e-3);
    let nb = 160;
    let rayons_um: Vec<f64> = (0..=nb).map(|k| r_max * k as f64 / nb as f64).collect();
    let mut geometrique = Vec::with_capacity(rayons_um.len());
    let (mut cumul, mut j) = (0.0, 0);
    for &r in &rayons_um {
        while j < distances.len() && distances[j].0 <= r {
            cumul += distances[j].1;
            j += 1;
        }
        geometrique.push(cumul.min(1.0));
    }
    // Rayons à 50 et 80 % directement sur la liste triée (plus précis que sur la courbe)
    let quantile = |niveau: f64| {
        let mut c = 0.0;
        for d in &distances {
            c += d.1;
            if c >= niveau {
                return d.0;
            }
        }
        distances.last().map(|d| d.0).unwrap_or(f64::NAN)
    };

    let lambda_um = sys.longueur_onde_ref_nm * 1e-3;
    let f_airy = rayon_airy_um / (1.22 * lambda_um);
    let airy: Vec<f64> = rayons_um.iter().map(|&r| energie_airy(r, lambda_um, f_airy)).collect();

    let diffraction = if diffraction {
        let (psf, _) = calculer_psf_ftm(sys, champ, None)?;
        Some(energie_psf(&psf.intensite, psf.taille, psf.pas_um, &rayons_um))
    } else {
        None
    };
    let r50_diffraction_um = diffraction.as_ref().map(|d| rayon_pour(&rayons_um, d, 0.5));
    let r80_diffraction_um = diffraction.as_ref().map(|d| rayon_pour(&rayons_um, d, 0.8));
    Ok(EnergieEncerclee {
        geometrique,
        diffraction,
        airy,
        r50_geo_um: quantile(0.5),
        r80_geo_um: quantile(0.8),
        r50_diffraction_um,
        r80_diffraction_um,
        rayon_airy_um,
        champ_deg: champ,
        rayons_um,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{ApertureSpec, Surface, SurfaceType, Wavelength};

    #[test]
    fn bessel_et_airy() {
        assert!((bessel_j0(0.0) - 1.0).abs() < 1e-8);
        assert!(bessel_j0(2.404825557695773).abs() < 1e-7);
        assert!(bessel_j1(3.831705970207512).abs() < 1e-7);
        assert!((bessel_j1(10.0) - 0.04347274616886144).abs() < 1e-7);
        // 83,8 % dans le premier anneau noir, 91 % dans le deuxième
        assert!((energie_airy(1.22 * 0.55 * 8.0, 0.55, 8.0) - 0.838).abs() < 0.001);
        assert!((energie_airy(2.233 * 0.55 * 8.0, 0.55, 8.0) - 0.910).abs() < 0.001);
    }

    #[test]
    fn parabole_parfaite_suit_airy() {
        // Parabole F/5 sur l'axe : géométriquement tout est dans un point, par diffraction la
        // courbe suit celle de la tache d'Airy
        let mut m = Surface::nouvelle(SurfaceType::Conique, -2000.0, -1000.0, "MIROIR", 110.0);
        m.conique = -1.0;
        m.est_diaphragme = true;
        let sys = OpticalSystem {
            longueurs_onde: vec![Wavelength::new(587.5618, 1.0, "d")],
            ouverture: ApertureSpec::PupilleEntreeMm(200.0),
            surfaces: vec![m, Surface::plan_image(20.0)],
            ..OpticalSystem::nouveau("parabole", 587.5618)
        };
        let ee = calculer_energie_encerclee(&sys, (0.0, 0.0), true).unwrap();
        assert!(ee.r80_geo_um < 1e-3, "r80 géo {}", ee.r80_geo_um);
        let d = ee.diffraction.as_ref().unwrap();
        for (k, &r) in ee.rayons_um.iter().enumerate() {
            let tolerance = 0.005;
            if r > 0.0 {
                assert!((d[k] - ee.airy[k]).abs() < tolerance, "r = {r} : {} / {}", d[k], ee.airy[k]);
            }
        }
        // r80 d'Airy par dichotomie : ≈ 0,90 λN
        let (mut a, mut b) = (0.0, 10.0);
        for _ in 0..60 {
            let m = 0.5 * (a + b);
            if energie_airy(m, 0.5875618, 5.0) < 0.8 { a = m } else { b = m }
        }
        let r80 = ee.r80_diffraction_um.unwrap();
        assert!((r80 - a).abs() < 0.02, "r80 {r80} / Airy {a}");
        assert!((a / (0.5875618 * 5.0) - 0.896).abs() < 0.005);
    }

    #[test]
    fn doublet_geometrique() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let ee = calculer_energie_encerclee(&sys, (0.0, 2.5), false).unwrap();
        assert!(ee.diffraction.is_none());
        assert!(ee.geometrique.windows(2).all(|w| w[1] >= w[0]));
        assert!((ee.geometrique.last().unwrap() - 1.0).abs() < 1e-9);
        assert!(ee.r50_geo_um < ee.r80_geo_um);
        let spot = calculer_spot_diagram(&sys, (0.0, 2.5), 6);
        assert!(ee.r80_geo_um < spot.rayon_geo_um);
    }
}
