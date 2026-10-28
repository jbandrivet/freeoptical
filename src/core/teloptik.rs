//! Import des fichiers de conception des programmes du livre *Telescope Optics: Evaluation and
//! Design* (H. G. J. Rutten et M. A. M. van Venrooij, Willmann-Bell, 1988) :
//! `.TDS` (TDESIGN, télescopes à miroirs et catadioptriques), `.RLD` (LENSDES, objectifs de
//! lunette) et `.DES` (RAYTRACE, systèmes divers).
//!
//! Ces fichiers sont écrits par l'instruction `WRITE #` du Turbo Basic : des valeurs séparées
//! par des virgules ou des fins de ligne, les chaînes entre guillemets. Leur structure :
//!
//! ```text
//! "Titre"
//! nb_surfaces, nb_indices, diamètre_pupille_entrée, focale, (valeur propre au programme)
//! pour chaque surface :
//!   rayon, distance_depuis_la_surface_précédente, code
//!   diamètre_max, diamètre_obstruction, décalage_axial, décentrement_x, sinus_bascule
//!   [code -1  : constante conique K]
//!   [code 1-5 : autant de coefficients asphériques, en r², r⁴, r⁶, r⁸, r¹⁰]
//!   [code 9   : lame de Schmidt — puissance relative, zone neutre, rayon du primaire, indice]
//!   nb_indices indices du milieu suivant (N1 = raie d, N2 = rouge C, N3 = bleu F),
//!   négatifs quand la lumière repart vers -Z après une réflexion
//! (paramètres propres à TDESIGN / LENSDES, ou drapeaux "Y"/"N" vignettage et décentrement)
//! ```
//!
//! Codes de surface : 0 sphère, -1 conique, 1 à 5 asphère, 6 pupille, 7 baffle, 8 diaphragme,
//! 9 lame de Schmidt, 10 plan. Un rayon nul désigne une surface plane. Les conventions de signe
//! (rayons, distances négatives après un miroir) sont celles de FreeOptical.

use crate::core::glass::LAMBDA_D_NM;
use crate::core::paraxial::calculer_paraxial;
use crate::core::raytracer::{VisePupille, tracer_rayon_unique};
use crate::core::types::{ApertureSpec, OpticalSystem, Surface, SurfaceType, Wavelength};
use crate::core::glass::GlassCatalog;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Extensions reconnues (comparaison insensible à la casse)
pub const EXTENSIONS_TELOPTIK: [&str; 3] = ["tds", "rld", "des"];

/// Résultat d'un import : le système reconstruit et ce que FreeOptical n'a pas pu reprendre
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportTeloptik {
    pub systeme: OpticalSystem,
    /// Éléments du fichier ignorés ou approchés (obstructions, couleurs supplémentaires…)
    pub avertissements: Vec<String>,
    /// Focale inscrite dans l'en-tête du fichier (mm, signée : négative après un nombre impair
    /// de miroirs), à comparer à la focale paraxiale recalculée par FreeOptical
    pub focale_fichier_mm: f64,
}

/// Indique si le chemin porte l'extension d'un fichier TDESIGN / LENSDES / RAYTRACE
pub fn est_fichier_teloptik(chemin: &Path) -> bool {
    chemin
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS_TELOPTIK.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// Lit et convertit un fichier `.TDS`, `.RLD` ou `.DES`
pub fn importer_fichier_teloptik(chemin: &Path) -> Result<ImportTeloptik, String> {
    let octets = std::fs::read(chemin)
        .map_err(|e| format!("Lecture de '{}' impossible : {}", chemin.display(), e))?;
    importer_teloptik(&octets)
        .map_err(|e| format!("Fichier '{}' : {}", chemin.display(), e))
}

/// Moitié haute de la page de code 437 (MS-DOS), pour les titres accentués
const CP437_HAUT: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■ ";

fn decoder_cp437(octets: &[u8]) -> String {
    let haut: Vec<char> = CP437_HAUT.chars().collect();
    octets
        .iter()
        .map(|&o| if o < 0x80 { o as char } else { haut[(o - 0x80) as usize] })
        .collect()
}

/// Découpe le fichier comme `INPUT #` : champs séparés par des virgules ou des fins de ligne,
/// chaînes entre guillemets. Les lignes vides ne produisent aucun champ.
fn decouper_champs(texte: &str) -> Vec<String> {
    let mut champs = Vec::new();
    let mut courant = String::new();
    let mut entre_guillemets = false;
    let mut champ_ouvert = false;

    for ch in texte.chars() {
        if entre_guillemets {
            if ch == '"' {
                entre_guillemets = false;
            } else {
                courant.push(ch);
            }
            continue;
        }
        match ch {
            '"' => {
                entre_guillemets = true;
                champ_ouvert = true;
            }
            ',' => {
                champs.push(courant.trim().to_string());
                courant.clear();
                champ_ouvert = false;
            }
            '\r' | '\n' | '\x1a' => {
                if champ_ouvert || !courant.trim().is_empty() {
                    champs.push(courant.trim().to_string());
                }
                courant.clear();
                champ_ouvert = false;
            }
            _ => {
                if !ch.is_whitespace() {
                    champ_ouvert = true;
                }
                courant.push(ch);
            }
        }
    }
    if champ_ouvert || !courant.trim().is_empty() {
        champs.push(courant.trim().to_string());
    }
    champs
}

struct Lecteur {
    champs: Vec<String>,
    pos: usize,
}

impl Lecteur {
    fn texte(&mut self, quoi: &str) -> Result<String, String> {
        let champ = self
            .champs
            .get(self.pos)
            .cloned()
            .ok_or_else(|| format!("fin de fichier prématurée (attendu : {quoi})"))?;
        self.pos += 1;
        Ok(champ)
    }

    fn nombre(&mut self, quoi: &str) -> Result<f64, String> {
        let champ = self.texte(quoi)?;
        // Turbo Basic écrit les doubles avec un exposant `D` (1.5D-03)
        let normalise = champ.replace(['D', 'd'], "E");
        if normalise.is_empty() {
            return Ok(0.0); // INPUT # lit un champ vide comme 0
        }
        normalise
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("valeur « {champ} » illisible (attendu : {quoi})"))
    }

    fn reste(&self) -> &[String] {
        &self.champs[self.pos.min(self.champs.len())..]
    }
}

/// Données d'une surface telles qu'écrites dans le fichier
struct SurfaceBrute {
    rayon: f64,
    distance: f64,
    code: i32,
    diametre_max: f64,
    diametre_obstruction: f64,
    decalage_axial: f64,
    decentrement_x: f64,
    sinus_bascule: f64,
    conique: f64,
    /// Coefficients en r², r⁴, r⁶, r⁸, r¹⁰
    coefficients: [f64; 5],
    /// Lame de Schmidt : puissance relative, zone neutre, rayon du primaire, indice de calcul
    schmidt: Option<[f64; 4]>,
    indices: Vec<f64>,
}

fn libelle_code(code: i32) -> &'static str {
    match code {
        0 => "Sphère",
        -1 => "Conique",
        1..=5 => "Asphère",
        6 => "Pupille",
        7 => "Baffle",
        8 => "Diaphragme",
        9 => "Lame de Schmidt",
        _ => "Plan",
    }
}

/// Convertit le contenu brut d'un fichier `.TDS`, `.RLD` ou `.DES` en système FreeOptical
pub fn importer_teloptik(octets: &[u8]) -> Result<ImportTeloptik, String> {
    let mut lecteur = Lecteur {
        champs: decouper_champs(&decoder_cp437(octets)),
        pos: 0,
    };
    let mut avertissements = Vec::new();

    let titre = lecteur.texte("titre")?.trim().to_string();
    let nb_surfaces = lecteur.nombre("nombre de surfaces")?;
    let nb_indices = lecteur.nombre("nombre d'indices")?;
    if !(1.0..=35.0).contains(&nb_surfaces) || nb_surfaces.fract() != 0.0 {
        return Err(format!(
            "nombre de surfaces {nb_surfaces} invalide (1 à 35) : ce n'est pas un fichier TDESIGN, LENSDES ou RAYTRACE"
        ));
    }
    if !(1.0..=5.0).contains(&nb_indices) || nb_indices.fract() != 0.0 {
        return Err(format!("nombre d'indices par surface {nb_indices} invalide (1 à 5)"));
    }
    let (nb_surfaces, nb_indices) = (nb_surfaces as usize, nb_indices as usize);
    let pupille_mm = lecteur.nombre("diamètre de la pupille d'entrée")?;
    if pupille_mm <= 0.0 {
        return Err(format!("diamètre de pupille d'entrée {pupille_mm} invalide"));
    }
    let focale_fichier_mm = lecteur.nombre("focale")?;
    lecteur.nombre("paramètre d'en-tête")?;

    let mut brutes = Vec::with_capacity(nb_surfaces);
    for i in 1..=nb_surfaces {
        let rayon = lecteur.nombre(&format!("rayon de la surface {i}"))?;
        let distance = lecteur.nombre(&format!("distance de la surface {i}"))?;
        let code_f = lecteur.nombre(&format!("code de la surface {i}"))?;
        if !(-1.0..=10.0).contains(&code_f) || code_f.fract() != 0.0 {
            return Err(format!("code de surface {code_f} inconnu (surface {i})"));
        }
        let code = code_f as i32;
        let mut s = SurfaceBrute {
            rayon,
            distance,
            code,
            diametre_max: lecteur.nombre(&format!("diamètre max. de la surface {i}"))?,
            diametre_obstruction: lecteur.nombre(&format!("obstruction de la surface {i}"))?,
            decalage_axial: lecteur.nombre(&format!("décalage axial de la surface {i}"))?,
            decentrement_x: lecteur.nombre(&format!("décentrement de la surface {i}"))?,
            sinus_bascule: lecteur.nombre(&format!("bascule de la surface {i}"))?,
            conique: 0.0,
            coefficients: [0.0; 5],
            schmidt: None,
            indices: Vec::with_capacity(nb_indices),
        };
        match code {
            -1 => s.conique = lecteur.nombre(&format!("constante conique de la surface {i}"))?,
            1..=5 => {
                for k in 0..code as usize {
                    s.coefficients[k] =
                        lecteur.nombre(&format!("coefficient asphérique {} de la surface {i}", k + 1))?;
                }
            }
            9 => {
                let mut p = [0.0; 4];
                for (k, v) in p.iter_mut().enumerate() {
                    *v = lecteur.nombre(&format!("paramètre {} de la lame de Schmidt (surface {i})", k + 1))?;
                }
                s.schmidt = Some(p);
            }
            _ => {}
        }
        for c in 1..=nb_indices {
            s.indices
                .push(lecteur.nombre(&format!("indice {c} après la surface {i}"))?);
        }
        if s.indices[0].abs() < 0.5 {
            return Err(format!("indice {} après la surface {i} invalide", s.indices[0]));
        }
        brutes.push(s);
    }

    // Drapeaux "Y"/"N" (vignettage, surfaces non centrées) des fichiers écrits par RAYTRACE
    let reste = lecteur.reste();
    let est_drapeau = |c: &String| c.eq_ignore_ascii_case("Y") || c.eq_ignore_ascii_case("N");
    let vignettage_actif = reste.len() >= 2
        && est_drapeau(&reste[0])
        && est_drapeau(&reste[1])
        && reste[0].eq_ignore_ascii_case("Y");

    let non_centrees: Vec<String> = brutes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.decentrement_x.abs() > 1e-9 || s.sinus_bascule.abs() > 1e-9)
        .map(|(i, _)| (i + 1).to_string())
        .collect();
    if !non_centrees.is_empty() {
        return Err(format!(
            "surface(s) {} basculée(s) ou décentrée(s) : FreeOptical ne gère pas encore les systèmes non centrés, l'import fausserait le système",
            non_centrees.join(", ")
        ));
    }

    // Positions axiales des sommets : distances cumulées, plus le décalage axial propre à chaque surface
    let mut z_base = 0.0;
    let mut z_sommets = Vec::with_capacity(nb_surfaces);
    for s in &brutes {
        z_base += s.distance;
        z_sommets.push(z_base + s.decalage_axial);
    }

    let catalogue = GlassCatalog::standard();
    let mut surfaces = Vec::with_capacity(nb_surfaces + 1);
    let mut indice_avant = 1.0_f64;
    let mut couleurs_ignorees = false;
    let mut index_stop = None;
    let mut obstructions = Vec::new();

    for (i, s) in brutes.iter().enumerate() {
        let n_apres = s.indices[0];
        let est_miroir = n_apres.signum() != indice_avant.signum();
        if est_miroir && (n_apres.abs() - indice_avant.abs()).abs() > 1e-6 {
            avertissements.push(format!(
                "Surface {} : l'indice change de valeur au moment de la réflexion ({} → {}) ; la réflexion est conservée dans le milieu incident.",
                i + 1,
                indice_avant,
                n_apres
            ));
        }

        let verre = if est_miroir {
            "MIROIR".to_string()
        } else if s.indices.iter().all(|n| (n.abs() - 1.0).abs() < 1e-7) {
            "AIR".to_string()
        } else {
            let nd = n_apres.abs();
            let vd = if s.indices.len() >= 3 {
                let (n_rouge, n_bleu) = (s.indices[1].abs(), s.indices[2].abs());
                if n_bleu - n_rouge > 1e-9 { (nd - 1.0) / (n_bleu - n_rouge) } else { 0.0 }
            } else {
                0.0
            };
            if s.indices.len() > 3 {
                couleurs_ignorees = true;
            }
            if vd > 0.0 {
                format!("{nd:.6}:{vd:.3}")
            } else {
                format!("{nd:.6}")
            }
        };

        let epaisseur = if i + 1 < nb_surfaces {
            z_sommets[i + 1] - z_sommets[i]
        } else {
            0.0 // remplacé plus bas par le tirage paraxial
        };

        let plane = s.rayon.abs() < 1e-9;
        let type_surface = match s.code {
            _ if plane && s.code != -1 && !(1..=5).contains(&s.code) && s.code != 9 => SurfaceType::Plane,
            -1 if plane => SurfaceType::Plane,
            -1 => SurfaceType::Conique,
            _ if plane && (s.code == 9 || (1..=5).contains(&s.code)) => SurfaceType::Aspherique,
            1..=5 | 9 => SurfaceType::Aspherique,
            _ => SurfaceType::Spherique,
        };
        let mut surf = Surface::nouvelle(type_surface, s.rayon, epaisseur, verre, 10.0);
        surf.conique = if type_surface == SurfaceType::Conique { s.conique } else { 0.0 };

        let mut commentaire = libelle_code(s.code).to_string();
        if est_miroir {
            commentaire = format!("Miroir ({})", commentaire.to_lowercase());
        }

        // Coefficients asphériques : r² repris dans la courbure, r⁴ à r¹⁰ dans A4..A10
        let mut a2 = 0.0;
        if (1..=5).contains(&s.code) {
            a2 = s.coefficients[0];
            surf.aspheriques = [s.coefficients[1], s.coefficients[2], s.coefficients[3], s.coefficients[4]];
        }
        if let Some([puissance, zone_neutre, rayon_primaire, indice]) = s.schmidt {
            // Profil de Schmidt au 3e ordre : z = P (r⁴ − 2 ν² h² r²) / (4 (n − 1) |R|³),
            // h = demi-ouverture, ν = zone neutre ; P > 0 face arrière, P < 0 face avant
            let n_calcul = if indice > 1.0 { indice } else { n_apres.abs().max(indice_avant.abs()) };
            let h = 0.5 * pupille_mm;
            if rayon_primaire.abs() > 1e-9 && n_calcul > 1.0 {
                let a4 = puissance / (4.0 * (n_calcul - 1.0) * rayon_primaire.abs().powi(3));
                surf.aspheriques[0] += a4;
                a2 = -2.0 * zone_neutre * zone_neutre * h * h * a4;
            }
            commentaire = format!(
                "Lame de Schmidt (P = {puissance}, zone neutre {zone_neutre}, R primaire {rayon_primaire})"
            );
        }
        if a2 != 0.0 {
            if let Some(ecart) = integrer_terme_r2(&mut surf, a2, 0.6 * pupille_mm.max(s.diametre_max)) {
                avertissements.push(format!(
                    "Surface {} : terme en r² approché par la courbure (écart de flèche {:.1e} mm au bord).",
                    i + 1,
                    ecart
                ));
            }
        }
        if s.schmidt.is_some() {
            avertissements.push(format!(
                "Surface {} : lame de Schmidt reconstruite au 3e ordre (termes r² et r⁴) à partir de sa puissance et de sa zone neutre.",
                i + 1
            ));
        }

        if s.diametre_obstruction > 0.0 {
            commentaire.push_str(&format!(" — obstruction Ø{} non modélisée", s.diametre_obstruction));
            obstructions.push(format!("{} (Ø{})", i + 1, s.diametre_obstruction));
        }
        surf.commentaire = commentaire;
        if s.code == 8 && index_stop.is_none() {
            index_stop = Some(i);
        }
        surfaces.push(surf);
        indice_avant = n_apres;
    }

    if !obstructions.is_empty() {
        avertissements.push(format!(
            "Obstructions centrales ignorées (non gérées par FreeOptical), surfaces {} : le spot diagram inclut les rayons centraux.",
            obstructions.join(", ")
        ));
    }
    if couleurs_ignorees {
        avertissements.push(
            "Plus de 3 indices par surface : seuls N1 (d), N2 (C) et N3 (F) sont repris.".to_string(),
        );
    }
    if nb_indices == 2 {
        avertissements.push(
            "2 indices par surface : couleur non identifiable, seul N1 est repris (verre sans dispersion).".to_string(),
        );
    }

    surfaces[index_stop.unwrap_or(0)].est_diaphragme = true;
    let mut image = Surface::plan_image(10.0);
    image.commentaire = "Plan Image (foyer paraxial)".to_string();
    surfaces.push(image);

    let mut systeme = OpticalSystem {
        nom: if titre.is_empty() { "Import TELOPTIK".to_string() } else { titre },
        longueur_onde_ref_nm: LAMBDA_D_NM,
        longueurs_onde: if nb_indices >= 3 {
            Wavelength::visible_fdc()
        } else {
            vec![Wavelength::new(LAMBDA_D_NM, 1.0, "d (Jaune 587.6 nm)")]
        },
        ouverture: ApertureSpec::PupilleEntreeMm(pupille_mm),
        champs_deg: vec![(0.0, 0.0), (0.0, 0.5)],
        surfaces,
        catalogue,
    };

    // Plan image au foyer paraxial
    let nb = systeme.surfaces.len();
    let bfl = calculer_paraxial(&systeme).tirage_bfl_mm;
    if bfl.is_finite() {
        systeme.surfaces[nb - 2].epaisseur = bfl;
    } else {
        avertissements.push("Système afocal : le plan image est laissé contre la dernière surface.".to_string());
    }

    // Demi-diamètres : ouverture libre tracée sur l'axe et en bord de champ ; le diamètre du
    // fichier ne limite le faisceau que si RAYTRACE avait le vignettage activé
    let libres = ouvertures_libres(&systeme);
    for (i, surf) in systeme.surfaces.iter_mut().enumerate() {
        let calcule = libres[i];
        let fichier = brutes.get(i).map_or(0.0, |s| 0.5 * s.diametre_max);
        surf.semi_diametre = if fichier > 0.0 && (vignettage_actif || fichier >= calcule) {
            fichier
        } else {
            calcule
        };
    }

    Ok(ImportTeloptik {
        systeme,
        avertissements,
        focale_fichier_mm,
    })
}

/// Ajoute un terme `a2·r²` à la flèche : exact sur une surface plane (paraboloïde de courbure
/// 2·a2), approché sinon par un développement limité reporté dans A4..A10. Retourne l'écart de
/// flèche résiduel au rayon `r_bord` s'il dépasse 1 nm.
fn integrer_terme_r2(surf: &mut Surface, a2: f64, r_bord: f64) -> Option<f64> {
    let c = surf.courbure();
    let k = match surf.type_surface {
        SurfaceType::Conique | SurfaceType::Aspherique => surf.conique,
        _ => 0.0,
    };
    // Développement de la flèche conique : c r²/2 + (1+k) c³ r⁴/8 + (1+k)² c⁵ r⁶/16 + …
    let e = 1.0 + k;
    surf.aspheriques[0] += e * c.powi(3) / 8.0;
    surf.aspheriques[1] += e.powi(2) * c.powi(5) / 16.0;
    surf.aspheriques[2] += 5.0 * e.powi(3) * c.powi(7) / 128.0;
    surf.aspheriques[3] += 7.0 * e.powi(4) * c.powi(9) / 256.0;
    let c_nouveau = c + 2.0 * a2;
    surf.type_surface = SurfaceType::Aspherique;
    surf.conique = -1.0; // paraboloïde : flèche exactement c r²/2
    surf.rayon = if c_nouveau.abs() > 1e-15 { 1.0 / c_nouveau } else { f64::INFINITY };
    let ecart = (21.0 * e.powi(5) * c.powi(11) / 1024.0 * r_bord.powi(12)).abs();
    (ecart > 1e-6).then_some(ecart)
}

/// Hauteur maximale atteinte sur chaque surface par les rayons du bord de la pupille, sur l'axe
/// et en bord de champ, à toutes les longueurs d'onde (demi-diamètres sans vignettage)
fn ouvertures_libres(systeme: &OpticalSystem) -> Vec<f64> {
    let mut libre = systeme.clone();
    // Aucune surface ne limite le faisceau ; la première garde une taille raisonnable car
    // elle fixe le plan de départ des rayons (flèche au bord)
    let r_pupille = 0.5 * match libre.ouverture {
        ApertureSpec::PupilleEntreeMm(d) => d,
        _ => 100.0,
    };
    for (i, s) in libre.surfaces.iter_mut().enumerate() {
        s.semi_diametre = if i == 0 { 1.2 * r_pupille } else { 1e5 };
    }
    let vise = VisePupille::calculer(&libre);
    let mut hauteurs = vec![0.0_f64; libre.surfaces.len()];
    let lambdas: Vec<f64> = libre.longueurs_onde.iter().map(|w| w.valeur_nm).collect();
    for &(hx, hy) in &libre.champs_deg {
        for &lambda in &lambdas {
            for k in 0..=16 {
                let (px, py) = if k == 16 {
                    (0.0, 0.0)
                } else {
                    let t = std::f64::consts::TAU * k as f64 / 16.0;
                    (t.cos(), t.sin())
                };
                let trace = tracer_rayon_unique(&libre, vise.rayon(px, py, hx, hy, lambda), (hx, hy), (px, py));
                for impact in &trace.impacts {
                    let r = impact.point_local.x.hypot(impact.point_local.y);
                    let h = &mut hauteurs[impact.index_surface];
                    *h = h.max(r);
                }
            }
        }
    }
    hauteurs
        .into_iter()
        .map(|h| ((h * 1.02 * 10.0).ceil() / 10.0).max(0.5))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::analysis::calculer_spot_diagram;

    /// Ritchey-Chrétien 200 mm f/8 : primaire f/3, secondaire à 400 mm, foyer 133,33 mm derrière
    /// le primaire (grandissement 8/3) ; constantes coniques des formules classiques du RC
    const RC_F8: &str = "\"RC 200 f/8\"\r\n\r\n2,1,200,1600,0\r\n\r\n-1200,0,-1\r\n\r\n0,0,0,0,0\r\n\r\n-1.140625\r\n\r\n-1\r\n\r\n-640,-400,-1\r\n\r\n0,0,0,0,0\r\n\r\n-6.568\r\n\r\n1\r\n\r\n\"N\",\"N\"\r\n";

    #[test]
    fn decoupage_des_champs_turbo_basic() {
        let champs = decouper_champs("\"Titre, avec virgule\"\r\n\r\n4,1,.5,-1.5D-03,\"\"\r\n");
        assert_eq!(champs, ["Titre, avec virgule", "4", "1", ".5", "-1.5D-03", ""]);
    }

    #[test]
    fn import_cassegrain_ritchey_chretien() {
        let import = importer_teloptik(RC_F8.as_bytes()).unwrap();
        let sys = &import.systeme;
        assert_eq!(sys.surfaces.len(), 3);
        assert_eq!(sys.surfaces[0].verre, "MIROIR");
        assert_eq!(sys.surfaces[1].verre, "MIROIR");
        assert_eq!(sys.surfaces[0].type_surface, SurfaceType::Conique);
        assert!((sys.surfaces[0].epaisseur + 400.0).abs() < 1e-9);
        let p = calculer_paraxial(sys);
        assert!((p.focale_effl_mm - 1600.0).abs() < 0.5, "EFFL = {}", p.focale_effl_mm);
        // Foyer à 133,33 mm derrière le primaire : 533,33 mm après le secondaire
        assert!((sys.surfaces[1].epaisseur - 533.333).abs() < 0.01, "BFL = {}", sys.surfaces[1].epaisseur);
        let spot = calculer_spot_diagram(sys, (0.0, 0.0), 6);
        assert!(spot.rayon_rms_um < 0.5, "Spot RMS = {} µm", spot.rayon_rms_um);
        assert!(import.avertissements.is_empty(), "{:?}", import.avertissements);
    }

    #[test]
    fn import_lame_de_schmidt_et_doublet_trois_couleurs() {
        // Chambre de Schmidt 200 mm f/2 : lame plane + face arrière de Schmidt (code 9),
        // miroir sphérique R = -800 au centre de courbure de la lame
        let chambre = |puissance: &str| {
            format!("\"Schmidt\"\r\n3,1,200,-400,0\r\n0,0,10\r\n0,0,0,0,0\r\n1.5168\r\n0,5,9\r\n0,0,0,0,0\r\n{puissance},.866,-800,1.5168\r\n1\r\n-800,800,0\r\n0,0,0,0,0\r\n-1\r\n\"N\",\"N\"\r\n")
        };
        let import = importer_teloptik(chambre("1").as_bytes()).unwrap();
        let sys = &import.systeme;
        assert_eq!(sys.surfaces[1].type_surface, SurfaceType::Aspherique);
        assert!(sys.surfaces[1].aspheriques[0] > 0.0, "face arrière : A4 > 0");
        let corrige = calculer_spot_diagram(sys, (0.0, 0.0), 6).rayon_rms_um;
        let sans_lame = importer_teloptik(chambre("0").as_bytes()).unwrap();
        let brut = calculer_spot_diagram(&sans_lame.systeme, (0.0, 0.0), 6).rayon_rms_um;
        // Profil du 3e ordre : la lame retire plus de 90 % de l'aberration sphérique du miroir f/2
        assert!(corrige < 0.1 * brut, "Spot RMS {corrige} µm avec lame, {brut} µm sans");

        // Doublet : trois indices par surface (d, C, F) → verre « nd:Vd »
        let doublet = "\"Doublet\"\r\n4,3,100,1000,0\r\n600,0,0\r\n0,0,0,0,0\r\n1.5168\r\n1.514322\r\n1.522376\r\n-450,8,0\r\n0,0,0,0,0\r\n1.6200\r\n1.615037\r\n1.632081\r\n-1500,3,0\r\n0,0,0,0,0\r\n1\r\n1\r\n1\r\n0,1,10\r\n0,0,0,0,0\r\n1\r\n1\r\n1\r\n";
        let import = importer_teloptik(doublet.as_bytes()).unwrap();
        assert_eq!(import.systeme.surfaces[0].verre, "1.516800:64.167");
        assert_eq!(import.systeme.longueurs_onde.len(), 3);
    }

    #[test]
    fn refus_des_surfaces_basculees() {
        let bascule = "\"Newton\"\r\n2,1,200,1000,0\r\n-2000,0,-1\r\n0,0,0,0,0\r\n-1\r\n-1\r\n0,-800,10\r\n0,0,0,0,-.70711\r\n1\r\n";
        let err = importer_teloptik(bascule.as_bytes()).unwrap_err();
        assert!(err.contains("surface(s) 2"), "{err}");
    }

    /// Vérification sur les fichiers d'origine : `FREEOPTICAL_TELOPTIK=/chemin/TELOPTIK cargo test -- --ignored`
    #[test]
    #[ignore]
    fn fichiers_teloptik_d_origine() {
        let dossier = std::env::var("FREEOPTICAL_TELOPTIK").expect("FREEOPTICAL_TELOPTIK non défini");
        let mut entrees: Vec<_> = std::fs::read_dir(dossier)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| est_fichier_teloptik(p))
            .collect();
        entrees.sort();
        for chemin in entrees {
            let nom = chemin.file_name().unwrap().to_string_lossy().to_string();
            match importer_fichier_teloptik(&chemin) {
                Ok(import) => {
                    let p = calculer_paraxial(&import.systeme);
                    let spot = calculer_spot_diagram(&import.systeme, (0.0, 0.0), 6);
                    let n = import.systeme.surfaces.len();
                    // Meilleur foyer sur l'axe (balayage de ±5 mm autour du foyer paraxial)
                    let mut essai = import.systeme.clone();
                    let bfl = essai.surfaces[n - 2].epaisseur;
                    let (mut meilleur, mut dz_min) = (f64::INFINITY, 0.0);
                    for k in -500..=500 {
                        let dz = k as f64 * 0.01;
                        essai.surfaces[n - 2].epaisseur = bfl + dz * bfl.signum();
                        let s = calculer_spot_diagram(&essai, (0.0, 0.0), 6);
                        if s.nb_rayons_valides == spot.nb_rayons_valides && s.rayon_rms_um < meilleur {
                            meilleur = s.rayon_rms_um;
                            dz_min = dz;
                        }
                    }
                    println!("{:13}      meilleur foyer {dz_min:+.2} mm : spot RMS {meilleur:.2} µm", "");
                    println!(
                        "{nom:13} OK   focale fichier {:>10.2}  EFFL {:>10.2}  tirage {:>9.2}  spot RMS {:>8.2} µm  ({} rayons)  {}",
                        import.focale_fichier_mm,
                        p.focale_effl_mm,
                        import.systeme.surfaces[n - 2].epaisseur,
                        spot.rayon_rms_um,
                        spot.nb_rayons_valides,
                        import.avertissements.len()
                    );
                }
                Err(e) => println!("{nom:13} REFUS {e}"),
            }
        }
    }
}
