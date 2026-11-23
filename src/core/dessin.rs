//! Géométrie des pièces pour les plans et la CAO : contours des lentilles (avec les méplats
//! quand les deux faces n'ont pas le même diamètre) et profils des miroirs, dans la coupe
//! méridienne (plan Y-Z).
//!
//! Un contour est une suite de sommets ; un sommet porte le « bombé » (bulge du DXF) de l'arc
//! qui va jusqu'au sommet suivant : 0 pour un segment droit, tan(θ/4) pour un arc de cercle
//! d'angle θ (positif dans le sens trigonométrique). Les faces sphériques sont donc des arcs
//! exacts ; les coniques et asphériques sont suivies point par point.

use crate::core::glass::MediumKind;
use crate::core::raytracer::tracer_eventail_meridional;
use crate::core::types::{OpticalSystem, Repere, Surface, SurfaceType};

/// Sommet d'un contour : position (z, y) en mm et bombé de l'arc vers le sommet suivant
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sommet {
    pub z: f64,
    pub y: f64,
    pub bombe: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NatureElement {
    Lentille,
    Miroir,
}

/// Une pièce optique à fabriquer
#[derive(Debug, Clone)]
pub struct Element {
    pub nature: NatureElement,
    /// « L1 », « L2 »… pour les lentilles, « M1 »… pour les miroirs
    pub nom: String,
    /// Indices (à partir de 0) de la première et de la dernière surface ; égaux pour un miroir
    pub premiere: usize,
    pub derniere: usize,
}

/// Nombre de points par face non sphérique
const POINTS_PAR_FACE: usize = 64;

fn est_verre(sys: &OpticalSystem, s: &Surface) -> bool {
    matches!(sys.catalogue.resoudre(&s.verre).kind, MediumKind::Verre { .. })
}

/// Surface réelle (ni rupture de repère, ni lentille idéale)
fn est_physique(s: &Surface) -> bool {
    !matches!(s.type_surface, SurfaceType::CoupureRepere | SurfaceType::Ideale)
}

/// Face exactement sphérique (arc de cercle), plane exclue
fn est_spherique(s: &Surface) -> bool {
    if !s.rayon.is_finite() || s.rayon == 0.0 {
        return false;
    }
    match s.type_surface {
        SurfaceType::Spherique => true,
        SurfaceType::Conique => s.conique == 0.0,
        SurfaceType::Aspherique => s.conique == 0.0 && s.aspheriques.iter().all(|&a| a == 0.0),
        _ => false,
    }
}

fn est_plane(s: &Surface) -> bool {
    s.fleche(1.0) == Some(0.0) && s.fleche(s.semi_diametre.max(1e-3)) == Some(0.0)
}

/// Lentilles (surface suivie d'un verre, fermée par la surface suivante) et miroirs, dans
/// l'ordre du système
pub fn elements(sys: &OpticalSystem) -> Vec<Element> {
    let mut v = Vec::new();
    let (mut nl, mut nm) = (0, 0);
    let n = sys.surfaces.len();
    for (i, s) in sys.surfaces.iter().enumerate() {
        if !est_physique(s) || s.semi_diametre <= 0.0 {
            continue;
        }
        if sys.catalogue.resoudre(&s.verre).est_miroir() {
            nm += 1;
            v.push(Element { nature: NatureElement::Miroir, nom: format!("M{nm}"), premiere: i, derniere: i });
        } else if est_verre(sys, s) && i + 1 < n && est_physique(&sys.surfaces[i + 1]) {
            nl += 1;
            v.push(Element { nature: NatureElement::Lentille, nom: format!("L{nl}"), premiere: i, derniere: i + 1 });
        }
    }
    v
}

/// Face de `y_debut` à `y_fin` (coordonnées locales), dans le repère donné ; le dernier point
/// n'est pas ajouté (il commence l'élément suivant du contour)
fn face(s: &Surface, rep: &Repere, y_debut: f64, y_fin: f64, sortie: &mut Vec<Sommet>) -> Result<(), String> {
    let point = |y: f64| -> Result<(f64, f64), String> {
        let f = s.fleche(y.abs()).ok_or_else(|| {
            trf!(
                "la face de rayon {:.3} mm ne peut pas avoir un demi-diamètre de {:.3} mm",
                "the face of radius {:.3} mm cannot have a semi-diameter of {:.3} mm",
                "die Fläche mit Radius {:.3} mm kann keinen halben Durchmesser von {:.3} mm haben",
                s.rayon,
                y.abs()
            )
        })?;
        let g = rep.vers_global(0.0, y, f);
        Ok((g.z, g.y))
    };
    let (z0, y0) = point(y_debut)?;
    if est_plane(s) {
        sortie.push(Sommet { z: z0, y: y0, bombe: 0.0 });
    } else if est_spherique(s) {
        // Arc exact autour du centre de courbure : bombé = tan(θ/4), θ angle orienté de l'arc
        let (z1, y1) = point(y_fin)?;
        let c = rep.vers_global(0.0, 0.0, s.rayon);
        let (az, ay, bz, by) = (z0 - c.z, y0 - c.y, z1 - c.z, y1 - c.y);
        let theta = (az * by - ay * bz).atan2(az * bz + ay * by);
        sortie.push(Sommet { z: z0, y: y0, bombe: (0.25 * theta).tan() });
    } else {
        for k in 0..POINTS_PAR_FACE {
            let y = y_debut + (y_fin - y_debut) * k as f64 / POINTS_PAR_FACE as f64;
            let (z, yy) = point(y)?;
            sortie.push(Sommet { z, y: yy, bombe: 0.0 });
        }
    }
    Ok(())
}

fn point_global(s: &Surface, rep: &Repere, y_face: f64, y: f64) -> Result<Sommet, String> {
    let f = s.fleche(y_face.abs()).ok_or_else(|| tr!("face impossible", "impossible face", "unmögliche Fläche").to_string())?;
    let g = rep.vers_global(0.0, y, f);
    Ok(Sommet { z: g.z, y: g.y, bombe: 0.0 })
}

/// Contour fermé d'une lentille dans le repère global (le dernier sommet rejoint le premier).
/// Le diamètre extérieur est le plus grand des deux ; la face la plus petite se prolonge par
/// un méplat jusqu'à lui.
pub fn contour_lentille(sys: &OpticalSystem, e: &Element) -> Result<Vec<Sommet>, String> {
    let reperes = sys.reperes();
    contour_lentille_reperes(sys, e, &reperes[e.premiere], &reperes[e.derniere])
}

/// Contour d'une lentille avec des repères donnés (repères locaux pour un plan de fabrication)
pub fn contour_lentille_reperes(sys: &OpticalSystem, e: &Element, r1: &Repere, r2: &Repere) -> Result<Vec<Sommet>, String> {
    let (s1, s2) = (&sys.surfaces[e.premiere], &sys.surfaces[e.derniere]);
    let (d1, d2) = (s1.semi_diametre, s2.semi_diametre);
    let rm = d1.max(d2);
    let mut c = Vec::new();
    // Face d'entrée de bas en haut, méplat, bord, face de sortie de haut en bas, méplat
    face(s1, r1, -d1, d1, &mut c)?;
    c.push(point_global(s1, r1, d1, d1)?);
    if d1 < rm {
        c.push(point_global(s1, r1, d1, rm)?);
    }
    if d2 < rm {
        c.push(point_global(s2, r2, d2, rm)?);
    }
    face(s2, r2, d2, -d2, &mut c)?;
    c.push(point_global(s2, r2, d2, -d2)?);
    if d2 < rm {
        c.push(point_global(s2, r2, d2, -rm)?);
    }
    if d1 < rm {
        c.push(point_global(s1, r1, d1, -rm)?);
    }
    Ok(c)
}

/// Demi-coupe fermée d'une lentille (au-dessus de son axe), dans le repère global : la forme à
/// faire tourner de 360° autour de l'axe de la lentille (le côté qui relie les deux sommets)
/// pour obtenir son volume en CAO
pub fn demi_contour_lentille(sys: &OpticalSystem, e: &Element) -> Result<Vec<Sommet>, String> {
    let reperes = sys.reperes();
    let (r1, r2) = (&reperes[e.premiere], &reperes[e.derniere]);
    let (s1, s2) = (&sys.surfaces[e.premiere], &sys.surfaces[e.derniere]);
    let (d1, d2) = (s1.semi_diametre, s2.semi_diametre);
    let rm = d1.max(d2);
    let mut c = Vec::new();
    face(s1, r1, 0.0, d1, &mut c)?;
    c.push(point_global(s1, r1, d1, d1)?);
    if d1 < rm {
        c.push(point_global(s1, r1, d1, rm)?);
    }
    if d2 < rm {
        c.push(point_global(s2, r2, d2, rm)?);
    }
    face(s2, r2, d2, 0.0, &mut c)?;
    c.push(point_global(s2, r2, 0.0, 0.0)?);
    Ok(c)
}

/// Profil ouvert d'un miroir (de -SD à +SD) dans le repère global
pub fn profil_miroir(sys: &OpticalSystem, e: &Element) -> Result<Vec<Sommet>, String> {
    let rep = &sys.reperes()[e.premiere];
    profil_miroir_repere(sys, e, rep)
}

pub fn profil_miroir_repere(sys: &OpticalSystem, e: &Element, rep: &Repere) -> Result<Vec<Sommet>, String> {
    let s = &sys.surfaces[e.premiere];
    let d = s.semi_diametre;
    let mut c = Vec::new();
    face(s, rep, -d, d, &mut c)?;
    c.push(point_global(s, rep, d, d)?);
    Ok(c)
}

/// Points d'un contour (arcs découpés en petits segments), pour un dessin
pub fn points(sommets: &[Sommet], ferme: bool) -> Vec<(f64, f64)> {
    let mut p = Vec::new();
    let n = sommets.len();
    for k in 0..n {
        let a = sommets[k];
        p.push((a.z, a.y));
        if !ferme && k + 1 == n {
            break;
        }
        let b = sommets[(k + 1) % n];
        if a.bombe != 0.0 {
            p.extend(points_arc(a, b).into_iter().skip(1));
            p.pop();
        }
    }
    if ferme {
        if let Some(&premier) = p.first() {
            p.push(premier);
        }
    }
    p
}

/// Points de l'arc de bombé `a.bombe` de a à b (bouts compris)
fn points_arc(a: Sommet, b: Sommet) -> Vec<(f64, f64)> {
    let (uz, uy) = (b.z - a.z, b.y - a.y);
    let c = (uz * uz + uy * uy).sqrt();
    if c < 1e-12 {
        return vec![(a.z, a.y), (b.z, b.y)];
    }
    let theta = 4.0 * a.bombe.atan();
    // Centre : sur la médiatrice, à gauche de la corde pour un arc dans le sens trigonométrique
    let r = c / (2.0 * (0.5 * theta).sin());
    let (mz, my) = (0.5 * (a.z + b.z), 0.5 * (a.y + b.y));
    let (nz, ny) = (-uy / c, uz / c);
    let d = r * (0.5 * theta).cos();
    let (oz, oy) = (mz + nz * d, my + ny * d);
    let nb = ((theta.abs() / 0.004).ceil() as usize).clamp(4, 600);
    let (vz, vy) = (a.z - oz, a.y - oy);
    (0..=nb)
        .map(|k| {
            let t = theta * k as f64 / nb as f64;
            let (s, co) = t.sin_cos();
            (oz + vz * co - vy * s, oy + vz * s + vy * co)
        })
        .collect()
}

/// Rayons méridiens (z, y) de chaque champ, jusqu'au plan image ou à leur arrêt
pub fn rayons_meridiens(sys: &OpticalSystem, nb_par_champ: usize) -> Vec<(usize, Vec<(f64, f64)>)> {
    let mut champs: Vec<f64> = sys.champs_deg.iter().map(|c| c.1).collect();
    if champs.is_empty() {
        champs.push(0.0);
    }
    champs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let mut v = Vec::new();
    for (k, &hy) in champs.iter().enumerate().take(4) {
        for tr in tracer_eventail_meridional(sys, hy, nb_par_champ, sys.longueur_onde_ref_nm) {
            let mut pts = vec![(tr.rayon_initial.origine.z, tr.rayon_initial.origine.y)];
            pts.extend(tr.impacts.iter().map(|h| (h.point_global.z, h.point_global.y)));
            if pts.len() > 1 {
                v.push((k, pts));
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::presets;

    #[test]
    fn elements_du_doublet_et_du_cassegrain() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let e = elements(&sys);
        assert_eq!(e.iter().filter(|e| e.nature == NatureElement::Lentille).count(), 2);
        assert_eq!((e[0].premiere, e[0].derniere, e[1].premiere, e[1].derniere), (0, 1, 1, 2));
        let cass = presets::creer_telescope_cassegrain();
        let m: Vec<_> = elements(&cass).into_iter().filter(|e| e.nature == NatureElement::Miroir).collect();
        assert_eq!(m.len(), 2);
        assert_eq!(m[1].nom, "M2");
    }

    #[test]
    fn arc_exact_d_une_face_spherique() {
        // Chaque point de l'arc découpé est à la distance R du centre de courbure
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let e = &elements(&sys)[0];
        let s1 = &sys.surfaces[0];
        let c = contour_lentille(&sys, e).unwrap();
        assert!(c[0].bombe != 0.0, "la face sphérique doit être un arc");
        let arc = points_arc(c[0], c[1]);
        let centre_z = sys.reperes()[0].vers_global(0.0, 0.0, s1.rayon).z;
        for (z, y) in &arc {
            let d = ((z - centre_z).powi(2) + y * y).sqrt();
            assert!((d - s1.rayon.abs()).abs() < 1e-9, "écart {}", d - s1.rayon.abs());
        }
        // C'est le petit arc, qui passe par le sommet de la face (z = 0, au plus près à un pas
        // de découpage près)
        let z_min = arc.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        assert!(z_min.abs() < 2e-4, "{z_min}");
    }

    #[test]
    fn meplat_quand_les_diametres_different() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[0].semi_diametre = 10.0;
        sys.surfaces[1].semi_diametre = 12.0;
        let e = &elements(&sys)[0];
        let p = points(&contour_lentille(&sys, e).unwrap(), true);
        let y_max = p.iter().map(|q| q.1).fold(f64::MIN, f64::max);
        let y_min = p.iter().map(|q| q.1).fold(f64::MAX, f64::min);
        assert!((y_max - 12.0).abs() < 1e-9 && (y_min + 12.0).abs() < 1e-9);
        // Le méplat de la face d'entrée est à la flèche de son bord (10 mm)
        let f1 = sys.surfaces[0].fleche(10.0).unwrap();
        assert!(p.iter().any(|q| (q.0 - f1).abs() < 1e-9 && (q.1 - 12.0).abs() < 1e-9));
    }

    #[test]
    fn demi_coupe_fermee_sur_l_axe() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let e = &elements(&sys)[0];
        let c = demi_contour_lentille(&sys, e).unwrap();
        // Commence au sommet de la face 1, finit au sommet de la face 2 (sur l'axe)
        assert_eq!((c[0].z, c[0].y), (0.0, 0.0));
        let fin = c.last().unwrap();
        assert!((fin.z - sys.surfaces[0].epaisseur).abs() < 1e-12 && fin.y == 0.0);
        let p = points(&c, true);
        assert!(p.iter().all(|q| q.1 >= -1e-9), "rien sous l'axe");
        // Aire de la demi-coupe = intégrale de l'épaisseur locale de 0 à 15 mm
        let (s1, s2) = (&sys.surfaces[0], &sys.surfaces[1]);
        let n = 20_000;
        let attendue: f64 = (0..n)
            .map(|k| {
                let r = 15.0 * (k as f64 + 0.5) / n as f64;
                (s1.epaisseur + s2.fleche(r).unwrap() - s1.fleche(r).unwrap()) * 15.0 / n as f64
            })
            .sum();
        let aire = 0.5 * p.windows(2).map(|w| w[0].0 * w[1].1 - w[1].0 * w[0].1).sum::<f64>();
        assert!((aire.abs() - attendue).abs() < 5e-3, "aire {} attendue {}", aire.abs(), attendue);
    }

    #[test]
    fn face_impossible_signalee() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[0].semi_diametre = sys.surfaces[0].rayon.abs() + 5.0;
        assert!(contour_lentille(&sys, &elements(&sys)[0]).is_err());
    }
}
