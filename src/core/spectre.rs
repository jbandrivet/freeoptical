//! Outils pour les spectrographes : position de chaque longueur d'onde sur le détecteur,
//! dispersion, tache par longueur d'onde, image de la fente et pouvoir de résolution R = λ/Δλ.

use crate::core::raytracer::{TracedRay, VisePupille, coordonnees_image, tracer_rayon_unique};
use crate::core::types::{OpticalSystem, Vec3};
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
    /// Efficacité de blaze du rayon principal (produit sur les réseaux dont le blaze est
    /// renseigné ; absente sans réseau blazé)
    #[serde(default)]
    pub efficacite_blaze: Vec<Option<f64>>,
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

/// Copie du système sans obstruction centrale : le rayon principal (centre de la pupille) d'un
/// télescope à obstruction est un rayon de référence, il ne doit pas être arrêté par l'ombre du
/// secondaire. Les taches restent calculées avec l'obstruction.
pub fn sans_obstruction(sys: &OpticalSystem) -> std::borrow::Cow<'_, OpticalSystem> {
    if sys.surfaces.iter().all(|s| s.rayon_obstruction == 0.0) {
        return std::borrow::Cow::Borrowed(sys);
    }
    let mut s = sys.clone();
    for surf in &mut s.surfaces {
        surf.rayon_obstruction = 0.0;
    }
    std::borrow::Cow::Owned(s)
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
    let sys_ref = sans_obstruction(sys);
    let position = |c: (f64, f64), l: f64| -> Option<(f64, f64)> {
        let tr = tracer_rayon_unique(&sys_ref, vise.rayon(0.0, 0.0, c.0, c.1, l), c, (0.0, 0.0));
        tr.impact_plan_image().map(|h| coordonnees_image(&sys_ref, h))
    };
    let positions: Vec<Option<(f64, f64)>> = lambdas.iter().map(|&l| position(champ, l)).collect();
    let efficacite: Vec<Option<f64>> = lambdas
        .iter()
        .map(|&l| {
            let tr = tracer_rayon_unique(&sys_ref, vise.rayon(0.0, 0.0, champ.0, champ.1, l), champ, (0.0, 0.0));
            if tr.vignette { None } else { efficacite_blaze(&sys_ref, &tr) }
        })
        .collect();
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
        efficacite_blaze: efficacite,
    })
}

/// Géométrie d'un rayon sur un réseau : normale tournée vers la lumière incidente `n`, direction
/// `g` perpendiculaire aux traits dans le plan du réseau (+Y local), directions incidente `d` et
/// diffractée `d2`, pas en mm
struct SurReseau {
    n: Vec3,
    g: Vec3,
    d: Vec3,
    d2: Vec3,
    pas_mm: f64,
    blaze: f64,
}

fn sur_reseau(sys: &OpticalSystem, tr: &TracedRay, idx: usize) -> Option<SurReseau> {
    let s = sys.surfaces.get(idx)?;
    if s.reseau_traits_mm == 0.0 {
        return None;
    }
    let k = tr.impacts.iter().position(|h| h.index_surface == idx)?;
    let hit = &tr.impacts[k];
    let d = if k == 0 { tr.rayon_initial.direction } else { tr.impacts[k - 1].direction_sortante };
    let mut n = hit.normale.normalize();
    if d.dot(&n) > 0.0 {
        n = -n;
    }
    let g_brut = sys.reperes()[idx].rotation * Vec3::new(0.0, 1.0, 0.0);
    let g_t = g_brut - g_brut.dot(&n) * n;
    if g_t.norm() < 1e-12 {
        return None;
    }
    Some(SurReseau {
        n,
        g: g_t.normalize(),
        d,
        d2: hit.direction_sortante,
        pas_mm: 1.0 / s.reseau_traits_mm.abs(),
        blaze: s.reseau_blaze_deg.to_radians(),
    })
}

/// Efficacité de blaze (théorie scalaire) d'un réseau en réflexion pour le rayon tracé :
/// η = sinc²[π (d cos θb / λ) (sin(α − θb) + sin(β − θb))], α et β mesurés depuis la normale dans
/// le plan perpendiculaire aux traits (signe de +Y local). Produit sur tous les réseaux blazés
/// du système ; `None` sans réseau blazé en réflexion.
pub fn efficacite_blaze(sys: &OpticalSystem, tr: &TracedRay) -> Option<f64> {
    let mut eta: Option<f64> = None;
    for (idx, s) in sys.surfaces.iter().enumerate() {
        if s.reseau_traits_mm == 0.0 || s.reseau_blaze_deg == 0.0 || !sys.catalogue.resoudre(&s.verre).est_miroir() {
            continue;
        }
        let r = sur_reseau(sys, tr, idx)?;
        let alpha = (-r.d.dot(&r.g)).atan2(-r.d.dot(&r.n));
        let beta = r.d2.dot(&r.g).atan2(r.d2.dot(&r.n));
        let lambda_mm = tr.longueur_onde_nm * 1e-6;
        let x = std::f64::consts::PI * r.pas_mm * r.blaze.cos() / lambda_mm
            * ((alpha - r.blaze).sin() + (beta - r.blaze).sin());
        let sinc = if x.abs() < 1e-12 { 1.0 } else { x.sin() / x };
        eta = Some(eta.unwrap_or(1.0) * sinc * sinc);
    }
    eta
}

/// Produit m·λ (nm) de la condition de blaze pour le rayon tracé sur le réseau `idx` : la lumière
/// diffractée repart comme réfléchie par la facette. Pour l'ordre m, λ_blaze = (m·λ)/m.
fn m_lambda_blaze_nm(sys: &OpticalSystem, tr: &TracedRay, idx: usize) -> Option<f64> {
    let r = sur_reseau(sys, tr, idx)?;
    let f = r.blaze.cos() * r.n + r.blaze.sin() * r.g;
    let refl = r.d - 2.0 * r.d.dot(&f) * f;
    Some((refl - r.d).dot(&r.g) * r.pas_mm * 1e6)
}

/// Un ordre d'un spectrographe échelle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrdreEchelle {
    pub ordre: i32,
    /// Longueur d'onde de blaze de l'ordre (nm)
    pub lambda_blaze_nm: f64,
    /// Intervalle spectral libre λ/m (nm) et ses bornes λ·m/(m ± ½)
    pub intervalle_libre_nm: f64,
    pub lambda_debut_nm: f64,
    pub lambda_fin_nm: f64,
    /// Spectre de l'ordre sur l'intervalle libre (positions, taches, dispersion, R, efficacité)
    pub spectre: Spectre,
    /// Partie de l'intervalle libre qui tombe sur le détecteur (nm), si les dimensions sont données
    pub sur_detecteur_nm: Option<(f64, f64)>,
    /// Écart au centre de l'ordre suivant, perpendiculairement à la dispersion (mm)
    pub ecart_ordre_suivant_mm: Option<f64>,
}

/// Échellogramme : tous les ordres d'un réseau échelle, avec leur dispersion croisée
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Echellogramme {
    /// Surface du réseau (index à partir de 0)
    pub surface_reseau: usize,
    pub ordres: Vec<OrdreEchelle>,
    /// Domaine couvert par les intervalles libres (sur le détecteur s'il est donné), nm
    pub couverture_nm: Option<(f64, f64)>,
    /// Lacunes entre ordres successifs sur le détecteur (nm)
    pub lacunes_nm: Vec<(f64, f64)>,
    /// Plus petit écart entre ordres voisins (mm)
    pub ecart_min_mm: Option<f64>,
    /// Ordres qui n'atteignent pas le détecteur ou ne sont pas tracés
    pub ordres_perdus: Vec<i32>,
}

/// Paramètres de `calculer_echellogramme`
#[derive(Debug, Clone, Default)]
pub struct ParametresEchellogramme {
    /// Surface du réseau ; par défaut le premier réseau blazé (ou le premier réseau)
    pub surface: Option<usize>,
    /// Ordres extrêmes ; par défaut ceux dont la longueur d'onde de blaze est dans le domaine
    pub ordres: Option<(i32, i32)>,
    /// Domaine (nm) servant à choisir les ordres ; par défaut celui des longueurs d'onde du système
    pub domaine_nm: Option<(f64, f64)>,
    /// Points par ordre (11 par défaut)
    pub nb_par_ordre: usize,
    pub largeur_fente: f64,
    /// Dimensions du détecteur (mm), centré sur l'origine du plan image
    pub detecteur_mm: Option<(f64, f64)>,
}

/// Calcule l'échellogramme : pour chaque ordre, longueur d'onde de blaze, intervalle libre,
/// spectre (positions, tache, R, efficacité), partie sur le détecteur et écart à l'ordre suivant.
pub fn calculer_echellogramme(
    sys: &OpticalSystem,
    champ: (f64, f64),
    p: &ParametresEchellogramme,
) -> Result<Echellogramme, String> {
    let idx = p
        .surface
        .or_else(|| sys.surfaces.iter().position(|s| s.reseau_traits_mm != 0.0 && s.reseau_blaze_deg != 0.0))
        .or_else(|| sys.surfaces.iter().position(|s| s.reseau_traits_mm != 0.0))
        .ok_or_else(|| tr!("aucun réseau dans le système", "no grating in the system", "kein Gitter im System").to_string())?;
    if sys.surfaces.get(idx).is_none_or(|s| s.reseau_traits_mm == 0.0) {
        return Err(trf!("la surface {} n'est pas un réseau", "surface {} is not a grating", "Fläche {} ist kein Gitter", idx));
    }
    if sys.surfaces[idx].reseau_blaze_deg == 0.0 {
        return Err(tr!(
            "angle de blaze du réseau non renseigné (reseau_blaze_deg)",
            "grating blaze angle not set (reseau_blaze_deg)",
            "Blazewinkel des Gitters nicht angegeben (reseau_blaze_deg)"
        )
        .to_string());
    }
    let signe = if sys.surfaces[idx].reseau_blaze_deg > 0.0 { 1 } else { -1 };
    let vise = VisePupille::calculer(sys);
    let sys_ref = sans_obstruction(sys);
    let avec_ordre_ref = |m: i32| {
        let mut s = sys_ref.clone().into_owned();
        s.surfaces[idx].reseau_ordre = m;
        s
    };
    let avec_ordre = |m: i32| {
        let mut s = sys.clone();
        s.surfaces[idx].reseau_ordre = m;
        s
    };
    // m·λ de blaze, recalculé à la longueur d'onde trouvée (le prisme éventuel en amont change
    // l'incidence avec λ)
    let m_lambda = |m: i32, l0: f64| -> Option<f64> {
        let s = avec_ordre_ref(m);
        let mut l = l0;
        let mut ml = 0.0;
        for _ in 0..4 {
            let tr = tracer_rayon_unique(&s, vise.rayon(0.0, 0.0, champ.0, champ.1, l), champ, (0.0, 0.0));
            ml = m_lambda_blaze_nm(&s, &tr, idx)?;
            l = (ml / m as f64).abs();
        }
        Some(ml.abs())
    };
    let (ordre_min, ordre_max) = match p.ordres {
        Some((a, b)) => (a.abs().min(b.abs()), a.abs().max(b.abs())),
        None => {
            let (lmin, lmax) = p.domaine_nm.unwrap_or_else(|| {
                let ls: Vec<f64> = sys.longueurs_onde.iter().map(|w| w.valeur_nm).collect();
                if ls.is_empty() {
                    (sys.longueur_onde_ref_nm, sys.longueur_onde_ref_nm)
                } else {
                    (ls.iter().cloned().fold(f64::MAX, f64::min), ls.iter().cloned().fold(0.0, f64::max))
                }
            });
            // m·λ de blaze à la longueur d'onde de référence (l'ordre ne change rien avant le réseau)
            let s1 = avec_ordre_ref(signe);
            let l = sys.longueur_onde_ref_nm;
            let tr = tracer_rayon_unique(&s1, vise.rayon(0.0, 0.0, champ.0, champ.1, l), champ, (0.0, 0.0));
            let ml = m_lambda_blaze_nm(&s1, &tr, idx).map(f64::abs).ok_or_else(|| tr!(
                "le rayon principal n'atteint pas le réseau",
                "the chief ray does not reach the grating",
                "der Hauptstrahl erreicht das Gitter nicht"
            ).to_string())?;
            (((ml / lmax).ceil() as i32).max(1), ((ml / lmin).floor() as i32).max(1))
        }
    };
    if ordre_max - ordre_min > 400 {
        return Err(tr!("trop d'ordres (400 au plus)", "too many orders (400 at most)", "zu viele Ordnungen (höchstens 400)").to_string());
    }
    let mut ordres = Vec::new();
    let mut perdus = Vec::new();
    for m in ordre_min..=ordre_max {
        let ms = signe * m;
        let Some(ml) = m_lambda(ms, sys.longueur_onde_ref_nm) else {
            perdus.push(ms);
            continue;
        };
        let mf = m as f64;
        let lb = ml / mf;
        let (la, lz) = (ml / (mf + 0.5), ml / (mf - 0.5).max(0.5));
        let s = avec_ordre(ms);
        let Ok(spectre) = calculer_spectre(&s, champ, la, lz, p.nb_par_ordre.max(3), p.largeur_fente) else {
            perdus.push(ms);
            continue;
        };
        let sur_detecteur_nm = p.detecteur_mm.and_then(|(w, h)| {
            let dedans: Vec<f64> = spectre
                .longueurs_onde_nm
                .iter()
                .zip(&spectre.positions_mm)
                .filter_map(|(&l, pos)| pos.filter(|q| q.0.abs() <= w / 2.0 && q.1.abs() <= h / 2.0).map(|_| l))
                .collect();
            (!dedans.is_empty()).then(|| (dedans[0], dedans[dedans.len() - 1]))
        });
        if p.detecteur_mm.is_some() && sur_detecteur_nm.is_none() {
            perdus.push(ms);
        }
        ordres.push(OrdreEchelle {
            ordre: ms,
            lambda_blaze_nm: lb,
            intervalle_libre_nm: lb / mf,
            lambda_debut_nm: la,
            lambda_fin_nm: lz,
            spectre,
            sur_detecteur_nm,
            ecart_ordre_suivant_mm: None,
        });
    }
    // Écart entre centres d'ordres voisins, perpendiculairement à la dispersion
    let centre = |o: &OrdreEchelle| o.spectre.positions_mm[o.spectre.positions_mm.len() / 2];
    for k in 0..ordres.len().saturating_sub(1) {
        if let (Some(a), Some(b)) = (centre(&ordres[k]), centre(&ordres[k + 1])) {
            let d = ordres[k].spectre.direction;
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            ordres[k].ecart_ordre_suivant_mm = Some((dx * -d.1 + dy * d.0).abs());
        }
    }
    let ecart_min_mm = ordres.iter().filter_map(|o| o.ecart_ordre_suivant_mm).reduce(f64::min);
    // Couverture et lacunes (ordres triés par longueur d'onde croissante)
    let mut bandes: Vec<(f64, f64)> = ordres
        .iter()
        .filter_map(|o| if p.detecteur_mm.is_some() { o.sur_detecteur_nm } else { Some((o.lambda_debut_nm, o.lambda_fin_nm)) })
        .collect();
    bandes.sort_by(|a, b| a.0.total_cmp(&b.0));
    let couverture_nm = (!bandes.is_empty()).then(|| (bandes[0].0, bandes.iter().map(|b| b.1).fold(0.0, f64::max)));
    let mut lacunes_nm = Vec::new();
    let mut fin = bandes.first().map_or(0.0, |b| b.1);
    for b in bandes.iter().skip(1) {
        if b.0 > fin + 1e-9 {
            lacunes_nm.push((fin, b.0));
        }
        fin = fin.max(b.1);
    }
    Ok(Echellogramme { surface_reseau: idx, ordres, couverture_nm, lacunes_nm, ecart_min_mm, ordres_perdus: perdus })
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

    /// Échelle 79 traits/mm, blaze 63°, montée en Littrow (incidence 63°), détecteur à 50 mm
    fn echelle_littrow() -> OpticalSystem {
        let mut reseau = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, -50.0, "MIROIR", 30.0);
        reseau.reseau_traits_mm = 79.0;
        reseau.reseau_ordre = 40;
        reseau.reseau_blaze_deg = 63.0;
        reseau.inclinaison_deg = [-63.0, 0.0];
        reseau.est_diaphragme = true;
        OpticalSystem {
            nom: "échelle".into(),
            longueur_onde_ref_nm: 550.0,
            longueurs_onde: vec![Wavelength::new(450.0, 1.0, "450"), Wavelength::new(650.0, 1.0, "650")],
            ouverture: ApertureSpec::PupilleEntreeMm(10.0),
            champs_deg: vec![(0.0, 0.0)],
            surfaces: vec![reseau, Surface::plan_image(80.0)],
            catalogue: Default::default(),
            distance_objet_mm: None,
            afocal: false,
            tolerances: Vec::new(),
        }
    }

    #[test]
    fn blaze_et_echellogramme_en_littrow() {
        let sys = echelle_littrow();
        let p = ParametresEchellogramme { nb_par_ordre: 11, largeur_fente: 1.0, ..Default::default() };
        let e = calculer_echellogramme(&sys, (0.0, 0.0), &p).unwrap();
        // Ordres dont le blaze est entre 450 et 650 nm : m λb = 2 d sin θb = 22 559 nm
        let ml = 2.0 / 79.0 * 63.0f64.to_radians().sin() * 1e6;
        let (m0, m1) = ((ml / 650.0).ceil() as i32, (ml / 450.0).floor() as i32);
        assert_eq!(e.ordres.first().unwrap().ordre, m0);
        assert_eq!(e.ordres.last().unwrap().ordre, m1);
        for o in &e.ordres {
            let m = o.ordre as f64;
            assert!((o.lambda_blaze_nm - ml / m).abs() < 1e-6 * ml, "ordre {} : {}", o.ordre, o.lambda_blaze_nm);
            assert!((o.intervalle_libre_nm - ml / m / m).abs() < 1e-6 * ml);
            // Centre de l'ordre : retour exact en Littrow, efficacité 1 ; bords de l'intervalle
            // libre : sinc²(½) = 0,405 (à la variation de β près)
            let n = o.spectre.efficacite_blaze.len();
            let mut s_m = sys.clone();
            s_m.surfaces[0].reseau_ordre = o.ordre;
            let vise = VisePupille::calculer(&s_m);
            let tr = tracer_rayon_unique(&s_m, vise.rayon(0.0, 0.0, 0.0, 0.0, o.lambda_blaze_nm), (0.0, 0.0), (0.0, 0.0));
            assert!((efficacite_blaze(&s_m, &tr).unwrap() - 1.0).abs() < 1e-9);
            let d = tr.impacts[0].direction_sortante;
            assert!((d.z + 1.0).abs() < 1e-9, "retour Littrow : {d:?}");
            let (pas, tb) = (1e6 / 79.0, 63.0f64.to_radians());
            for k in [0, n - 1] {
                let l = o.spectre.longueurs_onde_nm[k];
                let beta = (m * l / pas - tb.sin()).asin();
                let x = std::f64::consts::PI * pas * tb.cos() / l * (beta - tb).sin();
                let attendu = (x.sin() / x).powi(2);
                let eta = o.spectre.efficacite_blaze[k].unwrap();
                assert!((eta - attendu).abs() < 1e-9, "ordre {} bord {k} : η = {eta} au lieu de {attendu}", o.ordre);
                assert!(eta > 0.3 && eta < 0.5);
            }
        }
        // Intervalles libres contigus : aucune lacune
        assert!(e.lacunes_nm.is_empty(), "{:?}", e.lacunes_nm);
        // Une obstruction centrale n'arrête pas le rayon principal (rayon de référence)
        let mut obstrue = sys.clone();
        obstrue.surfaces[0].rayon_obstruction = 2.0;
        let eo = calculer_echellogramme(&obstrue, (0.0, 0.0), &p).unwrap();
        assert_eq!(eo.ordres.len(), e.ordres.len());
        assert!((eo.ordres[0].lambda_blaze_nm - e.ordres[0].lambda_blaze_nm).abs() < 1e-9);
        // Sans blaze : erreur explicite
        let mut sans = sys.clone();
        sans.surfaces[0].reseau_blaze_deg = 0.0;
        assert!(calculer_echellogramme(&sans, (0.0, 0.0), &p).is_err());
        // Ordre opposé : l'efficacité s'effondre
        let mut oppose = sys.clone();
        oppose.surfaces[0].reseau_ordre = -40;
        let vise = VisePupille::calculer(&oppose);
        let tr = tracer_rayon_unique(&oppose, vise.rayon(0.0, 0.0, 0.0, 0.0, 400.0), (0.0, 0.0), (0.0, 0.0));
        assert!(tr.vignette || efficacite_blaze(&oppose, &tr).unwrap() < 0.05);
    }
}
