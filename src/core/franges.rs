//! Analyse d'une photo d'interférogramme (Bath, Fizeau…) par la méthode de Takeda : transformée de
//! Fourier, sélection du lobe de la porteuse (franges de basculement), retour dans l'espace direct
//! et phase, puis déroulement guidé par la qualité.
//!
//! 1. Le disque du miroir est repéré dans la photo (ou donné à la main) et rééchantillonné sur une
//!    grille carrée (moyenne par blocs, sans repliement), l'axe Y vers le haut.
//! 2. Le fond lentement variable (profil gaussien du laser) est retiré par une moyenne locale
//!    normalisée sur une période de frange.
//! 3. Les franges sont prolongées hors du disque par l'algorithme de Gerchberg (filtrage
//!    alterné), ce qui supprime l'essentiel des erreurs de bord de la transformée de Fourier.
//! 4. Seul le lobe +1 est gardé (fenêtre plate à bords en cosinus), ramené à l'origine : la phase
//!    est celle du front d'onde, porteuse retirée, à 2π près.
//! 5. La phase est déroulée en partant du point le mieux modulé et en avançant toujours vers le
//!    voisin de plus forte modulation (les zones bruitées sont traitées en dernier).
//! 6. La phase est affinée : l'image est démodulée par la phase estimée (porteuse comprise) et,
//!    autour de chaque point, fond et franges sont ajustés par moindres carrés pondérés limités au
//!    disque. La correction, lissée sur une frange, rend au bord la phase que le filtre de Fourier
//!    avait lissée (sphérique d'un miroir rapide).
//! 7. Enfin le modèle d'image `fond + modulation·cos(porteuse + 2π·ΣcⱼZⱼ)` est ajusté sur toute
//!    la photo (Gauss-Newton amorti, 37 Zernike pour la phase, 11 pour le fond et la modulation) :
//!    les termes de bas ordre de la carte (sphérique, astigmatisme, coma…) en sortent sans biais de
//!    bord ; les détails plus fins (zones, bord rabattu) restent ceux de la carte locale.
//!
//! Une seule photo ne distingue pas une bosse d'un creux : le signe est choisi ensuite (voir
//! [`crate::core::bath::SigneCarte`]). Il faut assez de franges de basculement (une dizaine)
//! pour que le lobe de la porteuse se sépare du fond.

use crate::core::onde::{NB_ZERNIKE, ajuster_zernike, zernike};
use rustfft::{FftPlanner, num_complex::Complex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::path::Path;

/// Image en niveaux de gris (0 à 1), lignes de haut en bas
#[derive(Debug, Clone)]
pub struct ImageGris {
    pub largeur: usize,
    pub hauteur: usize,
    pub pixels: Vec<f32>,
}

impl ImageGris {
    pub fn charger(chemin: &Path) -> Result<Self, String> {
        let img = image::open(chemin).map_err(|e| {
            trf!(
                "lecture de l'image {} impossible : {e}",
                "cannot read image {}: {e}",
                "Bild {} kann nicht gelesen werden: {e}",
                chemin.display()
            )
        })?;
        let gris = img.to_luma32f();
        Ok(Self {
            largeur: gris.width() as usize,
            hauteur: gris.height() as usize,
            pixels: gris.into_raw(),
        })
    }

    /// Image carrée `taille`² depuis une grille de calcul (lignes de py = −1 à +1)
    pub fn depuis_grille(valeurs: &[f32], taille: usize) -> Self {
        let mut pixels = vec![0.0; taille * taille];
        for j in 0..taille {
            let ligne = taille - 1 - j;
            pixels[ligne * taille..(ligne + 1) * taille].copy_from_slice(&valeurs[j * taille..(j + 1) * taille]);
        }
        Self { largeur: taille, hauteur: taille, pixels }
    }

    pub fn enregistrer_png(&self, chemin: &Path) -> Result<(), String> {
        let octets: Vec<u8> = self.pixels.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
        image::GrayImage::from_raw(self.largeur as u32, self.hauteur as u32, octets)
            .ok_or_else(|| "image vide".to_string())?
            .save(chemin)
            .map_err(|e| {
                trf!(
                    "écriture de {} impossible : {e}",
                    "cannot write {}: {e}",
                    "{} kann nicht geschrieben werden: {e}",
                    chemin.display()
                )
            })
    }

    /// Interpolation bilinéaire (coordonnées en pixels, centre du premier pixel en 0,5)
    fn bilineaire(&self, x: f64, y: f64) -> f32 {
        let (x, y) = (x - 0.5, y - 0.5);
        let x0 = x.floor().clamp(0.0, (self.largeur - 1) as f64);
        let y0 = y.floor().clamp(0.0, (self.hauteur - 1) as f64);
        let (i0, j0) = (x0 as usize, y0 as usize);
        let (i1, j1) = ((i0 + 1).min(self.largeur - 1), (j0 + 1).min(self.hauteur - 1));
        let (fx, fy) = ((x - x0).clamp(0.0, 1.0) as f32, (y - y0).clamp(0.0, 1.0) as f32);
        let p = |i: usize, j: usize| self.pixels[j * self.largeur + i];
        (p(i0, j0) * (1.0 - fx) + p(i1, j0) * fx) * (1.0 - fy) + (p(i0, j1) * (1.0 - fx) + p(i1, j1) * fx) * fy
    }
}

/// Disque du miroir dans la photo (pixels, origine en haut à gauche)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Disque {
    pub cx: f64,
    pub cy: f64,
    pub rayon: f64,
}

/// Flou par moyenne glissante séparable (rayon `r` pixels), bords répliqués
fn flou_boite(v: &[f64], l: usize, h: usize, r: usize) -> Vec<f64> {
    let passe = |src: &[f64], longueur: usize, nb: usize, pas_ligne: usize, pas: usize| -> Vec<f64> {
        let mut dst = vec![0.0; src.len()];
        let norme = 1.0 / (2 * r + 1) as f64;
        for k in 0..nb {
            let base = k * pas_ligne;
            let val = |i: isize| src[base + (i.clamp(0, longueur as isize - 1) as usize) * pas];
            let mut somme: f64 = (-(r as isize)..=r as isize).map(val).sum();
            for i in 0..longueur as isize {
                dst[base + i as usize * pas] = somme * norme;
                somme += val(i + r as isize + 1) - val(i - r as isize);
            }
        }
        dst
    };
    let lignes = passe(v, l, h, l, 1);
    passe(&lignes, h, l, 1, l)
}

/// Seuil d'Otsu d'un ensemble de valeurs dans [0, 1]
fn seuil_otsu(v: &[f64]) -> f64 {
    let mut hist = [0usize; 256];
    for &x in v {
        hist[(x.clamp(0.0, 1.0) * 255.0) as usize] += 1;
    }
    let total = v.len() as f64;
    let somme: f64 = hist.iter().enumerate().map(|(i, &n)| i as f64 * n as f64).sum();
    let (mut poids_b, mut somme_b, mut meilleur, mut seuil) = (0.0, 0.0, 0.0, 128usize);
    for (i, &n) in hist.iter().enumerate() {
        poids_b += n as f64;
        if poids_b == 0.0 {
            continue;
        }
        let poids_f = total - poids_b;
        if poids_f == 0.0 {
            break;
        }
        somme_b += i as f64 * n as f64;
        let (mb, mf) = (somme_b / poids_b, (somme - somme_b) / poids_f);
        let var = poids_b * poids_f * (mb - mf).powi(2);
        if var > meilleur {
            meilleur = var;
            seuil = i;
        }
    }
    (seuil as f64 + 0.5) / 255.0
}

/// Repère le disque éclairé du miroir : réduction, flou de la taille des franges, seuil d'Otsu,
/// plus grande région connexe et son cadre, puis bord affiné rayon par rayon et cercle ajusté
pub fn detecter_disque(img: &ImageGris) -> Option<Disque> {
    if img.largeur < 16 || img.hauteur < 16 {
        return None;
    }
    let facteur = (img.largeur.max(img.hauteur) / 400).max(1);
    let (l, h) = (img.largeur / facteur, img.hauteur / facteur);
    let mut petit = vec![0.0f64; l * h];
    for j in 0..h {
        for i in 0..l {
            let mut s = 0.0;
            for dj in 0..facteur {
                for di in 0..facteur {
                    s += img.pixels[(j * facteur + dj) * img.largeur + i * facteur + di] as f64;
                }
            }
            petit[j * l + i] = s / (facteur * facteur) as f64;
        }
    }
    let r = ((l.min(h) as f32 * 0.025) as usize).max(2);
    let flou = flou_boite(&flou_boite(&petit, l, h, r), l, h, r);
    let (mini, maxi) = flou.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
    if maxi - mini < 1e-4 {
        return None;
    }
    let norme: Vec<f64> = flou.iter().map(|v| (v - mini) / (maxi - mini)).collect();
    let seuil = seuil_otsu(&norme);
    let masque: Vec<bool> = norme.iter().map(|&v| v > seuil).collect();

    // Plus grande région connexe (4-voisinage)
    let mut etiquette = vec![0u32; l * h];
    let mut meilleure = (0usize, 0u32);
    let mut courante = 0u32;
    let mut pile = Vec::new();
    for depart in 0..l * h {
        if !masque[depart] || etiquette[depart] != 0 {
            continue;
        }
        courante += 1;
        let mut taille = 0;
        pile.push(depart);
        etiquette[depart] = courante;
        while let Some(k) = pile.pop() {
            taille += 1;
            let (i, j) = (k % l, k / l);
            let voisins = [(i > 0).then(|| k - 1), (i + 1 < l).then(|| k + 1), (j > 0).then(|| k - l), (j + 1 < h).then(|| k + l)];
            for v in voisins.into_iter().flatten() {
                if masque[v] && etiquette[v] == 0 {
                    etiquette[v] = courante;
                    pile.push(v);
                }
            }
        }
        if taille > meilleure.0 {
            meilleure = (taille, courante);
        }
    }
    if meilleure.0 < 50 {
        return None;
    }
    let (mut x0, mut x1, mut y0, mut y1) = (usize::MAX, 0, usize::MAX, 0);
    for k in 0..l * h {
        if etiquette[k] == meilleure.1 {
            let (i, j) = (k % l, k / l);
            x0 = x0.min(i);
            x1 = x1.max(i);
            y0 = y0.min(j);
            y1 = y1.max(j);
        }
    }
    let mut d = Disque {
        cx: (x0 + x1 + 1) as f64 / 2.0,
        cy: (y0 + y1 + 1) as f64 / 2.0,
        rayon: ((x1 + 1 - x0) + (y1 + 1 - y0)) as f64 / 4.0,
    };
    // Affinage : sur chaque rayon, bord = plus forte descente de l'image floutée (indépendant du
    // niveau, donc de l'éclairage), puis cercle ajusté sur ces points
    let flou_img = ImageGris { largeur: l, hauteur: h, pixels: flou.iter().map(|&v| v as f32).collect() };
    for _ in 0..2 {
        let mut bords = Vec::new();
        let etendue = (4 * r) as f64;
        for a in 0..180 {
            let t = a as f64 * std::f64::consts::PI / 90.0;
            let (ux, uy) = (t.cos(), t.sin());
            let mut meilleur = (0.0, f64::NAN);
            let mut prec = None;
            let mut rho = (d.rayon - etendue).max(1.0);
            while rho <= d.rayon + etendue {
                let (x, y) = (d.cx + rho * ux, d.cy + rho * uy);
                if x < 0.0 || y < 0.0 || x >= l as f64 || y >= h as f64 {
                    break;
                }
                let v = flou_img.bilineaire(x, y) as f64;
                if let Some(p) = prec {
                    let pente = p - v;
                    if pente > meilleur.0 {
                        meilleur = (pente, rho - 0.25);
                    }
                }
                prec = Some(v);
                rho += 0.5;
            }
            if meilleur.1.is_finite() {
                bords.push((d.cx + meilleur.1 * ux, d.cy + meilleur.1 * uy));
            }
        }
        match ajuster_cercle(&bords) {
            Some(c) => d = c,
            None => break,
        }
    }
    let f = facteur as f64;
    let mut d = Disque { cx: d.cx * f, cy: d.cy * f, rayon: d.rayon * f };
    // Affinage final en pleine résolution, près du bord trouvé
    for _ in 0..2 {
        match affiner_bord(img, d, (2.0 * f).max(0.02 * d.rayon).max(6.0)) {
            Some(n) => d = n,
            None => break,
        }
    }
    Some(d)
}

/// Bord du disque en pleine résolution : profil radial moyenné à rayon constant sur 36 secteurs
/// de 10° (les franges s'y moyennent, pas le bord circulaire), plus forte descente à ±`etendue`
/// pixels du cercle `d`, position interpolée par une parabole, puis cercle ajusté
fn affiner_bord(img: &ImageGris, d: Disque, etendue: f64) -> Option<Disque> {
    const SECTEURS: usize = 36;
    let largeur_secteur = 2.0 * std::f64::consts::PI / SECTEURS as f64;
    let nb_angles = ((d.rayon * largeur_secteur).ceil() as usize).clamp(8, 400);
    let debut = (d.rayon - etendue).max(2.0);
    let fin = d.rayon + etendue;
    let pas = 0.5;
    let nb = ((fin - debut) / pas) as usize;
    let mut bords = Vec::new();
    for sect in 0..SECTEURS {
        let t0 = sect as f64 * largeur_secteur;
        let mut valeurs = vec![0.0; nb + 1];
        let mut compte = 0usize;
        for a in 0..nb_angles {
            let t = t0 + (a as f64 + 0.5) / nb_angles as f64 * largeur_secteur;
            let (ux, uy) = (t.cos(), t.sin());
            let (xf, yf) = (d.cx + fin * ux, d.cy + fin * uy);
            if xf < 1.0 || yf < 1.0 || xf > img.largeur as f64 - 1.0 || yf > img.hauteur as f64 - 1.0 {
                continue;
            }
            for (i, v) in valeurs.iter_mut().enumerate() {
                let rho = debut + i as f64 * pas;
                *v += img.bilineaire(d.cx + rho * ux, d.cy + rho * uy) as f64;
            }
            compte += 1;
        }
        if compte < nb_angles / 2 {
            continue;
        }
        let pentes: Vec<f64> = valeurs.windows(2).map(|w| w[0] - w[1]).collect();
        let Some((i, _)) = pentes.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)) else { continue };
        let mut decal = 0.0;
        if i > 0 && i + 1 < pentes.len() {
            let (g0, g1, g2) = (pentes[i - 1], pentes[i], pentes[i + 1]);
            let den = g0 - 2.0 * g1 + g2;
            if den.abs() > 1e-12 {
                decal = (0.5 * (g0 - g2) / den).clamp(-1.0, 1.0);
            }
        }
        let rho = debut + (i as f64 + 0.5 + decal) * pas;
        let t = t0 + 0.5 * largeur_secteur;
        bords.push((d.cx + rho * t.cos(), d.cy + rho * t.sin()));
    }
    ajuster_cercle(&bords)
}

/// Cercle des moindres carrés (méthode de Kåsa) avec un rejet des points aberrants
fn ajuster_cercle(points: &[(f64, f64)]) -> Option<Disque> {
    let ajuste = |pts: &[(f64, f64)]| -> Option<Disque> {
        if pts.len() < 8 {
            return None;
        }
        let a = nalgebra::DMatrix::from_fn(pts.len(), 3, |i, j| match j {
            0 => pts[i].0,
            1 => pts[i].1,
            _ => 1.0,
        });
        let b = nalgebra::DVector::from_iterator(pts.len(), pts.iter().map(|p| p.0 * p.0 + p.1 * p.1));
        let x = a.svd(true, true).solve(&b, 1e-12).ok()?;
        let (cx, cy) = (x[0] / 2.0, x[1] / 2.0);
        let r2 = x[2] + cx * cx + cy * cy;
        (r2 > 0.0).then(|| Disque { cx, cy, rayon: r2.sqrt() })
    };
    let d = ajuste(points)?;
    let mut ecarts: Vec<f64> = points.iter().map(|p| ((p.0 - d.cx).hypot(p.1 - d.cy) - d.rayon).abs()).collect();
    ecarts.sort_by(f64::total_cmp);
    let seuil = (3.0 * ecarts[ecarts.len() / 2]).max(1.0);
    let gardes: Vec<_> = points.iter().copied().filter(|p| ((p.0 - d.cx).hypot(p.1 - d.cy) - d.rayon).abs() <= seuil).collect();
    ajuste(&gardes).or(Some(d))
}

/// Réglages de l'extraction de phase
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptionsFranges {
    pub disque: Disque,
    /// Couronne exclue au bord du disque, en % du rayon (diffraction du bord)
    pub marge_pct: f64,
    /// Rayon relatif de l'obstruction centrale exclue (0 à 1)
    pub rho_obstruction: f64,
    /// Points sur le diamètre de la carte produite
    pub taille: usize,
}

/// Carte de phase extraite d'une photo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarteFranges {
    pub taille: usize,
    /// Front d'onde (ondes), lignes de py = −1 à +1, `None` hors de la zone mesurée
    pub opd_ondes: Vec<Option<f64>>,
    /// Porteuse : franges sur le diamètre selon X et Y
    pub porteuse_franges: (f64, f64),
    /// Contraste moyen des franges (0 à 1)
    pub contraste: f64,
}

/// Transformée de Fourier 2D en place d'une grille `m`²
fn fft2(donnees: &mut [Complex<f64>], m: usize, inverse: bool) {
    let mut planif = FftPlanner::new();
    let fft = if inverse { planif.plan_fft_inverse(m) } else { planif.plan_fft_forward(m) };
    for ligne in donnees.chunks_mut(m) {
        fft.process(ligne);
    }
    let mut colonne = vec![Complex::new(0.0, 0.0); m];
    for i in 0..m {
        for j in 0..m {
            colonne[j] = donnees[j * m + i];
        }
        fft.process(&mut colonne);
        for j in 0..m {
            donnees[j * m + i] = colonne[j];
        }
    }
    if inverse {
        let n = (m * m) as f64;
        for v in donnees.iter_mut() {
            *v /= n;
        }
    }
}

/// Fréquence signée de l'indice `k` d'une FFT de longueur `m`
fn freq(k: usize, m: usize) -> f64 {
    if k < m / 2 { k as f64 } else { k as f64 - m as f64 }
}

/// Fenêtre plate à bords en cosinus : 1 jusqu'à 70 % du rayon, 0 au-delà du rayon
fn fenetre(d: f64, rayon: f64) -> f64 {
    let u = d / rayon;
    if u <= 0.7 {
        1.0
    } else if u >= 1.0 {
        0.0
    } else {
        0.5 * (1.0 + (std::f64::consts::PI * (u - 0.7) / 0.3).cos())
    }
}

#[derive(PartialEq)]
struct Candidat(f32, usize);
impl Eq for Candidat {}
impl PartialOrd for Candidat {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Candidat {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0).then(o.1.cmp(&self.1))
    }
}

/// Nombre de passes de l'affinement de la phase
const ITERATIONS_AFFINEMENT: usize = 12;

/// Une passe d'affinement : autour de chaque point, ajustement pondéré (fenêtre d'environ une
/// frange, limitée au masque) de `g ≈ a + p·cos Φ + q·sin Φ` ; la correction de phase est
/// `atan2(−q, p)`. Corrige le lissage de la phase par le filtre de Fourier là où elle varie vite.
fn affiner_phase(g: &[f64], masque: &[bool], n: usize, periode: usize, phi: &mut [f64]) {
    let r = (periode / 2).max(2);
    let lisser = |v: Vec<f64>| flou_boite(&flou_boite(&v, n, n, r), n, n, r);
    let carte = |f: &dyn Fn(usize) -> f64| -> Vec<f64> { lisser((0..n * n).map(|k| if masque[k] { f(k) } else { 0.0 }).collect()) };
    let c: Vec<f64> = phi.iter().map(|p| p.cos()).collect();
    let s: Vec<f64> = phi.iter().map(|p| p.sin()).collect();
    let s1 = carte(&|_| 1.0);
    let sc = carte(&|k| c[k]);
    let ss = carte(&|k| s[k]);
    let scc = carte(&|k| c[k] * c[k]);
    let sss = carte(&|k| s[k] * s[k]);
    let scs = carte(&|k| c[k] * s[k]);
    let sg = carte(&|k| g[k]);
    let sgc = carte(&|k| g[k] * c[k]);
    let sgs = carte(&|k| g[k] * s[k]);
    let mut delta = vec![0.0; n * n];
    let mut poids = vec![0.0; n * n];
    for k in 0..n * n {
        // Fenêtre trop peu remplie (bord) : phase gardée
        if !masque[k] || s1[k] < 0.35 {
            continue;
        }
        let a = nalgebra::Matrix3::new(s1[k], sc[k], ss[k], sc[k], scc[k], scs[k], ss[k], scs[k], sss[k]);
        if a.determinant().abs() < 1e-3 * s1[k].powi(3) {
            continue;
        }
        let b = nalgebra::Vector3::new(sg[k], sgc[k], sgs[k]);
        if let Some(x) = a.lu().solve(&b) {
            let d = (-x[2]).atan2(x[1]);
            if d.is_finite() {
                delta[k] = d;
                poids[k] = 1.0;
            }
        }
    }
    // Correction lissée sur une frange (normalisée au masque) : seules les variations lentes sont
    // corrigées, les ondulations au pas des franges ne peuvent pas s'amplifier
    let r_lisse = (periode / 2).max(1);
    let lisser_d = |v: &[f64]| flou_boite(&flou_boite(v, n, n, r_lisse), n, n, r_lisse);
    let dl = lisser_d(&delta);
    let pl = lisser_d(&poids);
    for k in 0..n * n {
        if masque[k] && pl[k] > 1e-6 {
            phi[k] += dl[k] / pl[k];
        }
    }
}

/// Distance (en pas de grille, 4-voisinage) de chaque point du masque au point hors masque le
/// plus proche
fn distance_au_bord(masque: &[bool], n: usize) -> Vec<u32> {
    let mut d = vec![u32::MAX; n * n];
    let mut file = std::collections::VecDeque::new();
    for k in 0..n * n {
        let (i, j) = (k % n, k / n);
        let au_bord = i == 0 || j == 0 || i + 1 == n || j + 1 == n;
        if !masque[k] || au_bord {
            d[k] = 0;
            file.push_back(k);
        }
    }
    while let Some(k) = file.pop_front() {
        let (i, j) = (k % n, k / n);
        for v in [(i > 0).then(|| k - 1), (i + 1 < n).then(|| k + 1), (j > 0).then(|| k - n), (j + 1 < n).then(|| k + n)]
            .into_iter()
            .flatten()
        {
            if d[v] == u32::MAX {
                d[v] = d[k] + 1;
                file.push_back(v);
            }
        }
    }
    d
}

/// Termes de Zernike du fond et de la modulation dans le modèle d'image (jusqu'à la sphérique)
const TERMES_ECLAIRAGE: usize = 11;

/// Ajustement global par Gauss-Newton amorti de `g = a + b·cos(φ₀ + 2π·Σ cⱼZⱼ)` sur les points
/// du disque (`(indice, ρ, θ)`), `φ₀` = phase fixe de chaque point (porteuse et détail fin de la
/// carte locale), `a` et `b` développés sur les premiers Zernike. Part des coefficients `c0`
/// (ondes) et retourne les coefficients ajustés, ou `None` en cas d'échec.
fn ajustement_global(
    g: &[f64],
    points: &[(usize, f64, f64)],
    phase_fixe: &[f64],
    c0: &[f64],
) -> Option<Vec<f64>> {
    use nalgebra::{DMatrix, DVector};
    let ne = TERMES_ECLAIRAGE;
    let np = NB_ZERNIKE + 2 * ne;
    let deux_pi = 2.0 * std::f64::consts::PI;
    // Valeurs des Zernike en chaque point, calculées une fois
    let z: Vec<Vec<f64>> = points.iter().map(|&(_, r, t)| (1..=NB_ZERNIKE).map(|j| zernike(j, r, t)).collect()).collect();
    let phase = |c: &[f64]| -> Vec<f64> {
        phase_fixe
            .iter()
            .zip(&z)
            .map(|(f, zk)| f + deux_pi * zk.iter().zip(c).map(|(a, b)| a * b).sum::<f64>())
            .collect()
    };
    // Fond et modulation initiaux : moindres carrés linéaires à phase fixée
    let mut c: Vec<f64> = c0.to_vec();
    let phi = phase(&c);
    let a_lin = DMatrix::from_fn(points.len(), 2 * ne, |r, col| if col < ne { z[r][col] } else { z[r][col - ne] * phi[r].cos() });
    let b_lin = DVector::from_iterator(points.len(), points.iter().map(|&(k, _, _)| g[k]));
    let ab = a_lin.svd(true, true).solve(&b_lin, 1e-12).ok()?;
    let mut alpha: Vec<f64> = ab.iter().take(ne).copied().collect();
    let mut beta: Vec<f64> = ab.iter().skip(ne).copied().collect();

    let cout = |c: &[f64], alpha: &[f64], beta: &[f64]| -> f64 {
        let phi = phase(c);
        points
            .iter()
            .zip(&z)
            .zip(&phi)
            .map(|((&(k, _, _), zk), p)| {
                let a: f64 = zk.iter().zip(alpha).map(|(x, y)| x * y).sum();
                let b: f64 = zk.iter().zip(beta).map(|(x, y)| x * y).sum();
                (g[k] - a - b * p.cos()).powi(2)
            })
            .sum()
    };
    let mut lambda = 1e-3;
    let mut e = cout(&c, &alpha, &beta);
    for _ in 0..30 {
        let phi = phase(&c);
        let mut jtj = DMatrix::<f64>::zeros(np, np);
        let mut jtr = DVector::<f64>::zeros(np);
        let mut ligne = vec![0.0; np];
        for ((&(k, _, _), zk), p) in points.iter().zip(&z).zip(&phi) {
            let (cp, sp) = (p.cos(), p.sin());
            let a: f64 = zk.iter().zip(&alpha).map(|(x, y)| x * y).sum();
            let b: f64 = zk.iter().zip(&beta).map(|(x, y)| x * y).sum();
            let r = g[k] - a - b * cp;
            for j in 0..NB_ZERNIKE {
                ligne[j] = -b * sp * deux_pi * zk[j];
            }
            for j in 0..ne {
                ligne[NB_ZERNIKE + j] = zk[j];
                ligne[NB_ZERNIKE + ne + j] = zk[j] * cp;
            }
            for u in 0..np {
                jtr[u] += ligne[u] * r;
                for v in u..np {
                    jtj[(u, v)] += ligne[u] * ligne[v];
                }
            }
        }
        for u in 0..np {
            for v in 0..u {
                jtj[(u, v)] = jtj[(v, u)];
            }
        }
        let mut ameliore = false;
        for _ in 0..8 {
            let mut m = jtj.clone();
            for u in 0..np {
                m[(u, u)] += lambda * jtj[(u, u)].max(1e-12);
            }
            let Some(pas) = m.cholesky().map(|ch| ch.solve(&jtr)) else {
                lambda *= 10.0;
                continue;
            };
            let c2: Vec<f64> = (0..NB_ZERNIKE).map(|j| c[j] + pas[j]).collect();
            let a2: Vec<f64> = (0..ne).map(|j| alpha[j] + pas[NB_ZERNIKE + j]).collect();
            let b2: Vec<f64> = (0..ne).map(|j| beta[j] + pas[NB_ZERNIKE + ne + j]).collect();
            let e2 = cout(&c2, &a2, &b2);
            if e2 < e {
                let gain = (e - e2) / e.max(1e-300);
                c = c2;
                alpha = a2;
                beta = b2;
                e = e2;
                lambda = (lambda * 0.3).max(1e-9);
                ameliore = gain > 1e-10;
                break;
            }
            lambda *= 10.0;
        }
        if !ameliore {
            break;
        }
    }
    // Signe : la phase de l'image est celle du lobe +1, comme la carte locale
    c.iter().all(|v| v.is_finite()).then_some(c)
}

/// Extrait le front d'onde (ondes, au signe près) d'une photo d'interférogramme
pub fn extraire_phase(img: &ImageGris, opt: &OptionsFranges) -> Result<CarteFranges, String> {
    let d = opt.disque;
    if !(d.rayon >= 8.0) {
        return Err(tr!("disque du miroir trop petit", "mirror disc too small", "Spiegelscheibe zu klein").to_string());
    }
    let n = opt.taille.clamp(32, 512);
    let m = 2 * n;
    let decalage = (m - n) / 2;
    let rho_max = 1.0 - opt.marge_pct.clamp(0.0, 30.0) / 100.0;
    let rho_min = opt.rho_obstruction.clamp(0.0, 0.9);

    // 1. Rééchantillonnage par blocs sur la grille (py vers le haut)
    let sous = ((2.0 * d.rayon / n as f64).ceil() as usize).clamp(1, 16);
    let mut grille = vec![0.0f64; n * n];
    let mut masque = vec![false; n * n];
    let pas = 2.0 / n as f64;
    for j in 0..n {
        for i in 0..n {
            let (px, py) = (-1.0 + (i as f64 + 0.5) * pas, -1.0 + (j as f64 + 0.5) * pas);
            let rho = px.hypot(py);
            if rho > rho_max || rho < rho_min {
                continue;
            }
            let mut s = 0.0;
            for b in 0..sous {
                for a in 0..sous {
                    let qx = px + ((a as f64 + 0.5) / sous as f64 - 0.5) * pas;
                    let qy = py + ((b as f64 + 0.5) / sous as f64 - 0.5) * pas;
                    s += img.bilineaire(d.cx + qx * d.rayon, d.cy - qy * d.rayon) as f64;
                }
            }
            grille[j * n + i] = s / (sous * sous) as f64;
            masque[j * n + i] = true;
        }
    }
    let nb = masque.iter().filter(|&&b| b).count();
    if nb < 200 {
        return Err(tr!("zone mesurée trop petite", "measured area too small", "Messbereich zu klein").to_string());
    }
    let moyenne = grille.iter().zip(&masque).filter(|v| *v.1).map(|v| v.0).sum::<f64>() / nb as f64;

    let plonger = |valeurs: &[f64]| -> Vec<Complex<f64>> {
        let mut c = vec![Complex::new(0.0, 0.0); m * m];
        for j in 0..n {
            for i in 0..n {
                if masque[j * n + i] {
                    c[(j + decalage) * m + i + decalage] = Complex::new(valeurs[j * n + i], 0.0);
                }
            }
        }
        c
    };

    // Recherche de la porteuse (demi-plan fx > 0, hors du fond)
    let centre: Vec<f64> = grille.iter().zip(&masque).map(|(&v, &k)| if k { v - moyenne } else { 0.0 }).collect();
    let mut spectre = plonger(&centre);
    fft2(&mut spectre, m, false);
    let bins_par_frange = (m / n) as f64;
    let exclusion = 2.5 * bins_par_frange;
    let mut pic = (0.0, 0usize, 0usize);
    for ky in 0..m {
        for kx in 0..m {
            let (fx, fy) = (freq(kx, m), freq(ky, m));
            if fx < 0.0 || (fx == 0.0 && fy <= 0.0) || fx.hypot(fy) < exclusion || fx.hypot(fy) > m as f64 / 4.0 {
                continue;
            }
            let a = spectre[ky * m + kx].norm_sqr();
            if a > pic.0 {
                pic = (a, kx, ky);
            }
        }
    }
    if pic.0 == 0.0 {
        return Err(tr!(
            "aucune porteuse trouvée : il faut des franges de basculement",
            "no carrier found: tilt fringes are needed",
            "kein Träger gefunden: Kippstreifen sind nötig"
        )
        .to_string());
    }
    // Centroïde du pic (sous-pixel)
    let (mut sx, mut sy, mut sp) = (0.0, 0.0, 0.0);
    for dy in -2i64..=2 {
        for dx in -2i64..=2 {
            let kx = (pic.1 as i64 + dx).rem_euclid(m as i64) as usize;
            let ky = (pic.2 as i64 + dy).rem_euclid(m as i64) as usize;
            let a = spectre[ky * m + kx].norm_sqr();
            sx += a * (freq(pic.1, m) + dx as f64);
            sy += a * (freq(pic.2, m) + dy as f64);
            sp += a;
        }
    }
    let f0 = (sx / sp, sy / sp);
    let module_f0 = f0.0.hypot(f0.1);
    if module_f0 < 3.0 * bins_par_frange {
        return Err(tr!(
            "trop peu de franges de basculement (au moins 4 à 5 sur le diamètre, idéalement une dizaine)",
            "too few tilt fringes (at least 4 to 5 across the diameter, ideally about ten)",
            "zu wenige Kippstreifen (mindestens 4 bis 5 über den Durchmesser, besser etwa zehn)"
        )
        .to_string());
    }

    // 2. Retrait du fond : moyenne locale normalisée sur une période de frange
    let periode = (m as f64 / module_f0).round().max(3.0) as usize;
    let r_flou = periode / 2 + 1;
    let valeurs: Vec<f64> = grille.iter().zip(&masque).map(|(&v, &k)| if k { v } else { 0.0 }).collect();
    let poids: Vec<f64> = masque.iter().map(|&k| if k { 1.0 } else { 0.0 }).collect();
    let flou_v = flou_boite(&flou_boite(&valeurs, n, n, r_flou), n, n, r_flou);
    let flou_p = flou_boite(&flou_boite(&poids, n, n, r_flou), n, n, r_flou);
    let donnees: Vec<f64> = (0..n * n)
        .map(|k| if masque[k] && flou_p[k] > 1e-3 { grille[k] - flou_v[k] / flou_p[k] } else { 0.0 })
        .collect();

    // 3. Extrapolation de Gerchberg : lobes ±1 seulement, données gardées dans le disque
    let rayon_filtre = (module_f0 - 2.0 * bins_par_frange).max(0.55 * module_f0);
    let filtre_double: Vec<f64> = (0..m * m)
        .map(|k| {
            let (fx, fy) = (freq(k % m, m), freq(k / m, m));
            let a = fenetre((fx - f0.0).hypot(fy - f0.1), rayon_filtre);
            let b = fenetre((fx + f0.0).hypot(fy + f0.1), rayon_filtre);
            a.max(b)
        })
        .collect();
    let mut champ = plonger(&donnees);
    for _ in 0..25 {
        let mut f = champ.clone();
        fft2(&mut f, m, false);
        for (v, w) in f.iter_mut().zip(&filtre_double) {
            *v *= w;
        }
        fft2(&mut f, m, true);
        for j in 0..m {
            for i in 0..m {
                let dedans = j >= decalage && j < decalage + n && i >= decalage && i < decalage + n && masque[(j - decalage) * n + i - decalage];
                if !dedans {
                    champ[j * m + i] = Complex::new(f[j * m + i].re, 0.0);
                }
            }
        }
    }

    // 4. Lobe +1 ramené à l'origine
    fft2(&mut champ, m, false);
    let (kx0, ky0) = (f0.0.round() as i64, f0.1.round() as i64);
    let mut lobe = vec![Complex::new(0.0, 0.0); m * m];
    for ky in 0..m {
        for kx in 0..m {
            let (fx, fy) = (freq(kx, m), freq(ky, m));
            let w = fenetre((fx - f0.0).hypot(fy - f0.1), rayon_filtre);
            if w > 0.0 {
                let nx = (kx as i64 - kx0).rem_euclid(m as i64) as usize;
                let ny = (ky as i64 - ky0).rem_euclid(m as i64) as usize;
                lobe[ny * m + nx] = champ[ky * m + kx] * w;
            }
        }
    }
    fft2(&mut lobe, m, true);
    let mut phase = vec![0.0f64; n * n];
    let mut qualite = vec![0.0f32; n * n];
    let mut somme_mod = 0.0;
    let mut somme_fond = 0.0;
    for j in 0..n {
        for i in 0..n {
            let k = j * n + i;
            if masque[k] {
                let c = lobe[(j + decalage) * m + i + decalage];
                phase[k] = c.arg();
                qualite[k] = c.norm() as f32;
                somme_mod += 2.0 * c.norm();
                somme_fond += grille[k];
            }
        }
    }
    let contraste = if somme_fond > 0.0 { (somme_mod / somme_fond).min(1.0) } else { 0.0 };

    // 5. Déroulement guidé par la modulation
    let deux_pi = 2.0 * std::f64::consts::PI;
    let mut deroulee = vec![None; n * n];
    let depart = (0..n * n)
        .filter(|&k| {
            let (px, py) = (-1.0 + (k % n) as f64 * pas, -1.0 + (k / n) as f64 * pas);
            masque[k] && px.hypot(py) < 0.6 * rho_max
        })
        .max_by(|&a, &b| qualite[a].total_cmp(&qualite[b]))
        .or_else(|| (0..n * n).find(|&k| masque[k]))
        .unwrap_or(0);
    deroulee[depart] = Some(phase[depart]);
    let mut file = BinaryHeap::new();
    let pousser = |file: &mut BinaryHeap<Candidat>, deroulee: &[Option<f64>], k: usize| {
        let (i, j) = (k % n, k / n);
        let voisins = [(i > 0).then(|| k - 1), (i + 1 < n).then(|| k + 1), (j > 0).then(|| k - n), (j + 1 < n).then(|| k + n)];
        for v in voisins.into_iter().flatten() {
            if masque[v] && deroulee[v].is_none() {
                file.push(Candidat(qualite[v], v));
            }
        }
    };
    pousser(&mut file, &deroulee, depart);
    while let Some(Candidat(_, k)) = file.pop() {
        if deroulee[k].is_some() {
            continue;
        }
        let (i, j) = (k % n, k / n);
        let voisins = [(i > 0).then(|| k - 1), (i + 1 < n).then(|| k + 1), (j > 0).then(|| k - n), (j + 1 < n).then(|| k + n)];
        let reference = voisins
            .into_iter()
            .flatten()
            .filter_map(|v| deroulee[v].map(|p| (qualite[v], p)))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, p_ref)) = reference {
            let p = phase[k] + deux_pi * ((p_ref - phase[k]) / deux_pi).round();
            deroulee[k] = Some(p);
            pousser(&mut file, &deroulee, k);
        }
    }
    // 6. Affinement : démodulation par la phase estimée (porteuse comprise) et ajustement local
    //    de fond + franges, insensible au bord du disque
    let porteuse = |k: usize| deux_pi * (kx0 as f64 * ((k % n + decalage) as f64) + ky0 as f64 * ((k / n + decalage) as f64)) / m as f64;
    let mut totale: Vec<f64> = (0..n * n).map(|k| deroulee[k].map_or(0.0, |p| p + porteuse(k))).collect();
    let masque_ok: Vec<bool> = (0..n * n).map(|k| masque[k] && deroulee[k].is_some()).collect();
    for _ in 0..ITERATIONS_AFFINEMENT {
        affiner_phase(&grille, &masque_ok, n, periode, &mut totale);
    }
    let mut opd: Vec<Option<f64>> = (0..n * n)
        .map(|k| masque_ok[k].then(|| (totale[k] - porteuse(k)) / deux_pi))
        .collect();

    // 7. Ajustement global du modèle d'image sur les 37 Zernike : remplace les termes de bas ordre
    //    de la carte (sphérique, astigmatisme… sans biais de bord), les détails plus fins restent
    let points: Vec<(usize, f64, f64)> = (0..n * n)
        .filter(|&k| masque_ok[k])
        .map(|k| {
            let (px, py) = (-1.0 + ((k % n) as f64 + 0.5) * pas, -1.0 + ((k / n) as f64 + 0.5) * pas);
            (k, px.hypot(py), py.atan2(px))
        })
        .collect();
    let locale: Vec<(f64, f64, f64)> = points.iter().map(|&(k, r, t)| (r * t.cos(), r * t.sin(), opd[k].unwrap_or(0.0))).collect();
    if let Ok((c_locale, _)) = ajuster_zernike(&locale) {
        // Détail fin de la carte locale (au-delà des 37 termes), gardé dans le modèle d'image ;
        // atténué à moins de deux franges des bords du masque, où la carte locale est moins sûre
        let distance = distance_au_bord(&masque_ok, n);
        let fin: Vec<f64> = points
            .iter()
            .zip(&locale)
            .map(|(&(k, r, t), &(_, _, w))| {
                let poids = (distance[k] as f64 / (2 * periode) as f64).clamp(0.0, 1.0);
                poids * (w - (0..NB_ZERNIKE).map(|j| c_locale[j] * zernike(j + 1, r, t)).sum::<f64>())
            })
            .collect();
        let porteuse_et_fin: Vec<f64> = points.iter().zip(&fin).map(|(&(k, _, _), f)| porteuse(k) + deux_pi * f).collect();
        if let Some(c_global) = ajustement_global(&grille, &points, &porteuse_et_fin, &c_locale) {
        for &(k, r, t) in &points {
            let correction: f64 = (0..NB_ZERNIKE).map(|j| (c_global[j] - c_locale[j]) * zernike(j + 1, r, t)).sum();
            if let Some(w) = opd[k].as_mut() {
                *w += correction;
            }
        }
        }
    }
    let opd_ondes = opd;
    Ok(CarteFranges {
        taille: n,
        opd_ondes,
        porteuse_franges: (f0.0 / bins_par_frange, f0.1 / bins_par_frange),
        contraste,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::bath::{ParametresBath, SigneCarte, reduire_carte};

    /// Photo simulée : interférogramme dans un disque, fond sombre, éclairage gaussien et bruit
    pub(crate) fn photo_simulee(p: &ParametresBath, largeur: usize, hauteur: usize, d: Disque, bruit: f32) -> ImageGris {
        let sim = crate::core::bath::simuler_bath(&ParametresBath { taille: 2 * d.rayon.round() as usize, ..p.clone() }).unwrap();
        let n = sim.taille;
        let mut graine = 12345u64;
        let mut alea = move || {
            graine = graine.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((graine >> 33) as f32 / (1u64 << 31) as f32) - 0.5
        };
        let mut pixels = vec![0.0; largeur * hauteur];
        for y in 0..hauteur {
            for x in 0..largeur {
                let px = (x as f64 + 0.5 - d.cx) / d.rayon;
                let py = (d.cy - (y as f64 + 0.5)) / d.rayon;
                let mut v = 0.04;
                if px.hypot(py) <= 1.0 {
                    let i = (((px + 1.0) / 2.0 * n as f64) as usize).min(n - 1);
                    let j = (((py + 1.0) / 2.0 * n as f64) as usize).min(n - 1);
                    if let Some(w) = sim.opd_ondes[j * n + i] {
                        let eclairage = (-(px * px + py * py) * 0.7).exp();
                        v += (0.75 * eclairage * (0.55 + 0.45 * (2.0 * std::f64::consts::PI * w).cos())) as f32;
                    }
                }
                pixels[y * largeur + x] = (v + bruit * alea()).clamp(0.0, 1.0);
            }
        }
        ImageGris { largeur, hauteur, pixels }
    }

    #[test]
    fn detection_du_disque() {
        let p = ParametresBath::default();
        let d = Disque { cx: 331.0, cy: 247.0, rayon: 190.0 };
        let img = photo_simulee(&p, 640, 480, d, 0.05);
        let t = detecter_disque(&img).unwrap();
        assert!((t.cx - d.cx).abs() < 1.0 && (t.cy - d.cy).abs() < 1.0, "{t:?}");
        assert!((t.rayon - d.rayon).abs() < 1.5, "{t:?}");

        // Grande photo (réduite pour la détection) et chaîne complète sur le disque détecté
        let p = ParametresBath { conique: -0.9, basculement_x_franges: 3.0, basculement_y_franges: 13.0, defocalisation_franges: -2.0, ..Default::default() };
        let d = Disque { cx: 1010.0, cy: 760.0, rayon: 620.0 };
        let img = photo_simulee(&p, 2000, 1500, d, 0.1);
        let t = detecter_disque(&img).unwrap();
        assert!((t.cx - d.cx).abs() < 1.0 && (t.cy - d.cy).abs() < 1.0 && (t.rayon - d.rayon).abs() < 1.0, "{t:?}");
        let carte = extraire_phase(&img, &OptionsFranges { disque: t, marge_pct: 3.0, rho_obstruction: 0.0, taille: p.taille }).unwrap();
        let red = reduire_carte(&carte.opd_ondes, &p, SigneCarte::Auto).unwrap();
        assert!((red.conique_mesuree + 0.9).abs() < 0.01, "k = {}", red.conique_mesuree);
    }

    /// Analyse la photo simulée de `p` et la compare à la réduction exacte sur la même zone ;
    /// retourne (écart de conique, écart de RMS, écart RMS point par point)
    fn comparer_photo(p: &ParametresBath, bruit: f32) -> (f64, f64, f64) {
        let d = Disque { cx: 402.0, cy: 300.0, rayon: 260.0 };
        let img = photo_simulee(p, 800, 600, d, bruit);
        let marge = 3.0;
        let carte = extraire_phase(&img, &OptionsFranges { disque: d, marge_pct: marge, rho_obstruction: p.rho_obstruction(), taille: p.taille }).unwrap();
        let attendu = p.basculement_x_franges.hypot(p.basculement_y_franges);
        assert!((carte.porteuse_franges.0.hypot(carte.porteuse_franges.1) - attendu).abs() < 0.5, "{:?}", carte.porteuse_franges);
        let mesure = reduire_carte(&carte.opd_ondes, p, SigneCarte::Auto).unwrap();
        assert_eq!(mesure.signe, 1.0);

        // Référence : la carte exacte sur la même zone (bord exclu)
        let n = p.taille;
        let exacte: Vec<_> = crate::core::bath::simuler_bath(p)
            .unwrap()
            .opd_ondes
            .iter()
            .enumerate()
            .map(|(k, w)| {
                let (px, py) = crate::core::bath::coord_grille(k % n, k / n, n);
                w.filter(|_| px.hypot(py) <= 1.0 - marge / 100.0)
            })
            .collect();
        let verite = reduire_carte(&exacte, p, SigneCarte::Normal).unwrap();
        let (mut s, mut nb) = (0.0, 0.0);
        for (a, b) in mesure.residu_ondes.iter().zip(&verite.residu_ondes) {
            if let (Some(a), Some(b)) = (a, b) {
                s += (a - b).powi(2);
                nb += 1.0;
            }
        }
        let ecart = (s / nb).sqrt();
        println!(
            "vérité k={:.4} rms={:.4} | photo k={:.4} rms={:.4} | écart carte {:.4}",
            verite.conique_mesuree, verite.rms_ondes, mesure.conique_mesuree, mesure.rms_ondes, ecart
        );
        (mesure.conique_mesuree - verite.conique_mesuree, mesure.rms_ondes - verite.rms_ondes, ecart)
    }

    #[test]
    fn photo_reduite_comme_la_simulation() {
        // Miroir 200 F/6 sous-corrigé (k = −0,85), astigmatisme et zone, 15 franges, mise au
        // point réglée pour des franges aussi droites que possible, bruit de capteur
        let mut p = ParametresBath { conique: -0.85, basculement_x_franges: 12.0, basculement_y_franges: 9.0, defocalisation_franges: -2.0, ..Default::default() };
        p.defauts.zernike_nm = vec![(5, 12.0)];
        p.defauts.zone_rayon_pct = 55.0;
        p.defauts.zone_hauteur_nm = 25.0;
        p.defauts.zone_largeur_pct = 20.0;
        let (dk, drms, ecart) = comparer_photo(&p, 0.08);
        assert!(dk.abs() < 0.008 && drms.abs() < 0.005 && ecart < 0.01, "{dk} {drms} {ecart}");

        // Bord rabattu sur 12 % du rayon, plus fin qu'une frange
        p.defauts.bord_hauteur_nm = -60.0;
        p.defauts.bord_debut_pct = 88.0;
        let (dk, drms, ecart) = comparer_photo(&p, 0.08);
        assert!(dk.abs() < 0.01 && drms.abs() < 0.005 && ecart < 0.015, "{dk} {drms} {ecart}");

        // Miroir obstrué (trou central de 25 %) et franges verticales
        let mut p = ParametresBath { conique: -1.1, basculement_x_franges: 14.0, defocalisation_franges: -2.0, diametre_obstruction_mm: 50.0, ..Default::default() };
        p.ecart_mm = 8.0;
        let (dk, drms, ecart) = comparer_photo(&p, 0.05);
        assert!(dk.abs() < 0.008 && drms.abs() < 0.005 && ecart < 0.013, "{dk} {drms} {ecart}");
    }
}
