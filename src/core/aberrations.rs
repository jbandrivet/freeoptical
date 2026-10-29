//! Aberrations géométriques : courbes de rayons (aberrations transverses et différence de
//! marche le long des coupes tangentielle et sagittale de la pupille), courbure de champ,
//! distorsion et coefficients de Seidel surface par surface.

use crate::core::onde::opd_pupille;
use crate::core::paraxial::{calculer_paraxial, position_pupille_entree};
use crate::core::raytracer::{SurfaceHit, VisePupille, tracer_rayon_unique};
use crate::core::types::{OpticalSystem, SurfaceType, Vec3};
use crate::core::glass::{LAMBDA_C_NM, LAMBDA_F_NM};
use serde::{Deserialize, Serialize};

/// Courbes de rayons d'un champ : coordonnées pupillaires de -1 à 1 et, par longueur d'onde,
/// aberrations transverses (µm, par rapport au rayon principal à la longueur d'onde de
/// référence) et différence de marche (ondes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourbesRayons {
    pub champ_deg: (f64, f64),
    pub pupille: Vec<f64>,
    pub longueurs_onde_nm: Vec<f64>,
    /// ΔY en fonction de py (coupe tangentielle)
    pub transverse_t_um: Vec<Vec<Option<f64>>>,
    /// ΔX en fonction de px (coupe sagittale)
    pub transverse_s_um: Vec<Vec<Option<f64>>>,
    pub opd_t_ondes: Vec<Vec<Option<f64>>>,
    pub opd_s_ondes: Vec<Vec<Option<f64>>>,
}

/// Point d'impact dans le repère local du plan image
fn impact_image(sys: &OpticalSystem, hit: &SurfaceHit) -> Vec3 {
    let (x, y) = crate::core::raytracer::coordonnees_image(sys, hit);
    Vec3::new(x, y, 0.0)
}

pub fn calculer_courbes_rayons(sys: &OpticalSystem, champ: (f64, f64), nb: usize) -> Result<CourbesRayons, String> {
    let nb = nb.max(5) | 1;
    let pupille: Vec<f64> = (0..nb).map(|k| -1.0 + 2.0 * k as f64 / (nb - 1) as f64).collect();
    let lambdas: Vec<f64> = if sys.longueurs_onde.is_empty() {
        vec![sys.longueur_onde_ref_nm]
    } else {
        sys.longueurs_onde.iter().map(|w| w.valeur_nm).collect()
    };
    let vise = VisePupille::calculer(sys);
    let lambda_ref = sys.longueur_onde_ref_nm;
    let chef = tracer_rayon_unique(sys, vise.rayon(0.0, 0.0, champ.0, champ.1, lambda_ref), champ, (0.0, 0.0));
    let c = chef
        .impact_plan_image()
        .map(|h| impact_image(sys, h))
        .ok_or_else(|| tr!("le rayon principal n'atteint pas le plan image", "the chief ray does not reach the image plane", "der Hauptstrahl erreicht die Bildebene nicht").to_string())?;

    let mut res = CourbesRayons {
        champ_deg: champ,
        pupille: pupille.clone(),
        longueurs_onde_nm: lambdas.clone(),
        transverse_t_um: Vec::new(),
        transverse_s_um: Vec::new(),
        opd_t_ondes: Vec::new(),
        opd_s_ondes: Vec::new(),
    };
    for &lambda in &lambdas {
        let ecart = |px: f64, py: f64| -> Option<Vec3> {
            let tr = tracer_rayon_unique(sys, vise.rayon(px, py, champ.0, champ.1, lambda), champ, (px, py));
            tr.impact_plan_image().map(|h| (impact_image(sys, h) - c) * 1e3)
        };
        res.transverse_t_um.push(pupille.iter().map(|&p| ecart(0.0, p).map(|e| e.y)).collect());
        res.transverse_s_um.push(pupille.iter().map(|&p| ecart(p, 0.0).map(|e| e.x)).collect());
        let pts_t: Vec<(f64, f64)> = pupille.iter().map(|&p| (0.0, p)).collect();
        let pts_s: Vec<(f64, f64)> = pupille.iter().map(|&p| (p, 0.0)).collect();
        res.opd_t_ondes.push(opd_pupille(sys, champ, lambda, &pts_t).unwrap_or_else(|_| vec![None; nb]));
        res.opd_s_ondes.push(opd_pupille(sys, champ, lambda, &pts_s).unwrap_or_else(|_| vec![None; nb]));
    }
    Ok(res)
}

/// Courbure de champ (position des foyers tangentiel et sagittal par rapport au plan image,
/// comptée dans le sens de la lumière) et distorsion, en fonction de l'angle de champ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourbureDistorsion {
    pub longueur_onde_nm: f64,
    pub angles_deg: Vec<f64>,
    pub foyer_t_mm: Vec<Option<f64>>,
    pub foyer_s_mm: Vec<Option<f64>>,
    /// Hauteur réelle du rayon principal sur le plan image (mm)
    pub hauteur_mm: Vec<Option<f64>>,
    /// Distorsion (h_réelle - h_paraxiale) / h_paraxiale (%)
    pub distorsion_pct: Vec<Option<f64>>,
}

/// Champ maximal du système (degrés)
pub fn champ_maximal_deg(sys: &OpticalSystem) -> f64 {
    sys.champs_deg.iter().map(|c| c.0.hypot(c.1)).fold(0.0, f64::max)
}

pub fn calculer_courbure_distorsion(sys: &OpticalSystem, lambda_nm: f64, nb: usize) -> Result<CourbureDistorsion, String> {
    let theta_max = champ_maximal_deg(sys);
    if theta_max <= 0.0 {
        return Err(tr!("aucun champ hors axe défini", "no off-axis field defined", "kein außeraxiales Feld definiert").to_string());
    }
    let nb = nb.max(3);
    let vise = VisePupille::calculer(sys);
    let inverse = sys.reperes().last().ok_or("")?.rotation.inverse();
    let eps = 0.05;
    // Rayons voisins du rayon principal : intersection de leurs projections dans le repère local
    // du plan image, le long de l'axe local, orientée dans le sens de propagation
    let foyer = |theta: f64, dx: f64, dy: f64| -> Option<f64> {
        let mut pts = Vec::new();
        for s in [-1.0, 1.0] {
            let tr = tracer_rayon_unique(sys, vise.rayon(s * dx, s * dy, 0.0, theta, lambda_nm), (0.0, theta), (s * dx, s * dy));
            let h = tr.impact_plan_image()?;
            let d = inverse * h.direction_sortante;
            if d.z.abs() < 1e-12 {
                return None;
            }
            pts.push((h.point_local, d));
        }
        let ((p1, d1), (p2, d2)) = (pts[0], pts[1]);
        let (a1, a2, b1, b2) = if dy != 0.0 {
            (p1.y, p2.y, d1.y / d1.z, d2.y / d2.z)
        } else {
            (p1.x, p2.x, d1.x / d1.z, d2.x / d2.z)
        };
        if (b1 - b2).abs() < 1e-15 {
            return None;
        }
        let z = -(a1 - a2) / (b1 - b2) * d1.z.signum();
        // En afocal : vergence en dioptries (inverse de la distance du foyer, en m)
        if sys.afocal {
            return (z.abs() > 1e-9).then(|| 1000.0 / z);
        }
        Some(z)
    };
    let hauteur = |theta: f64| -> Option<f64> {
        let tr = tracer_rayon_unique(sys, vise.rayon(0.0, 0.0, 0.0, theta, lambda_nm), (0.0, theta), (0.0, 0.0));
        tr.impact_plan_image().map(|h| impact_image(sys, h).y)
    };
    // Grandeur paraxiale du champ : angle (tangente) pour un objet à l'infini, hauteur sinon
    let mesure = |theta: f64| if sys.objet_fini().is_some() { theta } else { theta.to_radians().tan() };
    let delta = theta_max * 1e-4;
    let pente = hauteur(delta).map(|h| h / mesure(delta));
    let mut res = CourbureDistorsion {
        longueur_onde_nm: lambda_nm,
        angles_deg: Vec::new(),
        foyer_t_mm: Vec::new(),
        foyer_s_mm: Vec::new(),
        hauteur_mm: Vec::new(),
        distorsion_pct: Vec::new(),
    };
    for k in 0..nb {
        let theta = theta_max * k as f64 / (nb - 1) as f64;
        res.angles_deg.push(theta);
        res.foyer_t_mm.push(foyer(theta, 0.0, eps));
        res.foyer_s_mm.push(foyer(theta, eps, 0.0));
        let h = hauteur(theta);
        res.hauteur_mm.push(h);
        res.distorsion_pct.push(match (h, pente) {
            (Some(h), Some(p)) if k > 0 => {
                let h_par = p * mesure(theta);
                (h_par.abs() > 1e-12).then(|| 100.0 * (h - h_par) / h_par)
            }
            _ => Some(0.0).filter(|_| k == 0),
        });
    }
    Ok(res)
}

/// Coefficients de Seidel d'une surface (mm) : S_I sphérique, S_II coma, S_III astigmatisme,
/// S_IV courbure de Petzval, S_V distorsion, C_L chromatisme axial, C_T chromatisme latéral
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LigneSeidel {
    pub surface: usize,
    pub s1: f64,
    pub s2: f64,
    pub s3: f64,
    pub s4: f64,
    pub s5: f64,
    pub cl: f64,
    pub ct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seidel {
    pub lignes: Vec<LigneSeidel>,
    pub total: LigneSeidel,
    /// Champ utilisé pour le rayon principal (degrés)
    pub champ_max_deg: f64,
    pub longueur_onde_nm: f64,
    /// Coefficients d'onde équivalents (ondes) : W040 = S_I/8, W131 = S_II/2, W222 = S_III/2,
    /// W220 = (S_III + S_IV)/4, W311 = S_V/2
    pub w040: f64,
    pub w131: f64,
    pub w222: f64,
    pub w220: f64,
    pub w311: f64,
}

/// Coefficients de Seidel par le tracé paraxial des rayons marginal et principal
/// (formules de Welford, indices signés après chaque miroir ; contributions des coniques et du
/// terme A4 des asphères ; les bascules et les réseaux sont ignorés)
pub fn calculer_seidel(sys: &OpticalSystem) -> Result<Seidel, String> {
    let p = calculer_paraxial(sys);
    if !p.focale_effl_mm.is_finite() && !sys.afocal {
        return Err(tr!("système afocal", "afocal system", "afokales System").to_string());
    }
    let lambda = sys.longueur_onde_ref_nm;
    let theta = champ_maximal_deg(sys);
    let nb = sys.surfaces.len().saturating_sub(1);
    let r_pupille = 0.5 * p.diametre_pupille_entree_mm;
    let z_ep = position_pupille_entree(sys);
    let (mut y, mut u, mut yb, mut ub) = match sys.objet_fini() {
        // Objet à distance finie : marginal issu du point axial, principal du bord du champ (mm)
        Some(d) => {
            let u0 = r_pupille / (z_ep + d);
            let ub0 = -theta / (z_ep + d);
            (u0 * d, u0, theta + ub0 * d, ub0)
        }
        None => {
            let ub0 = theta.to_radians().tan();
            (r_pupille, 0.0, -ub0 * z_ep, ub0)
        }
    };
    let mut n = 1.0_f64;
    let mut dn = 0.0_f64; // nF - nC du milieu courant (signé comme n)
    let mut signe = 1.0;
    let h = n * (ub * y - u * yb);
    let mut lignes = Vec::new();
    for (i, surf) in sys.surfaces.iter().take(nb).enumerate() {
        if surf.est_coupure() {
            // Rupture de repère : simple transfert
            lignes.push(LigneSeidel { surface: i + 1, ..Default::default() });
            y += surf.epaisseur * u;
            yb += surf.epaisseur * ub;
            continue;
        }
        if surf.type_surface == SurfaceType::Ideale && surf.focale_ideale_mm != 0.0 {
            // Lentille idéale : aucune aberration, seulement la déviation paraxiale
            u = (n * u - y * n.abs() / surf.focale_ideale_mm) / n;
            ub = (n * ub - yb * n.abs() / surf.focale_ideale_mm) / n;
            lignes.push(LigneSeidel { surface: i + 1, ..Default::default() });
            y += surf.epaisseur * u;
            yb += surf.epaisseur * ub;
            continue;
        }
        let mat = sys.catalogue.resoudre(&surf.verre);
        let (n2, dn2) = if mat.est_miroir() {
            signe = -signe;
            (-n, -dn)
        } else {
            let nm = mat.indice_refraction(lambda);
            (signe * nm, signe * (mat.indice_refraction(LAMBDA_F_NM) - mat.indice_refraction(LAMBDA_C_NM)))
        };
        let c = surf.courbure();
        let a = n * (u + y * c);
        let ab = n * (ub + yb * c);
        let u2 = (n * u - y * (n2 - n) * c) / n2;
        let ub2 = (n * ub - yb * (n2 - n) * c) / n2;
        let d_un = u2 / n2 - u / n;
        let d_1n = 1.0 / n2 - 1.0 / n;
        let mut l = LigneSeidel {
            surface: i + 1,
            s1: -a * a * y * d_un,
            s2: -a * ab * y * d_un,
            s3: -ab * ab * y * d_un,
            s4: -h * h * c * d_1n,
            ..Default::default()
        };
        l.s5 = if a.abs() > 1e-12 { ab / a * (l.s3 + l.s4) } else { 0.0 };
        // Déformation asphérique du 4e ordre : conique et terme A4
        let b4 = match surf.type_surface {
            SurfaceType::Conique | SurfaceType::Aspherique => {
                surf.conique * c.powi(3) / 8.0
                    + if surf.type_surface == SurfaceType::Aspherique { surf.aspheriques[0] } else { 0.0 }
            }
            _ => 0.0,
        };
        if b4 != 0.0 {
            let k = 8.0 * b4 * (n2 - n);
            l.s1 += k * y.powi(4);
            l.s2 += k * y.powi(3) * yb;
            l.s3 += k * y * y * yb * yb;
            l.s5 += k * y * yb.powi(3);
        }
        let d_chrom = if n2.abs() > 0.0 { dn2 / n2 - dn / n } else { 0.0 };
        l.cl = a * y * d_chrom;
        l.ct = ab * y * d_chrom;
        lignes.push(l);
        // Transfert vers la surface suivante
        y += surf.epaisseur * u2;
        yb += surf.epaisseur * ub2;
        u = u2;
        ub = ub2;
        n = n2;
        dn = dn2;
    }
    let mut total = LigneSeidel::default();
    for l in &lignes {
        total.s1 += l.s1;
        total.s2 += l.s2;
        total.s3 += l.s3;
        total.s4 += l.s4;
        total.s5 += l.s5;
        total.cl += l.cl;
        total.ct += l.ct;
    }
    let onde = lambda * 1e-6;
    Ok(Seidel {
        w040: total.s1 / 8.0 / onde,
        w131: total.s2 / 2.0 / onde,
        w222: total.s3 / 2.0 / onde,
        w220: (total.s3 + total.s4) / 4.0 / onde,
        w311: total.s5 / 2.0 / onde,
        lignes,
        total,
        champ_max_deg: theta,
        longueur_onde_nm: lambda,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::onde::calculer_front_onde;
    use crate::core::types::{ApertureSpec, Surface, Wavelength};

    fn miroir(r: f64, k: f64, epd: f64, champ: f64) -> OpticalSystem {
        let mut m = Surface::nouvelle(SurfaceType::Conique, r, r / 2.0, "MIROIR", epd);
        m.conique = k;
        m.est_diaphragme = true;
        OpticalSystem {
            nom: "miroir".into(),
            longueur_onde_ref_nm: 587.5618,
            longueurs_onde: vec![Wavelength::new(587.5618, 1.0, "d")],
            ouverture: ApertureSpec::PupilleEntreeMm(epd),
            champs_deg: vec![(0.0, 0.0), (0.0, champ)],
            surfaces: vec![m, Surface::plan_image(40.0)],
            catalogue: Default::default(),
            distance_objet_mm: None,
            afocal: false,
        }
    }

    #[test]
    fn seidel_d_un_miroir_spherique_et_d_une_parabole() {
        // Sphère R = 1000, h = 50 : |S_I| = 2 h⁴ / R³ ; parabole : S_I nul
        let s = calculer_seidel(&miroir(-1000.0, 0.0, 100.0, 0.5)).unwrap();
        assert!((s.total.s1.abs() - 0.0125).abs() < 1e-9, "S_I = {}", s.total.s1);
        let p = calculer_seidel(&miroir(-2000.0, -1.0, 200.0, 0.5)).unwrap();
        assert!(p.total.s1.abs() < 1e-12);
        // Coma de la parabole à 0,5° : W131 = S_II / 2 égale à la coma du front d'onde réel
        // (coefficient de Zernike Z7 = W131 / (3 √8))
        let carte = calculer_front_onde(&miroir(-2000.0, -1.0, 200.0, 0.5), (0.0, 0.5), 587.5618, 48).unwrap();
        let w131_reel = carte.zernike_ondes[6].abs() * 3.0 * 8f64.sqrt();
        assert!((p.w131.abs() - w131_reel).abs() < 0.02 * w131_reel, "W131 {} / {}", p.w131, w131_reel);
        // Miroir seul, pupille sur le miroir : pas de distorsion ; Petzval = -2 H² c ... non nul
        assert!(p.total.s5.abs() < 1e-12);
    }

    #[test]
    fn seidel_et_chromatisme_du_doublet() {
        let d = OpticalSystem::doublet_achromatique_defaut();
        let s = calculer_seidel(&d).unwrap();
        assert_eq!(s.lignes.len(), 3);
        // Doublet achromatique : chromatisme axial résiduel petit devant celui d'une lentille seule
        let mut singlet = d.clone();
        singlet.surfaces[1].verre = "AIR".into();
        let s1 = calculer_seidel(&singlet).unwrap();
        assert!(s.total.cl.abs() < 0.1 * s1.total.cl.abs(), "CL {} / {}", s.total.cl, s1.total.cl);
    }

    #[test]
    fn courbure_de_champ_d_un_miroir_spherique() {
        // Miroir sphérique, pupille au centre de courbure : pas d'astigmatisme, surface focale
        // sphérique de rayon f (Petzval) : décalage ≈ -h² / (2 f) pour h = f tan θ
        let mut sys = miroir(-1000.0, 0.0, 20.0, 5.0);
        let mut pupille = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 1000.0, "AIR", 20.0);
        pupille.est_diaphragme = true;
        sys.surfaces[0].est_diaphragme = false;
        sys.surfaces[0].semi_diametre = 100.0;
        sys.surfaces.insert(0, pupille);
        let c = calculer_courbure_distorsion(&sys, 587.5618, 6).unwrap();
        let k = c.angles_deg.len() - 1;
        let (t, s) = (c.foyer_t_mm[k].unwrap(), c.foyer_s_mm[k].unwrap());
        assert!((t - s).abs() < 0.02 * s.abs(), "T {t} S {s}");
        let h = 500.0 * 5f64.to_radians().sin();
        assert!((s.abs() - h * h / 1000.0).abs() < 0.1 * h * h / 1000.0, "S {s} attendu {}", h * h / 1000.0);
    }

    #[test]
    fn courbes_de_rayons_et_distorsion_d_une_parabole() {
        let sys = miroir(-2000.0, -1.0, 200.0, 0.5);
        let c = calculer_courbes_rayons(&sys, (0.0, 0.0), 11).unwrap();
        assert!(c.transverse_t_um[0].iter().flatten().all(|v| v.abs() < 1e-6));
        let c = calculer_courbes_rayons(&sys, (0.0, 0.5), 11).unwrap();
        // Partie paire de la courbe tangentielle aux bords = coma tangentielle 3 θ f / (16 F²)
        let t = &c.transverse_t_um[0];
        let coma = 0.5 * (t[0].unwrap() + t[10].unwrap());
        let attendu = 3.0 * 0.5f64.to_radians() * 1000.0 / (16.0 * 25.0) * 1e3;
        assert!((coma.abs() - attendu).abs() < 0.03 * attendu, "coma {coma} µm, attendu {attendu}");
        let d = calculer_courbure_distorsion(&sys, 587.5618, 5).unwrap();
        assert!(d.distorsion_pct.iter().flatten().all(|v| v.abs() < 0.01));
    }
}
