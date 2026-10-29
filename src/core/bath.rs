//! Interféromètre de Bath : simulation de l'interférogramme d'un miroir concave testé à son centre
//! de courbure, et réduction des mesures (interférogramme simulé ou photographié).
//!
//! **Montage.** Le faisceau d'un laser est séparé par un cube en deux faisceaux parallèles écartés
//! de `s`. Le premier traverse la lentille de Bath (courte focale) : il diverge depuis son foyer S
//! et éclaire tout le miroir (onde test). Le second, non élargi, frappe le centre du miroir sur un
//! ou deux millimètres, revient et traverse la lentille au retour : il diverge alors depuis un
//! point Q, écarté de S de `s`, en onde sphérique quasi parfaite (onde de référence). L'onde test
//! revenue près de Q interfère avec la référence ; l'observateur (oculaire ou caméra, derrière le
//! cube) met au point sur le miroir. Les deux ondes traversent la même lentille sur le même cône
//! (l'une à l'aller, l'autre au retour) : ses aberrations se compensent (chemin commun).
//!
//! **Modèle.** Repère du miroir : sommet à l'origine, axe Z vers le centre de courbure, flèche
//! exacte de la conique plus les défauts de surface. Pour un point M du miroir, la différence de
//! marche entre l'onde test et la référence sphérique centrée en Q vaut `|SM| + |MQ|` à une
//! constante près (développement de Hopkins : l'erreur est du second ordre en l'écart des rayons
//! à Q, quelques millièmes d'onde au plus). Un miroir sphérique avec `s = 0` donne une teinte
//! plate ; l'écart `s` ajoute l'astigmatisme de Bath, `−(s/2)²x²/R³` selon la direction de
//! l'écart, soit `s²D²/(16R³)` crête-à-creux.
//!
//! **Réglages.** Basculer le miroir de `t/R` revient à déplacer S et Q de `t` : la différence de
//! marche gagne `−2t·x/R`, donc N franges sur le diamètre pour `t = NλR/(2D)`. Avancer le montage
//! de `dz` vers le miroir ajoute `+dz·(r/R)²`, donc N franges du centre au bord pour
//! `dz = 4NλR²/D²`.
//!
//! **Réduction.** La carte mesurée (en ondes à la longueur d'onde du test) est comparée à la carte
//! théorique du miroir parfait de conique visée dans le même montage (aberration sphérique au
//! centre de courbure et astigmatisme de Bath) ; piston, basculements et défocalisation sont
//! retirés par ajustement de Zernike. Le résidu est l'erreur du front d'onde réfléchi : au foyer,
//! sur une étoile, il est le même (réflexion sous incidence quasi normale = deux fois l'erreur de
//! surface). La conique mesurée vient du coefficient de sphérique Z11, comparé à sa variation
//! théorique avec k.

use crate::core::onde::{NB_ZERNIKE, ajuster_zernike, zernike};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Longueur d'onde à laquelle sont ramenés les écarts et le Strehl (vert, maximum de l'œil)
pub const LAMBDA_VISUELLE_NM: f64 = 550.0;

/// Défauts de la surface du miroir simulé (hauteurs positives = bosse, matière en plus)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DefautsSurface {
    /// Coefficients de Zernike de la surface, normalisation de Noll (RMS) : (indice, nm)
    pub zernike_nm: Vec<(usize, f64)>,
    /// Zone annulaire (gaussienne) : rayon en % du rayon du miroir
    pub zone_rayon_pct: f64,
    /// Hauteur de la zone (nm, négative pour un creux)
    pub zone_hauteur_nm: f64,
    /// Largeur à mi-hauteur de la zone, en % du rayon du miroir
    pub zone_largeur_pct: f64,
    /// Bord : écart au bord du miroir (nm, négatif pour un bord rabattu)
    pub bord_hauteur_nm: f64,
    /// Début du bord rabattu ou relevé, en % du rayon (variation quadratique jusqu'au bord)
    pub bord_debut_pct: f64,
}

impl DefautsSurface {
    /// Écart de la surface à la conique (mm) au rayon normalisé `rho` et à l'angle `theta`
    pub fn ecart_mm(&self, rho: f64, theta: f64) -> f64 {
        let mut h = 0.0;
        for &(j, c) in &self.zernike_nm {
            if (1..=NB_ZERNIKE).contains(&j) && c != 0.0 {
                h += c * zernike(j, rho, theta);
            }
        }
        if self.zone_hauteur_nm != 0.0 && self.zone_largeur_pct > 0.0 {
            let u = (rho - self.zone_rayon_pct / 100.0) / (self.zone_largeur_pct / 100.0);
            h += self.zone_hauteur_nm * (-4.0 * std::f64::consts::LN_2 * u * u).exp();
        }
        let debut = (self.bord_debut_pct / 100.0).clamp(0.0, 0.999);
        if self.bord_hauteur_nm != 0.0 && rho > debut {
            h += self.bord_hauteur_nm * ((rho - debut) / (1.0 - debut)).powi(2);
        }
        h * 1e-6
    }
}

/// Signe de la carte extraite d'une photo (une seule photo ne distingue pas bosse et creux)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SigneCarte {
    /// Le signe qui rapproche le plus le miroir de la conique visée
    #[default]
    Auto,
    Normal,
    Inverse,
}

/// Miroir, montage, réglages et options de réduction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ParametresBath {
    /// Diamètre utile du miroir (mm)
    pub diametre_mm: f64,
    /// Rayon de courbure au sommet (mm), le double de la focale
    pub rayon_courbure_mm: f64,
    /// Constante conique réelle du miroir simulé (−1 = parabole)
    pub conique: f64,
    /// Constante conique visée, profil de référence de la réduction
    pub conique_visee: f64,
    /// Diamètre de l'obstruction centrale exclue de la mesure (mm)
    pub diametre_obstruction_mm: f64,
    /// Longueur d'onde du laser (nm)
    pub longueur_onde_nm: f64,
    /// Écart entre le point source et le point de référence (mm)
    pub ecart_mm: f64,
    /// Direction de l'écart, comptée depuis l'axe X de l'image (degrés)
    pub angle_ecart_deg: f64,
    /// Franges de basculement sur le diamètre, selon X et selon Y
    pub basculement_x_franges: f64,
    pub basculement_y_franges: f64,
    /// Franges de défocalisation du centre au bord (positif : montage avancé vers le miroir)
    pub defocalisation_franges: f64,
    pub defauts: DefautsSurface,
    /// Retirer aussi l'astigmatisme mesuré (s'il vient du support ou du montage)
    pub retirer_astigmatisme: bool,
    /// Points sur le diamètre de la carte
    pub taille: usize,
}

impl Default for ParametresBath {
    /// Miroir de 200 mm à F/6 parabolique, laser He-Ne, 10 franges de basculement et mise au
    /// point réglée pour des franges aussi droites que possible
    fn default() -> Self {
        let mut p = Self {
            diametre_mm: 200.0,
            rayon_courbure_mm: 2400.0,
            conique: -1.0,
            conique_visee: -1.0,
            diametre_obstruction_mm: 0.0,
            longueur_onde_nm: 632.8,
            ecart_mm: 5.0,
            angle_ecart_deg: 0.0,
            basculement_x_franges: 10.0,
            basculement_y_franges: 0.0,
            defocalisation_franges: 0.0,
            defauts: DefautsSurface::default(),
            retirer_astigmatisme: false,
            taille: 200,
        };
        p.defocalisation_franges = p.defocalisation_franges_droites();
        p
    }
}

impl ParametresBath {
    pub fn valider(&self) -> Result<(), String> {
        if !(self.diametre_mm > 0.0 && self.rayon_courbure_mm > 0.0 && self.longueur_onde_nm > 0.0) {
            return Err(tr!(
                "diamètre, rayon de courbure et longueur d'onde doivent être positifs",
                "diameter, radius of curvature and wavelength must be positive",
                "Durchmesser, Krümmungsradius und Wellenlänge müssen positiv sein"
            )
            .to_string());
        }
        if self.rayon_courbure_mm < 0.6 * self.diametre_mm {
            return Err(tr!(
                "miroir trop ouvert : le rayon de courbure doit dépasser 0,6 fois le diamètre",
                "mirror too fast: the radius of curvature must exceed 0.6 times the diameter",
                "Spiegel zu lichtstark: der Krümmungsradius muss das 0,6-Fache des Durchmessers übersteigen"
            )
            .to_string());
        }
        let r = self.diametre_mm / 2.0;
        let c = 1.0 / self.rayon_courbure_mm;
        if 1.0 - (1.0 + self.conique.max(self.conique_visee)) * c * c * r * r <= 0.0 {
            return Err(tr!(
                "la conique n'est pas définie jusqu'au bord du miroir (k trop grand)",
                "the conic is not defined up to the mirror edge (k too large)",
                "der Kegelschnitt ist nicht bis zum Spiegelrand definiert (k zu groß)"
            )
            .to_string());
        }
        Ok(())
    }

    /// Rapport d'ouverture F/D du miroir
    pub fn rapport_ouverture(&self) -> f64 {
        self.rayon_courbure_mm / 2.0 / self.diametre_mm
    }

    /// Rayon relatif de l'obstruction (0 à 1)
    pub fn rho_obstruction(&self) -> f64 {
        (self.diametre_obstruction_mm / self.diametre_mm).clamp(0.0, 0.95)
    }

    fn lambda_mm(&self) -> f64 {
        self.longueur_onde_nm * 1e-6
    }

    /// Déplacement latéral (mm) du couple source/référence équivalent aux franges de basculement
    pub fn decalage_mm(&self) -> (f64, f64) {
        let k = -self.lambda_mm() * self.rayon_courbure_mm / (2.0 * self.diametre_mm);
        (k * self.basculement_x_franges, k * self.basculement_y_franges)
    }

    /// Déplacement axial (mm, positif vers le miroir) équivalent aux franges de défocalisation
    pub fn defocalisation_mm(&self) -> f64 {
        4.0 * self.defocalisation_franges * self.lambda_mm() * self.rayon_courbure_mm.powi(2) / self.diametre_mm.powi(2)
    }

    /// Défocalisation (franges) qui compense la partie en ρ² de la sphérique du miroir : avec
    /// `W ≈ A·ρ⁴` (A = 6√5·Z11), −A franges donnent `A·(ρ⁴ − ρ²)`, les franges les plus droites
    pub fn defocalisation_franges_droites(&self) -> f64 {
        -6.0 * 5f64.sqrt() * self.spherique_theorique_ondes(self.conique)
    }

    /// Astigmatisme de Bath crête-à-creux attendu, s²D²/(16R³), en ondes
    pub fn astigmatisme_bath_theorique_ondes(&self) -> f64 {
        self.ecart_mm.powi(2) * self.diametre_mm.powi(2) / (16.0 * self.rayon_courbure_mm.powi(3)) / self.lambda_mm()
    }

    /// Coefficient Z11 (ondes) de l'aberration sphérique d'une conique k au centre de courbure,
    /// au premier ordre : −k·D⁴/(384√5·R³) (la parabole, plus plate que la sphère, retarde le bord)
    pub fn spherique_theorique_ondes(&self, k: f64) -> f64 {
        -k * self.diametre_mm.powi(4) / (384.0 * 5f64.sqrt() * self.rayon_courbure_mm.powi(3)) / self.lambda_mm()
    }
}

/// Coordonnées normalisées `(px, py)` du centre du point `(i, j)` d'une grille `taille`²
pub fn coord_grille(i: usize, j: usize, taille: usize) -> (f64, f64) {
    let pas = 2.0 / taille as f64;
    (-1.0 + (i as f64 + 0.5) * pas, -1.0 + (j as f64 + 0.5) * pas)
}

/// Flèche exacte d'une conique de courbure `c` à la distance `r` de l'axe
fn fleche_conique(c: f64, k: f64, r2: f64) -> f64 {
    c * r2 / (1.0 + (1.0 - (1.0 + k) * c * c * r2).max(0.0).sqrt())
}

/// Différence de marche (ondes) entre l'onde test et la référence, sur la grille, piston retiré.
/// `None` hors du miroir et dans l'obstruction.
fn carte_opd(p: &ParametresBath, k: f64, defauts: Option<&DefautsSurface>, reglages: bool) -> Vec<Option<f64>> {
    let n = p.taille.clamp(16, 1024);
    let rayon = p.diametre_mm / 2.0;
    let r_c = p.rayon_courbure_mm;
    let c = 1.0 / r_c;
    let a = p.angle_ecart_deg.to_radians();
    let (demi_x, demi_y) = (0.5 * p.ecart_mm * a.cos(), 0.5 * p.ecart_mm * a.sin());
    let ((tx, ty), dz) = if reglages { (p.decalage_mm(), p.defocalisation_mm()) } else { ((0.0, 0.0), 0.0) };
    let zs = r_c - dz;
    let s = (demi_x + tx, demi_y + ty, zs);
    let q = (-demi_x + tx, -demi_y + ty, zs);
    let rho_min = p.rho_obstruction();
    let lambda = p.lambda_mm();
    // Distance de référence : chemin au sommet, soustrait avant la division (précision)
    let l0 = (s.0 * s.0 + s.1 * s.1 + s.2 * s.2).sqrt() + (q.0 * q.0 + q.1 * q.1 + q.2 * q.2).sqrt();
    let mut carte = vec![None; n * n];
    let mut somme = 0.0;
    let mut nb = 0usize;
    for j in 0..n {
        for i in 0..n {
            let (px, py) = coord_grille(i, j, n);
            let rho = px.hypot(py);
            if rho > 1.0 || rho < rho_min {
                continue;
            }
            let (x, y) = (px * rayon, py * rayon);
            let mut z = fleche_conique(c, k, x * x + y * y);
            if let Some(d) = defauts {
                z += d.ecart_mm(rho, py.atan2(px));
            }
            let d1 = ((s.0 - x).powi(2) + (s.1 - y).powi(2) + (s.2 - z).powi(2)).sqrt();
            let d2 = ((q.0 - x).powi(2) + (q.1 - y).powi(2) + (q.2 - z).powi(2)).sqrt();
            let w = (d1 + d2 - l0) / lambda;
            somme += w;
            nb += 1;
            carte[j * n + i] = Some(w);
        }
    }
    let moyenne = if nb > 0 { somme / nb as f64 } else { 0.0 };
    for v in carte.iter_mut().flatten() {
        *v -= moyenne;
    }
    carte
}

/// Interférogramme simulé
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationBath {
    pub taille: usize,
    /// Différence de marche test − référence (ondes), lignes de py = −1 à +1
    pub opd_ondes: Vec<Option<f64>>,
    /// Éclairement normalisé (0 à 1), 0 hors du miroir
    pub intensite: Vec<f32>,
    /// Déplacement latéral et axial du montage équivalents aux réglages (mm)
    pub decalage_mm: (f64, f64),
    pub defocalisation_mm: f64,
}

/// Simule l'interférogramme du miroir décrit par `p` (conique réelle et défauts)
pub fn simuler_bath(p: &ParametresBath) -> Result<SimulationBath, String> {
    p.valider()?;
    let opd = carte_opd(p, p.conique, Some(&p.defauts), true);
    let intensite = opd
        .iter()
        .map(|w| w.map_or(0.0, |w| (0.5 * (1.0 + (2.0 * std::f64::consts::PI * w).cos())) as f32))
        .collect();
    Ok(SimulationBath {
        taille: p.taille.clamp(16, 1024),
        opd_ondes: opd,
        intensite,
        decalage_mm: p.decalage_mm(),
        defocalisation_mm: p.defocalisation_mm(),
    })
}

/// Résultat de la réduction d'une carte mesurée
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionBath {
    pub taille: usize,
    /// Zernike de la carte mesurée (ondes à la longueur d'onde du test)
    pub zernike_mesure_ondes: Vec<f64>,
    /// Zernike de la carte théorique du miroir parfait de conique visée dans le même montage
    pub zernike_reference_ondes: Vec<f64>,
    /// Erreur du front d'onde par rapport à la conique visée (ondes au λ du test), alignement retiré
    pub residu_ondes: Vec<Option<f64>>,
    pub zernike_residu_ondes: Vec<f64>,
    pub conique_visee: f64,
    pub conique_mesuree: f64,
    /// Écarts du front d'onde au λ du test
    pub pv_ondes: f64,
    pub rms_ondes: f64,
    /// Mêmes écarts ramenés à 550 nm
    pub pv_ondes_550: f64,
    pub rms_ondes_550: f64,
    /// Strehl à 550 nm (Maréchal, exp(−(2πσ)²))
    pub strehl_550: f64,
    /// Écarts de la surface (moitié du front d'onde), en nm
    pub pv_surface_nm: f64,
    pub rms_surface_nm: f64,
    /// Astigmatisme de Bath du montage (amplitude crête-à-creux, ondes)
    pub astigmatisme_bath_ondes: f64,
    /// Sphérique Z11 attendue pour la conique visée (ondes)
    pub spherique_visee_ondes: f64,
    /// Signe retenu pour la carte (−1 si inversée)
    pub signe: f64,
    pub nb_points: usize,
}

/// Crête-à-creux d'un couple de Zernike d'astigmatisme (Z5, Z6) : 2√6·√(a²+b²)
fn pv_astigmatisme(z: &[f64]) -> f64 {
    2.0 * 6f64.sqrt() * z[4].hypot(z[5])
}

fn points_valides(carte: &[Option<f64>], n: usize, signe: f64) -> Vec<(f64, f64, f64)> {
    let mut pts = Vec::new();
    for j in 0..n {
        for i in 0..n {
            if let Some(w) = carte[j * n + i] {
                let (px, py) = coord_grille(i, j, n);
                pts.push((px, py, signe * w));
            }
        }
    }
    pts
}

/// Sous-échantillonne les points pour l'ajustement de Zernike (au plus ~20 000 points)
fn eclaircir(pts: &[(f64, f64, f64)]) -> Vec<(f64, f64, f64)> {
    let pas = (pts.len() / 20_000).max(1);
    pts.iter().step_by(pas).copied().collect()
}

/// Réduit une carte mesurée (ondes au λ du test, grille `p.taille`², py croissant) : comparaison
/// à la conique visée, conique mesurée, écarts et Strehl
pub fn reduire_carte(carte: &[Option<f64>], p: &ParametresBath, signe: SigneCarte) -> Result<ReductionBath, String> {
    p.valider()?;
    let n = p.taille.clamp(16, 1024);
    if carte.len() != n * n {
        return Err(tr!(
            "taille de la carte incohérente",
            "inconsistent map size",
            "inkonsistente Kartengröße"
        )
        .to_string());
    }
    // Cartes théoriques restreintes à la zone mesurée (bord exclu, trous…) : les coefficients de
    // Zernike ne se comparent que sur le même domaine
    let sur_zone = |c: Vec<Option<f64>>| -> Vec<Option<f64>> { c.into_iter().zip(carte).map(|(r, m)| m.and(r)).collect() };
    let reference = sur_zone(carte_opd(p, p.conique_visee, None, false));
    let pts_ref = points_valides(&reference, n, 1.0);
    let (z_ref, _) = ajuster_zernike(&eclaircir(&pts_ref))?;
    // Variation de Z11 avec la conique (différence centrée), pour la conique mesurée
    let dk = 0.05;
    let z11 = |k: f64| -> Result<f64, String> {
        let c = sur_zone(carte_opd(p, k, None, false));
        Ok(ajuster_zernike(&eclaircir(&points_valides(&c, n, 1.0)))?.0[10])
    };
    let pente = (z11(p.conique_visee + dk)? - z11(p.conique_visee - dk)?) / (2.0 * dk);

    let reduire = |s: f64| -> Result<ReductionBath, String> {
        let mut pts = Vec::new();
        let mut index = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let k = j * n + i;
                if let (Some(w), Some(r)) = (carte[k], reference[k]) {
                    let (px, py) = coord_grille(i, j, n);
                    pts.push((px, py, s * w - r));
                    index.push(k);
                }
            }
        }
        let mesure: Vec<_> = index
            .iter()
            .zip(&pts)
            .map(|(&k, &(px, py, _))| (px, py, s * carte[k].unwrap_or(0.0)))
            .collect();
        let (z_mes, _) = ajuster_zernike(&eclaircir(&mesure))?;
        let (z_ecart, _) = ajuster_zernike(&eclaircir(&pts))?;
        // Termes d'alignement retirés : piston, basculements, défocalisation (et astigmatisme)
        let nb_retires = if p.retirer_astigmatisme { 6 } else { 4 };
        let mut residu = vec![None; n * n];
        let mut valeurs = Vec::with_capacity(pts.len());
        for (&k, &(px, py, w)) in index.iter().zip(&pts) {
            let (rho, theta) = (px.hypot(py), py.atan2(px));
            let alignement: f64 = (1..=nb_retires).map(|j| z_ecart[j - 1] * zernike(j, rho, theta)).sum();
            let v = w - alignement;
            residu[k] = Some(v);
            valeurs.push(v);
        }
        let nb = valeurs.len().max(1) as f64;
        let moyenne = valeurs.iter().sum::<f64>() / nb;
        for v in residu.iter_mut().flatten() {
            *v -= moyenne;
        }
        let rms = (valeurs.iter().map(|v| (v - moyenne).powi(2)).sum::<f64>() / nb).sqrt();
        let (mini, maxi) = valeurs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        let pv = maxi - mini;
        let mut z_res = z_ecart.clone();
        for c in z_res.iter_mut().take(nb_retires) {
            *c = 0.0;
        }
        let vers_550 = p.longueur_onde_nm / LAMBDA_VISUELLE_NM;
        let conique_mesuree = if pente.abs() > 1e-12 {
            p.conique_visee + (z_mes[10] - z_ref[10]) / pente
        } else {
            f64::NAN
        };
        Ok(ReductionBath {
            taille: n,
            zernike_mesure_ondes: z_mes,
            zernike_reference_ondes: z_ref.clone(),
            residu_ondes: residu,
            zernike_residu_ondes: z_res,
            conique_visee: p.conique_visee,
            conique_mesuree,
            pv_ondes: pv,
            rms_ondes: rms,
            pv_ondes_550: pv * vers_550,
            rms_ondes_550: rms * vers_550,
            strehl_550: (-(2.0 * std::f64::consts::PI * rms * vers_550).powi(2)).exp(),
            pv_surface_nm: pv * p.longueur_onde_nm / 2.0,
            rms_surface_nm: rms * p.longueur_onde_nm / 2.0,
            astigmatisme_bath_ondes: pv_astigmatisme(&z_ref),
            spherique_visee_ondes: z_ref[10],
            signe: s,
            nb_points: valeurs.len(),
        })
    };
    match signe {
        SigneCarte::Normal => reduire(1.0),
        SigneCarte::Inverse => reduire(-1.0),
        SigneCarte::Auto => {
            let a = reduire(1.0)?;
            let b = reduire(-1.0)?;
            Ok(if b.rms_ondes < a.rms_ondes { b } else { a })
        }
    }
}

/// Simulation puis réduction de la carte simulée (le résultat qu'on obtiendrait sans bruit)
pub fn simuler_et_reduire(p: &ParametresBath) -> Result<(SimulationBath, ReductionBath), String> {
    let sim = simuler_bath(p)?;
    let red = reduire_carte(&sim.opd_ondes, p, SigneCarte::Normal)?;
    Ok((sim, red))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pv(carte: &[Option<f64>]) -> f64 {
        let (a, b) = carte.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
        b - a
    }

    #[test]
    fn sphere_sans_ecart_teinte_plate() {
        let p = ParametresBath {
            conique: 0.0,
            conique_visee: 0.0,
            ecart_mm: 0.0,
            basculement_x_franges: 0.0,
            defocalisation_franges: 0.0,
            ..Default::default()
        };
        let sim = simuler_bath(&p).unwrap();
        assert!(pv(&sim.opd_ondes) < 1e-6, "PV = {}", pv(&sim.opd_ondes));
    }

    #[test]
    fn reglages_en_franges() {
        // 10 franges de basculement : 10 ondes d'un bord à l'autre selon X
        let p = ParametresBath { conique: 0.0, ecart_mm: 0.0, basculement_x_franges: 10.0, defocalisation_franges: 0.0, ..Default::default() };
        let sim = simuler_bath(&p).unwrap();
        let n = sim.taille;
        let w = |i: usize| sim.opd_ondes[(n / 2) * n + i].unwrap();
        let pente = (w(n - 1) - w(0)) / (coord_grille(n - 1, 0, n).0 - coord_grille(0, 0, n).0);
        assert!((pente * 2.0 - 10.0).abs() < 0.01, "pente = {pente}");

        // 3 franges de défocalisation du centre au bord
        let p = ParametresBath { conique: 0.0, ecart_mm: 0.0, basculement_x_franges: 0.0, defocalisation_franges: 3.0, ..Default::default() };
        let sim = simuler_bath(&p).unwrap();
        let (z, _) = ajuster_zernike(&points_valides(&sim.opd_ondes, sim.taille, 1.0)).unwrap();
        // Défocalisation √3(2ρ²−1) : centre → bord = 2√3·Z4
        assert!((2.0 * 3f64.sqrt() * z[3] - 3.0).abs() < 0.02, "Z4 = {}", z[3]);
    }

    #[test]
    fn astigmatisme_de_bath_et_spherique_de_la_parabole() {
        // Miroir rapide (300 mm F/3) et grand écart : astigmatisme bien visible
        let p = ParametresBath {
            diametre_mm: 300.0,
            rayon_courbure_mm: 1800.0,
            conique: 0.0,
            conique_visee: 0.0,
            ecart_mm: 10.0,
            basculement_x_franges: 0.0,
            defocalisation_franges: 0.0,
            ..Default::default()
        };
        let red = reduire_carte(&simuler_bath(&p).unwrap().opd_ondes, &p, SigneCarte::Normal).unwrap();
        let theorie = p.astigmatisme_bath_theorique_ondes();
        assert!((red.astigmatisme_bath_ondes / theorie - 1.0).abs() < 0.02, "{} / {}", red.astigmatisme_bath_ondes, theorie);
        // Le miroir parfait est retrouvé parfait, astigmatisme de Bath déduit
        assert!(red.rms_ondes < 1e-4, "RMS = {}", red.rms_ondes);

        // Parabole 200 F/6 au centre de courbure : Z11 ≈ −k·D⁴/(384√5·R³)
        let p = ParametresBath { basculement_x_franges: 0.0, ..Default::default() };
        let red = reduire_carte(&simuler_bath(&p).unwrap().opd_ondes, &p, SigneCarte::Normal).unwrap();
        let attendu = p.spherique_theorique_ondes(-1.0);
        assert!((red.zernike_mesure_ondes[10] / attendu - 1.0).abs() < 0.01, "{} / {}", red.zernike_mesure_ondes[10], attendu);
        assert!((red.conique_mesuree + 1.0).abs() < 1e-3);
    }

    #[test]
    fn reduction_retrouve_conique_et_defauts() {
        // Miroir sous-corrigé (k = −0,8) avec une zone : la réduction le compare à la parabole
        let mut p = ParametresBath {
            conique: -0.8,
            basculement_y_franges: 4.0,
            defocalisation_franges: 2.0,
            ..Default::default()
        };
        let red = simuler_et_reduire(&p).unwrap().1;
        assert!((red.conique_mesuree + 0.8).abs() < 0.005, "k = {}", red.conique_mesuree);
        // Sphérique résiduelle = différence des sphériques des deux coniques (sous-correction : Z11 < 0)
        let z11 = red.zernike_residu_ondes[10];
        let attendu = p.spherique_theorique_ondes(-0.8) - p.spherique_theorique_ondes(-1.0);
        assert!(z11 < 0.0 && (z11 - attendu).abs() < 0.003, "Z11 = {z11} / {attendu}");

        // Astigmatisme de surface 20 nm RMS → 40 nm de front d'onde
        p.conique = -1.0;
        p.defauts.zernike_nm = vec![(6, 20.0)];
        let red = simuler_et_reduire(&p).unwrap().1;
        let attendu = -40.0 / p.longueur_onde_nm; // bosse = chemin raccourci
        assert!((red.zernike_residu_ondes[5] - attendu).abs() < 0.003, "Z6 = {}", red.zernike_residu_ondes[5]);
        assert!((red.rms_surface_nm - 20.0).abs() < 0.5, "RMS surface = {}", red.rms_surface_nm);
        p.retirer_astigmatisme = true;
        assert!(simuler_et_reduire(&p).unwrap().1.rms_surface_nm < 0.2);
    }

    #[test]
    fn reduction_sur_une_zone_partielle() {
        // Bord exclu et trou central : la parabole parfaite reste parfaite, k = −1
        let p = ParametresBath { diametre_obstruction_mm: 40.0, ..Default::default() };
        let n = p.taille;
        let carte: Vec<_> = simuler_bath(&p)
            .unwrap()
            .opd_ondes
            .into_iter()
            .enumerate()
            .map(|(k, w)| {
                let (px, py) = coord_grille(k % n, k / n, n);
                w.filter(|_| px.hypot(py) < 0.9)
            })
            .collect();
        let red = reduire_carte(&carte, &p, SigneCarte::Normal).unwrap();
        assert!((red.conique_mesuree + 1.0).abs() < 1e-3, "k = {}", red.conique_mesuree);
        assert!(red.rms_ondes < 1e-4);
    }

    #[test]
    fn signe_automatique() {
        let p = ParametresBath { conique: -0.9, ..Default::default() };
        let sim = simuler_bath(&p).unwrap();
        let inverse: Vec<_> = sim.opd_ondes.iter().map(|w| w.map(|w| -w)).collect();
        let red = reduire_carte(&inverse, &p, SigneCarte::Auto).unwrap();
        assert_eq!(red.signe, -1.0);
        assert!((red.conique_mesuree + 0.9).abs() < 0.005);
    }
}
