//! Export DXF de la coupe méridienne pour la CAO (FreeCAD, LibreCAD, QCAD…).
//!
//! Format R12 (AC1009), le plus largement lu. Unités : mm. L'axe optique est l'axe X du
//! dessin (Z du système), la hauteur est l'axe Y. Un calque par pièce (L1, L2…, M1…) :
//! demi-coupe fermée pour une lentille (au-dessus de son axe : la faire tourner de 360°
//! autour de cet axe donne le volume de la lentille), profil complet pour un miroir. Les
//! faces sphériques sont des arcs exacts.
//! Calques « AXE » (axe optique) et « RAYONS » (rayons méridiens de chaque champ).

use crate::core::dessin::{self, NatureElement, Sommet};
use crate::core::types::OpticalSystem;

/// Résultat : texte du fichier et remarques (pièces impossibles à dessiner)
pub struct ExportDxf {
    pub texte: String,
    pub nb_pieces: usize,
    pub remarques: Vec<String>,
}

fn groupe(t: &mut String, code: i32, valeur: impl std::fmt::Display) {
    t.push_str(&format!("{code}\n{valeur}\n"));
}

fn polyligne(t: &mut String, calque: &str, sommets: &[Sommet], ferme: bool) {
    groupe(t, 0, "POLYLINE");
    groupe(t, 8, calque);
    groupe(t, 66, 1);
    groupe(t, 10, "0.0");
    groupe(t, 20, "0.0");
    groupe(t, 30, "0.0");
    groupe(t, 70, if ferme { 1 } else { 0 });
    for s in sommets {
        groupe(t, 0, "VERTEX");
        groupe(t, 8, calque);
        groupe(t, 10, format!("{:.6}", s.z));
        groupe(t, 20, format!("{:.6}", s.y));
        groupe(t, 30, "0.0");
        if s.bombe != 0.0 {
            groupe(t, 42, format!("{:.10}", s.bombe));
        }
    }
    groupe(t, 0, "SEQEND");
    groupe(t, 8, calque);
}

fn nom_calque(n: &str) -> String {
    n.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c.to_ascii_uppercase() } else { '_' }).collect()
}

pub fn exporter_dxf(sys: &OpticalSystem) -> ExportDxf {
    let elements = dessin::elements(sys);
    let mut entites = String::new();
    let mut calques: Vec<(String, i32)> = vec![("AXE".into(), 8), ("RAYONS".into(), 3)];
    let mut remarques = Vec::new();
    let mut nb_pieces = 0;
    for e in &elements {
        let (contour, ferme, couleur) = match e.nature {
            NatureElement::Lentille => (dessin::demi_contour_lentille(sys, e), true, 5),
            NatureElement::Miroir => (dessin::profil_miroir(sys, e), false, 6),
        };
        match contour {
            Ok(c) => {
                let calque = nom_calque(&e.nom);
                polyligne(&mut entites, &calque, &c, ferme);
                calques.push((calque, couleur));
                nb_pieces += 1;
            }
            Err(m) => remarques.push(format!("{} : {m}", e.nom)),
        }
    }
    // Axe optique (ligne brisée s'il y a des ruptures de repère)
    let [z_min, z_max, _, _] = sys.boite_englobante();
    let axe: Vec<Sommet> = if sys.a_des_coupures() {
        sys.points_axe().iter().map(|p| Sommet { z: p.z, y: p.y, bombe: 0.0 }).collect()
    } else {
        vec![Sommet { z: z_min - 10.0, y: 0.0, bombe: 0.0 }, Sommet { z: z_max + 10.0, y: 0.0, bombe: 0.0 }]
    };
    polyligne(&mut entites, "AXE", &axe, false);
    for (_, r) in dessin::rayons_meridiens(sys, 7) {
        let s: Vec<Sommet> = r.iter().map(|&(z, y)| Sommet { z, y, bombe: 0.0 }).collect();
        polyligne(&mut entites, "RAYONS", &s, false);
    }

    let mut t = String::new();
    groupe(&mut t, 999, format!("FreeOptical {} - {}", env!("CARGO_PKG_VERSION"), sys.nom.replace('\n', " ")));
    groupe(&mut t, 0, "SECTION");
    groupe(&mut t, 2, "HEADER");
    groupe(&mut t, 9, "$ACADVER");
    groupe(&mut t, 1, "AC1009");
    groupe(&mut t, 9, "$INSUNITS");
    groupe(&mut t, 70, 4);
    groupe(&mut t, 0, "ENDSEC");
    groupe(&mut t, 0, "SECTION");
    groupe(&mut t, 2, "TABLES");
    groupe(&mut t, 0, "TABLE");
    groupe(&mut t, 2, "LTYPE");
    groupe(&mut t, 70, 1);
    groupe(&mut t, 0, "LTYPE");
    groupe(&mut t, 2, "CONTINUOUS");
    groupe(&mut t, 70, 0);
    groupe(&mut t, 3, "Solid line");
    groupe(&mut t, 72, 65);
    groupe(&mut t, 73, 0);
    groupe(&mut t, 40, "0.0");
    groupe(&mut t, 0, "ENDTAB");
    groupe(&mut t, 0, "TABLE");
    groupe(&mut t, 2, "LAYER");
    groupe(&mut t, 70, calques.len());
    for (nom, couleur) in &calques {
        groupe(&mut t, 0, "LAYER");
        groupe(&mut t, 2, nom);
        groupe(&mut t, 70, 0);
        groupe(&mut t, 62, couleur);
        groupe(&mut t, 6, "CONTINUOUS");
    }
    groupe(&mut t, 0, "ENDTAB");
    groupe(&mut t, 0, "ENDSEC");
    groupe(&mut t, 0, "SECTION");
    groupe(&mut t, 2, "ENTITIES");
    t.push_str(&entites);
    groupe(&mut t, 0, "ENDSEC");
    groupe(&mut t, 0, "EOF");
    ExportDxf { texte: t, nb_pieces, remarques }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::presets;

    /// Relit les couples (code, valeur) du fichier
    fn couples(t: &str) -> Vec<(i32, String)> {
        let l: Vec<&str> = t.lines().collect();
        l.chunks(2).map(|c| (c[0].trim().parse().unwrap(), c[1].to_string())).collect()
    }

    #[test]
    fn doublet_et_cassegrain() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let d = exporter_dxf(&sys);
        assert_eq!(d.nb_pieces, 2);
        assert!(d.remarques.is_empty());
        let c = couples(&d.texte);
        assert_eq!(c.last().unwrap(), &(0, "EOF".to_string()));
        // Autant de SEQEND que de POLYLINE ; deux calques de pièces ; arcs avec bombé
        let nb = |v: &str| c.iter().filter(|(k, x)| *k == 0 && x == v).count();
        assert_eq!(nb("POLYLINE"), nb("SEQEND"));
        assert!(c.iter().any(|(k, x)| *k == 8 && x == "L1") && c.iter().any(|(k, x)| *k == 8 && x == "L2"));
        assert!(c.iter().any(|(k, _)| *k == 42));
        let cass = exporter_dxf(&presets::creer_telescope_cassegrain());
        assert_eq!(cass.nb_pieces, 2);
        assert!(cass.texte.contains("\nM2\n"));
    }
}
