//! Calculs automatiques (solves) et reprises (pickups) des paramètres de surface, à la manière
//! de Zemax : mise au point par le rayon marginal, surface dans une pupille par le rayon
//! principal, nombre d'ouverture imposé par la dernière courbure, surface aplanétique, valeur
//! recopiée d'une autre surface. Ils sont appliqués surface par surface, dans l'ordre (le rayon
//! avant l'épaisseur), avant chaque analyse et à chaque pas de l'optimiseur.

use crate::core::aberrations::champ_maximal_deg;
use crate::core::paraxial::{indice_signe_apres, position_pupille_entree, tracer_paraxial};
use crate::core::raytracer::{VisePupille, rayon_pupille_entree, tracer_rayon_unique};
use crate::core::types::{OpticalSystem, Solve, SurfaceType};

/// Paramètre de surface concerné par un solve
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParametreSolve {
    Rayon,
    Epaisseur,
    Conique,
}

impl ParametreSolve {
    fn nom(self) -> &'static str {
        match self {
            ParametreSolve::Rayon => tr!("rayon", "radius", "Radius"),
            ParametreSolve::Epaisseur => tr!("épaisseur", "thickness", "Dicke"),
            ParametreSolve::Conique => tr!("conique", "conic", "Konik"),
        }
    }
}

/// Vrai si le solve peut s'appliquer à ce paramètre
pub fn solve_admis(solve: &Solve, parametre: ParametreSolve) -> bool {
    match solve {
        Solve::Reprise { .. } => true,
        Solve::HauteurMarginale { .. } | Solve::HauteurPrincipale { .. } => parametre == ParametreSolve::Epaisseur,
        Solve::AngleMarginal { .. } | Solve::AnglePrincipal { .. } | Solve::NombreOuverture { .. } | Solve::Aplanetique => {
            parametre == ParametreSolve::Rayon
        }
    }
}

/// Rayons paraxiaux de base `(y, u)` au sommet de la première surface : marginal (bord de la
/// pupille d'entrée, point objet axial) et principal (centre de la pupille, bord du champ)
fn rayons_de_base(sys: &OpticalSystem) -> ((f64, f64), (f64, f64)) {
    let r = rayon_pupille_entree(sys);
    let z_ep = position_pupille_entree(sys);
    let champ = champ_maximal_deg(sys);
    match sys.objet_fini() {
        Some(d) => {
            let u = r / (z_ep + d);
            let h = if champ > 0.0 { champ } else { 1.0 };
            let ub = -h / (z_ep + d);
            ((u * d, u), (h + ub * d, ub))
        }
        None => {
            let ub = if champ > 0.0 { champ.to_radians().tan() } else { 0.01 };
            ((r, 0.0), (-ub * z_ep, ub))
        }
    }
}

/// Rayon paraxial `(y, u, n)` arrivant sur la surface `i`, avant réfraction
fn avant_surface(sys: &OpticalSystem, i: usize, (y0, u0): (f64, f64)) -> (f64, f64, f64) {
    let lambda = sys.longueur_onde_ref_nm;
    let (mut y, u, n) = tracer_paraxial(sys, lambda, 0, i, y0, u0);
    if i > 0 {
        y += sys.surfaces[i - 1].epaisseur * u;
    }
    (y, u, n)
}

/// Indice signé du milieu après la surface `i`
fn indice_apres(sys: &OpticalSystem, i: usize) -> f64 {
    indice_signe_apres(sys, i, sys.longueur_onde_ref_nm)
}

fn valeur(sys: &OpticalSystem, i: usize, parametre: ParametreSolve) -> f64 {
    let s = &sys.surfaces[i];
    match parametre {
        ParametreSolve::Rayon => s.rayon,
        ParametreSolve::Epaisseur => s.epaisseur,
        ParametreSolve::Conique => s.conique,
    }
}

/// Valeur imposée par un solve sur la surface `i`, ou message d'erreur
fn resoudre(sys: &OpticalSystem, i: usize, parametre: ParametreSolve, solve: &Solve) -> Result<f64, String> {
    let (marginal, principal) = rayons_de_base(sys);
    let impossible = || {
        trf!(
            "solve impossible ({}) : rayon paraxial nul ou parallèle",
            "solve impossible ({}): paraxial ray at zero height or parallel",
            "Solve unmöglich ({}): paraxialer Strahl mit Höhe null oder parallel",
            solve.libelle()
        )
    };
    match *solve {
        Solve::Reprise { surface, facteur, decalage } => {
            let j = surface.checked_sub(1).filter(|&j| j < sys.surfaces.len() && j != i).ok_or_else(|| {
                trf!(
                    "reprise : surface #{} inexistante",
                    "pickup: surface #{} does not exist",
                    "Übernahme: Fläche #{} existiert nicht",
                    surface
                )
            })?;
            let v = valeur(sys, j, parametre);
            Ok(if v.is_finite() { facteur * v + decalage } else { v })
        }
        Solve::HauteurMarginale { hauteur_mm, zone } if zone != 0.0 => {
            // Rayon réel du point objet axial passant à la fraction `zone` de la pupille
            let lambda = sys.longueur_onde_ref_nm;
            let vise = VisePupille::calculer(sys);
            let tr = tracer_rayon_unique(sys, vise.rayon(0.0, zone, 0.0, 0.0, lambda), (0.0, 0.0), (0.0, zone));
            let hit = tr.impacts.get(i).filter(|h| !h.vignette).ok_or_else(|| {
                trf!(
                    "solve de mise au point : le rayon de zone {} est arrêté avant la surface #{}",
                    "focus solve: the zone {} ray is stopped before surface #{}",
                    "Fokus-Solve: der Strahl der Zone {} wird vor Fläche #{} gestoppt",
                    zone,
                    i + 1
                )
            })?;
            let rep = sys.reperes()[i];
            let p = rep.vers_local(&hit.point_global);
            let d = rep.rotation.inverse() * hit.direction_sortante;
            if d.y.abs() < 1e-14 {
                return Err(impossible());
            }
            Ok(p.z + (hauteur_mm - p.y) * d.z / d.y)
        }
        Solve::HauteurMarginale { hauteur_mm, .. } | Solve::HauteurPrincipale { hauteur_mm } => {
            let depart = if matches!(solve, Solve::HauteurPrincipale { .. }) { principal } else { marginal };
            let (y, u, _) = tracer_paraxial(sys, sys.longueur_onde_ref_nm, 0, i + 1, depart.0, depart.1);
            if u.abs() < 1e-15 {
                return Err(impossible());
            }
            Ok((hauteur_mm - y) / u)
        }
        Solve::AngleMarginal { .. } | Solve::AnglePrincipal { .. } | Solve::NombreOuverture { .. } | Solve::Aplanetique => {
            let depart = if matches!(solve, Solve::AnglePrincipal { .. }) { principal } else { marginal };
            let (y, u, n) = avant_surface(sys, i, depart);
            let n2 = indice_apres(sys, i);
            let u2 = match *solve {
                Solve::AngleMarginal { angle } | Solve::AnglePrincipal { angle } => angle,
                Solve::NombreOuverture { f_nombre } if f_nombre != 0.0 => -n2.signum() / (2.0 * f_nombre),
                Solve::NombreOuverture { .. } => return Err(impossible()),
                // Aplanétisme : u'/n' = u/n (ni aberration sphérique ni coma)
                _ => u * n2 / n,
            };
            // n' u' = n u − y (n' − n) c
            if y.abs() < 1e-12 || (n2 - n).abs() < 1e-12 {
                return Err(impossible());
            }
            let c = (n * u - n2 * u2) / (y * (n2 - n));
            Ok(if c.abs() < 1e-12 { f64::INFINITY } else { 1.0 / c })
        }
    }
}

/// Applique tous les solves du système, surface par surface ; retourne les avertissements
/// (solve impossible ou mal placé), la valeur précédente étant alors conservée
pub fn appliquer_solves(sys: &mut OpticalSystem) -> Vec<String> {
    let mut avertissements = Vec::new();
    if !sys.surfaces.iter().any(|s| s.solve_rayon.is_some() || s.solve_epaisseur.is_some() || s.solve_conique.is_some()) {
        return avertissements;
    }
    for i in 0..sys.surfaces.len() {
        for parametre in [ParametreSolve::Rayon, ParametreSolve::Conique, ParametreSolve::Epaisseur] {
            let s = &sys.surfaces[i];
            let solve = match parametre {
                ParametreSolve::Rayon => &s.solve_rayon,
                ParametreSolve::Epaisseur => &s.solve_epaisseur,
                ParametreSolve::Conique => &s.solve_conique,
            };
            let Some(solve) = solve.clone() else { continue };
            let sans_courbure = matches!(s.type_surface, SurfaceType::Ideale | SurfaceType::CoupureRepere);
            if !solve_admis(&solve, parametre) || (parametre != ParametreSolve::Epaisseur && sans_courbure) {
                avertissements.push(trf!(
                    "surface #{} : solve {} sans effet sur le paramètre {}",
                    "surface #{}: solve {} has no effect on the {}",
                    "Fläche #{}: Solve {} ohne Wirkung auf {}",
                    i + 1,
                    solve.libelle(),
                    parametre.nom()
                ));
                continue;
            }
            match resoudre(sys, i, parametre, &solve) {
                Ok(v) if v.is_nan() => {}
                Ok(v) => {
                    let s = &mut sys.surfaces[i];
                    match parametre {
                        ParametreSolve::Rayon => {
                            s.rayon = if v.is_finite() && v.abs() > 1e-9 { v } else { f64::INFINITY };
                            if s.rayon.is_finite() && s.type_surface == SurfaceType::Plane {
                                s.type_surface = SurfaceType::Spherique;
                            }
                        }
                        ParametreSolve::Epaisseur if v.is_finite() => s.epaisseur = v,
                        ParametreSolve::Epaisseur => {}
                        ParametreSolve::Conique => {
                            s.conique = v;
                            if v != 0.0 && s.type_surface == SurfaceType::Spherique {
                                s.type_surface = SurfaceType::Conique;
                            }
                        }
                    }
                }
                Err(e) => avertissements.push(trf!("surface #{} : {}", "surface #{}: {}", "Fläche #{}: {}", i + 1, e)),
            }
        }
    }
    avertissements
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::aberrations::calculer_seidel;
    use crate::core::analysis::calculer_spot_diagram;
    use crate::core::paraxial::calculer_paraxial;
    use crate::core::types::Surface;

    #[test]
    fn mise_au_point_paraxiale_par_le_rayon_marginal() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        let attendu = calculer_paraxial(&sys).tirage_bfl_mm;
        sys.surfaces[2].epaisseur = 50.0;
        sys.surfaces[2].solve_epaisseur = Some(Solve::HauteurMarginale { hauteur_mm: 0.0, zone: 0.0 });
        assert!(appliquer_solves(&mut sys).is_empty());
        assert!((sys.surfaces[2].epaisseur - attendu).abs() < 1e-9, "{} / {attendu}", sys.surfaces[2].epaisseur);
        // La mise au point suit un changement de courbure
        sys.surfaces[0].rayon = 50.0;
        appliquer_solves(&mut sys);
        assert!(calculer_paraxial(&sys).tirage_bfl_mm - sys.surfaces[2].epaisseur < 1e-9);
    }

    #[test]
    fn mise_au_point_sur_la_zone_0707() {
        // Miroir sphérique F/4 : le rayon de zone 0,7071 coupe l'axe entre le foyer paraxial
        // et le foyer marginal, et la tache y est plus petite qu'au foyer paraxial
        let mut m = Surface::nouvelle(SurfaceType::Spherique, -800.0, -400.0, "MIROIR", 60.0);
        m.est_diaphragme = true;
        let mut sys = OpticalSystem {
            surfaces: vec![m, Surface::plan_image(10.0)],
            ouverture: crate::core::types::ApertureSpec::PupilleEntreeMm(100.0),
            ..OpticalSystem::nouveau("miroir", 587.5618)
        };
        let paraxial = calculer_spot_diagram(&sys, (0.0, 0.0), 8).rayon_rms_um;
        sys.surfaces[0].solve_epaisseur = Some(Solve::HauteurMarginale { hauteur_mm: 0.0, zone: 0.7071 });
        assert!(appliquer_solves(&mut sys).is_empty());
        let e = sys.surfaces[0].epaisseur;
        // Aberration sphérique longitudinale d'un miroir sphérique : ≈ h² / (4R) vers le miroir
        let h = 0.7071 * 50.0;
        assert!((e - (-400.0 + h * h / (4.0 * 800.0))).abs() < 0.01, "e = {e}");
        assert!(calculer_spot_diagram(&sys, (0.0, 0.0), 8).rayon_rms_um < 0.6 * paraxial);
    }

    #[test]
    fn nombre_d_ouverture_et_reprise() {
        // Singlet : le solve F/# sur la seconde face donne la focale EPD × N
        let mut sys = OpticalSystem::nouveau("singlet", 587.5618);
        let mut s1 = Surface::nouvelle(SurfaceType::Spherique, 100.0, 5.0, "BK7", 15.0);
        s1.est_diaphragme = true;
        let mut s2 = Surface::nouvelle(SurfaceType::Spherique, -100.0, 90.0, "AIR", 15.0);
        s2.solve_rayon = Some(Solve::NombreOuverture { f_nombre: 8.0 });
        s2.solve_epaisseur = Some(Solve::HauteurMarginale { hauteur_mm: 0.0, zone: 0.0 });
        sys.surfaces = vec![s1, s2, Surface::plan_image(10.0)];
        sys.ouverture = crate::core::types::ApertureSpec::PupilleEntreeMm(20.0);
        assert!(appliquer_solves(&mut sys).is_empty());
        let p = calculer_paraxial(&sys);
        assert!((p.focale_effl_mm - 160.0).abs() < 1e-9, "f = {}", p.focale_effl_mm);
        assert!((sys.surfaces[1].epaisseur - p.tirage_bfl_mm).abs() < 1e-9);
        // Lentille symétrique : R2 = −R1 par reprise
        sys.surfaces[1].solve_rayon = Some(Solve::Reprise { surface: 1, facteur: -1.0, decalage: 0.0 });
        sys.surfaces[0].rayon = 77.0;
        appliquer_solves(&mut sys);
        assert_eq!(sys.surfaces[1].rayon, -77.0);
        // Reprise impossible : signalée, valeur conservée
        sys.surfaces[1].solve_rayon = Some(Solve::Reprise { surface: 9, facteur: 1.0, decalage: 0.0 });
        assert_eq!(appliquer_solves(&mut sys).len(), 1);
        assert_eq!(sys.surfaces[1].rayon, -77.0);
        // Solve de rayon sur une épaisseur : refusé
        sys.surfaces[1].solve_rayon = None;
        sys.surfaces[0].solve_epaisseur = Some(Solve::Aplanetique);
        assert_eq!(appliquer_solves(&mut sys).len(), 1);
    }

    #[test]
    fn surface_aplanetique() {
        // Ménisque de microscope : seconde face aplanétique pour le faisceau convergent de la
        // première ; la contribution de Seidel S_I et S_II de cette face est nulle
        let mut sys = OpticalSystem::nouveau("aplanétique", 587.5618);
        let mut s1 = Surface::nouvelle(SurfaceType::Spherique, 60.0, 4.0, "BK7", 12.0);
        s1.est_diaphragme = true;
        let mut s2 = Surface::nouvelle(SurfaceType::Spherique, 30.0, 50.0, "SF11", 12.0);
        s2.solve_rayon = Some(Solve::Aplanetique);
        let s3 = Surface::nouvelle(SurfaceType::Spherique, -200.0, 60.0, "AIR", 12.0);
        sys.surfaces = vec![s1, s2, s3, Surface::plan_image(10.0)];
        sys.ouverture = crate::core::types::ApertureSpec::PupilleEntreeMm(15.0);
        sys.champs_deg = vec![(0.0, 0.0), (0.0, 1.0)];
        assert!(appliquer_solves(&mut sys).is_empty());
        let s = calculer_seidel(&sys).unwrap();
        let l = &s.lignes[1];
        assert!(l.s1.abs() < 1e-12 && l.s2.abs() < 1e-12, "{l:?}");
        assert!(s.lignes[0].s1.abs() > 1e-6);
    }

    #[test]
    fn surface_dans_la_pupille_par_le_rayon_principal() {
        // Objectif idéal de 100 mm, diaphragme 20 mm devant : le solve place le plan suivant à
        // la pupille de sortie (le rayon principal y recoupe l'axe)
        let mut stop = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 20.0, "AIR", 10.0);
        stop.est_diaphragme = true;
        let mut l = Surface::nouvelle(SurfaceType::Ideale, f64::INFINITY, 10.0, "AIR", 20.0);
        l.focale_ideale_mm = 100.0;
        l.solve_epaisseur = Some(Solve::HauteurPrincipale { hauteur_mm: 0.0 });
        let mut sys = OpticalSystem::nouveau("pupille", 587.5618);
        sys.surfaces = vec![stop, l, Surface::plan_image(10.0)];
        sys.champs_deg = vec![(0.0, 0.0), (0.0, 2.0)];
        assert!(appliquer_solves(&mut sys).is_empty());
        // Image du diaphragme : 1/s' = 1/f − 1/20 → s' = −25 mm (virtuelle)
        assert!((sys.surfaces[1].epaisseur + 25.0).abs() < 1e-9, "{}", sys.surfaces[1].epaisseur);
        assert!((calculer_paraxial(&sys).position_pupille_sortie_mm + 25.0).abs() < 1e-9);
    }
}
