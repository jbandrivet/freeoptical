//! Analyses ondulatoires : différence de marche (OPD), carte de front d'onde, polynômes de Zernike,
//! rapport de Strehl, PSF et fonction de transfert de modulation (FTM / MTF).
//!
//! **Référence.** La différence de marche est mesurée sur la sphère de référence centrée sur
//! l'image du rayon principal à la longueur d'onde de référence et passant par la pupille de
//! sortie paraxiale (convention de Zemax). Pour chaque rayon, le chemin optique est prolongé ou
//! raccourci depuis le plan image jusqu'à cette sphère :
//! `OPD = chemin(rayon) − chemin(rayon principal)`, positive quand le rayon a un chemin optique
//! plus long. Un front d'onde de la forme `W = a·y` déplace l'image de `+a·R` en y.
//!
//! **Pupille.** Les rayons sont repérés par leurs coordonnées normalisées dans la pupille
//! d'entrée `(px, py)`, cercle unité. Les rayons vignettés sont exclus.
//!
//! **PSF.** PSF de Fraunhofer par FFT de la fonction pupille `exp(2iπW/λ)`. Le nombre
//! d'échantillons sur la pupille (64 à 256 à la plus courte longueur d'onde) est choisi d'après la
//! tache géométrique pour qu'elle tienne dans la grille sans repliement, et la grille FFT en compte
//! quatre fois plus (pas image λF/4). Le pas d'échantillonnage de la pupille est adapté à chaque
//! longueur d'onde pour que toutes les PSF partagent la même grille image : la PSF
//! polychromatique est leur somme pondérée et la FTM sa transformée de Fourier.

use crate::core::paraxial::calculer_paraxial;
use crate::core::raytracer::{TracedRay, VisePupille, tracer_rayon_unique};
use crate::core::types::{OpticalSystem, Point3, Vec3};
use nalgebra::{DMatrix, DVector};
use rustfft::{FftPlanner, num_complex::Complex};
use serde::{Deserialize, Serialize};

/// Nombre de polynômes de Zernike ajustés (indices de Noll 1 à 37)
pub const NB_ZERNIKE: usize = 37;
/// Échantillons sur le diamètre de la pupille à la plus courte longueur d'onde (bornes)
const ECHANTILLONS_PUPILLE_MIN: usize = 64;
const ECHANTILLONS_PUPILLE_MAX: usize = 256;

// ---------------------------------------------------------------------------------------------
// Polynômes de Zernike (normalisation et ordre de Noll)
// ---------------------------------------------------------------------------------------------

/// Ordre radial `n` et fréquence azimutale signée `m` du polynôme de Noll `j` (j ≥ 1) :
/// `m > 0` pour cos(mθ), `m < 0` pour sin(|m|θ)
pub fn noll_vers_nm(j: usize) -> (usize, i32) {
    let j = j.max(1);
    let n = ((-1.0 + ((8 * (j - 1) + 1) as f64).sqrt()) / 2.0).floor() as usize;
    let p = j - n * (n + 1) / 2;
    let k = n % 2;
    let mut m = (((p + k) / 2) * 2) as i32 - k as i32;
    if m != 0 && j % 2 == 1 {
        m = -m;
    }
    (n, m)
}

/// Valeur du polynôme de Zernike de Noll `j` (normalisé : moyenne de Z² sur le disque = 1)
pub fn zernike(j: usize, rho: f64, theta: f64) -> f64 {
    let (n, m) = noll_vers_nm(j);
    let ma = m.unsigned_abs() as usize;
    let mut radial = 0.0;
    for s in 0..=((n - ma) / 2) {
        let signe = if s % 2 == 0 { 1.0 } else { -1.0 };
        let num = factorielle(n - s);
        let den = factorielle(s) * factorielle((n + ma) / 2 - s) * factorielle((n - ma) / 2 - s);
        radial += signe * num / den * rho.powi((n - 2 * s) as i32);
    }
    if m == 0 {
        ((n + 1) as f64).sqrt() * radial
    } else {
        let angulaire = if m > 0 { (ma as f64 * theta).cos() } else { (ma as f64 * theta).sin() };
        (2.0 * (n + 1) as f64).sqrt() * radial * angulaire
    }
}

fn factorielle(n: usize) -> f64 {
    (1..=n).map(|k| k as f64).product()
}

/// Nom usuel du terme de Zernike de Noll `j`
pub fn nom_zernike(j: usize) -> String {
    let nom = match j {
        1 => tr!("Piston", "Piston", "Kolben"),
        2 => tr!("Basculement X", "Tilt X", "Kippung X"),
        3 => tr!("Basculement Y", "Tilt Y", "Kippung Y"),
        4 => tr!("Défocalisation", "Defocus", "Defokus"),
        5 => tr!("Astigmatisme 45°", "Astigmatism 45°", "Astigmatismus 45°"),
        6 => tr!("Astigmatisme 0°", "Astigmatism 0°", "Astigmatismus 0°"),
        7 => tr!("Coma Y", "Coma Y", "Koma Y"),
        8 => tr!("Coma X", "Coma X", "Koma X"),
        9 => tr!("Trèfle Y", "Trefoil Y", "Dreiblatt Y"),
        10 => tr!("Trèfle X", "Trefoil X", "Dreiblatt X"),
        11 => tr!("Sphérique 3e ordre", "Primary spherical", "Sphärische Aberration 3. Ordnung"),
        12 => tr!("Astigmatisme 2nd 0°", "Secondary astigmatism 0°", "Astigmatismus 2. Ord. 0°"),
        13 => tr!("Astigmatisme 2nd 45°", "Secondary astigmatism 45°", "Astigmatismus 2. Ord. 45°"),
        14 => tr!("Quadrifoil 0°", "Tetrafoil 0°", "Vierblatt 0°"),
        15 => tr!("Quadrifoil 45°", "Tetrafoil 45°", "Vierblatt 45°"),
        16 => tr!("Coma 2nd X", "Secondary coma X", "Koma 2. Ord. X"),
        17 => tr!("Coma 2nd Y", "Secondary coma Y", "Koma 2. Ord. Y"),
        22 => tr!("Sphérique 5e ordre", "Secondary spherical", "Sphärische Aberration 5. Ordnung"),
        37 => tr!("Sphérique 7e ordre", "Tertiary spherical", "Sphärische Aberration 7. Ordnung"),
        _ => "",
    };
    let (n, m) = noll_vers_nm(j);
    if nom.is_empty() {
        format!("n={n}, m={m}")
    } else {
        nom.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Différence de marche
// ---------------------------------------------------------------------------------------------

/// Sphère (ou plan, en espace image télécentrique) de référence d'un champ
#[derive(Debug, Clone, Copy)]
struct Reference {
    centre: Point3,
    /// Rayon de la sphère (mm) ; `None` pour un plan de référence
    rayon: Option<f64>,
    /// Direction du rayon principal dans l'espace image
    direction_chef: Vec3,
    /// Nombre d'ouverture de travail (rayon de la sphère / diamètre de la pupille de sortie)
    f_travail: f64,
}

fn reference(sys: &OpticalSystem, champ: (f64, f64)) -> Result<Reference, String> {
    let p = calculer_paraxial(sys);
    if !p.focale_effl_mm.is_finite() {
        return Err(tr!(
            "système afocal : pas d'image à analyser",
            "afocal system: no image to analyse",
            "afokales System: kein Bild zu analysieren"
        )
        .to_string());
    }
    let lambda = sys.longueur_onde_ref_nm;
    let vise = VisePupille::calculer(sys);
    let chef = tracer_rayon_unique(sys, vise.rayon(0.0, 0.0, champ.0, champ.1, lambda), champ, (0.0, 0.0));
    let impact = chef.impact_plan_image().ok_or_else(|| {
        tr!(
            "le rayon principal n'atteint pas le plan image",
            "the chief ray does not reach the image plane",
            "der Hauptstrahl erreicht die Bildebene nicht"
        )
        .to_string()
    })?;
    let centre = impact.point_global;
    let d = impact.direction_sortante.normalize();

    let nb = sys.surfaces.len();
    let z = sys.positions_z_sommets();
    let z_pupille = if nb >= 2 { z[nb - 2] + p.position_pupille_sortie_mm } else { f64::NAN };
    let mut rayon = None;
    if z_pupille.is_finite() && d.z.abs() > 1e-9 {
        let s = (z_pupille - centre.z) / d.z;
        let r = (s * d).norm();
        if r > 1e-6 && r < 1e7 {
            rayon = Some(r);
        }
    }
    let f_travail = match rayon {
        Some(r) if p.diametre_pupille_sortie_mm.is_finite() && p.diametre_pupille_sortie_mm > 1e-9 => {
            r / p.diametre_pupille_sortie_mm
        }
        _ => p.nombre_ouverture_fnum,
    };
    if !(f_travail.is_finite() && f_travail > 0.0) {
        return Err(tr!(
            "nombre d'ouverture de travail indéfini (système afocal ?)",
            "undefined working F-number (afocal system?)",
            "Arbeitsblendenzahl unbestimmt (afokales System?)"
        )
        .to_string());
    }
    Ok(Reference { centre, rayon, direction_chef: d, f_travail })
}

/// Chemin optique du rayon jusqu'à la sphère de référence, compté depuis le front d'onde plan
/// incident passant par l'origine du rayon principal
fn chemin_jusqu_a_reference(tr: &TracedRay, origine_chef: &Point3, refe: &Reference) -> Option<f64> {
    let impact = tr.impact_plan_image()?;
    let q = impact.point_global;
    let d = impact.direction_sortante.normalize();
    let n = tr
        .impacts
        .len()
        .checked_sub(2)
        .map(|i| tr.impacts[i].indice_apres.abs())
        .unwrap_or(1.0);
    // Objet à l'infini : le front d'onde incident est un plan perpendiculaire à la direction
    let depart = (tr.rayon_initial.origine - origine_chef).dot(&tr.rayon_initial.direction);
    let w = q - refe.centre;
    let t = match refe.rayon {
        Some(r) => {
            let b = w.dot(&d);
            let disc = b * b - w.norm_squared() + r * r;
            if disc < 0.0 {
                return None;
            }
            -b - disc.sqrt()
        }
        None => {
            let cos = d.dot(&refe.direction_chef);
            if cos.abs() < 1e-12 {
                return None;
            }
            -w.dot(&refe.direction_chef) / cos
        }
    };
    Some(tr.chemin_optique_total + depart + n * t)
}

/// Différence de marche (en ondes à `lambda_nm`) des rayons visant les points `(px, py)` de la
/// pupille d'entrée ; `None` pour un rayon vignetté
fn opd_points(
    sys: &OpticalSystem,
    champ: (f64, f64),
    lambda_nm: f64,
    points: &[(f64, f64)],
    refe: &Reference,
) -> Result<Vec<Option<f64>>, String> {
    let vise = VisePupille::calculer(sys);
    let chef = tracer_rayon_unique(sys, vise.rayon(0.0, 0.0, champ.0, champ.1, lambda_nm), champ, (0.0, 0.0));
    let origine_chef = chef.rayon_initial.origine;
    let s_chef = chemin_jusqu_a_reference(&chef, &origine_chef, refe).ok_or_else(|| {
        trf!(
            "le rayon principal est vignetté à {:.1} nm",
            "the chief ray is vignetted at {:.1} nm",
            "der Hauptstrahl ist bei {:.1} nm vignettiert",
            lambda_nm
        )
    })?;
    let lambda_mm = lambda_nm * 1e-6;
    Ok(points
        .iter()
        .map(|&(px, py)| {
            let tr = tracer_rayon_unique(sys, vise.rayon(px, py, champ.0, champ.1, lambda_nm), champ, (px, py));
            chemin_jusqu_a_reference(&tr, &origine_chef, refe).map(|s| (s - s_chef) / lambda_mm)
        })
        .collect())
}

/// Différence de marche d'un seul rayon (en ondes), pour les éventails d'OPD
pub fn opd_rayon(
    sys: &OpticalSystem,
    champ: (f64, f64),
    lambda_nm: f64,
    px: f64,
    py: f64,
) -> Result<Option<f64>, String> {
    let refe = reference(sys, champ)?;
    Ok(opd_points(sys, champ, lambda_nm, &[(px, py)], &refe)?[0])
}

// ---------------------------------------------------------------------------------------------
// Carte de front d'onde et Zernike
// ---------------------------------------------------------------------------------------------

/// Carte de front d'onde sur la pupille et sa décomposition en polynômes de Zernike
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarteFrontOnde {
    /// Côté de la grille (points par diamètre de pupille)
    pub taille: usize,
    /// Différence de marche en ondes, ligne par ligne de py = -1 à +1 (`None` hors pupille)
    pub valeurs_ondes: Vec<Option<f64>>,
    pub longueur_onde_nm: f64,
    pub champ_deg: (f64, f64),
    /// Écart crête-à-creux (ondes)
    pub pv_ondes: f64,
    /// Écart quadratique moyen, piston retiré (ondes)
    pub rms_ondes: f64,
    /// Rapport de Strehl estimé par l'approximation de Maréchal exp(-(2πσ)²)
    pub strehl_marechal: f64,
    /// Coefficients de Zernike (normalisation de Noll, ondes), indices 1 à 37
    pub zernike_ondes: Vec<f64>,
    /// Résidu quadratique moyen de l'ajustement de Zernike (ondes)
    pub residu_zernike_ondes: f64,
    /// Nombre de rayons transmis
    pub nb_points: usize,
}

/// Calcule la carte de front d'onde d'un champ à une longueur d'onde, sur une grille `taille`²
pub fn calculer_front_onde(
    sys: &OpticalSystem,
    champ: (f64, f64),
    lambda_nm: f64,
    taille: usize,
) -> Result<CarteFrontOnde, String> {
    let taille = taille.clamp(8, 256);
    let refe = reference(sys, champ)?;
    let mut index = Vec::new();
    let mut points = Vec::new();
    for j in 0..taille {
        for i in 0..taille {
            let px = -1.0 + (i as f64 + 0.5) * 2.0 / taille as f64;
            let py = -1.0 + (j as f64 + 0.5) * 2.0 / taille as f64;
            if px * px + py * py <= 1.0 {
                index.push(j * taille + i);
                points.push((px, py));
            }
        }
    }
    let opd = opd_points(sys, champ, lambda_nm, &points, &refe)?;
    let mut valeurs = vec![None; taille * taille];
    let mut valides = Vec::new();
    for ((&k, &(px, py)), w) in index.iter().zip(&points).zip(&opd) {
        if let Some(w) = *w {
            valeurs[k] = Some(w);
            valides.push((px, py, w));
        }
    }
    if valides.len() < NB_ZERNIKE + 1 {
        return Err(tr!(
            "trop peu de rayons atteignent l'image pour calculer le front d'onde",
            "too few rays reach the image to compute the wavefront",
            "zu wenige Strahlen erreichen das Bild, um die Wellenfront zu berechnen"
        )
        .to_string());
    }
    let nb = valides.len() as f64;
    let moyenne = valides.iter().map(|v| v.2).sum::<f64>() / nb;
    let rms = (valides.iter().map(|v| (v.2 - moyenne).powi(2)).sum::<f64>() / nb).sqrt();
    let (mini, maxi) = valides
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v.2), b.max(v.2)));

    // Ajustement de Zernike par moindres carrés
    let a = DMatrix::from_fn(valides.len(), NB_ZERNIKE, |r, c| {
        let (px, py, _) = valides[r];
        zernike(c + 1, px.hypot(py), py.atan2(px))
    });
    let b = DVector::from_iterator(valides.len(), valides.iter().map(|v| v.2));
    let coefficients = a
        .clone()
        .svd(true, true)
        .solve(&b, 1e-12)
        .map_err(|e| e.to_string())?;
    let residu = ((&a * &coefficients - &b).norm_squared() / nb).sqrt();

    Ok(CarteFrontOnde {
        taille,
        valeurs_ondes: valeurs,
        longueur_onde_nm: lambda_nm,
        champ_deg: champ,
        pv_ondes: maxi - mini,
        rms_ondes: rms,
        strehl_marechal: (-(2.0 * std::f64::consts::PI * rms).powi(2)).exp(),
        zernike_ondes: coefficients.iter().copied().collect(),
        residu_zernike_ondes: residu,
        nb_points: valides.len(),
    })
}

// ---------------------------------------------------------------------------------------------
// PSF et FTM
// ---------------------------------------------------------------------------------------------

/// PSF de diffraction (polychromatique ou à une longueur d'onde)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultatPsf {
    /// Côté de la grille image
    pub taille: usize,
    /// Pas de la grille image (µm)
    pub pas_um: f64,
    /// Intensité ligne par ligne de y négatif à y positif, normalisée au pic de la PSF limitée
    /// par la diffraction (le maximum vaut donc le rapport de Strehl)
    pub intensite: Vec<f64>,
    /// Rapport de Strehl : pic de la PSF / pic de la PSF sans aberration, même pupille
    pub strehl: f64,
    /// Rayon du premier anneau noir d'Airy à la longueur d'onde de référence (µm)
    pub rayon_airy_um: f64,
    /// Nombre d'ouverture de travail utilisé pour l'échelle
    pub f_travail: f64,
    pub longueurs_onde_nm: Vec<f64>,
    pub champ_deg: (f64, f64),
}

/// FTM tangentielle et sagittale d'un champ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultatFtm {
    /// Fréquences spatiales (cycles/mm)
    pub frequences_cyc_mm: Vec<f64>,
    pub tangentielle: Vec<f64>,
    pub sagittale: Vec<f64>,
    /// FTM de la même pupille sans aberration
    pub limite_diffraction: Vec<f64>,
    /// Fréquence de coupure à la plus courte longueur d'onde (cycles/mm)
    pub frequence_coupure_cyc_mm: f64,
    pub champ_deg: (f64, f64),
}

fn fft2(planificateur: &mut FftPlanner<f64>, donnees: &mut [Complex<f64>], n: usize) {
    let fft = planificateur.plan_fft_forward(n);
    for ligne in donnees.chunks_mut(n) {
        fft.process(ligne);
    }
    let mut colonne = vec![Complex::new(0.0, 0.0); n];
    for i in 0..n {
        for j in 0..n {
            colonne[j] = donnees[j * n + i];
        }
        fft.process(&mut colonne);
        for j in 0..n {
            donnees[j * n + i] = colonne[j];
        }
    }
}

/// Plus grand écart (mm) entre les rayons du bord de la pupille et l'image du rayon principal
fn rayon_geometrique(sys: &OpticalSystem, champ: (f64, f64), lambdas: &[(f64, f64)], refe: &Reference) -> f64 {
    let vise = VisePupille::calculer(sys);
    let mut r_max: f64 = 0.0;
    for &(lambda, _) in lambdas {
        for k in 0..24 {
            let t = std::f64::consts::TAU * k as f64 / 24.0;
            for rho in [0.7, 1.0] {
                let (px, py) = (rho * t.cos(), rho * t.sin());
                let tr = tracer_rayon_unique(sys, vise.rayon(px, py, champ.0, champ.1, lambda), champ, (px, py));
                if let Some(impact) = tr.impact_plan_image() {
                    r_max = r_max.max((impact.point_global - refe.centre).norm());
                }
            }
        }
    }
    r_max
}

/// Calcule la PSF et la FTM d'un champ ; `lambda_nm = None` pour la PSF polychromatique
/// (longueurs d'onde et poids du système)
pub fn calculer_psf_ftm(
    sys: &OpticalSystem,
    champ: (f64, f64),
    lambda_nm: Option<f64>,
) -> Result<(ResultatPsf, ResultatFtm), String> {
    let lambdas: Vec<(f64, f64)> = match lambda_nm {
        Some(l) => vec![(l, 1.0)],
        None if sys.longueurs_onde.is_empty() => vec![(sys.longueur_onde_ref_nm, 1.0)],
        None => sys.longueurs_onde.iter().map(|w| (w.valeur_nm, w.poids.max(0.0))).collect(),
    };
    let poids_total: f64 = lambdas.iter().map(|l| l.1).sum();
    if poids_total <= 0.0 {
        return Err(tr!("poids des longueurs d'onde nuls", "all wavelength weights are zero", "alle Wellenlängengewichte sind null").to_string());
    }
    let refe = reference(sys, champ)?;
    let l_min = lambdas.iter().map(|l| l.0).fold(f64::INFINITY, f64::min);
    // La grille image couvre ±N λF/2 : N ≥ 2,5 r_geo / (λF) garde la tache entière avec une marge,
    // et assure moins d'une demi-onde de variation du front d'onde entre deux échantillons
    let lambda_f = l_min * 1e-6 * refe.f_travail;
    let r_geo = rayon_geometrique(sys, champ, &lambdas, &refe);
    let echantillons = ((2.5 * r_geo / lambda_f / 16.0).ceil() as usize * 16)
        .clamp(ECHANTILLONS_PUPILLE_MIN, ECHANTILLONS_PUPILLE_MAX);
    let m = (4 * echantillons).next_power_of_two();
    let echantillons_f = echantillons as f64;
    let pas_mm = lambda_f * echantillons_f / m as f64;

    let mut planificateur = FftPlanner::new();
    let mut psf = vec![0.0; m * m];
    let mut psf_ideale = vec![0.0; m * m];
    for &(lambda, poids) in &lambdas {
        if poids <= 0.0 {
            continue;
        }
        // Pupille échantillonnée plus finement aux grandes longueurs d'onde : même grille image
        let n_lambda = echantillons_f * l_min / lambda;
        let pas_pupille = 2.0 / n_lambda;
        let mut cellules = Vec::new();
        let mut points = Vec::new();
        for j in 0..m {
            for i in 0..m {
                let u = (i as f64 - m as f64 / 2.0 + 0.5) * pas_pupille;
                let v = (j as f64 - m as f64 / 2.0 + 0.5) * pas_pupille;
                if u * u + v * v <= 1.0 {
                    cellules.push(j * m + i);
                    points.push((u, v));
                }
            }
        }
        let opd = opd_points(sys, champ, lambda, &points, &refe)?;
        let mut pupille = vec![Complex::new(0.0, 0.0); m * m];
        let mut ideale = vec![Complex::new(0.0, 0.0); m * m];
        let mut nb_valides = 0usize;
        for (&k, w) in cellules.iter().zip(&opd) {
            if let Some(w) = w {
                // (-1)^(i+j) centre la PSF au milieu de la grille
                let signe = if (k / m + k % m) % 2 == 0 { 1.0 } else { -1.0 };
                let phase = 2.0 * std::f64::consts::PI * w;
                pupille[k] = Complex::new(signe * phase.cos(), signe * phase.sin());
                ideale[k] = Complex::new(signe, 0.0);
                nb_valides += 1;
            }
        }
        if nb_valides == 0 {
            continue;
        }
        fft2(&mut planificateur, &mut pupille, m);
        fft2(&mut planificateur, &mut ideale, m);
        // Énergie de chaque longueur d'onde égale à son poids (théorème de Parseval)
        let norme = poids / poids_total / (m * m * nb_valides) as f64;
        for k in 0..m * m {
            psf[k] += pupille[k].norm_sqr() * norme;
            psf_ideale[k] += ideale[k].norm_sqr() * norme;
        }
    }
    let pic_ideal = psf_ideale.iter().copied().fold(0.0, f64::max);
    if pic_ideal <= 0.0 {
        return Err(tr!(
            "aucun rayon n'atteint l'image pour ce champ",
            "no ray reaches the image for this field",
            "kein Strahl erreicht das Bild für dieses Feld"
        )
        .to_string());
    }
    let strehl = psf.iter().copied().fold(0.0, f64::max) / pic_ideal;

    // FTM : module de la transformée de Fourier de la PSF, normalisée à la fréquence nulle
    let coupes = |intensite: &[f64], planificateur: &mut FftPlanner<f64>| {
        let mut otf: Vec<Complex<f64>> = intensite.iter().map(|&v| Complex::new(v, 0.0)).collect();
        fft2(planificateur, &mut otf, m);
        let zero = otf[0].norm().max(1e-300);
        let k_max = echantillons;
        let tangentielle: Vec<f64> = (0..=k_max).map(|k| otf[k * m].norm() / zero).collect();
        let sagittale: Vec<f64> = (0..=k_max).map(|k| otf[k].norm() / zero).collect();
        (tangentielle, sagittale)
    };
    let (mut tangentielle, mut sagittale) = coupes(&psf, &mut planificateur);
    let (limite, _) = coupes(&psf_ideale, &mut planificateur);
    // Champ selon X : la direction tangentielle est X
    if champ.0.abs() > champ.1.abs() {
        std::mem::swap(&mut tangentielle, &mut sagittale);
    }
    let pas_frequence = 1.0 / (m as f64 * pas_mm);
    let frequences = (0..tangentielle.len()).map(|k| k as f64 * pas_frequence).collect();

    let lambda_ref = lambda_nm.unwrap_or(sys.longueur_onde_ref_nm);
    Ok((
        ResultatPsf {
            taille: m,
            pas_um: pas_mm * 1e3,
            intensite: psf.iter().map(|v| v / pic_ideal).collect(),
            strehl,
            rayon_airy_um: 1.22 * lambda_ref * 1e-3 * refe.f_travail,
            f_travail: refe.f_travail,
            longueurs_onde_nm: lambdas.iter().map(|l| l.0).collect(),
            champ_deg: champ,
        },
        ResultatFtm {
            frequences_cyc_mm: frequences,
            tangentielle,
            sagittale,
            limite_diffraction: limite,
            frequence_coupure_cyc_mm: 1.0 / (l_min * 1e-6 * refe.f_travail),
            champ_deg: champ,
        },
    ))
}

// ---------------------------------------------------------------------------------------------
// Sélection et cache des analyses pour les interfaces
// ---------------------------------------------------------------------------------------------

/// Côté de la grille de la carte de front d'onde affichée
pub const TAILLE_CARTE: usize = 64;

/// Champ et longueur d'onde choisis pour les analyses ondulatoires, avec les résultats déjà
/// calculés (à invalider à chaque modification du système)
#[derive(Debug, Clone, Default)]
pub struct AnalysesOndes {
    /// Index du champ dans `champs_deg`
    pub champ: usize,
    /// Index de la longueur d'onde ; `None` = polychromatique (PSF, FTM) ou référence (front d'onde)
    pub lambda: Option<usize>,
    front: Option<Result<CarteFrontOnde, String>>,
    psf: Option<Result<(ResultatPsf, ResultatFtm), String>>,
    ftm_champs: Option<Vec<Result<ResultatFtm, String>>>,
    /// Incrémenté à chaque nouveau calcul (pour rafraîchir les images affichées)
    pub generation: u64,
}

impl AnalysesOndes {
    pub fn invalider(&mut self) {
        self.front = None;
        self.psf = None;
        self.ftm_champs = None;
        self.generation += 1;
    }

    fn borner(&mut self, sys: &OpticalSystem) {
        if self.champ >= sys.champs_deg.len().max(1) {
            self.champ = 0;
        }
        if self.lambda.is_some_and(|l| l >= sys.longueurs_onde.len()) {
            self.lambda = None;
        }
    }

    pub fn champ_deg(&self, sys: &OpticalSystem) -> (f64, f64) {
        sys.champs_deg.get(self.champ).copied().unwrap_or((0.0, 0.0))
    }

    /// Longueur d'onde choisie ; `None` = polychromatique
    pub fn lambda_nm(&self, sys: &OpticalSystem) -> Option<f64> {
        self.lambda.and_then(|i| sys.longueurs_onde.get(i)).map(|w| w.valeur_nm)
    }

    /// Longueur d'onde de la carte de front d'onde (la référence en mode polychromatique)
    pub fn lambda_front_nm(&self, sys: &OpticalSystem) -> f64 {
        self.lambda_nm(sys).unwrap_or(sys.longueur_onde_ref_nm)
    }

    pub fn champ_suivant(&mut self, sys: &OpticalSystem) {
        self.champ = (self.champ + 1) % sys.champs_deg.len().max(1);
        self.invalider();
    }

    pub fn lambda_suivante(&mut self, sys: &OpticalSystem) {
        let n = sys.longueurs_onde.len();
        self.lambda = match self.lambda {
            None if n > 0 => Some(0),
            Some(i) if i + 1 < n => Some(i + 1),
            _ => None,
        };
        self.invalider();
    }

    pub fn front_onde(&mut self, sys: &OpticalSystem) -> &Result<CarteFrontOnde, String> {
        self.borner(sys);
        let (champ, lambda) = (self.champ_deg(sys), self.lambda_front_nm(sys));
        self.front
            .get_or_insert_with(|| calculer_front_onde(sys, champ, lambda, TAILLE_CARTE))
    }

    pub fn psf(&mut self, sys: &OpticalSystem) -> &Result<(ResultatPsf, ResultatFtm), String> {
        self.borner(sys);
        let (champ, lambda) = (self.champ_deg(sys), self.lambda_nm(sys));
        self.psf.get_or_insert_with(|| calculer_psf_ftm(sys, champ, lambda))
    }

    /// FTM de tous les champs du système
    pub fn ftm_champs(&mut self, sys: &OpticalSystem) -> &Vec<Result<ResultatFtm, String>> {
        self.borner(sys);
        let lambda = self.lambda_nm(sys);
        self.ftm_champs.get_or_insert_with(|| {
            let champs = if sys.champs_deg.is_empty() { vec![(0.0, 0.0)] } else { sys.champs_deg.clone() };
            champs
                .iter()
                .map(|&c| calculer_psf_ftm(sys, c, lambda).map(|(_, f)| f))
                .collect()
        })
    }

    /// Résultats déjà calculés (pour un affichage sans accès mutable)
    pub fn front_calcule(&self) -> Option<&Result<CarteFrontOnde, String>> {
        self.front.as_ref()
    }
    pub fn psf_calculee(&self) -> Option<&Result<(ResultatPsf, ResultatFtm), String>> {
        self.psf.as_ref()
    }
    pub fn ftm_calculees(&self) -> Option<&Vec<Result<ResultatFtm, String>>> {
        self.ftm_champs.as_ref()
    }
}

/// FTM de diffraction d'une pupille circulaire sans aberration, ν = f / f_coupure
pub fn ftm_ideale_circulaire(nu: f64) -> f64 {
    if nu >= 1.0 {
        return 0.0;
    }
    let nu = nu.max(0.0);
    2.0 / std::f64::consts::PI * (nu.acos() - nu * (1.0 - nu * nu).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{ApertureSpec, Surface, SurfaceType, Wavelength};

    /// Miroir de rayon `r` (conique `k`) et pupille `epd`, plan image au foyer paraxial décalé de `dz`
    fn miroir(r: f64, k: f64, epd: f64, dz: f64) -> OpticalSystem {
        let mut m = Surface::nouvelle(SurfaceType::Conique, r, r / 2.0 + dz * r.signum(), "MIROIR", epd);
        m.conique = k;
        m.est_diaphragme = true;
        OpticalSystem {
            nom: "miroir".into(),
            longueur_onde_ref_nm: 587.5618,
            longueurs_onde: vec![Wavelength::new(587.5618, 1.0, "d")],
            ouverture: ApertureSpec::PupilleEntreeMm(epd),
            champs_deg: vec![(0.0, 0.0), (0.0, 0.5)],
            surfaces: vec![m, Surface::plan_image(20.0)],
            catalogue: Default::default(),
        }
    }

    #[test]
    fn systeme_afocal_refuse() {
        let sys = OpticalSystem::nouveau("vide", 587.5618);
        assert!(calculer_front_onde(&sys, (0.0, 0.0), 587.5618, 16).is_err());
        assert!(calculer_psf_ftm(&sys, (0.0, 0.0), None).is_err());
    }

    #[test]
    fn indices_de_noll() {
        assert_eq!(noll_vers_nm(1), (0, 0));
        assert_eq!(noll_vers_nm(2), (1, 1));
        assert_eq!(noll_vers_nm(3), (1, -1));
        assert_eq!(noll_vers_nm(4), (2, 0));
        assert_eq!(noll_vers_nm(5), (2, -2));
        assert_eq!(noll_vers_nm(6), (2, 2));
        assert_eq!(noll_vers_nm(7), (3, -1));
        assert_eq!(noll_vers_nm(8), (3, 1));
        assert_eq!(noll_vers_nm(11), (4, 0));
        assert_eq!(noll_vers_nm(22), (6, 0));
        assert_eq!(noll_vers_nm(37), (8, 0));
    }

    #[test]
    fn zernike_orthonormes_sur_le_disque() {
        let n = 400;
        let mut points = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let x = -1.0 + (i as f64 + 0.5) * 2.0 / n as f64;
                let y = -1.0 + (j as f64 + 0.5) * 2.0 / n as f64;
                if x * x + y * y <= 1.0 {
                    points.push((x.hypot(y), y.atan2(x)));
                }
            }
        }
        for a in 1..=15 {
            for b in a..=15 {
                let s: f64 = points.iter().map(|&(r, t)| zernike(a, r, t) * zernike(b, r, t)).sum::<f64>()
                    / points.len() as f64;
                let attendu = if a == b { 1.0 } else { 0.0 };
                assert!((s - attendu).abs() < 0.02, "<Z{a}, Z{b}> = {s}");
            }
        }
    }

    #[test]
    fn parabole_parfaite_sur_l_axe() {
        let sys = miroir(-2000.0, -1.0, 200.0, 0.0);
        let carte = calculer_front_onde(&sys, (0.0, 0.0), 587.5618, 32).unwrap();
        assert!(carte.rms_ondes < 1e-4, "RMS = {}", carte.rms_ondes);
        let (psf, ftm) = calculer_psf_ftm(&sys, (0.0, 0.0), None).unwrap();
        assert!((psf.strehl - 1.0).abs() < 1e-6, "Strehl = {}", psf.strehl);
        // F/5 à 587,56 nm : coupure 1 / (λ F) = 340,4 cycles/mm
        assert!((ftm.frequence_coupure_cyc_mm - 340.4).abs() < 1.0, "{}", ftm.frequence_coupure_cyc_mm);
        for (k, f) in ftm.frequences_cyc_mm.iter().enumerate() {
            let nu = f / ftm.frequence_coupure_cyc_mm;
            let attendu = ftm_ideale_circulaire(nu);
            assert!((ftm.tangentielle[k] - attendu).abs() < 0.02, "ν = {nu} : {} au lieu de {attendu}", ftm.tangentielle[k]);
            assert!((ftm.sagittale[k] - attendu).abs() < 0.02);
        }
        // Premier anneau noir d'Airy à 1,22 λ F = 3,58 µm
        assert!((psf.rayon_airy_um - 3.584).abs() < 0.01);
        let m = psf.taille;
        let profil: Vec<f64> = (0..m / 2).map(|i| psf.intensite[(m / 2) * m + m / 2 + i]).collect();
        let premier_min = (1..profil.len() - 1)
            .find(|&i| profil[i] < profil[i - 1] && profil[i] <= profil[i + 1])
            .unwrap() as f64
            * psf.pas_um;
        assert!((premier_min - psf.rayon_airy_um).abs() < psf.pas_um, "minimum à {premier_min} µm");
    }

    #[test]
    fn defocalisation_connue() {
        // Plan image reculé de 50 µm sur un miroir f/5 : W020 = δ / (8 F²) = 0,4255 onde
        let sys = miroir(-2000.0, -1.0, 200.0, 0.05);
        let w020 = 0.05 / (8.0 * 25.0) / 587.5618e-6;
        let carte = calculer_front_onde(&sys, (0.0, 0.0), 587.5618, 48).unwrap();
        let z4 = carte.zernike_ondes[3].abs();
        assert!((z4 - w020 / (2.0 * 3f64.sqrt())).abs() < 0.01 * w020, "Z4 = {z4}");
        assert!((carte.rms_ondes - w020 / 12f64.sqrt()).abs() < 0.01 * w020, "RMS = {}", carte.rms_ondes);
        for (j, c) in carte.zernike_ondes.iter().enumerate().skip(4) {
            assert!(c.abs() < 2e-3, "Z{} = {c}", j + 1);
        }
        // Strehl exact d'une défocalisation pure : sinc²(π W020)
        let x = std::f64::consts::PI * w020;
        let strehl_exact = (x.sin() / x).powi(2);
        let (psf, _) = calculer_psf_ftm(&sys, (0.0, 0.0), None).unwrap();
        assert!((psf.strehl - strehl_exact).abs() < 0.015, "Strehl {} au lieu de {strehl_exact}", psf.strehl);
    }

    #[test]
    fn aberration_spherique_d_un_miroir_spherique() {
        // Miroir sphérique R = 1000, h = 50 au foyer paraxial : W040 = h⁴ / (4 R³) = 2,66 ondes,
        // coefficient de Z11 = W040 / (6 √5)
        let sys = miroir(-1000.0, 0.0, 100.0, 0.0);
        let w040 = 50f64.powi(4) / (4.0 * 1000f64.powi(3)) / 587.5618e-6;
        let carte = calculer_front_onde(&sys, (0.0, 0.0), 587.5618, 48).unwrap();
        let z11 = carte.zernike_ondes[10].abs();
        let attendu = w040 / (6.0 * 5f64.sqrt());
        assert!((z11 - attendu).abs() < 0.03 * attendu, "Z11 = {z11}, attendu {attendu}");
        assert!(carte.residu_zernike_ondes < 0.01, "résidu {}", carte.residu_zernike_ondes);
    }

    #[test]
    fn centroide_psf_egal_au_centroide_geometrique() {
        // Doublet très aberré à 2,5° : la PSF doit tenir dans la grille (échantillonnage adapté)
        // et son centre de gravité coïncider avec celui du spot diagram, dans le même sens
        let mut d = OpticalSystem::doublet_achromatique_defaut();
        d.longueurs_onde.retain(|w| (w.valeur_nm - 587.5618).abs() < 1.0);
        for champ in [(0.0, 2.5), (2.5, 0.0)] {
            let (p, _) = calculer_psf_ftm(&d, champ, None).unwrap();
            let m = p.taille;
            let (mut sx, mut sy, mut s0) = (0.0, 0.0, 0.0);
            for j in 0..m {
                for i in 0..m {
                    let v = p.intensite[j * m + i];
                    sx += v * (i as f64 - (m / 2) as f64) * p.pas_um;
                    sy += v * (j as f64 - (m / 2) as f64) * p.pas_um;
                    s0 += v;
                }
            }
            let vise = VisePupille::calculer(&d);
            let chef = tracer_rayon_unique(&d, vise.rayon(0.0, 0.0, champ.0, champ.1, 587.5618), champ, (0.0, 0.0));
            let c = chef.impact_plan_image().unwrap().point_global;
            let spot = crate::core::analysis::calculer_spot_diagram(&d, champ, 20);
            let (gx, gy) = ((spot.centroide_mm.0 - c.x) * 1e3, (spot.centroide_mm.1 - c.y) * 1e3);
            let ecart = ((sx / s0 - gx).powi(2) + (sy / s0 - gy).powi(2)).sqrt();
            assert!(ecart < 0.05 * gx.hypot(gy), "PSF ({}, {}) µm, spot ({gx}, {gy}) µm", sx / s0, sy / s0);
        }
    }

    #[test]
    fn coma_hors_axe_d_une_parabole() {
        let sys = miroir(-2000.0, -1.0, 200.0, 0.0);
        let carte = calculer_front_onde(&sys, (0.0, 0.5), 587.5618, 48).unwrap();
        let z = &carte.zernike_ondes;
        // Champ selon Y : coma Y (Z7) dominante, coma X (Z8) nulle par symétrie
        assert!(z[6].abs() > 0.1, "Z7 = {}", z[6]);
        assert!(z[7].abs() < 1e-3 * z[6].abs().max(1.0), "Z8 = {}", z[7]);
        let (psf, ftm) = calculer_psf_ftm(&sys, (0.0, 0.5), None).unwrap();
        assert!(psf.strehl < 0.99);
        let milieu = ftm.tangentielle.len() / 2;
        assert!(ftm.tangentielle[milieu] < ftm.limite_diffraction[milieu]);
    }
}
