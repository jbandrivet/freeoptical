//! Import des fichiers de lentilles Zemax OpticStudio séquentiels (`.zmx`).
//!
//! Format texte, en UTF-16 (avec ou sans BOM) dans les versions récentes, en ASCII/UTF-8 dans
//! les anciennes. Une ligne = un mot-clé de quatre lettres suivi de ses valeurs. Mots-clés repris :
//!
//! * système : `NAME`, `UNIT` (MM, CM, M, IN), `ENPD` (diamètre de pupille d'entrée), `FNUM`
//!   (nombre d'ouverture image), `OBNA` (ouverture numérique objet), `FLOA` (pupille fixée par
//!   le diaphragme), `FTYP` (type de champ : 0 angle, 1 hauteur objet, 2 hauteur image paraxiale,
//!   3 hauteur image réelle), `XFLN`/`YFLN` (ou `XFLD`/`YFLD`), `WAVL`/`WWGT` ou `WAVM i λ poids`
//!   (µm), `PWAV` (longueur d'onde principale) ;
//! * surfaces (`SURF n`, la surface 0 est l'objet, la dernière l'image) : `TYPE` (STANDARD,
//!   EVENASPH, COORDBRK, PARAXIAL, DGRATING), `CURV` (courbure), `DISZ` (épaisseur, INFINITY),
//!   `GLAS nom a b nd Vd…` (MIRROR pour un miroir), `DIAM` (demi-diamètre), `CONI`, `PARM i v`,
//!   `STOP`, `COMM`, `CLAP rmin rmax`, `OBSC rmin rmax`.
//!
//! `PARM` : EVENASPH, PARM n = coefficient de r^(2n) (PARM 2 → A4 … PARM 5 → A10) ; COORDBRK,
//! PARM 1-2 décentrement X/Y, PARM 3-5 inclinaisons X/Y/Z, PARM 6 ordre ; PARAXIAL, PARM 1
//! focale ; DGRATING, PARM 1 traits par µm, PARM 2 ordre. Les solves et reprises éventuels sont
//! importés avec leurs valeurs du moment, figées.

use crate::core::glass::GlassCatalog;
use crate::core::paraxial::{calculer_paraxial, index_diaphragme, tracer_paraxial};
use crate::core::types::{ApertureSpec, OpticalSystem, Surface, SurfaceType, Wavelength};
use std::collections::BTreeSet;
use std::path::Path;

/// Résultat d'un import : système et éléments non repris ou approchés
#[derive(Debug, Clone)]
pub struct ImportZemax {
    pub systeme: OpticalSystem,
    pub avertissements: Vec<String>,
}

/// Vrai pour un fichier `.zmx`
pub fn est_fichier_zemax(chemin: &Path) -> bool {
    chemin.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("zmx"))
}

pub fn importer_fichier_zemax(chemin: &Path) -> Result<ImportZemax, String> {
    let octets = std::fs::read(chemin).map_err(|e| format!("{}: {e}", chemin.display()))?;
    let mut import = importer_zemax(&octets)?;
    if import.systeme.nom.trim().is_empty() {
        import.systeme.nom = chemin.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    Ok(import)
}

/// Texte du fichier : UTF-16 petit ou gros boutiste (BOM ou octets nuls alternés), sinon UTF-8
/// (avec repli Latin-1)
pub fn decoder_texte(octets: &[u8]) -> String {
    let utf16 = |octets: &[u8], petit: bool| {
        let mots: Vec<u16> = octets
            .chunks_exact(2)
            .map(|c| if petit { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&mots)
    };
    match octets {
        [0xFF, 0xFE, reste @ ..] => utf16(reste, true),
        [0xFE, 0xFF, reste @ ..] => utf16(reste, false),
        [0xEF, 0xBB, 0xBF, reste @ ..] => String::from_utf8_lossy(reste).into_owned(),
        _ => {
            let n = octets.len().min(400);
            let nuls_impairs = octets[..n].iter().skip(1).step_by(2).filter(|&&b| b == 0).count();
            let nuls_pairs = octets[..n].iter().step_by(2).filter(|&&b| b == 0).count();
            if n >= 4 && nuls_impairs * 3 > n {
                utf16(octets, true)
            } else if n >= 4 && nuls_pairs * 3 > n {
                utf16(octets, false)
            } else {
                match std::str::from_utf8(octets) {
                    Ok(t) => t.to_string(),
                    Err(_) => octets.iter().map(|&b| b as char).collect(),
                }
            }
        }
    }
}

/// Données brutes d'une surface lue dans le fichier
#[derive(Debug, Clone, Default)]
struct SurfaceZmx {
    genre: String,
    courbure: f64,
    epaisseur: f64,
    verre: Option<Vec<String>>,
    demi_diametre: f64,
    conique: f64,
    parametres: Vec<(usize, f64)>,
    stop: bool,
    commentaire: String,
    clap: Option<(f64, f64)>,
    obsc: Option<(f64, f64)>,
}

fn nombre(t: Option<&str>) -> f64 {
    match t {
        Some(s) if s.eq_ignore_ascii_case("INFINITY") || s.eq_ignore_ascii_case("INF") => f64::INFINITY,
        Some(s) => s.trim_matches('"').parse().unwrap_or(0.0),
        None => 0.0,
    }
}

/// Mots-clés sans effet sur le tracé séquentiel (affichage, tolérances, réglages de l'interface)
const SANS_EFFET: &[&str] = &[
    "VERS", "MODE", "NOTE", "PFIL", "LANG", "ENVD", "GFAC", "GCAT", "RAIM", "PUSH", "SDMA", "ROPD", "PICB", "FWGN", "FWGT",
    "WWGN", "WAVN", "MNCA", "MNEA", "MNCG", "MNEG", "MXCA", "MXCG", "RGLA", "TOL", "TOLE", "MOFF", "MNUM", "HIDE", "MIRR",
    "SLAB", "POPS", "COAT", "FIMP", "BLNK", "TCED", "TCMM", "PPAR", "VPAR", "EDGE", "OPDX", "MAZH", "CONF", "PUPD", "EFFL",
    "UDAD", "USAP", "POLS", "GLRS", "COFN", "NSCD", "GSTD", "DMFS", "ISNA", "VDSZ", "TRAC", "PMAG", "TOTR", "MEMA", "OEMA",
    "FLAP", "VANN", "VCXN", "VCYN", "ZVDX", "ZVDY", "ZVCX", "ZVCY", "ZVAN", "XFIE", "YFIE", "PWAV", "WAVL", "WWGT", "WAVM",
    "BLAN", "SCOL", "SWID", "SQAP", "ELAP", "ELOB", "SQOB", "OBDC", "CWAV", "POLD", "GLCZ", "TCE", "OMMA", "OPTI",
];

pub fn importer_zemax(octets: &[u8]) -> Result<ImportZemax, String> {
    let texte = decoder_texte(octets);
    let mut avertissements = Vec::new();
    let mut non_repris = BTreeSet::new();
    let mut nom = String::new();
    let mut echelle = 1.0_f64;
    let mut ouverture: Option<ApertureSpec> = None;
    let mut flottante = false;
    let mut type_champ = 0;
    let (mut xs, mut ys): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let mut vignettage = false;
    let (mut wavl, mut wwgt): (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let mut wavm: Vec<(usize, f64, f64)> = Vec::new();
    let mut pwav = 1usize;
    let mut surfaces: Vec<SurfaceZmx> = Vec::new();
    let mut sequentiel = true;

    for ligne in texte.lines() {
        let ligne = ligne.trim();
        if ligne.is_empty() {
            continue;
        }
        let (mot, reste) = ligne.split_once(char::is_whitespace).unwrap_or((ligne, ""));
        let reste = reste.trim();
        let mut t = reste.split_whitespace();
        let surf = surfaces.last_mut();
        match (mot, surf) {
            ("NAME", _) => nom = reste.trim_matches('"').to_string(),
            ("MODE", _) => sequentiel = !reste.to_uppercase().starts_with("NSC"),
            ("UNIT", _) => {
                echelle = match t.next().unwrap_or("MM").to_uppercase().as_str() {
                    "MM" => 1.0,
                    "CM" => 10.0,
                    "METER" | "M" => 1000.0,
                    "IN" | "INCH" => 25.4,
                    autre => {
                        avertissements.push(trf!("unité {} inconnue : millimètres supposés", "unknown unit {}: millimetres assumed", "unbekannte Einheit {}: Millimeter angenommen", autre));
                        1.0
                    }
                }
            }
            ("ENPD", _) => ouverture = Some(ApertureSpec::PupilleEntreeMm(nombre(t.next()))),
            ("FNUM", _) => ouverture = Some(ApertureSpec::NombreOuverture(nombre(t.next()))),
            ("OBNA", _) => ouverture = Some(ApertureSpec::OuvertureNumeriqueObjet(nombre(t.next()))),
            ("FLOA", _) => flottante = true,
            ("FTYP", _) => type_champ = nombre(t.next()) as i32,
            ("XFLN" | "XFLD", _) => xs = t.map(|v| nombre(Some(v))).collect(),
            ("YFLN" | "YFLD", _) => ys = t.map(|v| nombre(Some(v))).collect(),
            ("VDXN" | "VDYN", _) => vignettage |= t.any(|v| nombre(Some(v)) != 0.0),
            ("WAVL", _) => wavl = t.map(|v| nombre(Some(v))).collect(),
            ("WWGT", _) => wwgt = t.map(|v| nombre(Some(v))).collect(),
            ("WAVM", _) => {
                let i = nombre(t.next()) as usize;
                let l = nombre(t.next());
                let w = t.next().map(|v| nombre(Some(v))).unwrap_or(1.0);
                wavm.push((i, l, w));
            }
            ("PWAV", _) => pwav = (nombre(t.next()) as usize).max(1),
            ("SURF", _) => surfaces.push(SurfaceZmx { genre: "STANDARD".into(), ..Default::default() }),
            ("TYPE", Some(s)) => s.genre = t.next().unwrap_or("STANDARD").to_uppercase(),
            ("CURV", Some(s)) => s.courbure = nombre(t.next()),
            ("DISZ", Some(s)) => s.epaisseur = nombre(t.next()),
            ("GLAS", Some(s)) => s.verre = Some(t.map(|v| v.to_string()).collect()),
            ("DIAM", Some(s)) => s.demi_diametre = nombre(t.next()),
            ("CONI", Some(s)) => s.conique = nombre(t.next()),
            ("PARM", Some(s)) => {
                let i = nombre(t.next()) as usize;
                s.parametres.push((i, nombre(t.next())));
            }
            ("STOP", Some(s)) => s.stop = true,
            ("COMM", Some(s)) => s.commentaire = reste.trim_matches('"').to_string(),
            ("CLAP", Some(s)) => s.clap = Some((nombre(t.next()), nombre(t.next()))),
            ("OBSC", Some(s)) => s.obsc = Some((nombre(t.next()), nombre(t.next()))),
            (m, _) if SANS_EFFET.contains(&m) => {}
            (m, _) => {
                non_repris.insert(m.to_string());
            }
        }
    }
    if !sequentiel {
        return Err(tr!(
            "fichier non séquentiel (NSC) : seul le mode séquentiel est pris en charge",
            "non-sequential (NSC) file: only sequential mode is supported",
            "nichtsequentielle Datei (NSC): nur der sequentielle Modus wird unterstützt"
        )
        .to_string());
    }
    if surfaces.len() < 3 {
        return Err(tr!(
            "fichier .zmx sans surfaces (il faut l'objet, au moins une surface et l'image)",
            "a .zmx file without surfaces (object, at least one surface and the image are needed)",
            "eine .zmx-Datei ohne Flächen (Objekt, mindestens eine Fläche und das Bild sind nötig)"
        )
        .to_string());
    }

    // Longueurs d'onde (µm → nm) ; Zemax récent écrit ses 24 emplacements, les vides valant 0,55
    let mut longueurs: Vec<(f64, f64)> = if !wavm.is_empty() {
        wavm.sort_by_key(|w| w.0);
        let mut v: Vec<(f64, f64)> = wavm.iter().map(|w| (w.1 * 1000.0, w.2)).collect();
        while v.len() > pwav.max(1) && v.last().is_some_and(|w| (w.0 - 550.0).abs() < 1e-9) {
            v.pop();
        }
        v
    } else {
        wavl.iter().enumerate().map(|(i, &l)| (l * 1000.0, wwgt.get(i).copied().unwrap_or(1.0))).collect()
    };
    longueurs.retain(|w| w.0 > 0.0);
    if longueurs.is_empty() {
        longueurs.push((587.5618, 1.0));
    }
    let lambda_ref = longueurs.get(pwav - 1).map(|w| w.0).unwrap_or(longueurs[0].0);

    // Surfaces : la première est l'objet, la dernière l'image
    let objet = surfaces.remove(0);
    let catalogue = GlassCatalog::standard();
    let mut sys = OpticalSystem::nouveau(nom.clone(), lambda_ref);
    sys.longueurs_onde = longueurs.iter().map(|&(l, w)| Wavelength::new(l, w, format!("{l:.1} nm"))).collect();
    sys.distance_objet_mm = (objet.epaisseur.is_finite() && objet.epaisseur > 0.0).then_some(objet.epaisseur * echelle);
    if objet.epaisseur.is_finite() && objet.epaisseur <= 0.0 {
        avertissements.push(tr!(
            "objet virtuel ou au contact de la première surface : placé à l'infini",
            "virtual object or object touching the first surface: set at infinity",
            "virtuelles Objekt oder Objekt an der ersten Fläche: ins Unendliche gesetzt"
        )
        .to_string());
    }
    let nb = surfaces.len();
    let mut verres_inconnus = BTreeSet::new();
    for (k, z) in surfaces.iter().enumerate() {
        let est_image = k + 1 == nb;
        let genre = z.genre.as_str();
        let rayon = if z.courbure.abs() > 1e-15 { echelle / z.courbure } else { f64::INFINITY };
        let type_surface = match genre {
            "COORDBRK" => SurfaceType::CoupureRepere,
            "PARAXIAL" => SurfaceType::Ideale,
            "EVENASPH" => SurfaceType::Aspherique,
            _ if !rayon.is_finite() && z.conique == 0.0 => SurfaceType::Plane,
            _ if z.conique != 0.0 => SurfaceType::Conique,
            _ => SurfaceType::Spherique,
        };
        if !matches!(genre, "STANDARD" | "EVENASPH" | "COORDBRK" | "PARAXIAL" | "DGRATING") {
            avertissements.push(trf!(
                "surface {} : type {} non pris en charge, importée comme surface standard",
                "surface {}: type {} not supported, imported as a standard surface",
                "Fläche {}: Typ {} nicht unterstützt, als Standardfläche importiert",
                k + 1,
                genre
            ));
        }
        // Milieu après la surface
        let verre = match &z.verre {
            _ if type_surface == SurfaceType::CoupureRepere => "AIR".to_string(),
            None => "AIR".to_string(),
            Some(v) => {
                let nom = v.first().map(|s| s.to_uppercase()).unwrap_or_default();
                let nd = nombre(v.get(3).map(|s| s.as_str()));
                let vd = nombre(v.get(4).map(|s| s.as_str()));
                if nom == "MIRROR" {
                    "MIROIR".to_string()
                } else if nom != "___BLANK" && catalogue.est_connu(&nom) {
                    nom
                } else if nd > 1.0 {
                    if nom != "___BLANK" {
                        verres_inconnus.insert(trf!("{} (modèle nd = {}, Vd = {})", "{} (model nd = {}, Vd = {})", "{} (Modell nd = {}, Vd = {})", nom, nd, vd));
                    }
                    if vd > 0.0 { format!("{nd}:{vd}") } else { format!("{nd}") }
                } else {
                    verres_inconnus.insert(trf!("{} (remplacé par l'air)", "{} (replaced by air)", "{} (durch Luft ersetzt)", nom));
                    "AIR".to_string()
                }
            }
        };
        let epaisseur = if est_image { 0.0 } else if z.epaisseur.is_finite() { z.epaisseur * echelle } else { 0.0 };
        let mut s = Surface::nouvelle(type_surface, rayon, epaisseur, verre, (z.demi_diametre * echelle).max(0.1));
        s.conique = z.conique;
        s.commentaire = z.commentaire.clone();
        s.est_diaphragme = z.stop && !est_image;
        if z.demi_diametre <= 0.0 {
            s.semi_diametre = 0.0;
        }
        let parm = |i: usize| z.parametres.iter().rev().find(|p| p.0 == i).map(|p| p.1).unwrap_or(0.0);
        match genre {
            "EVENASPH" => {
                // PARM n : coefficient de r^(2n), mis à l'échelle de l'unité (A' = A·s^(1−2n))
                for n in 2..=5 {
                    s.aspheriques[n - 2] = parm(n) * echelle.powi(1 - 2 * n as i32);
                }
                if parm(1) != 0.0 || (6..=8).any(|n| parm(n) != 0.0) {
                    avertissements.push(trf!(
                        "surface {} : termes asphériques en r², r¹², r¹⁴ ou r¹⁶ ignorés",
                        "surface {}: aspheric r², r¹², r¹⁴ or r¹⁶ terms ignored",
                        "Fläche {}: asphärische Terme in r², r¹², r¹⁴ oder r¹⁶ ignoriert",
                        k + 1
                    ));
                }
            }
            "COORDBRK" => {
                s.decentrement = [parm(1) * echelle, parm(2) * echelle];
                s.inclinaison_deg = [parm(3), parm(4)];
                s.inclinaison_z_deg = parm(5);
                s.inclinaison_avant_decentrement = parm(6) != 0.0;
                s.commentaire = if z.commentaire.is_empty() { tr!("Rupture de repère", "Coordinate break", "Koordinatenbruch").to_string() } else { z.commentaire.clone() };
            }
            "PARAXIAL" => s.focale_ideale_mm = parm(1) * echelle,
            "DGRATING" => {
                s.reseau_traits_mm = parm(1) * 1000.0 / echelle;
                s.reseau_ordre = parm(2).round() as i32;
            }
            _ => {}
        }
        if let Some((rmin, rmax)) = z.clap {
            if rmax > 0.0 {
                s.semi_diametre = rmax * echelle;
            }
            s.rayon_obstruction = rmin.max(0.0) * echelle;
        }
        if let Some((rmin, rmax)) = z.obsc {
            if rmin > 0.0 {
                avertissements.push(trf!(
                    "surface {} : obstruction annulaire réduite à une obstruction centrale de {} mm",
                    "surface {}: annular obscuration reduced to a central one of {} mm",
                    "Fläche {}: ringförmige Abschattung auf eine zentrale von {} mm reduziert",
                    k + 1,
                    rmax * echelle
                ));
            }
            s.rayon_obstruction = rmax * echelle;
        }
        if est_image {
            s.commentaire = if z.commentaire.is_empty() { tr!("Plan Image", "Image Plane", "Bildebene").to_string() } else { z.commentaire.clone() };
            s.verre = "AIR".into();
        }
        sys.surfaces.push(s);
    }
    sys.surfaces.drain(..2); // surfaces de `OpticalSystem::nouveau`
    if !sys.surfaces.iter().any(|s| s.est_diaphragme) {
        avertissements.push(tr!("aucun diaphragme (STOP) : première surface prise", "no STOP: first surface used", "keine Blende (STOP): erste Fläche verwendet").to_string());
        sys.surfaces[0].est_diaphragme = true;
    }
    for v in verres_inconnus {
        avertissements.push(trf!(
            "verre absent des catalogues : {} — importez le catalogue .agf pour la dispersion exacte",
            "glass missing from the catalogues: {} — import the .agf catalogue for the exact dispersion",
            "Glas nicht in den Katalogen: {} — den .agf-Katalog für die genaue Dispersion importieren",
            v
        ));
    }

    // Ouverture
    let stop = index_diaphragme(&sys);
    sys.ouverture = ouverture.clone().unwrap_or(ApertureSpec::PupilleEntreeMm(10.0));
    if flottante || ouverture.is_none() {
        // Pupille d'entrée fixée par le demi-diamètre du diaphragme : grandissement paraxial
        // entre la pupille d'entrée (hauteur 1 sur la première surface) et le diaphragme
        let lambda = sys.longueur_onde_ref_nm;
        let (y, u, _) = tracer_paraxial(&sys, lambda, 0, stop, 1.0, 0.0);
        let a = if stop == 0 { 1.0 } else { y + sys.surfaces[stop - 1].epaisseur * u };
        let sd = sys.surfaces[stop].semi_diametre;
        if a.abs() > 1e-12 && sd > 0.0 {
            sys.ouverture = ApertureSpec::PupilleEntreeMm(2.0 * sd / a.abs());
        }
        if ouverture.is_none() && !flottante {
            avertissements.push(tr!("ouverture absente : déduite du diaphragme", "aperture missing: derived from the stop", "Öffnung fehlt: aus der Blende abgeleitet").to_string());
        }
    }

    // Champs : on garde le champ 0 et ceux jusqu'au dernier non nul
    let n = xs.len().max(ys.len());
    let mut champs: Vec<(f64, f64)> = (0..n).map(|i| (xs.get(i).copied().unwrap_or(0.0), ys.get(i).copied().unwrap_or(0.0))).collect();
    let dernier = champs.iter().rposition(|c| c.0 != 0.0 || c.1 != 0.0).unwrap_or(0);
    champs.truncate(dernier + 1);
    if champs.is_empty() {
        champs.push((0.0, 0.0));
    }
    match type_champ {
        0 => {
            if sys.distance_objet_mm.is_some() {
                avertissements.push(tr!(
                    "champs en angle avec un objet à distance finie : objet placé à l'infini",
                    "angular fields with an object at finite distance: object set at infinity",
                    "Winkelfelder bei endlicher Objektentfernung: Objekt ins Unendliche gesetzt"
                )
                .to_string());
                sys.distance_objet_mm = None;
            }
        }
        1 => {
            if sys.distance_objet_mm.is_none() {
                return Err(tr!(
                    "champs en hauteur d'objet avec un objet à l'infini",
                    "object-height fields with an object at infinity",
                    "Objekthöhenfelder bei einem Objekt im Unendlichen"
                )
                .to_string());
            }
            for c in &mut champs {
                *c = (c.0 * echelle, c.1 * echelle);
            }
        }
        2 | 3 => {
            // Hauteurs image : converties par l'optique paraxiale (f·tan θ, ou h / grandissement)
            sys.champs_deg = vec![(0.0, 0.0)];
            let p = calculer_paraxial(&sys);
            for c in &mut champs {
                *c = match sys.distance_objet_mm {
                    None => ((c.0 * echelle / p.focale_effl_mm).atan().to_degrees().abs() * c.0.signum(), (c.1 * echelle / p.focale_effl_mm).atan().to_degrees().abs() * c.1.signum()),
                    Some(_) => (c.0 * echelle / p.grandissement, c.1 * echelle / p.grandissement),
                };
            }
            avertissements.push(if type_champ == 2 {
                tr!(
                    "champs en hauteur image paraxiale convertis en champs objet",
                    "paraxial image height fields converted to object fields",
                    "paraxiale Bildhöhenfelder in Objektfelder umgerechnet"
                )
                .to_string()
            } else {
                tr!(
                    "champs en hauteur image réelle convertis en champs objet par l'optique paraxiale (distorsion négligée)",
                    "real image height fields converted to object fields with paraxial optics (distortion neglected)",
                    "reale Bildhöhenfelder paraxial in Objektfelder umgerechnet (Verzeichnung vernachlässigt)"
                )
                .to_string()
            });
        }
        t => avertissements.push(trf!("type de champ {} non pris en charge : angles supposés", "field type {} not supported: angles assumed", "Feldtyp {} nicht unterstützt: Winkel angenommen", t)),
    }
    sys.champs_deg = champs;
    if vignettage {
        avertissements.push(tr!(
            "facteurs de vignettage (VDX/VDY) ignorés",
            "vignetting factors (VDX/VDY) ignored",
            "Vignettierungsfaktoren (VDX/VDY) ignoriert"
        )
        .to_string());
    }

    // Demi-diamètres absents : ceux qu'il faut pour transmettre tous les faisceaux
    if sys.surfaces.iter().any(|s| s.semi_diametre <= 0.0) {
        let requis = crate::core::empreinte::demi_diametres_requis(&{
            let mut libre = sys.clone();
            for s in &mut libre.surfaces {
                s.semi_diametre = s.semi_diametre.max(0.1);
            }
            libre
        });
        for (s, r) in sys.surfaces.iter_mut().zip(requis) {
            if s.semi_diametre <= 0.0 {
                s.semi_diametre = (r * 1.0001).max(0.1);
            }
        }
    }
    if !non_repris.is_empty() {
        avertissements.push(trf!(
            "mots-clés non repris : {}",
            "keywords not imported: {}",
            "nicht übernommene Schlüsselwörter: {}",
            non_repris.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(ImportZemax { systeme: sys, avertissements })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::analysis::calculer_spot_diagram;

    /// Encode en UTF-16 petit boutiste avec BOM, comme OpticStudio
    fn utf16(texte: &str) -> Vec<u8> {
        let mut o = vec![0xFF, 0xFE];
        for u in texte.encode_utf16() {
            o.extend(u.to_le_bytes());
        }
        o
    }

    const DOUBLET: &str = "VERS 190513 80 123457 L123457\r\nMODE SEQ\r\nNAME Doublet 100 mm\r\nUNIT MM X W X CM MR CPMM\r\n\
ENPD 24\r\nENVD 2.0E+1 1 0\r\nGFAC 0 0\r\nGCAT SCHOTT\r\nRAIM 0 0 1 1 0 0 0 0 0 1\r\nFTYP 0 0 3 3 0 0 0\r\n\
XFLN 0 0 0 0 0 0 0 0 0 0 0 0\r\nYFLN 0 1.75 2.5 0 0 0 0 0 0 0 0 0\r\nFWGN 1 1 1 1 1 1 1 1 1 1 1 1\r\n\
WAVM 1 0.4861327 1\r\nWAVM 2 0.5875618 1\r\nWAVM 3 0.6562725 1\r\nWAVM 4 0.55 1\r\nWAVM 5 0.55 1\r\nPWAV 2\r\n\
SURF 0\r\n  TYPE STANDARD\r\n  CURV 0.0 0 0 0 0 \"\"\r\n  HIDE 0 0 0 0 0 0 0 0 0 0\r\n  MIRR 2 0\r\n  DISZ INFINITY\r\n\
SURF 1\r\n  STOP\r\n  COMM Crown\r\n  TYPE STANDARD\r\n  CURV 2.200514E-2 0 0 0 0 \"\"\r\n  DISZ 7\r\n  GLAS N-BK7 0 0 1.5168 6.4167336E+1 0 0 0 0 0 0\r\n  DIAM 15 1 0 0 1 \"\"\r\n\
SURF 2\r\n  TYPE STANDARD\r\n  CURV -2.286973E-2 0 0 0 0 \"\"\r\n  DISZ 2.5\r\n  GLAS F2 0 0 1.62004 3.6366 0 0 0 0 0 0\r\n  DIAM 15 1 0 0 1 \"\"\r\n\
SURF 3\r\n  TYPE STANDARD\r\n  CURV -1.410601E-3 0 0 0 0 \"\"\r\n  DISZ 93.6852\r\n  DIAM 15 1 0 0 1 \"\"\r\n\
SURF 4\r\n  TYPE STANDARD\r\n  CURV 0.0 0 0 0 0 \"\"\r\n  DISZ 0\r\n  DIAM 6 0 0 0 1 \"\"\r\nTOL TOFF 0 0 0 0 0 0\r\nZZZZ 1 2\r\n";

    #[test]
    fn doublet_utf16_identique_au_doublet_du_programme() {
        let import = importer_zemax(&utf16(DOUBLET)).unwrap();
        let sys = &import.systeme;
        assert_eq!(sys.nom, "Doublet 100 mm");
        assert_eq!(sys.surfaces.len(), 4);
        assert_eq!(sys.longueurs_onde.len(), 3, "les emplacements vides à 0,55 µm sont retirés");
        assert!((sys.longueur_onde_ref_nm - 587.5618).abs() < 1e-9);
        assert_eq!(sys.champs_deg, vec![(0.0, 0.0), (0.0, 1.75), (0.0, 2.5)]);
        assert_eq!(sys.surfaces[0].verre, "N-BK7");
        assert_eq!(sys.surfaces[0].commentaire, "Crown");
        assert!(sys.surfaces[0].est_diaphragme);
        assert!(import.avertissements.iter().any(|a| a.contains("ZZZZ")), "{:?}", import.avertissements);
        let reference = OpticalSystem::doublet_achromatique_defaut();
        let (a, b) = (calculer_paraxial(sys), calculer_paraxial(&reference));
        assert!((a.focale_effl_mm - b.focale_effl_mm).abs() < 1e-3, "{} / {}", a.focale_effl_mm, b.focale_effl_mm);
        let (sa, sb) = (calculer_spot_diagram(sys, (0.0, 2.5), 6), calculer_spot_diagram(&reference, (0.0, 2.5), 6));
        assert!((sa.rayon_rms_um - sb.rayon_rms_um).abs() < 0.05, "{} / {}", sa.rayon_rms_um, sb.rayon_rms_um);
        // Même fichier en ASCII et en pouces
        let pouces = DOUBLET.replace("UNIT MM", "UNIT IN");
        let p = importer_zemax(pouces.as_bytes()).unwrap();
        assert!((calculer_paraxial(&p.systeme).focale_effl_mm - 25.4 * a.focale_effl_mm).abs() < 0.05);
    }

    #[test]
    fn newton_replie_asphere_et_lentille_paraxiale() {
        // Newton F/5 replié par un miroir plan entre deux ruptures, puis plan image ; le
        // demi-diamètre du plan est absent (0) : il est calculé
        let newton = "MODE SEQ\nNAME Newton\nUNIT MM\nENPD 200\nFTYP 0 0 2 1 0 0 0\nXFLN 0 0\nYFLN 0 0.3\nWAVL 0.5875618\nWWGT 1\n\
SURF 0\n DISZ INFINITY\nSURF 1\n STOP\n CURV -5.0E-4\n CONI -1\n DISZ -800\n GLAS MIRROR 0 0 0 0\n DIAM 100 1\n OBSC 0 30\n\
SURF 2\n TYPE COORDBRK\n DISZ 0\n PARM 1 0\n PARM 2 0\n PARM 3 45\n PARM 4 0\n PARM 5 0\n PARM 6 0\n\
SURF 3\n DISZ 0\n GLAS MIRROR 0 0 0 0\n DIAM 0 0\nSURF 4\n TYPE COORDBRK\n DISZ 200\n PARM 3 45\nSURF 5\n DIAM 10\n";
        let import = importer_zemax(newton.as_bytes()).unwrap();
        let sys = &import.systeme;
        assert_eq!(sys.surfaces[1].type_surface, SurfaceType::CoupureRepere);
        assert_eq!(sys.surfaces[0].rayon_obstruction, 30.0);
        assert!(sys.surfaces[2].semi_diametre > 20.0 && sys.surfaces[2].semi_diametre < 60.0, "{}", sys.surfaces[2].semi_diametre);
        let spot = calculer_spot_diagram(sys, (0.0, 0.0), 6);
        assert!(spot.rayon_rms_um < 1e-3 && spot.nb_rayons_valides > 100);
        let img = sys.reperes()[4];
        assert!((img.origine.y + 200.0).abs() < 1e-9 && (img.origine.z + 800.0).abs() < 1e-9);

        // Asphère paire et lentille paraxiale
        let texte = "NAME essai\nENPD 10\nYFLN 0 1\nWAVL 0.55\nSURF 0\n DISZ INFINITY\nSURF 1\n STOP\n TYPE EVENASPH\n CURV 0.01\n PARM 2 1.0E-6\n PARM 3 -2.0E-9\n DISZ 3\n GLAS ___BLANK 1 0 1.6 40 0\n DIAM 8\n\
SURF 2\n TYPE PARAXIAL\n PARM 1 50\n DISZ 40\n DIAM 8\nSURF 3\n TYPE TOROIDAL\n DISZ 0\n DIAM 5\n";
        let import = importer_zemax(texte.as_bytes()).unwrap();
        let s = &import.systeme.surfaces;
        assert_eq!(s[0].type_surface, SurfaceType::Aspherique);
        assert_eq!(s[0].aspheriques, [1.0e-6, -2.0e-9, 0.0, 0.0]);
        assert_eq!(s[0].verre, "1.6:40");
        assert_eq!(s[1].type_surface, SurfaceType::Ideale);
        assert_eq!(s[1].focale_ideale_mm, 50.0);
        assert!(import.avertissements.iter().any(|a| a.contains("TOROIDAL")));
    }

    #[test]
    fn champs_en_hauteur_image_et_pupille_flottante() {
        // Lentille paraxiale f = 100, diaphragme de 5 mm de demi-diamètre, champ image 10 mm
        let texte = "NAME f100\nFLOA\nFTYP 2 0 2\nYFLN 0 10\nWAVL 0.55\nSURF 0\n DISZ INFINITY\nSURF 1\n STOP\n TYPE PARAXIAL\n PARM 1 100\n DISZ 100\n DIAM 5\nSURF 2\n DIAM 20\n";
        let import = importer_zemax(texte.as_bytes()).unwrap();
        let sys = &import.systeme;
        assert_eq!(sys.ouverture, ApertureSpec::PupilleEntreeMm(10.0));
        assert!((sys.champs_deg[1].1 - 0.1f64.atan().to_degrees()).abs() < 1e-9);
        let spot = calculer_spot_diagram(sys, sys.champs_deg[1], 4);
        assert!((spot.centroide_mm.1.abs() - 10.0).abs() < 1e-6);
        // Fichier non séquentiel : refusé
        assert!(importer_zemax(b"MODE NSC\n").is_err());
    }
}
