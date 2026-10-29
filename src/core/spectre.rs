//! Outils pour les spectrographes : position de chaque longueur d'onde sur le détecteur,
//! dispersion, tache par longueur d'onde, image de la fente et pouvoir de résolution R = λ/Δλ.

use crate::core::raytracer::{VisePupille, coordonnees_image, tracer_rayon_unique};
use crate::core::types::OpticalSystem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spectre {
    pub champ: (f64, f64),
    pub longueurs_onde_nm: Vec<f64>,
    /// Position du rayon principal sur le détecteur (mm, repère local du plan image)
    pub positions_mm: Vec<Option<(f64, f64)>>,
    /// Écart quadratique de la tache de chaque longueur d'onde autour de son propre centroïde (µm)
    pub tache_rms_um: Vec<Option<f64>>,
    /// Écart quadratique de la tache projeté sur la direction de dispersion (µm)
    pub tache_rms_dispersion_um: Vec<Option<f64>>,
    /// Dispersion linéaire inverse (nm/mm)
    pub dispersion_nm_mm: Vec<Option<f64>>,
    /// Direction moyenne de la dispersion sur le détecteur (vecteur unitaire)
    pub direction: (f64, f64),
    /// Largeur de fente demandée : µm pour un objet à distance finie, secondes d'arc sinon
    pub largeur_fente: f64,
    /// Largeur de l'image de la fente dans la direction de dispersion (µm), à la longueur d'onde centrale
    pub image_fente_um: Option<f64>,
    /// Pouvoir de résolution R = λ / Δλ (largeur combinée image de fente et tache)
    pub resolution: Vec<Option<f64>>,
    /// Longueur du spectre sur le détecteur (mm)
    pub longueur_spectre_mm: f64,
}

/// Points d'une grille hexapolaire de 3 anneaux
fn grille() -> Vec<(f64, f64)> {
    let mut pts = vec![(0.0, 0.0)];
    for k in 1..=3 {
        let r = k as f64 / 3.0;
        for j in 0..6 * k {
            let t = std::f64::consts::TAU * j as f64 / (6 * k) as f64;
            pts.push((r * t.cos(), r * t.sin()));
        }
    }
    pts
}

pub fn calculer_spectre(
    sys: &OpticalSystem,
    champ: (f64, f64),
    lambda_min_nm: f64,
    lambda_max_nm: f64,
    nb: usize,
    largeur_fente: f64,
) -> Result<Spectre, String> {
    let nb = nb.clamp(3, 401);
    if !(lambda_min_nm > 0.0 && lambda_max_nm > lambda_min_nm) {
        return Err(tr!("domaine de longueurs d'onde invalide", "invalid wavelength range", "ungültiger Wellenlängenbereich").to_string());
    }
    let lambdas: Vec<f64> = (0..nb)
        .map(|k| lambda_min_nm + (lambda_max_nm - lambda_min_nm) * k as f64 / (nb - 1) as f64)
        .collect();
    let vise = VisePupille::calculer(sys);
    let position = |c: (f64, f64), l: f64| -> Option<(f64, f64)> {
        let tr = tracer_rayon_unique(sys, vise.rayon(0.0, 0.0, c.0, c.1, l), c, (0.0, 0.0));
        tr.impact_plan_image().map(|h| coordonnees_image(sys, h))
    };
    let positions: Vec<Option<(f64, f64)>> = lambdas.iter().map(|&l| position(champ, l)).collect();
    let valides: Vec<(f64, f64)> = positions.iter().flatten().copied().collect();
    if valides.len() < 2 {
        return Err(tr!(
            "moins de deux longueurs d'onde atteignent le détecteur",
            "fewer than two wavelengths reach the detector",
            "weniger als zwei Wellenlängen erreichen den Detektor"
        )
        .to_string());
    }
    let (p0, p1) = (valides[0], valides[valides.len() - 1]);
    let longueur = (p1.0 - p0.0).hypot(p1.1 - p0.1);
    let direction = if longueur > 1e-12 { ((p1.0 - p0.0) / longueur, (p1.1 - p0.1) / longueur) } else { (0.0, 1.0) };

    // Dispersion par différences centrées sur la courbe des positions
    let mut dispersion = vec![None; nb];
    for k in 0..nb {
        let (a, b) = (k.saturating_sub(1), (k + 1).min(nb - 1));
        if let (Some(pa), Some(pb)) = (positions[a], positions[b]) {
            let ds = (pb.0 - pa.0).hypot(pb.1 - pa.1);
            if ds > 1e-12 && b > a {
                dispersion[k] = Some((lambdas[b] - lambdas[a]) / ds);
            }
        }
    }

    // Tache de chaque longueur d'onde autour de son centroïde
    let points = grille();
    let mut tache = vec![None; nb];
    let mut tache_disp = vec![None; nb];
    for (k, &l) in lambdas.iter().enumerate() {
        let impacts: Vec<(f64, f64)> = points
            .iter()
            .filter_map(|&(px, py)| {
                let tr = tracer_rayon_unique(sys, vise.rayon(px, py, champ.0, champ.1, l), champ, (px, py));
                tr.impact_plan_image().map(|h| coordonnees_image(sys, h))
            })
            .collect();
        if impacts.len() >= 3 {
            let n = impacts.len() as f64;
            let cx = impacts.iter().map(|p| p.0).sum::<f64>() / n;
            let cy = impacts.iter().map(|p| p.1).sum::<f64>() / n;
            let rms = (impacts.iter().map(|p| (p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sum::<f64>() / n).sqrt();
            let proj = (impacts
                .iter()
                .map(|p| ((p.0 - cx) * direction.0 + (p.1 - cy) * direction.1).powi(2))
                .sum::<f64>()
                / n)
                .sqrt();
            tache[k] = Some(rms * 1e3);
            tache_disp[k] = Some(proj * 1e3);
        }
    }

    // Image de la fente : déplacement de l'image quand le point objet bouge de ±δ, projeté sur
    // la direction de dispersion ; on garde la direction objet qui y correspond le mieux
    let k_milieu = nb / 2;
    let l_milieu = lambdas[k_milieu];
    let delta = if sys.objet_fini().is_some() { 0.01 } else { 1.0 / 3600.0 };
    let par_unite = if sys.objet_fini().is_some() { 1e-3 } else { 1.0 / 3600.0 };
    let image_fente = [(delta, 0.0), (0.0, delta)]
        .iter()
        .filter_map(|&(dx, dy)| {
            let a = position((champ.0 - dx, champ.1 - dy), l_milieu)?;
            let b = position((champ.0 + dx, champ.1 + dy), l_milieu)?;
            let proj = ((b.0 - a.0) * direction.0 + (b.1 - a.1) * direction.1).abs() / (2.0 * delta);
            Some(proj * largeur_fente * par_unite * 1e3)
        })
        .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));

    let resolution = (0..nb)
        .map(|k| {
            let d = dispersion[k]?;
            let s = tache_disp[k].unwrap_or(0.0) * 2.355;
            let fente = image_fente.unwrap_or(0.0);
            let largeur_um = (fente * fente + s * s).sqrt();
            (largeur_um > 1e-9).then(|| lambdas[k] / (d * largeur_um * 1e-3))
        })
        .collect();

    Ok(Spectre {
        champ,
        longueurs_onde_nm: lambdas,
        positions_mm: positions,
        tache_rms_um: tache,
        tache_rms_dispersion_um: tache_disp,
        dispersion_nm_mm: dispersion,
        direction,
        largeur_fente,
        image_fente_um: image_fente,
        resolution,
        longueur_spectre_mm: longueur,
    })
}

/// Couleur approximative d'une longueur d'onde visible (rouge, vert, bleu entre 0 et 1)
pub fn couleur_spectrale(l: f64) -> (f64, f64, f64) {
    let (r, g, b) = if l < 440.0 {
        ((440.0 - l) / 60.0, 0.0, 1.0)
    } else if l < 490.0 {
        (0.0, (l - 440.0) / 50.0, 1.0)
    } else if l < 510.0 {
        (0.0, 1.0, (510.0 - l) / 20.0)
    } else if l < 580.0 {
        ((l - 510.0) / 70.0, 1.0, 0.0)
    } else if l < 645.0 {
        (1.0, (645.0 - l) / 65.0, 0.0)
    } else {
        (1.0, 0.0, 0.0)
    };
    let attenuation = if l < 420.0 { 0.3 + 0.7 * ((l - 380.0) / 40.0).max(0.0) } else if l > 700.0 { 0.3 + 0.7 * ((780.0 - l) / 80.0).max(0.0) } else { 1.0 };
    (r.clamp(0.0, 1.0) * attenuation, g.clamp(0.0, 1.0) * attenuation, b.clamp(0.0, 1.0) * attenuation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{ApertureSpec, Surface, SurfaceType, Wavelength};

    /// Réseau en transmission 600 traits/mm (pupille), caméra idéale f = 200 mm
    fn spectrographe() -> OpticalSystem {
        let mut reseau = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 20.0, "AIR", 30.0);
        reseau.reseau_traits_mm = 600.0;
        reseau.reseau_ordre = 1;
        reseau.est_diaphragme = true;
        let mut camera = Surface::nouvelle(SurfaceType::Ideale, f64::INFINITY, 200.0, "AIR", 60.0);
        camera.focale_ideale_mm = 200.0;
        OpticalSystem {
            nom: "spectro".into(),
            longueur_onde_ref_nm: 550.0,
            longueurs_onde: vec![Wavelength::new(550.0, 1.0, "550")],
            ouverture: ApertureSpec::PupilleEntreeMm(20.0),
            champs_deg: vec![(0.0, 0.0)],
            surfaces: vec![reseau, camera, Surface::plan_image(80.0)],
            catalogue: Default::default(),
            distance_objet_mm: None,
            afocal: false,
            tolerances: Vec::new(),
        }
    }

    #[test]
    fn dispersion_et_resolution_d_un_reseau() {
        let s = calculer_spectre(&spectrographe(), (0.0, 0.0), 450.0, 650.0, 21, 2.0).unwrap();
        let k = 10; // 550 nm
        let sin: f64 = 550.0e-6 * 600.0;
        let cos = (1.0 - sin * sin).sqrt();
        // Position : y = f tan θ ; dispersion inverse dλ/dy = cos³θ / (f N) en nm/mm
        let (_, y) = s.positions_mm[k].unwrap();
        assert!((y - 200.0 * sin / cos).abs() < 1e-6, "y = {y}");
        let attendu = cos.powi(3) / (200.0 * 600.0e-6);
        let d = s.dispersion_nm_mm[k].unwrap();
        assert!((d - attendu).abs() < 2e-3 * attendu, "{d} nm/mm au lieu de {attendu}");
        assert!(s.tache_rms_um[k].unwrap() < 1e-3);
        // Image d'une fente de 2" : f δ / cos³θ ; R = λ / (dλ/dy · largeur)
        let fente = 200.0 * (2.0f64 / 3600.0).to_radians() / cos.powi(3) * 1e3;
        let image = s.image_fente_um.unwrap();
        assert!((image - fente).abs() < 0.01 * fente, "fente {image} µm au lieu de {fente}");
        let r = s.resolution[k].unwrap();
        assert!((r - 550.0 / (attendu * fente * 1e-3)).abs() < 0.02 * r, "R = {r}");
        // Spectre de 450 à 650 nm : ~ 28 mm
        assert!((s.longueur_spectre_mm - 200.0 * ((0.39f64.asin()).tan() - (0.27f64.asin()).tan())).abs() < 1e-6);
    }
}
