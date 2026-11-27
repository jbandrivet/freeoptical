//! Fiche du système en PDF : caractéristiques, qualité d'image, coupe à l'échelle, tableau des
//! surfaces, puis un plan de fabrication coté par lentille et par miroir (présentation inspirée
//! de la norme ISO 10110 : rayons avec CX / CC, épaisseurs au centre et au bord, diamètres,
//! verre, tolérances du système quand il y en a).

use crate::core::analysis::calculer_spot_diagram;
use crate::core::dessin::{self, Element, NatureElement, Sommet};
use crate::core::glass::{LAMBDA_D_NM, MediumKind};
use crate::core::onde::calculer_front_onde;
use crate::core::paraxial::calculer_paraxial;
use crate::core::pdf::{A4, Alignement, Couleur, Document, GRIS, NOIR, Page, Police};
use crate::core::tolerance::TypeTolerance;
use crate::core::types::{ApertureSpec, OpticalSystem, Repere, SurfaceType};
use crate::i18n::{self, Langue};

const MARGE: f64 = 15.0;
const BLEU_FONCE: Couleur = [0.05, 0.18, 0.45];
const VERRE: Couleur = [0.80, 0.89, 0.98];
const FOND_TITRE: Couleur = [0.90, 0.93, 0.97];
const COULEURS_CHAMPS: [Couleur; 4] = [[0.0, 0.35, 0.85], [0.0, 0.6, 0.2], [0.85, 0.1, 0.1], [0.9, 0.5, 0.0]];

/// Nombre écrit avec la virgule décimale en français et en allemand
pub fn nb(v: f64, decimales: usize) -> String {
    if !v.is_finite() {
        return if v.is_nan() { "—".into() } else { tr!("infini", "infinite", "unendlich").into() };
    }
    let s = format!("{:.*}", decimales, v);
    let s = if s.starts_with('-') && s[1..].chars().all(|c| c == '0' || c == '.') { s[1..].to_string() } else { s };
    match i18n::langue() {
        Langue::Fr | Langue::De => s.replace('.', ","),
        Langue::En => s,
    }
}

/// Nombre en notation scientifique (coefficients asphériques)
fn sci(v: f64) -> String {
    let s = format!("{:.6e}", v);
    match i18n::langue() {
        Langue::Fr | Langue::De => s.replace('.', ","),
        Langue::En => s,
    }
}

/// Date du jour AAAA-MM-JJ (temps universel)
fn date_du_jour() -> String {
    let secondes = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    // Algorithme « days from civil » inversé (H. Hinnant)
    let z = secondes.div_euclid(86_400) + 719_468;
    let ere = z.div_euclid(146_097);
    let jde = z - ere * 146_097;
    let ade = (jde - jde / 1460 + jde / 36_524 - jde / 146_096) / 365;
    let jda = jde - (365 * ade + ade / 4 - ade / 100);
    let mp = (5 * jda + 2) / 153;
    let jour = jda - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = ade + ere * 400 + if mois <= 2 { 1 } else { 0 };
    format!("{annee:04}-{mois:02}-{jour:02}")
}

fn titre_section(p: &mut Page, y: f64, texte: &str) -> f64 {
    p.rectangle_plein(MARGE, y - 4.2, A4.0 - 2.0 * MARGE, 5.8, FOND_TITRE);
    p.texte(MARGE + 1.5, y, texte, Police::HelveticaGras, 9.5, Alignement::Gauche, BLEU_FONCE);
    y + 6.5
}

/// Ligne « libellé ........ valeur » dans une colonne
fn ligne_valeur(p: &mut Page, x: f64, largeur: f64, y: f64, libelle: &str, valeur: &str) {
    p.texte(x, y, libelle, Police::Helvetica, 8.5, Alignement::Gauche, NOIR);
    p.texte(x + largeur, y, valeur, Police::HelveticaGras, 8.5, Alignement::Droite, NOIR);
}

fn libelle_champ(sys: &OpticalSystem, c: (f64, f64)) -> String {
    let unite = if sys.objet_fini().is_some() { " mm" } else { "°" };
    if c.0.abs() > 1e-12 {
        format!("x {}{u}, y {}{u}", nb(c.0, 2), nb(c.1, 2), u = unite)
    } else {
        format!("{}{}", nb(c.1, 2), unite)
    }
}

fn nom_type(t: SurfaceType) -> &'static str {
    match t {
        SurfaceType::Plane => tr!("Plan", "Plane", "Plan"),
        SurfaceType::Spherique => tr!("Sphère", "Sphere", "Sphäre"),
        SurfaceType::Conique => tr!("Conique", "Conic", "Konisch"),
        SurfaceType::Aspherique => tr!("Asphère", "Asphere", "Asphäre"),
        SurfaceType::Ideale => tr!("Idéale", "Ideal", "Ideal"),
        SurfaceType::CoupureRepere => tr!("Rupt.rep.", "Coord.brk", "Koord.br."),
    }
}

fn nom_conique(k: f64) -> &'static str {
    if k.abs() < 1e-9 {
        tr!("sphère", "sphere", "Kugel")
    } else if (k + 1.0).abs() < 1e-9 {
        tr!("parabole", "paraboloid", "Paraboloid")
    } else if k < -1.0 {
        tr!("hyperbole", "hyperboloid", "Hyperboloid")
    } else if k < 0.0 {
        tr!("ellipsoïde allongé", "prolate ellipsoid", "gestrecktes Ellipsoid")
    } else {
        tr!("ellipsoïde aplati", "oblate ellipsoid", "abgeplattetes Ellipsoid")
    }
}

/// Indice nd et constringence Vd d'un matériau, s'il s'agit d'un verre
fn nd_vd(sys: &OpticalSystem, verre: &str) -> Option<(f64, f64, String)> {
    let m = sys.catalogue.resoudre(verre);
    match &m.kind {
        MediumKind::Verre { .. } => Some((m.indice_refraction(LAMBDA_D_NM), m.abbe(), m.catalogue.clone())),
        _ => None,
    }
}

/// Tolérances du système portant sur la surface `num` (à partir de 1), mises en forme
fn tolerances(sys: &OpticalSystem, num: usize, types: &[TypeTolerance]) -> Vec<String> {
    sys.tolerances
        .iter()
        .filter(|t| t.surface == num && types.contains(&t.type_tolerance))
        .map(|t| {
            let (unite, dec) = match t.type_tolerance {
                TypeTolerance::Rayon | TypeTolerance::Epaisseur => (" mm", 3),
                TypeTolerance::Franges => (tr!(" franges", " fringes", " Ringe"), 1),
                TypeTolerance::Abbe => (" %", 2),
                TypeTolerance::Indice | TypeTolerance::Conique => ("", 4),
                _ => ("", 3),
            };
            let nom = match t.type_tolerance {
                TypeTolerance::Rayon => tr!("rayon", "radius", "Radius"),
                TypeTolerance::Franges => tr!("puissance", "power", "Passe"),
                TypeTolerance::Epaisseur => tr!("épaisseur", "thickness", "Dicke"),
                TypeTolerance::Conique => tr!("conique", "conic", "Konik"),
                TypeTolerance::Indice => tr!("indice", "index", "Brechzahl"),
                TypeTolerance::Abbe => tr!("constringence", "Abbe number", "Abbezahl"),
                _ => "",
            };
            let ecart = if (t.min + t.max).abs() < 1e-12 {
                format!("± {}", nb(t.max, dec))
            } else {
                format!("+{} / {}", nb(t.max, dec), nb(t.min, dec))
            };
            format!("{nom} {ecart}{unite}")
        })
        .collect()
}

/// Cadre de dessin : transforme les coordonnées optiques (z, y) en coordonnées de page
struct Cadre {
    x: f64,
    y: f64,
    largeur: f64,
    hauteur: f64,
    echelle: f64,
    z0: f64,
    y0: f64,
}

impl Cadre {
    /// Échelle isotrope pour faire tenir [z_min, z_max] × [y_min, y_max] avec une marge
    fn ajuster(x: f64, y: f64, largeur: f64, hauteur: f64, b: [f64; 4], marge: f64) -> Self {
        let (lz, ly) = ((b[1] - b[0]).max(1e-6), (b[3] - b[2]).max(1e-6));
        let echelle = ((largeur - 2.0 * marge) / lz).min((hauteur - 2.0 * marge) / ly);
        Self { x, y, largeur, hauteur, echelle, z0: 0.5 * (b[0] + b[1]), y0: 0.5 * (b[2] + b[3]) }
    }
    fn p(&self, z: f64, y: f64) -> (f64, f64) {
        (self.x + 0.5 * self.largeur + (z - self.z0) * self.echelle, self.y + 0.5 * self.hauteur - (y - self.y0) * self.echelle)
    }
    fn pts(&self, v: &[(f64, f64)]) -> Vec<(f64, f64)> {
        v.iter().map(|&(z, y)| self.p(z, y)).collect()
    }
}

fn etendre(b: &mut [f64; 4], pts: &[(f64, f64)]) {
    for &(z, y) in pts {
        b[0] = b[0].min(z);
        b[1] = b[1].max(z);
        b[2] = b[2].min(y);
        b[3] = b[3].max(y);
    }
}

/// Échelle « ronde » (1:1, 1:2, 2:1…) affichée sous un dessin
fn texte_echelle(echelle: f64) -> String {
    if echelle >= 1.0 {
        format!("{}:1", nb(echelle, if (echelle - echelle.round()).abs() < 0.05 { 0 } else { 2 }))
    } else {
        let inv = 1.0 / echelle;
        format!("1:{}", nb(inv, if (inv - inv.round()).abs() < 0.05 { 0 } else { 2 }))
    }
}

/// Plus grande échelle normalisée (…, 1:2, 1:2,5, 1:5, 1:10, …, 2:1, 5:1…) qui ne dépasse pas `max`
fn echelle_normalisee(max: f64) -> f64 {
    let mut candidates = Vec::new();
    for p in -4..=2 {
        for m in [1.0, 2.0, 2.5, 5.0] {
            candidates.push(m * 10f64.powi(p));
            candidates.push(1.0 / (m * 10f64.powi(p)));
        }
    }
    candidates.into_iter().filter(|&e| e <= max * (1.0 + 1e-9)).fold(f64::MIN, f64::max).max(max.min(1e-4))
}

/// Coupe méridienne du système dans un cadre : pièces, axe, rayons de chaque champ
fn dessiner_coupe(p: &mut Page, sys: &OpticalSystem, x: f64, y: f64, largeur: f64, hauteur: f64, remarques: &mut Vec<String>) {
    let elements = dessin::elements(sys);
    let mut pieces: Vec<(Element, Vec<(f64, f64)>)> = Vec::new();
    for e in elements {
        let r = match e.nature {
            NatureElement::Lentille => dessin::contour_lentille(sys, &e).map(|c| dessin::points(&c, true)),
            NatureElement::Miroir => dessin::profil_miroir(sys, &e).map(|c| dessin::points(&c, false)),
        };
        match r {
            Ok(pts) => pieces.push((e, pts)),
            Err(m) => remarques.push(format!("{} : {m}", e.nom)),
        }
    }
    let mut b = sys.boite_englobante();
    for (_, pts) in &pieces {
        etendre(&mut b, pts);
    }
    // Place pour les rayons qui arrivent
    let lz = b[1] - b[0];
    b[0] -= 0.08 * lz.max(10.0);
    let cadre = Cadre::ajuster(x, y, largeur, hauteur, b, 6.0);
    // Cadre ramené à la hauteur du dessin (système long et fin)
    let hauteur = hauteur.min(((b[3] - b[2]) * cadre.echelle + 24.0).max(55.0));
    let cadre = Cadre { hauteur, ..cadre };
    p.rectangle(x, y, largeur, hauteur, 0.25, GRIS);
    p.decoupe(x, y, largeur, hauteur);
    // Axe optique en trait mixte
    let axe: Vec<(f64, f64)> = if sys.a_des_coupures() {
        let mut v = vec![(b[0], 0.0)];
        v.extend(sys.points_axe().iter().map(|o| (o.z, o.y)));
        v
    } else {
        vec![(b[0] - 50.0, 0.0), (b[1] + 50.0, 0.0)]
    };
    p.polyligne_tirets(&cadre.pts(&axe), 0.18, GRIS, Some((4.0, 1.2)));
    // Pièces
    for (e, pts) in &pieces {
        let q = cadre.pts(pts);
        match e.nature {
            NatureElement::Lentille => p.polygone(&q, VERRE, Some((0.3, BLEU_FONCE))),
            NatureElement::Miroir => p.polyligne(&q, 0.7, [0.45, 0.0, 0.45]),
        }
        let haut = q.iter().map(|v| v.1).fold(f64::MAX, f64::min);
        let milieu = 0.5 * (q.iter().map(|v| v.0).fold(f64::MAX, f64::min) + q.iter().map(|v| v.0).fold(f64::MIN, f64::max));
        p.texte(milieu, haut - 1.5, &e.nom, Police::HelveticaGras, 7.0, Alignement::Centre, BLEU_FONCE);
    }
    // Surfaces qui ne sont pas des pièces : lentille idéale (symbole à flèches), réseau ou
    // dioptre isolé (trait fin)
    let reperes = sys.reperes();
    let dernier = sys.surfaces.len().saturating_sub(1);
    for (i, s) in sys.surfaces.iter().enumerate() {
        let dans_piece = pieces.iter().any(|(e, _)| (e.premiere..=e.derniere).contains(&i));
        if dans_piece || i == dernier || s.est_coupure() || s.semi_diametre <= 0.0 {
            continue;
        }
        let rep = &reperes[i];
        let d = s.semi_diametre;
        if s.type_surface == SurfaceType::Ideale {
            let (a, c) = (rep.vers_global(0.0, -d, 0.0), rep.vers_global(0.0, d, 0.0));
            p.cote(cadre.p(a.z, a.y), cadre.p(c.z, c.y), 0.35, BLEU_FONCE);
            let h = cadre.p(c.z, c.y);
            p.texte(h.0, h.1 - 1.5, &format!("f {}", nb(s.focale_ideale_mm, 1)), Police::Helvetica, 6.5, Alignement::Centre, BLEU_FONCE);
        } else if s.reseau_traits_mm > 0.0 || !s.verre.eq_ignore_ascii_case("AIR") || s.rayon.is_finite() {
            let pts: Vec<(f64, f64)> = (0..=32)
                .filter_map(|k| {
                    let yy = -d + 2.0 * d * k as f64 / 32.0;
                    s.fleche(yy.abs()).map(|f| {
                        let g = rep.vers_global(0.0, yy, f);
                        cadre.p(g.z, g.y)
                    })
                })
                .collect();
            p.polyligne(&pts, 0.35, GRIS);
            if s.reseau_traits_mm > 0.0 {
                if let Some(&h) = pts.last() {
                    p.texte(h.0, h.1 - 1.5, tr!("réseau", "grating", "Gitter"), Police::Helvetica, 6.5, Alignement::Centre, BLEU_FONCE);
                }
            }
        }
    }
    // Plan image
    if let (Some(s), Some(rep)) = (sys.surfaces.last(), reperes.last()) {
        let d = s.semi_diametre.max(0.5);
        let a = rep.vers_global(0.0, -d, 0.0);
        let c = rep.vers_global(0.0, d, 0.0);
        p.ligne(cadre.p(a.z, a.y), cadre.p(c.z, c.y), 0.5, NOIR);
    }
    // Rayons
    for (k, r) in dessin::rayons_meridiens(sys, 7) {
        p.polyligne(&cadre.pts(&r), 0.15, COULEURS_CHAMPS[k % COULEURS_CHAMPS.len()]);
    }
    p.fin_decoupe();
    // Barre d'échelle
    let pas = [1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
    let longueur = pas.iter().copied().filter(|l| l * cadre.echelle <= 0.25 * largeur).fold(pas[0], f64::max);
    let (bx, by) = (x + 4.0, y + hauteur - 4.0);
    p.ligne((bx, by), (bx + longueur * cadre.echelle, by), 0.5, NOIR);
    for xx in [bx, bx + longueur * cadre.echelle] {
        p.ligne((xx, by - 1.0), (xx, by + 1.0), 0.3, NOIR);
    }
    p.texte(bx + longueur * cadre.echelle + 2.0, by + 1.0, &format!("{} mm", nb(longueur, 0)), Police::Helvetica, 7.0, Alignement::Gauche, NOIR);
    // Légende des champs
    let mut champs: Vec<(f64, f64)> = sys.champs_deg.clone();
    if champs.is_empty() {
        champs.push((0.0, 0.0));
    }
    champs.dedup_by(|a, b| (a.1 - b.1).abs() < 1e-9);
    let mut lx = x + largeur - 4.0;
    for (k, c) in champs.iter().enumerate().take(4).rev() {
        let t = format!("{} {}", tr!("champ", "field", "Feld"), libelle_champ(sys, *c));
        p.texte(lx, y + hauteur - 3.0, &t, Police::Helvetica, 7.0, Alignement::Droite, COULEURS_CHAMPS[k]);
        lx -= crate::core::pdf::largeur_texte(&t, Police::Helvetica, 7.0) + 4.0;
    }
}

fn page_caracteristiques(doc: &mut Document, sys: &OpticalSystem, remarques: &mut Vec<String>) {
    let p = doc.nouvelle_page(A4);
    let par = calculer_paraxial(sys);
    p.texte(MARGE, 20.0, &sys.nom, Police::HelveticaGras, 15.0, Alignement::Gauche, BLEU_FONCE);
    p.texte(
        A4.0 - MARGE,
        20.0,
        &format!("FreeOptical {} — {}", env!("CARGO_PKG_VERSION"), date_du_jour()),
        Police::Helvetica,
        8.0,
        Alignement::Droite,
        GRIS,
    );
    p.ligne((MARGE, 23.5), (A4.0 - MARGE, 23.5), 0.4, BLEU_FONCE);

    // Deux colonnes : conditions d'utilisation, caractéristiques paraxiales
    let col = 0.5 * (A4.0 - 2.0 * MARGE) - 4.0;
    let (xg, xd) = (MARGE + 1.5, MARGE + col + 9.5);
    let mut y = 32.0;
    p.texte(xg, y, tr!("CONDITIONS D'UTILISATION", "OPERATING CONDITIONS", "EINSATZBEDINGUNGEN"), Police::HelveticaGras, 9.0, Alignement::Gauche, BLEU_FONCE);
    p.texte(xd, y, tr!("CARACTÉRISTIQUES (PARAXIALES)", "CHARACTERISTICS (PARAXIAL)", "KENNWERTE (PARAXIAL)"), Police::HelveticaGras, 9.0, Alignement::Gauche, BLEU_FONCE);
    y += 5.5;
    let mut gauche: Vec<(String, String)> = Vec::new();
    gauche.push((
        tr!("Ouverture", "Aperture", "Öffnung").into(),
        match sys.ouverture {
            ApertureSpec::PupilleEntreeMm(d) => format!("{} {} mm", tr!("pupille Ø", "pupil Ø", "Pupille Ø"), nb(d, 2)),
            ApertureSpec::OuvertureNumeriqueObjet(na) => format!("{} {}", tr!("ON objet", "object NA", "NA Objekt"), nb(na, 4)),
            ApertureSpec::NombreOuverture(f) => format!("F/{}", nb(f, 2)),
        },
    ));
    gauche.push((
        tr!("Objet", "Object", "Objekt").into(),
        match sys.objet_fini() {
            Some(d) => format!("{} {} mm", tr!("à", "at", "in"), nb(d, 2)),
            None => tr!("à l'infini", "at infinity", "im Unendlichen").into(),
        },
    ));
    let champs: Vec<String> = sys.champs_deg.iter().map(|&c| libelle_champ(sys, c)).collect();
    gauche.push((tr!("Champs", "Fields", "Felder").into(), if champs.is_empty() { "0°".into() } else { champs.join(" ; ") }));
    let lambdas: Vec<String> = sys.longueurs_onde.iter().map(|w| nb(w.valeur_nm, 1)).collect();
    gauche.push((
        tr!("Longueurs d'onde (nm)", "Wavelengths (nm)", "Wellenlängen (nm)").into(),
        if lambdas.is_empty() { nb(sys.longueur_onde_ref_nm, 1) } else { lambdas.join(" ; ") },
    ));
    gauche.push((tr!("Longueur d'onde de référence", "Reference wavelength", "Bezugswellenlänge").into(), format!("{} nm", nb(sys.longueur_onde_ref_nm, 1))));
    gauche.push((tr!("Nombre de surfaces", "Number of surfaces", "Anzahl der Flächen").into(), sys.surfaces.len().to_string()));
    let el = dessin::elements(sys);
    let (nl, nm) = (el.iter().filter(|e| e.nature == NatureElement::Lentille).count(), el.iter().filter(|e| e.nature == NatureElement::Miroir).count());
    gauche.push((tr!("Lentilles / miroirs", "Lenses / mirrors", "Linsen / Spiegel").into(), format!("{nl} / {nm}")));

    let mut droite: Vec<(String, String)> = Vec::new();
    if sys.afocal {
        droite.push((tr!("Système afocal : grossissement", "Afocal system: magnification", "Afokales System: Vergrößerung").into(), format!("{}×", nb(par.grossissement_angulaire, 3))));
    } else {
        droite.push((tr!("Focale (EFFL)", "Focal length (EFFL)", "Brennweite (EFFL)").into(), format!("{} mm", nb(par.focale_effl_mm, 3))));
        droite.push((tr!("Tirage (BFL)", "Back focal length (BFL)", "Schnittweite (BFL)").into(), format!("{} mm", nb(par.tirage_bfl_mm, 3))));
        droite.push((tr!("Nombre d'ouverture", "F-number", "Blendenzahl").into(), format!("F/{}", nb(par.nombre_ouverture_fnum, 3))));
        droite.push((tr!("Ouverture numérique image", "Image-space NA", "Bildseitige NA").into(), nb(par.ouverture_numerique_image, 4)));
    }
    droite.push((tr!("Pupille d'entrée : Ø / position", "Entrance pupil: Ø / position", "Eintrittspupille: Ø / Lage").into(), format!("{} / {} mm", nb(par.diametre_pupille_entree_mm, 3), nb(par.position_pupille_entree_mm, 3))));
    droite.push((tr!("Pupille de sortie : Ø / position", "Exit pupil: Ø / position", "Austrittspupille: Ø / Lage").into(), format!("{} / {} mm", nb(par.diametre_pupille_sortie_mm, 3), nb(par.position_pupille_sortie_mm, 3))));
    if sys.objet_fini().is_some() {
        droite.push((tr!("Grandissement", "Magnification", "Abbildungsmaßstab").into(), format!("{}×", nb(par.grandissement, 4))));
    }
    droite.push((tr!("Longueur totale", "Total length", "Baulänge").into(), format!("{} mm", nb(par.longueur_totale_mm, 3))));

    for k in 0..gauche.len().max(droite.len()) {
        if let Some((l, v)) = gauche.get(k) {
            ligne_valeur(p, xg, col, y, l, v);
        }
        if let Some((l, v)) = droite.get(k) {
            ligne_valeur(p, xd, col, y, l, v);
        }
        y += 4.6;
    }

    // Qualité d'image par champ
    y += 4.0;
    y = titre_section(p, y, tr!("QUALITÉ D'IMAGE (TRACÉ DE RAYONS RÉELS)", "IMAGE QUALITY (REAL RAY TRACE)", "BILDQUALITÄT (REALE STRAHLDURCHRECHNUNG)"));
    let unite = if sys.afocal { "(µrad)" } else { "(µm)" };
    let ondes = tr!("(ondes)", "(waves)", "(Wellen)");
    let entetes = [
        (tr!("Champ", "Field", "Feld"), ""),
        (tr!("Tache RMS", "RMS spot", "RMS-Fleck"), unite),
        (tr!("Tache GEO", "GEO spot", "GEO-Fleck"), unite),
        (tr!("Rayon d'Airy", "Airy radius", "Airy-Radius"), unite),
        (tr!("Front d'onde RMS", "RMS wavefront", "RMS-Wellenfront"), ondes),
        (tr!("Front d'onde PV", "PV wavefront", "PV-Wellenfront"), ondes),
        ("Strehl", ""),
    ];
    let colonnes = [MARGE + 1.5, 62.0, 87.0, 112.0, 142.0, 170.0, A4.0 - MARGE - 1.5];
    for (k, (e, u)) in entetes.iter().enumerate() {
        let al = if k == 0 { Alignement::Gauche } else { Alignement::Droite };
        p.texte(colonnes[k], y, e, Police::HelveticaGras, 7.5, al, NOIR);
        p.texte(colonnes[k], y + 3.3, u, Police::Helvetica, 7.0, al, GRIS);
    }
    y += 4.8;
    p.ligne((MARGE, y), (A4.0 - MARGE, y), 0.2, GRIS);
    y += 4.0;
    let mut champs = sys.champs_deg.clone();
    if champs.is_empty() {
        champs.push((0.0, 0.0));
    }
    for &c in champs.iter().take(12) {
        let spot = calculer_spot_diagram(sys, c, 6);
        let onde = calculer_front_onde(sys, c, sys.longueur_onde_ref_nm, 48);
        let valeurs = [
            libelle_champ(sys, c),
            nb(spot.rayon_rms_um, 2),
            nb(spot.rayon_geo_um, 2),
            nb(spot.rayon_airy_um, 2),
            onde.as_ref().map(|o| nb(o.rms_ondes, 4)).unwrap_or_else(|_| "—".into()),
            onde.as_ref().map(|o| nb(o.pv_ondes, 4)).unwrap_or_else(|_| "—".into()),
            onde.as_ref().map(|o| nb(o.strehl_marechal, 3)).unwrap_or_else(|_| "—".into()),
        ];
        for (k, v) in valeurs.iter().enumerate() {
            let al = if k == 0 { Alignement::Gauche } else { Alignement::Droite };
            p.texte(colonnes[k], y, v, Police::Helvetica, 8.0, al, NOIR);
        }
        y += 4.3;
    }
    p.texte(
        MARGE + 1.5,
        y + 0.5,
        &trf!(
            "Front d'onde et Strehl (approximation de Maréchal) à {} nm ; taches sur toutes les longueurs d'onde.",
            "Wavefront and Strehl (Maréchal approximation) at {} nm; spots over all wavelengths.",
            "Wellenfront und Strehl (Maréchal-Näherung) bei {} nm; Flecke über alle Wellenlängen.",
            nb(sys.longueur_onde_ref_nm, 1)
        ),
        Police::Helvetica,
        7.0,
        Alignement::Gauche,
        GRIS,
    );
    y += 7.0;

    // Coupe à l'échelle
    y = titre_section(p, y, tr!("COUPE MÉRIDIENNE", "MERIDIONAL SECTION", "MERIDIONALSCHNITT"));
    let bas = A4.1 - 16.0;
    dessiner_coupe(p, sys, MARGE, y - 2.0, A4.0 - 2.0 * MARGE, (bas - y + 2.0).max(40.0), remarques);
}

fn pages_surfaces(doc: &mut Document, sys: &OpticalSystem) {
    // Colonnes du tableau (Courier, largeur fixe)
    let entete = format!(
        "{:>4} {:<10} {:>11} {:>10} {:<10} {:>8} {:>6} {:>9} {:>9}  {}",
        tr!("N°", "No.", "Nr."),
        tr!("Type", "Type", "Typ"),
        tr!("Rayon", "Radius", "Radius"),
        tr!("Épaisseur", "Thickness", "Dicke"),
        tr!("Matériau", "Material", "Material"),
        "nd",
        "Vd",
        tr!("Ø utile", "Clear Ø", "Nutz-Ø"),
        tr!("Conique", "Conic", "Konik"),
        tr!("Commentaire", "Comment", "Kommentar")
    );
    let mut lignes: Vec<String> = Vec::new();
    let mut details: Vec<String> = Vec::new();
    for (i, s) in sys.surfaces.iter().enumerate() {
        let num = format!("{}{}", i + 1, if s.est_diaphragme { "*" } else { " " });
        let rayon = if s.rayon.is_finite() && s.rayon != 0.0 { nb(s.rayon, 4) } else { "inf.".into() };
        let (nd, vd) = match nd_vd(sys, &s.verre) {
            Some((n, v, _)) => (nb(n, 5), nb(v, 2)),
            None => (String::new(), String::new()),
        };
        let conique = if matches!(s.type_surface, SurfaceType::Conique | SurfaceType::Aspherique) { nb(s.conique, 5) } else { String::new() };
        let mut verre = s.verre.clone();
        verre.truncate(10);
        let mut comm = s.commentaire.clone();
        if comm.chars().count() > 26 {
            comm = comm.chars().take(25).collect::<String>() + "…";
        }
        lignes.push(format!(
            "{:>4} {:<10} {:>11} {:>10} {:<10} {:>8} {:>6} {:>9} {:>9}  {}",
            num,
            nom_type(s.type_surface),
            rayon,
            nb(s.epaisseur, 4),
            verre,
            nd,
            vd,
            nb(2.0 * s.semi_diametre, 3),
            conique,
            comm
        ));
        let n = i + 1;
        if s.type_surface == SurfaceType::Aspherique && s.aspheriques.iter().any(|&a| a != 0.0) {
            details.push(format!("S{n} : A4 = {} ; A6 = {} ; A8 = {} ; A10 = {}", sci(s.aspheriques[0]), sci(s.aspheriques[1]), sci(s.aspheriques[2]), sci(s.aspheriques[3])));
        }
        if s.type_surface == SurfaceType::Ideale {
            details.push(trf!("S{} : lentille idéale de focale {} mm", "S{}: ideal lens of focal length {} mm", "S{}: ideale Linse mit Brennweite {} mm", n, nb(s.focale_ideale_mm, 3)));
        }
        if s.decentrement != [0.0, 0.0] || s.inclinaison_deg != [0.0, 0.0] || s.inclinaison_z_deg != 0.0 {
            details.push(trf!(
                "S{} : décentrement x {} / y {} mm ; inclinaison x {}° / y {}°{}",
                "S{}: decenter x {} / y {} mm; tilt x {}° / y {}°{}",
                "S{}: Dezentrierung x {} / y {} mm; Kippung x {}° / y {}°{}",
                n,
                nb(s.decentrement[0], 3),
                nb(s.decentrement[1], 3),
                nb(s.inclinaison_deg[0], 3),
                nb(s.inclinaison_deg[1], 3),
                if s.inclinaison_z_deg != 0.0 { format!(" / z {}°", nb(s.inclinaison_z_deg, 3)) } else { String::new() }
            ));
        }
        if s.reseau_traits_mm > 0.0 {
            details.push(trf!(
                "S{} : réseau {} traits/mm, ordre {}{}",
                "S{}: grating {} lines/mm, order {}{}",
                "S{}: Gitter {} Linien/mm, Ordnung {}{}",
                n,
                nb(s.reseau_traits_mm, 2),
                s.reseau_ordre,
                if s.reseau_blaze_deg != 0.0 { format!(", blaze {}°", nb(s.reseau_blaze_deg, 2)) } else { String::new() }
            ));
        }
        if s.rayon_obstruction > 0.0 {
            details.push(trf!("S{} : obstruction centrale Ø {} mm", "S{}: central obstruction Ø {} mm", "S{}: Mittenabschattung Ø {} mm", n, nb(2.0 * s.rayon_obstruction, 3)));
        }
    }
    let lignes_par_page = 52;
    let nb_pages = lignes.len().div_ceil(lignes_par_page).max(1);
    for k in 0..nb_pages {
        let p = doc.nouvelle_page(A4);
        let mut y = titre_section(p, 20.0, tr!("TABLEAU DES SURFACES (mm)", "SURFACE DATA (mm)", "FLÄCHENDATEN (mm)"));
        p.texte(MARGE + 1.0, y, &entete, Police::CourierGras, 7.2, Alignement::Gauche, NOIR);
        y += 1.6;
        p.ligne((MARGE, y), (A4.0 - MARGE, y), 0.2, GRIS);
        y += 3.6;
        for (j, l) in lignes.iter().enumerate().skip(k * lignes_par_page).take(lignes_par_page) {
            if j % 2 == 1 {
                p.rectangle_plein(MARGE, y - 2.9, A4.0 - 2.0 * MARGE, 4.0, [0.96, 0.96, 0.96]);
            }
            p.texte(MARGE + 1.0, y, l, Police::Courier, 7.2, Alignement::Gauche, NOIR);
            y += 4.0;
        }
        if k + 1 == nb_pages {
            y += 2.0;
            p.texte(
                MARGE + 1.0,
                y,
                tr!(
                    "* diaphragme d'ouverture. Épaisseur : distance jusqu'à la surface suivante (négative après un miroir). Ø utile : diamètre utilisé par la lumière.",
                    "* aperture stop. Thickness: distance to the next surface (negative after a mirror). Clear Ø: diameter used by the light.",
                    "* Aperturblende. Dicke: Abstand zur nächsten Fläche (negativ nach einem Spiegel). Nutz-Ø: vom Licht genutzter Durchmesser."
                ),
                Police::Helvetica,
                7.0,
                Alignement::Gauche,
                GRIS,
            );
            y += 7.0;
            if !details.is_empty() {
                y = titre_section(p, y, tr!("PRÉCISIONS", "DETAILS", "ERGÄNZUNGEN"));
                for d in &details {
                    if y > A4.1 - 18.0 {
                        break;
                    }
                    p.texte(MARGE + 1.5, y, d, Police::Helvetica, 8.0, Alignement::Gauche, NOIR);
                    y += 4.3;
                }
            }
        }
    }
}

/// Configurations multiples : paramètres qui changent et comparaison des configurations
fn page_configurations(doc: &mut Document, sys: &OpticalSystem) {
    use crate::core::configurations::{TypeOperande, ValeurConfig, bilan_configurations};
    let mut sys = sys.clone();
    sys.capturer_configuration();
    let nb_configs = sys.nb_configurations();
    let active = sys.configurations.active;
    let bilan = bilan_configurations(&sys);
    let p = doc.nouvelle_page(A4);
    let mut y = titre_section(p, 20.0, &trf!("CONFIGURATIONS ({})", "CONFIGURATIONS ({})", "KONFIGURATIONEN ({})", nb_configs));
    p.texte(
        MARGE + 1.5,
        y,
        &trf!(
            "Les autres pages décrivent la configuration active : {} ({}). Les paramètres absents de ce tableau sont communs.",
            "The other pages describe the active configuration: {} ({}). Parameters missing from this table are shared.",
            "Die anderen Seiten beschreiben die aktive Konfiguration: {} ({}). Nicht aufgeführte Parameter sind gemeinsam.",
            active + 1,
            sys.configurations.nom(active)
        ),
        Police::Helvetica,
        8.0,
        Alignement::Gauche,
        NOIR,
    );
    y += 7.0;
    let valeur = |t: TypeOperande, v: &ValeurConfig| -> String {
        let mut s = match (t, v.nombre()) {
            (TypeOperande::Verre, _) | (_, None) => v.texte(),
            (TypeOperande::ReseauOrdre, Some(n)) => format!("{}", n.round() as i64),
            (_, Some(n)) => nb(n, 4),
        };
        s.truncate(12);
        s
    };
    let ou = |t: TypeOperande, i: usize| -> String {
        if t.porte_sur_surface() {
            format!("S{}", i + 1)
        } else if !t.a_un_index() {
            String::new()
        } else if matches!(t, TypeOperande::ChampX | TypeOperande::ChampY) {
            trf!("champ {}", "field {}", "Feld {}", i + 1)
        } else {
            format!("λ {}", i + 1)
        }
    };
    // Six configurations par bloc pour tenir dans la largeur
    for debut in (0..nb_configs).step_by(6) {
        let fin = (debut + 6).min(nb_configs);
        let mut entete = format!("{:<18} {:<8}", tr!("Paramètre", "Parameter", "Parameter"), tr!("Où", "Where", "Wo"));
        for k in debut..fin {
            let mut n = format!("{}{} {}", if k == active { "*" } else { "" }, k + 1, sys.configurations.nom(k));
            n = n.chars().take(14).collect();
            entete.push_str(&format!(" {:>14}", n));
        }
        p.texte(MARGE + 1.0, y, &entete, Police::CourierGras, 7.2, Alignement::Gauche, NOIR);
        y += 1.6;
        p.ligne((MARGE, y), (A4.0 - MARGE, y), 0.2, GRIS);
        y += 3.6;
        for (j, op) in sys.configurations.operandes.iter().enumerate() {
            if y > A4.1 - 60.0 {
                break;
            }
            let mut nom: String = op.type_op.libelle().chars().take(17).collect();
            if op.variable {
                nom.push_str(" V");
            }
            let mut l = format!("{:<18} {:<8}", nom, ou(op.type_op, op.index));
            for k in debut..fin {
                l.push_str(&format!(" {:>14}", op.valeurs.get(k).map(|v| valeur(op.type_op, v)).unwrap_or_default()));
            }
            if j % 2 == 1 {
                p.rectangle_plein(MARGE, y - 2.9, A4.0 - 2.0 * MARGE, 4.0, [0.96, 0.96, 0.96]);
            }
            p.texte(MARGE + 1.0, y, &l, Police::Courier, 7.2, Alignement::Gauche, NOIR);
            y += 4.0;
        }
        y += 4.0;
    }
    p.texte(
        MARGE + 1.0,
        y,
        tr!(
            "* configuration active. V : réglé séparément dans chaque configuration par l'optimiseur.",
            "* active configuration. V: set separately in each configuration by the optimizer.",
            "* aktive Konfiguration. V: vom Optimierer in jeder Konfiguration getrennt eingestellt."
        ),
        Police::Helvetica,
        7.0,
        Alignement::Gauche,
        GRIS,
    );
    y += 9.0;

    y = titre_section(p, y, tr!("COMPARAISON DES CONFIGURATIONS", "COMPARISON OF THE CONFIGURATIONS", "VERGLEICH DER KONFIGURATIONEN"));
    let entete = format!(
        "{:<20} {:>10} {:>7} {:>10} {:>10} {:>8}  {}",
        "Configuration",
        "EFFL",
        "F/#",
        tr!("Tir. parax", "Parax. BFL", "Parax. SW"),
        tr!("Longueur", "Length", "Länge"),
        "Airy µm",
        tr!("Spot RMS par champ (µm)", "RMS spot per field (µm)", "RMS-Spot je Feld (µm)")
    );
    p.texte(MARGE + 1.0, y, &entete, Police::CourierGras, 7.2, Alignement::Gauche, NOIR);
    y += 1.6;
    p.ligne((MARGE, y), (A4.0 - MARGE, y), 0.2, GRIS);
    y += 3.6;
    for (j, b) in bilan.iter().enumerate() {
        if y > A4.1 - 18.0 {
            break;
        }
        let nom: String = format!("{}{} {}", if b.numero == active { "*" } else { "" }, b.numero + 1, b.nom).chars().take(20).collect();
        let spots = b.spot_rms_um.iter().map(|v| nb(*v, 2)).collect::<Vec<_>>().join(" / ");
        let l = format!(
            "{:<20} {:>10} {:>7} {:>10} {:>10} {:>8}  {}",
            nom,
            nb(b.focale_mm, 3),
            nb(b.f_nombre, 2),
            nb(b.tirage_mm, 3),
            nb(b.longueur_totale_mm, 2),
            nb(b.rayon_airy_um, 2),
            spots
        );
        if j % 2 == 1 {
            p.rectangle_plein(MARGE, y - 2.9, A4.0 - 2.0 * MARGE, 4.0, [0.96, 0.96, 0.96]);
        }
        p.texte(MARGE + 1.0, y, &l, Police::Courier, 7.2, Alignement::Gauche, NOIR);
        y += 4.0;
    }
}

/// Repères locaux d'un plan de fabrication : première face au sommet, axe horizontal
fn reperes_plan(z1: f64, z2: f64) -> (Repere, Repere) {
    let r = |z: f64| Repere { origine: crate::core::types::Point3::new(0.0, 0.0, z), rotation: nalgebra::Rotation3::identity() };
    (r(z1), r(z2))
}

/// CX (convexe) ou CC (concave) vu de l'extérieur de la pièce ; `entree` : face côté lumière
fn sens(rayon_oriente: f64, entree: bool) -> &'static str {
    if !rayon_oriente.is_finite() || rayon_oriente == 0.0 {
        tr!("plan", "flat", "plan")
    } else if (rayon_oriente > 0.0) == entree {
        "CX"
    } else {
        "CC"
    }
}

fn libelle_rayon(r: f64, entree: bool) -> String {
    if !r.is_finite() || r == 0.0 {
        tr!("plan", "flat", "plan").into()
    } else {
        format!("R {} {}", nb(r.abs(), 3), sens(r, entree))
    }
}

/// Petit tableau « libellé / valeur » ; retourne le bas du tableau
fn tableau(p: &mut Page, x: f64, y: f64, largeur: f64, titre: &str, lignes: &[(String, String)]) -> f64 {
    p.rectangle_plein(x, y - 3.8, largeur, 5.2, FOND_TITRE);
    p.texte(x + 1.2, y, titre, Police::HelveticaGras, 8.0, Alignement::Gauche, BLEU_FONCE);
    let mut yy = y + 5.0;
    for (l, v) in lignes {
        p.texte(x + 1.2, yy, l, Police::Helvetica, 7.5, Alignement::Gauche, NOIR);
        p.texte(x + largeur - 1.2, yy, v, Police::HelveticaGras, 7.5, Alignement::Droite, NOIR);
        yy += 4.1;
    }
    p.rectangle(x, y - 3.8, largeur, yy - y + 1.5, 0.2, GRIS);
    yy
}

fn lignes_face(sys: &OpticalSystem, i: usize, r_oriente: f64, entree: bool) -> Vec<(String, String)> {
    let s = &sys.surfaces[i];
    let mut v = vec![(
        tr!("Rayon", "Radius", "Radius").to_string(),
        if r_oriente.is_finite() && r_oriente != 0.0 { format!("{} mm {}", nb(r_oriente.abs(), 4), sens(r_oriente, entree)) } else { tr!("plan", "flat", "plan").into() },
    )];
    v.push((tr!("Ø utile", "Clear Ø", "Nutz-Ø").into(), format!("{} mm", nb(2.0 * s.semi_diametre, 3))));
    if let Some(f) = s.fleche(s.semi_diametre) {
        v.push((tr!("Flèche au bord utile", "Sag at clear edge", "Pfeilhöhe am Nutzrand").into(), format!("{} mm", nb(f.abs(), 4))));
    }
    if matches!(s.type_surface, SurfaceType::Conique | SurfaceType::Aspherique) && s.conique != 0.0 {
        v.push((tr!("Conique k", "Conic k", "Konik k").into(), format!("{} ({})", nb(s.conique, 5), nom_conique(s.conique))));
    }
    if s.type_surface == SurfaceType::Aspherique {
        for (k, a) in s.aspheriques.iter().enumerate() {
            if *a != 0.0 {
                v.push((format!("A{}", 4 + 2 * k), sci(*a)));
            }
        }
    }
    for t in tolerances(sys, i + 1, &[TypeTolerance::Rayon, TypeTolerance::Franges, TypeTolerance::Conique]) {
        v.push((tr!("Tolérance", "Tolerance", "Toleranz").into(), t));
    }
    v
}

fn page_lentille(doc: &mut Document, sys: &OpticalSystem, e: &Element, elements: &[Element], remarques: &mut Vec<String>) {
    let (i1, i2) = (e.premiere, e.derniere);
    let (s1, s2) = (&sys.surfaces[i1], &sys.surfaces[i2]);
    let ct = s1.epaisseur;
    let signe = if ct < 0.0 { -1.0 } else { 1.0 };
    let (r1, r2) = reperes_plan(0.0, ct);
    let contour = match dessin::contour_lentille_reperes(sys, e, &r1, &r2) {
        Ok(c) => c,
        Err(m) => {
            remarques.push(format!("{} : {m}", e.nom));
            return;
        }
    };
    // Lumière de gauche à droite : symétrie si la lentille est traversée vers -Z
    let contour: Vec<Sommet> = contour.iter().map(|s| Sommet { z: signe * s.z, y: s.y, bombe: signe * s.bombe }).collect();
    let pts = dessin::points(&contour, true);
    let (ro1, ro2) = (signe * s1.rayon, signe * s2.rayon);
    let rm = s1.semi_diametre.max(s2.semi_diametre);
    let (f1, f2) = (signe * s1.fleche(s1.semi_diametre).unwrap_or(0.0), signe * s2.fleche(s2.semi_diametre).unwrap_or(0.0));
    let ct_abs = ct.abs();
    let et = ct_abs + f2 - f1;
    let materiau = nd_vd(sys, &s1.verre);
    let n_ref = sys.catalogue.resoudre(&s1.verre).indice_refraction(sys.longueur_onde_ref_nm);
    // Focale de la lentille seule, dans l'air (formule de la lentille épaisse)
    let c = |r: f64| if r.is_finite() && r != 0.0 { 1.0 / r } else { 0.0 };
    let (c1, c2) = (c(ro1), c(ro2));
    let puissance = (n_ref - 1.0) * (c1 - c2 + (n_ref - 1.0) * ct_abs * c1 * c2 / n_ref);
    let colle_avant = elements.iter().any(|o| o.nature == NatureElement::Lentille && o.derniere == i1);
    let colle_apres = elements.iter().any(|o| o.nature == NatureElement::Lentille && o.premiere == i2);

    let p = doc.nouvelle_page(A4);
    p.texte(MARGE, 20.0, &trf!("Plan de fabrication — lentille {}", "Manufacturing drawing — lens {}", "Fertigungszeichnung — Linse {}", e.nom), Police::HelveticaGras, 14.0, Alignement::Gauche, BLEU_FONCE);
    p.texte(
        MARGE,
        25.5,
        &trf!("Surfaces {} et {} du système « {} »", "Surfaces {} and {} of the system “{}”", "Flächen {} und {} des Systems „{}“", i1 + 1, i2 + 1, sys.nom),
        Police::Helvetica,
        8.5,
        Alignement::Gauche,
        GRIS,
    );
    p.ligne((MARGE, 28.0), (A4.0 - MARGE, 28.0), 0.4, BLEU_FONCE);

    // Dessin coté
    let (dx, dy, dl, dh) = (MARGE, 32.0, A4.0 - 2.0 * MARGE, 118.0);
    let mut b = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    etendre(&mut b, &pts);
    // Place des cotes autour de la pièce (en mm de papier ramenés à l'objet plus bas)
    let max_echelle = ((dl - 70.0) / (b[1] - b[0]).max(1e-6)).min((dh - 40.0) / (b[3] - b[2]).max(1e-6));
    let echelle = echelle_normalisee(max_echelle);
    let cadre = Cadre { x: dx, y: dy, largeur: dl, hauteur: dh, echelle, z0: 0.5 * (b[0] + b[1]), y0: 0.0 };
    p.rectangle(dx, dy, dl, dh, 0.25, GRIS);
    p.polyligne_tirets(&[cadre.p(b[0] - 12.0 / echelle, 0.0), cadre.p(b[1] + 12.0 / echelle, 0.0)], 0.18, GRIS, Some((5.0, 1.2)));
    p.polygone(&cadre.pts(&pts), VERRE, Some((0.45, NOIR)));
    // Épaisseur au centre (sous la pièce)
    let (a_ct, b_ct) = (cadre.p(0.0, 0.0), cadre.p(ct_abs, 0.0));
    let y_ct = cadre.p(0.0, -rm).1 + 9.0;
    p.ligne(a_ct, (a_ct.0, y_ct + 1.5), 0.15, GRIS);
    p.ligne(b_ct, (b_ct.0, y_ct + 1.5), 0.15, GRIS);
    p.cote((a_ct.0, y_ct), (b_ct.0, y_ct), 0.2, NOIR);
    p.texte(0.5 * (a_ct.0 + b_ct.0), y_ct + 4.5, &format!("{} {}", tr!("CT", "CT", "Mittendicke"), nb(ct_abs, 3)), Police::HelveticaGras, 8.0, Alignement::Centre, NOIR);
    // Épaisseur au bord (au-dessus)
    let (a_et, b_et) = (cadre.p(f1, rm), cadre.p(ct_abs + f2, rm));
    let y_et = a_et.1.min(b_et.1) - 7.0;
    p.ligne(a_et, (a_et.0, y_et - 1.5), 0.15, GRIS);
    p.ligne(b_et, (b_et.0, y_et - 1.5), 0.15, GRIS);
    p.cote((a_et.0, y_et), (b_et.0, y_et), 0.2, NOIR);
    p.texte(0.5 * (a_et.0 + b_et.0), y_et - 2.0, &format!("{} {}", tr!("ET", "ET", "Randdicke"), nb(et, 3)), Police::HelveticaGras, 8.0, Alignement::Centre, NOIR);
    // Diamètre extérieur (à gauche)
    let x_d = cadre.p(b[0], 0.0).0 - 14.0;
    let (h_d, bas_d) = (cadre.p(f1, rm), cadre.p(f1, -rm));
    p.ligne(h_d, (x_d - 1.5, h_d.1), 0.15, GRIS);
    p.ligne(bas_d, (x_d - 1.5, bas_d.1), 0.15, GRIS);
    p.cote((x_d, h_d.1), (x_d, bas_d.1), 0.2, NOIR);
    p.texte_tourne(x_d - 1.5, 0.5 * (h_d.1 + bas_d.1), 90.0, &format!("Ø {}", nb(2.0 * rm, 3)), Police::HelveticaGras, 8.0, Alignement::Centre, NOIR);
    // Rayons des faces : texte au-dessus de la pièce, trait de rappel vers la face
    let y_texte = cadre.p(0.0, rm).1 - 15.0;
    let (gauche, droite) = (cadre.p(b[0], 0.0).0, cadre.p(b[1], 0.0).0);
    let ancre1 = s1.fleche(0.7 * s1.semi_diametre).map(|f| cadre.p(signe * f, 0.7 * s1.semi_diametre));
    let ancre2 = s2.fleche(0.7 * s2.semi_diametre).map(|f| cadre.p(ct_abs + signe * f, 0.7 * s2.semi_diametre));
    let t1 = format!("{} : {}", tr!("face 1", "face 1", "Fläche 1"), libelle_rayon(ro1, true));
    let t2 = format!("{} : {}", tr!("face 2", "face 2", "Fläche 2"), libelle_rayon(ro2, false));
    p.texte(gauche - 3.0, y_texte, &t1, Police::Helvetica, 8.0, Alignement::Droite, BLEU_FONCE);
    p.texte(droite + 3.0, y_texte, &t2, Police::Helvetica, 8.0, Alignement::Gauche, BLEU_FONCE);
    if let Some(a) = ancre1 {
        p.polyligne(&[(gauche - 2.0, y_texte + 1.0), a], 0.15, BLEU_FONCE);
    }
    if let Some(a) = ancre2 {
        p.polyligne(&[(droite + 2.0, y_texte + 1.0), a], 0.15, BLEU_FONCE);
    }
    p.texte(dx + 3.0, dy + dh - 3.0, &format!("{} {} — {}", tr!("Échelle", "Scale", "Maßstab"), texte_echelle(echelle), tr!("cotes en mm, lumière de gauche à droite", "dimensions in mm, light from left to right", "Maße in mm, Licht von links nach rechts")), Police::Helvetica, 7.0, Alignement::Gauche, GRIS);

    // Tableaux : face 1 | matériau et épaisseurs | face 2
    let largeur = (A4.0 - 2.0 * MARGE - 8.0) / 3.0;
    let y0 = dy + dh + 10.0;
    let mut bas = tableau(p, MARGE, y0, largeur, &format!("{} ({})", tr!("Face 1", "Face 1", "Fläche 1"), trf!("surface {}", "surface {}", "Fläche {}", i1 + 1)), &lignes_face(sys, i1, ro1, true));
    let mut milieu = vec![(tr!("Matériau", "Material", "Material").to_string(), s1.verre.clone())];
    if let Some((nd, vd, cat)) = &materiau {
        milieu.push(("nd / Vd".into(), format!("{} / {}", nb(*nd, 5), nb(*vd, 2))));
        if !cat.is_empty() {
            milieu.push((tr!("Catalogue", "Catalogue", "Katalog").into(), cat.clone()));
        }
    }
    milieu.push((trf!("Indice à {} nm", "Index at {} nm", "Brechzahl bei {} nm", nb(sys.longueur_onde_ref_nm, 1)), nb(n_ref, 6)));
    milieu.push((tr!("Épaisseur au centre CT", "Centre thickness CT", "Mittendicke").into(), format!("{} mm", nb(ct_abs, 3))));
    for t in tolerances(sys, i1 + 1, &[TypeTolerance::Epaisseur, TypeTolerance::Indice, TypeTolerance::Abbe]) {
        milieu.push((tr!("Tolérance", "Tolerance", "Toleranz").into(), t));
    }
    milieu.push((tr!("Épaisseur au bord ET", "Edge thickness ET", "Randdicke").into(), format!("{} mm", nb(et, 3))));
    milieu.push((tr!("Ø extérieur", "Outer Ø", "Außen-Ø").into(), format!("{} mm", nb(2.0 * rm, 3))));
    milieu.push((
        tr!("Focale (seule, dans l'air)", "Focal length (alone, in air)", "Brennweite (allein, in Luft)").into(),
        if puissance.abs() > 1e-12 { format!("{} mm", nb(1.0 / puissance, 2)) } else { tr!("infinie", "infinite", "unendlich").into() },
    ));
    bas = bas.max(tableau(p, MARGE + largeur + 4.0, y0, largeur, tr!("Matériau et épaisseurs", "Material and thicknesses", "Material und Dicken"), &milieu));
    bas = bas.max(tableau(p, MARGE + 2.0 * (largeur + 4.0), y0, largeur, &format!("{} ({})", tr!("Face 2", "Face 2", "Fläche 2"), trf!("surface {}", "surface {}", "Fläche {}", i2 + 1)), &lignes_face(sys, i2, ro2, false)));

    // Remarques
    let mut notes: Vec<(String, bool)> = Vec::new();
    if et <= 0.0 {
        notes.push((tr!("ATTENTION : épaisseur au bord nulle ou négative, lentille impossible à fabriquer.", "WARNING: zero or negative edge thickness, the lens cannot be made.", "ACHTUNG: Randdicke null oder negativ, Linse nicht herstellbar.").into(), true));
    } else if et < 1.0 || et < 0.03 * 2.0 * rm {
        notes.push((tr!("Épaisseur au bord faible : lentille fragile, à vérifier avec l'opticien.", "Thin edge: fragile lens, check with the optician.", "Geringe Randdicke: empfindliche Linse, mit dem Optiker prüfen.").into(), true));
    }
    if colle_avant || colle_apres {
        let voisins: Vec<&str> = elements
            .iter()
            .filter(|o| o.nature == NatureElement::Lentille && (o.derniere == i1 || o.premiere == i2))
            .map(|o| o.nom.as_str())
            .collect();
        notes.push((trf!("Lentille collée à {} (face commune).", "Lens cemented to {} (shared face).", "Mit {} verkittet (gemeinsame Fläche).", voisins.join(", ")), false));
    }
    if (s1.semi_diametre - s2.semi_diametre).abs() > 1e-9 {
        notes.push((tr!("Les deux faces n'ont pas le même Ø utile : la plus petite est prolongée par un méplat.", "The two faces have different clear Ø: the smaller one continues as a flat.", "Die Flächen haben unterschiedliche Nutz-Ø: die kleinere wird plan fortgesetzt.").into(), false));
    }
    notes.push((tr!("Ø extérieur = plus grand Ø utile : ajouter la marge de montage (appui, biseaux).", "Outer Ø = largest clear Ø: add the mounting margin (seat, bevels).", "Außen-Ø = größter Nutz-Ø: Fassungsrand (Auflage, Fasen) hinzufügen.").into(), false));
    let mut y = bas + 7.0;
    for (n, alerte) in notes {
        p.texte(MARGE + 1.0, y, &format!("• {n}"), Police::Helvetica, 8.0, Alignement::Gauche, if alerte { [0.75, 0.0, 0.0] } else { NOIR });
        y += 4.5;
    }
}

fn page_miroir(doc: &mut Document, sys: &OpticalSystem, e: &Element, remarques: &mut Vec<String>) {
    let i = e.premiere;
    let s = &sys.surfaces[i];
    let (r1, _) = reperes_plan(0.0, 0.0);
    let profil = match dessin::profil_miroir_repere(sys, e, &r1) {
        Ok(c) => c,
        Err(m) => {
            remarques.push(format!("{} : {m}", e.nom));
            return;
        }
    };
    // Lumière arrivant de la gauche : symétrie si elle arrive en allant vers -Z (après un
    // nombre impair de miroirs). Vu de la lumière, le miroir est concave si son centre de
    // courbure est du côté de la lumière (rayon orienté négatif)
    let signe = if i > 0 && sys.surfaces[i - 1].epaisseur < 0.0 { -1.0 } else { 1.0 };
    let profil: Vec<Sommet> = profil.iter().map(|q| Sommet { z: signe * q.z, y: q.y, bombe: signe * q.bombe }).collect();
    let pts = dessin::points(&profil, false);
    let d = s.semi_diametre;
    let fleche = s.fleche(d).unwrap_or(0.0).abs();
    let concave = s.rayon.is_finite() && signe * s.rayon < 0.0;
    let epaisseur_support = (2.0 * d / 6.0).max(fleche + 2.0);

    let p = doc.nouvelle_page(A4);
    p.texte(MARGE, 20.0, &trf!("Plan de fabrication — miroir {}", "Manufacturing drawing — mirror {}", "Fertigungszeichnung — Spiegel {}", e.nom), Police::HelveticaGras, 14.0, Alignement::Gauche, BLEU_FONCE);
    p.texte(MARGE, 25.5, &trf!("Surface {} du système « {} »", "Surface {} of the system “{}”", "Fläche {} des Systems „{}“", i + 1, sys.nom), Police::Helvetica, 8.5, Alignement::Gauche, GRIS);
    p.ligne((MARGE, 28.0), (A4.0 - MARGE, 28.0), 0.4, BLEU_FONCE);

    let (dx, dy, dl, dh) = (MARGE, 32.0, A4.0 - 2.0 * MARGE, 118.0);
    let z_face_min = pts.iter().map(|q| q.0).fold(f64::MAX, f64::min);
    let z_face_max = pts.iter().map(|q| q.0).fold(f64::MIN, f64::max);
    let z_dos = z_face_max + epaisseur_support;
    let b = [z_face_min, z_dos, -d, d];
    let max_echelle = ((dl - 70.0) / (b[1] - b[0]).max(1e-6)).min((dh - 40.0) / (2.0 * d).max(1e-6));
    let echelle = echelle_normalisee(max_echelle);
    let cadre = Cadre { x: dx, y: dy, largeur: dl, hauteur: dh, echelle, z0: 0.5 * (b[0] + b[1]), y0: 0.0 };
    p.rectangle(dx, dy, dl, dh, 0.25, GRIS);
    p.polyligne_tirets(&[cadre.p(b[0] - 12.0 / echelle, 0.0), cadre.p(b[1] + 12.0 / echelle, 0.0)], 0.18, GRIS, Some((5.0, 1.2)));
    // Support (épaisseur indicative) en tirets, face optique en trait fort
    let (haut, bas) = (pts.last().copied().unwrap_or((0.0, d)), pts.first().copied().unwrap_or((0.0, -d)));
    p.polyligne_tirets(&cadre.pts(&[haut, (z_dos, d), (z_dos, -d), bas]), 0.3, GRIS, Some((2.0, 1.0)));
    let mut corps = pts.clone();
    corps.extend([(z_dos, d), (z_dos, -d)]);
    p.polygone(&cadre.pts(&corps), [0.93, 0.93, 0.93], None);
    p.polyligne(&cadre.pts(&pts), 0.7, [0.45, 0.0, 0.45]);
    // Diamètre
    let x_d = cadre.p(b[0], 0.0).0 - 14.0;
    let (h_d, bas_d) = (cadre.p(haut.0, d), cadre.p(bas.0, -d));
    p.ligne(h_d, (x_d - 1.5, h_d.1), 0.15, GRIS);
    p.ligne(bas_d, (x_d - 1.5, bas_d.1), 0.15, GRIS);
    p.cote((x_d, h_d.1), (x_d, bas_d.1), 0.2, NOIR);
    p.texte_tourne(x_d - 1.5, 0.5 * (h_d.1 + bas_d.1), 90.0, &format!("Ø {}", nb(2.0 * d, 3)), Police::HelveticaGras, 8.0, Alignement::Centre, NOIR);
    let r_txt = if s.rayon.is_finite() && s.rayon != 0.0 { format!("R {} {}", nb(s.rayon.abs(), 3), if concave { "CC" } else { "CX" }) } else { tr!("plan", "flat", "plan").into() };
    let y_texte = cadre.p(0.0, d).1 - 12.0;
    let x_texte = cadre.p(b[0], 0.0).0 - 3.0;
    p.texte(x_texte, y_texte, &r_txt, Police::Helvetica, 8.0, Alignement::Droite, BLEU_FONCE);
    if let Some(f) = s.fleche(0.7 * d) {
        p.polyligne(&[(x_texte + 1.0, y_texte + 1.0), cadre.p(signe * f, 0.7 * d)], 0.15, BLEU_FONCE);
    }
    p.texte(
        dx + 3.0,
        dy + dh - 3.0,
        &format!(
            "{} {} — {}",
            tr!("Échelle", "Scale", "Maßstab"),
            texte_echelle(echelle),
            tr!("support en tirets : épaisseur indicative (Ø/6), non définie par le système", "dashed blank: indicative thickness (Ø/6), not defined by the system", "Träger gestrichelt: Richtdicke (Ø/6), im System nicht festgelegt")
        ),
        Police::Helvetica,
        7.0,
        Alignement::Gauche,
        GRIS,
    );

    // Tableau
    let largeur = 0.5 * (A4.0 - 2.0 * MARGE - 4.0);
    let y0 = dy + dh + 10.0;
    let mut v = vec![(tr!("Rayon de courbure", "Radius of curvature", "Krümmungsradius").to_string(), r_txt.replace("R ", "") + if r_txt.starts_with('R') { " mm" } else { "" })];
    if s.rayon.is_finite() && s.rayon != 0.0 {
        let f = 0.5 * s.rayon.abs();
        v.push((tr!("Focale (R/2)", "Focal length (R/2)", "Brennweite (R/2)").into(), format!("{} mm", nb(f, 3))));
        v.push((tr!("Rapport F/D", "F/D ratio", "Öffnungsverhältnis F/D").into(), format!("F/{}", nb(f / (2.0 * d), 3))));
    }
    v.push((tr!("Ø utile", "Clear Ø", "Nutz-Ø").into(), format!("{} mm", nb(2.0 * d, 3))));
    v.push((tr!("Flèche au bord utile", "Sag at clear edge", "Pfeilhöhe am Nutzrand").into(), format!("{} mm", nb(fleche, 4))));
    if s.rayon_obstruction > 0.0 {
        v.push((tr!("Obstruction centrale", "Central obstruction", "Mittenabschattung").into(), format!("Ø {} mm", nb(2.0 * s.rayon_obstruction, 3))));
    }
    let bas_g = tableau(p, MARGE, y0, largeur, &format!("{} ({})", tr!("Face optique", "Optical face", "Optische Fläche"), trf!("surface {}", "surface {}", "Fläche {}", i + 1)), &v);

    let mut forme = Vec::new();
    let k = if matches!(s.type_surface, SurfaceType::Conique | SurfaceType::Aspherique) { s.conique } else { 0.0 };
    forme.push((tr!("Profil", "Profile", "Profil").to_string(), nom_conique(k).to_string()));
    forme.push((tr!("Conique k", "Conic k", "Konik k").into(), nb(k, 5)));
    if s.type_surface == SurfaceType::Aspherique {
        for (j, a) in s.aspheriques.iter().enumerate() {
            if *a != 0.0 {
                forme.push((format!("A{}", 4 + 2 * j), sci(*a)));
            }
        }
    }
    // Écart de profil à la sphère qui a le même sommet et passe par le bord utile
    if fleche > 0.0 && (k != 0.0 || s.type_surface == SurfaceType::Aspherique) {
        let z_bord = s.fleche(d).unwrap_or(0.0);
        let rs = (d * d + z_bord * z_bord) / (2.0 * z_bord);
        let mut ecart: f64 = 0.0;
        for j in 0..=200 {
            let r = d * j as f64 / 200.0;
            let zs = rs - rs.signum() * (rs * rs - r * r).max(0.0).sqrt();
            if let Some(zc) = s.fleche(r) {
                ecart = ecart.max((zc - zs).abs());
            }
        }
        forme.push((tr!("Écart à la sphère par le bord", "Departure from edge sphere", "Abweichung von der Randkugel").into(), format!("{} µm", nb(ecart * 1000.0, 3))));
        forme.push((tr!("Rayon de cette sphère", "Radius of that sphere", "Radius dieser Kugel").into(), format!("{} mm", nb(rs.abs(), 3))));
    }
    for t in tolerances(sys, i + 1, &[TypeTolerance::Rayon, TypeTolerance::Franges, TypeTolerance::Conique]) {
        forme.push((tr!("Tolérance", "Tolerance", "Toleranz").into(), t));
    }
    let bas_d2 = tableau(p, MARGE + largeur + 4.0, y0, largeur, tr!("Forme", "Shape", "Form"), &forme);
    let y = bas_g.max(bas_d2) + 7.0;
    p.texte(
        MARGE + 1.0,
        y,
        tr!(
            "• Écart à la sphère : différence de profil (pas de front d'onde) entre la face et la sphère de même sommet passant par le bord utile.",
            "• Departure: profile difference (not wavefront) between the face and the sphere with the same vertex through the clear edge.",
            "• Abweichung: Profilunterschied (nicht Wellenfront) zwischen Fläche und Kugel mit gleichem Scheitel durch den Nutzrand."
        ),
        Police::Helvetica,
        7.5,
        Alignement::Gauche,
        NOIR,
    );
}

/// Fiche complète. Retourne le PDF et les remarques (pièces qu'il a été impossible de dessiner)
pub fn fiche_pdf(sys: &OpticalSystem) -> (Vec<u8>, Vec<String>) {
    let mut doc = Document::nouveau(sys.nom.clone());
    let mut remarques = Vec::new();
    page_caracteristiques(&mut doc, sys, &mut remarques);
    pages_surfaces(&mut doc, sys);
    if sys.nb_configurations() > 1 {
        page_configurations(&mut doc, sys);
    }
    let elements = dessin::elements(sys);
    for e in &elements {
        match e.nature {
            NatureElement::Lentille => page_lentille(&mut doc, sys, e, &elements, &mut remarques),
            NatureElement::Miroir => page_miroir(&mut doc, sys, e, &mut remarques),
        }
    }
    remarques.sort();
    remarques.dedup();
    let total = doc.nb_pages();
    let nom = sys.nom.clone();
    for (k, p) in doc.pages_mut().iter_mut().enumerate() {
        p.ligne((MARGE, A4.1 - 10.0), (A4.0 - MARGE, A4.1 - 10.0), 0.2, GRIS);
        p.texte(MARGE, A4.1 - 6.5, &format!("FreeOptical — {nom}"), Police::Helvetica, 7.0, Alignement::Gauche, GRIS);
        p.texte(A4.0 - MARGE, A4.1 - 6.5, &format!("{} {} / {}", tr!("page", "page", "Seite"), k + 1, total), Police::Helvetica, 7.0, Alignement::Droite, GRIS);
    }
    (doc.octets(), remarques)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::presets;

    fn texte(o: &[u8]) -> String {
        String::from_utf8_lossy(o).into_owned()
    }

    #[test]
    fn fiche_avec_configurations() {
        let sys = presets::creer_lunette_deux_mises_au_point();
        let (o, _) = fiche_pdf(&sys);
        let t = texte(&o);
        // Caractéristiques + surfaces + configurations + 2 plans de lentilles
        assert!(t.contains("/Count 5"), "nombre de pages");
        assert!(t.contains("CONFIGURATIONS \\(2\\)") && t.contains("COMPARAISON DES CONFIGURATIONS"));
        assert!(t.contains(&nb(95.5598, 4)), "tirage de la config 2");
    }

    #[test]
    fn fiche_du_doublet() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let (o, remarques) = fiche_pdf(&sys);
        assert!(remarques.is_empty(), "{remarques:?}");
        let t = texte(&o);
        // Caractéristiques + surfaces + 2 plans de lentilles
        assert!(t.contains("/Count 4"), "nombre de pages");
        assert!(t.contains("Plan de fabrication ") && t.contains("lentille L2"));
        let par = calculer_paraxial(&sys);
        assert!(t.contains(&nb(par.focale_effl_mm, 3)), "focale affichée");
        assert!(t.contains("Lentille coll"), "doublet collé signalé");
    }

    #[test]
    fn epaisseur_au_bord_et_focale_d_une_lentille() {
        // Lentille biconvexe symétrique R 50, CT 6, Ø 30 dans le système vierge
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[0].rayon = 50.0;
        sys.surfaces[0].semi_diametre = 15.0;
        sys.surfaces[0].epaisseur = 6.0;
        sys.surfaces[1].rayon = -50.0;
        sys.surfaces[1].verre = "AIR".into();
        sys.surfaces[1].semi_diametre = 15.0;
        let f = sys.surfaces[0].fleche(15.0).unwrap();
        let (o, _) = fiche_pdf(&sys);
        let t = texte(&o);
        assert!(t.contains(&format!("ET {}", nb(6.0 - 2.0 * f, 3))), "épaisseur au bord");
        let n = sys.catalogue.resoudre(&sys.surfaces[0].verre).indice_refraction(sys.longueur_onde_ref_nm);
        let p = (n - 1.0) * (2.0 / 50.0 - (n - 1.0) * 6.0 / (n * 2500.0));
        assert!(t.contains(&format!("{} mm", nb(1.0 / p, 2))), "focale de la lentille épaisse");
        assert!(t.contains("R 50,000 CX"));
    }

    #[test]
    fn lentille_impossible_signalee() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[0].epaisseur = 0.5;
        let (o, _) = fiche_pdf(&sys);
        assert!(texte(&o).contains("ATTENTION"));
    }

    #[test]
    fn fiche_du_cassegrain_et_des_exemples() {
        let sys = presets::creer_telescope_cassegrain();
        let (o, remarques) = fiche_pdf(&sys);
        assert!(remarques.is_empty(), "{remarques:?}");
        let t = texte(&o);
        assert!(t.contains("miroir M1") && t.contains("miroir M2"));
        assert!(t.contains("parabole") || t.contains("hyperbole"));
        // Les autres exemples passent sans panique
        for s in [presets::creer_interferometre_bath(), presets::creer_singlet_aspherique(), presets::creer_spectrographe_reseau()] {
            let (o, _) = fiche_pdf(&s);
            assert!(o.starts_with(b"%PDF"));
        }
    }

    #[test]
    fn echelles_normalisees() {
        assert_eq!(echelle_normalisee(1.3), 1.0);
        assert_eq!(echelle_normalisee(3.0), 2.5);
        assert!((echelle_normalisee(0.3) - 0.25).abs() < 1e-12);
        assert_eq!(texte_echelle(0.5), "1:2");
        assert_eq!(texte_echelle(2.0), "2:1");
    }
}

/// Écrit les fiches et DXF des exemples dans le dossier $FREEOPTICAL_FICHES, pour les regarder :
/// FREEOPTICAL_FICHES=/tmp/fiches cargo test -- --ignored fiches_des_exemples
#[cfg(test)]
#[test]
#[ignore]
fn fiches_des_exemples() {
    use crate::core::presets;
    let dossier = std::path::PathBuf::from(std::env::var("FREEOPTICAL_FICHES").expect("FREEOPTICAL_FICHES"));
    std::fs::create_dir_all(&dossier).unwrap();
    for (nom, s) in [
        ("doublet", OpticalSystem::doublet_achromatique_defaut()),
        ("cassegrain", presets::creer_telescope_cassegrain()),
        ("singlet", presets::creer_singlet_aspherique()),
        ("spectro", presets::creer_spectrographe_reseau()),
        ("bath", presets::creer_interferometre_bath()),
        ("lunette_2_configs", presets::creer_lunette_deux_mises_au_point()),
    ] {
        let (o, r) = fiche_pdf(&s);
        std::fs::write(dossier.join(format!("{nom}.pdf")), o).unwrap();
        std::fs::write(dossier.join(format!("{nom}.dxf")), crate::core::dxf::exporter_dxf(&s).texte).unwrap();
        println!("{nom} : {r:?}");
    }
}
