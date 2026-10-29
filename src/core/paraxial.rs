use crate::core::raytracer::rayon_pupille_entree;
use crate::core::types::{OpticalSystem, serde_rayon_inf};
use serde::{Deserialize, Serialize};

/// Données optiques paraxiales du premier ordre (Approximation de Gauss)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParaxialData {
    /// Distance focale équivalente effective (EFFL) en mm
    #[serde(with = "serde_rayon_inf")]
    pub focale_effl_mm: f64,
    /// Tirage image paraxial (Back Focal Length - BFL) depuis la dernière surface optique en mm
    /// (compté le long de Z : négatif quand la lumière repart vers -Z après un nombre impair de miroirs)
    #[serde(with = "serde_rayon_inf")]
    pub tirage_bfl_mm: f64,
    /// Diamètre de la pupille d'entrée (ENPD) en mm
    pub diametre_pupille_entree_mm: f64,
    /// Position de la pupille d'entrée par rapport au sommet de la première surface (ENPP) en mm
    #[serde(default)]
    pub position_pupille_entree_mm: f64,
    /// Diamètre de la pupille de sortie (EXPD) en mm
    pub diametre_pupille_sortie_mm: f64,
    /// Position de la pupille de sortie par rapport à la dernière surface optique (EXPP) en mm
    #[serde(with = "serde_rayon_inf")]
    pub position_pupille_sortie_mm: f64,
    /// Nombre d'ouverture paraxial (F/# = EFFL / ENPD)
    pub nombre_ouverture_fnum: f64,
    /// Grandissement transversal pour un objet à distance finie (NaN pour un objet à l'infini)
    #[serde(default, with = "serde_rayon_inf")]
    pub grandissement: f64,
    /// Grossissement angulaire (pente du rayon principal en sortie / en entrée), utile en afocal
    #[serde(default, with = "serde_rayon_inf")]
    pub grossissement_angulaire: f64,
    /// Ouverture numérique dans l'espace image (NA)
    pub ouverture_numerique_image: f64,
    /// Puissance optique totale (dioptries = 1000 / EFFL_mm)
    pub puissance_dioptries: f64,
    /// Épaisseur totale du système (du sommet 0 au plan image) en mm
    pub longueur_totale_mm: f64,
}

/// Indice signé du milieu situé après la surface `idx` : le signe s'inverse à chaque miroir
/// (convention Zemax, cohérente avec les épaisseurs négatives après une réflexion).
pub(crate) fn indice_signe_apres(systeme: &OpticalSystem, idx: usize, lambda: f64) -> f64 {
    let mut signe = 1.0;
    let mut n = 1.0;
    for surf in systeme.surfaces[..=idx].iter().filter(|s| !s.est_coupure()) {
        let mat = systeme.catalogue.resoudre(&surf.verre);
        if mat.est_miroir() {
            signe = -signe;
        } else {
            n = mat.indice_refraction(lambda);
        }
    }
    signe * n
}

/// Propage un rayon paraxial `(y, u)` à partir du sommet de la surface `debut` (avant réfraction)
/// à travers les surfaces `debut..fin`. Retourne `(y, u, n)` juste après la réfraction sur la
/// surface `fin - 1`, `u` étant la pente dy/dz globale et `n` l'indice signé du milieu atteint.
pub(crate) fn tracer_paraxial(
    systeme: &OpticalSystem,
    lambda: f64,
    debut: usize,
    fin: usize,
    mut y: f64,
    mut u: f64,
) -> (f64, f64, f64) {
    let mut n = if debut == 0 {
        1.0
    } else {
        indice_signe_apres(systeme, debut - 1, lambda)
    };

    for idx in debut..fin {
        if idx > debut {
            y += systeme.surfaces[idx - 1].epaisseur * u;
        }
        let surf = &systeme.surfaces[idx];
        if surf.est_coupure() {
            continue;
        }
        if surf.type_surface == crate::core::types::SurfaceType::Ideale && surf.focale_ideale_mm != 0.0 {
            // Lentille idéale de puissance |n| / f dans le milieu courant
            u = (n * u - y * n.abs() / surf.focale_ideale_mm) / n;
            continue;
        }
        let c = surf.courbure();
        let mat = systeme.catalogue.resoudre(&surf.verre);
        let n_prime = if mat.est_miroir() {
            -n
        } else {
            n.signum() * mat.indice_refraction(lambda)
        };

        // Réfraction paraxiale : n' * u' = n * u - y * (n' - n) * c
        u = (n * u - y * (n_prime - n) * c) / n_prime;
        n = n_prime;
    }

    (y, u, n)
}

/// Pour chaque lentille idéale, position (mm, comptée dans le sens de la lumière depuis la
/// lentille, négative quand il est devant) du point objet paraxial axial qu'elle reçoit ;
/// `None` pour un faisceau parallèle ou une autre surface
pub fn objets_lentilles_ideales(systeme: &OpticalSystem) -> Vec<Option<f64>> {
    let lambda = systeme.longueur_onde_ref_nm;
    let nb = systeme.surfaces.len();
    let (mut y, mut u) = match systeme.objet_fini() {
        Some(d) => (d, 1.0),
        None => (1.0, 0.0),
    };
    let mut n = 1.0_f64;
    let mut objets = vec![None; nb];
    for idx in 0..nb.saturating_sub(1) {
        if idx > 0 {
            y += systeme.surfaces[idx - 1].epaisseur * u;
        }
        let surf = &systeme.surfaces[idx];
        if surf.est_coupure() {
            continue;
        }
        if surf.type_surface == crate::core::types::SurfaceType::Ideale && surf.focale_ideale_mm != 0.0 {
            if u.abs() > 1e-15 {
                objets[idx] = Some(-y / u * n.signum());
            }
            u = (n * u - y * n.abs() / surf.focale_ideale_mm) / n;
            continue;
        }
        let mat = systeme.catalogue.resoudre(&surf.verre);
        let n2 = if mat.est_miroir() { -n } else { n.signum() * mat.indice_refraction(lambda) };
        u = (n * u - y * (n2 - n) * surf.courbure()) / n2;
        n = n2;
    }
    objets
}

/// Index de la surface portant le diaphragme d'ouverture (première surface STOP, 0 par défaut).
/// Le plan image ne peut pas être diaphragme.
pub fn index_diaphragme(systeme: &OpticalSystem) -> usize {
    let nb_optiques = systeme.surfaces.len().saturating_sub(1);
    systeme
        .surfaces
        .iter()
        .take(nb_optiques)
        .position(|s| s.est_diaphragme)
        .unwrap_or(0)
}

/// Focale effective paraxiale (EFFL, en mm) à la longueur d'onde de référence
pub fn focale_paraxiale(systeme: &OpticalSystem) -> f64 {
    let nb_optiques = systeme.surfaces.len().saturating_sub(1);
    let (_, u, n) = tracer_paraxial(systeme, systeme.longueur_onde_ref_nm, 0, nb_optiques, 1.0, 0.0);
    if u.abs() > 1e-12 {
        -1.0 / (n * u)
    } else {
        f64::INFINITY
    }
}

/// Hauteurs au diaphragme des deux rayons de base partant du sommet de la première surface :
/// `A` pour (y = 1, u = 0) et `B` pour (y = 0, u = 1). Un rayon objet (y0, u0) y arrive en y0·A + u0·B.
fn hauteurs_au_diaphragme(systeme: &OpticalSystem) -> (f64, f64) {
    let stop = index_diaphragme(systeme);
    if stop == 0 {
        return (1.0, 0.0);
    }
    let lambda = systeme.longueur_onde_ref_nm;
    let ep = systeme.surfaces[stop - 1].epaisseur;
    let (ya, ua, _) = tracer_paraxial(systeme, lambda, 0, stop, 1.0, 0.0);
    let (yb, ub, _) = tracer_paraxial(systeme, lambda, 0, stop, 0.0, 1.0);
    (ya + ep * ua, yb + ep * ub)
}

/// Position axiale (en mm, depuis le sommet de la première surface) de la pupille d'entrée,
/// image paraxiale du diaphragme par les surfaces qui le précèdent.
pub fn position_pupille_entree(systeme: &OpticalSystem) -> f64 {
    let (a, b) = hauteurs_au_diaphragme(systeme);
    if a.abs() > 1e-12 { b / a } else { 0.0 }
}

/// Calcule les paramètres paraxiaux du système optique à la longueur d'onde de référence
/// par suivi d'un rayon marginal paraxial (hauteur initiale y0 = 1.0, pente u0 = 0.0)
/// et d'un rayon principal (chef) issu du centre du diaphragme.
pub fn calculer_paraxial(systeme: &OpticalSystem) -> ParaxialData {
    let lambda = systeme.longueur_onde_ref_nm;
    let nb = systeme.surfaces.len();
    let nb_optiques = nb.saturating_sub(1); // Exclut le plan image final pour EFFL / BFL

    let (y, u, n) = tracer_paraxial(systeme, lambda, 0, nb_optiques, 1.0, 0.0);
    let focale_effl_mm = if u.abs() > 1e-12 { -1.0 / (n * u) } else { f64::INFINITY };
    let mut tirage_bfl_mm = if u.abs() > 1e-12 { -y / u } else { f64::INFINITY };
    // Objet à distance finie : rayon issu du point objet axial (pente 1 dans l'espace objet)
    let mut grandissement = f64::NAN;
    if let Some(d) = systeme.objet_fini() {
        let (yo, uo, no) = tracer_paraxial(systeme, lambda, 0, nb_optiques, d, 1.0);
        tirage_bfl_mm = if uo.abs() > 1e-12 { -yo / uo } else { f64::INFINITY };
        grandissement = if uo.abs() > 1e-12 { 1.0 / (no * uo) } else { f64::INFINITY };
    }
    // Rayon principal passant par le centre de la pupille d'entrée avec une pente 1
    let z_ep = position_pupille_entree(systeme);
    let (_, uc_sortie, _) = tracer_paraxial(systeme, lambda, 0, nb_optiques, -z_ep, 1.0);
    let grossissement_angulaire = uc_sortie;

    let enpd = 2.0 * rayon_pupille_entree(systeme);
    let enpp = position_pupille_entree(systeme);

    // Pupille de sortie : image du diaphragme par les surfaces qui le suivent
    let stop = index_diaphragme(systeme);
    let (a, _) = hauteurs_au_diaphragme(systeme);
    let rayon_stop = 0.5 * enpd * a.abs();
    let n_stop = if stop == 0 {
        1.0
    } else {
        indice_signe_apres(systeme, stop - 1, lambda)
    };
    let (yc, uc, nc) = if nb_optiques > stop {
        tracer_paraxial(systeme, lambda, stop, nb_optiques, 0.0, 1.0)
    } else {
        (0.0, 1.0, n_stop)
    };
    let (expd, expp) = if uc.abs() > 1e-12 {
        (2.0 * rayon_stop * (n_stop / (nc * uc)).abs(), -yc / uc)
    } else {
        (f64::INFINITY, f64::INFINITY) // Espace image télécentrique
    };

    let fnum = if enpd > 1e-9 && focale_effl_mm.is_finite() {
        (focale_effl_mm / enpd).abs()
    } else {
        0.0
    };

    let na_image = if fnum > 1e-9 {
        1.0 / (2.0 * fnum)
    } else {
        0.0
    };

    let puissance_dioptries = if focale_effl_mm.is_finite() && focale_effl_mm.abs() > 1e-9 {
        1000.0 / focale_effl_mm
    } else {
        0.0
    };

    let longueur_totale_mm: f64 = systeme
        .surfaces
        .iter()
        .take(nb.saturating_sub(1))
        .map(|s| s.epaisseur)
        .sum();

    ParaxialData {
        focale_effl_mm,
        tirage_bfl_mm,
        diametre_pupille_entree_mm: enpd,
        position_pupille_entree_mm: enpp,
        diametre_pupille_sortie_mm: expd,
        position_pupille_sortie_mm: expp,
        nombre_ouverture_fnum: fnum,
        grandissement,
        grossissement_angulaire,
        ouverture_numerique_image: na_image,
        puissance_dioptries,
        longueur_totale_mm,
    }
}

#[cfg(test)]
mod tests_conjugues {
    use super::*;
    use crate::core::aberrations::calculer_courbure_distorsion;
    use crate::core::analysis::calculer_spot_diagram;
    use crate::core::onde::{calculer_front_onde, calculer_psf_ftm};
    use crate::core::types::{ApertureSpec, Surface, SurfaceType, Wavelength};

    fn ideale(f: f64, e: f64) -> Surface {
        let mut s = Surface::nouvelle(SurfaceType::Ideale, f64::INFINITY, e, "AIR", 40.0);
        s.focale_ideale_mm = f;
        s
    }

    fn systeme(surfaces: Vec<Surface>, epd: f64, champs: Vec<(f64, f64)>) -> OpticalSystem {
        OpticalSystem {
            nom: "essai".into(),
            longueur_onde_ref_nm: 587.5618,
            longueurs_onde: vec![Wavelength::new(587.5618, 1.0, "d")],
            ouverture: ApertureSpec::PupilleEntreeMm(epd),
            champs_deg: champs,
            surfaces,
            catalogue: Default::default(),
            distance_objet_mm: None,
            afocal: false,
        }
    }

    #[test]
    fn lentille_ideale_sans_aberration() {
        let mut l = ideale(100.0, 100.0);
        l.est_diaphragme = true;
        let sys = systeme(vec![l, Surface::plan_image(20.0)], 20.0, vec![(0.0, 0.0), (0.0, 5.0)]);
        assert!((calculer_paraxial(&sys).focale_effl_mm - 100.0).abs() < 1e-9);
        for champ in [(0.0, 0.0), (0.0, 5.0)] {
            let spot = calculer_spot_diagram(&sys, champ, 6);
            assert!(spot.rayon_rms_um < 1e-6, "spot {} à {champ:?}", spot.rayon_rms_um);
            let carte = calculer_front_onde(&sys, champ, 587.5618, 32).unwrap();
            assert!(carte.rms_ondes < 1e-6, "OPD {} à {champ:?}", carte.rms_ondes);
        }
        let c = calculer_courbure_distorsion(&sys, 587.5618, 5).unwrap();
        assert!((c.hauteur_mm[4].unwrap() - 100.0 * 5f64.to_radians().tan()).abs() < 1e-9);
        assert!(c.distorsion_pct.iter().flatten().all(|d| d.abs() < 1e-9));
    }

    #[test]
    fn objet_a_distance_finie() {
        // Objet à 2f : image à 2f, grandissement -1, ouverture numérique objet 0,05
        let mut l = ideale(100.0, 200.0);
        l.est_diaphragme = true;
        let mut sys = systeme(vec![l, Surface::plan_image(20.0)], 20.0, vec![(0.0, 0.0), (0.0, 5.0)]);
        sys.distance_objet_mm = Some(200.0);
        sys.ouverture = ApertureSpec::OuvertureNumeriqueObjet(0.05);
        let p = calculer_paraxial(&sys);
        assert!((p.tirage_bfl_mm - 200.0).abs() < 1e-9, "image à {}", p.tirage_bfl_mm);
        assert!((p.grandissement + 1.0).abs() < 1e-9, "m = {}", p.grandissement);
        assert!((p.diametre_pupille_entree_mm - 2.0 * 200.0 * 0.05f64.asin().tan()).abs() < 1e-9);
        let spot = calculer_spot_diagram(&sys, (0.0, 5.0), 6);
        assert!(spot.rayon_rms_um < 1e-6);
        assert!((spot.centroide_mm.1 + 5.0).abs() < 1e-9, "image de 5 mm en {}", spot.centroide_mm.1);
        let carte = calculer_front_onde(&sys, (0.0, 5.0), 587.5618, 32).unwrap();
        assert!(carte.rms_ondes < 1e-6, "OPD {}", carte.rms_ondes);

        // Doublet réel à distance finie : tirage plus long qu'à l'infini, image réelle
        let mut d = OpticalSystem::doublet_achromatique_defaut();
        let bfl_infini = calculer_paraxial(&d).tirage_bfl_mm;
        d.distance_objet_mm = Some(1000.0);
        let p = calculer_paraxial(&d);
        let attendu = 1.0 / (1.0 / p.focale_effl_mm - 1.0 / (1000.0 + 0.0)) - (p.focale_effl_mm - bfl_infini);
        assert!(p.tirage_bfl_mm > bfl_infini && (p.tirage_bfl_mm - attendu).abs() < 2.0, "{} / {attendu}", p.tirage_bfl_mm);
    }

    #[test]
    fn lunette_afocale_de_kepler() {
        // Objectif f = 100, oculaire f = 20 : grossissement -5, pupille de sortie 4 mm
        let mut obj = ideale(100.0, 120.0);
        obj.est_diaphragme = true;
        let mut sys = systeme(vec![obj, ideale(20.0, 24.0), Surface::plan_image(10.0)], 20.0, vec![(0.0, 0.0), (0.0, 1.0)]);
        sys.afocal = true;
        let p = calculer_paraxial(&sys);
        assert!(!p.focale_effl_mm.is_finite());
        assert!((p.grossissement_angulaire + 5.0).abs() < 1e-9, "G = {}", p.grossissement_angulaire);
        assert!((p.diametre_pupille_sortie_mm - 4.0).abs() < 1e-9);
        for champ in [(0.0, 0.0), (0.0, 1.0)] {
            let spot = calculer_spot_diagram(&sys, champ, 6);
            assert!(spot.rayon_rms_um < 1e-3, "spot angulaire {} µrad", spot.rayon_rms_um);
            let carte = calculer_front_onde(&sys, champ, 587.5618, 32).unwrap();
            assert!(carte.rms_ondes < 1e-6, "OPD {}", carte.rms_ondes);
        }
        let (psf, ftm) = calculer_psf_ftm(&sys, (0.0, 0.0), None).unwrap();
        assert!(psf.strehl > 0.999);
        // Coupure angulaire D / λ = 4 / 587,56e-6 cycles/rad = 6,808 cycles/mrad
        assert!((ftm.frequence_coupure_cyc_mm - 6.808).abs() < 0.01, "{}", ftm.frequence_coupure_cyc_mm);
        // Sortie à 1° : tan θ' = -5 tan 1° (exprimée en mrad sur la lentille idéale de 1000 mm)
        let c = calculer_courbure_distorsion(&sys, 587.5618, 3).unwrap();
        assert!((c.hauteur_mm[2].unwrap() + 5000.0 * 1f64.to_radians().tan()).abs() < 1e-6);
    }
}
