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
    /// Ouverture numérique dans l'espace image (NA)
    pub ouverture_numerique_image: f64,
    /// Puissance optique totale (dioptries = 1000 / EFFL_mm)
    pub puissance_dioptries: f64,
    /// Épaisseur totale du système (du sommet 0 au plan image) en mm
    pub longueur_totale_mm: f64,
}

/// Indice signé du milieu situé après la surface `idx` : le signe s'inverse à chaque miroir
/// (convention Zemax, cohérente avec les épaisseurs négatives après une réflexion).
fn indice_signe_apres(systeme: &OpticalSystem, idx: usize, lambda: f64) -> f64 {
    let mut signe = 1.0;
    let mut n = 1.0;
    for surf in &systeme.surfaces[..=idx] {
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
fn tracer_paraxial(
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
    let tirage_bfl_mm = if u.abs() > 1e-12 { -y / u } else { f64::INFINITY };

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
        ouverture_numerique_image: na_image,
        puissance_dioptries,
        longueur_totale_mm,
    }
}
