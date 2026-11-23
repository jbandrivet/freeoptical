//! Module Core : Moteur de conception et de simulation optique séquentielle.

pub mod aberrations;
pub mod bath;
pub mod chromatique;
pub mod dessin;
pub mod dxf;
pub mod empreinte;
pub mod energie;
pub mod fiche;
pub mod franges;
pub mod analysis;
pub mod glass;
pub mod onde;
pub mod optimizer;
pub mod paraxial;
pub mod pdf;
pub mod presets;
pub mod raytracer;
pub mod solves;
pub mod spectre;
pub mod teloptik;
pub mod tolerance;
pub mod types;
pub mod zemax;

use std::path::Path;

/// Ouvre un fichier système : JSON de FreeOptical, Zemax `.zmx`, ou `.TDS` / `.RLD` / `.DES`
/// (programmes de Rutten et van Venrooij). Retourne le système et les avertissements d'import.
pub fn ouvrir_fichier_systeme(chemin: &Path) -> Result<(types::OpticalSystem, Vec<String>), String> {
    if zemax::est_fichier_zemax(chemin) {
        let import = zemax::importer_fichier_zemax(chemin)?;
        Ok((import.systeme, import.avertissements))
    } else if teloptik::est_fichier_teloptik(chemin) {
        let import = teloptik::importer_fichier_teloptik(chemin)?;
        Ok((import.systeme, import.avertissements))
    } else {
        let contenu = std::fs::read_to_string(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
        let systeme = serde_json::from_str::<types::OpticalSystem>(&contenu)
            .map_err(|e| format!("{} : {e}", chemin.display()))?;
        Ok((systeme, Vec::new()))
    }
}

/// Enregistre le système en JSON (format des fichiers de FreeOptical)
pub fn enregistrer_fichier_systeme(systeme: &types::OpticalSystem, chemin: &Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(systeme).map_err(|e| e.to_string())?;
    std::fs::write(chemin, json).map_err(|e| format!("{} : {e}", chemin.display()))
}
