//! Mise à jour en ligne depuis les releases GitHub du dépôt (paquet .deb installé par pkexec)

use std::path::{Path, PathBuf};
use std::process::Command;

const API_DERNIERE_RELEASE: &str =
    "https://api.github.com/repos/jbandrivet/freeoptical/releases/latest";
pub const PAGE_RELEASES: &str = "https://github.com/jbandrivet/freeoptical/releases";
pub const VERSION_COURANTE: &str = env!("CARGO_PKG_VERSION");
const BINAIRE_INSTALLE: &str = "/usr/bin/freeoptical";
const TAILLE_MAX_DEB: u64 = 200 * 1024 * 1024;

/// Version publiée plus récente que la version en cours d'exécution
#[derive(Clone, Debug)]
pub struct VersionDisponible {
    pub version: String,
    pub page: String,
    /// Paquet .deb amd64 joint à la release (absent : installation manuelle depuis la page)
    pub deb: Option<PaquetDeb>,
}

#[derive(Clone, Debug)]
pub struct PaquetDeb {
    pub nom: String,
    pub url: String,
    pub sha256: Option<String>,
}

/// « v1.2.3 », « 1.2 » ou « 1.2.3-rc1 » → (1, 2, 3) ; le suffixe de pré-version est ignoré
pub fn parser_version(texte: &str) -> Option<(u64, u64, u64)> {
    let base = texte.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parties = base.split('.');
    let majeure = parties.next()?.parse().ok()?;
    let mineure = parties.next().map_or(Some(0), |p| p.parse().ok())?;
    let correctif = parties.next().map_or(Some(0), |p| p.parse().ok())?;
    if parties.next().is_some() {
        return None;
    }
    Some((majeure, mineure, correctif))
}

pub fn est_plus_recente(distante: &str, locale: &str) -> bool {
    match (parser_version(distante), parser_version(locale)) {
        (Some(d), Some(l)) => d > l,
        _ => false,
    }
}

/// L'installation automatique demande dpkg/apt et pkexec (Debian, Ubuntu…)
pub fn installation_automatique_possible() -> bool {
    cfg!(all(target_os = "linux", target_arch = "x86_64"))
        && Path::new("/usr/bin/pkexec").exists()
        && Path::new("/usr/bin/apt-get").exists()
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .user_agent(concat!("freeoptical/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Interroge GitHub ; `Ok(None)` si la version en cours est la plus récente
pub fn rechercher() -> Result<Option<VersionDisponible>, String> {
    let texte = agent()
        .get(API_DERNIERE_RELEASE)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("GitHub injoignable : {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Réponse GitHub illisible : {e}"))?;
    analyser_release(&texte, VERSION_COURANTE)
}

fn analyser_release(json: &str, locale: &str) -> Result<Option<VersionDisponible>, String> {
    let release: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Réponse GitHub invalide : {e}"))?;
    let tag = release["tag_name"]
        .as_str()
        .ok_or("Réponse GitHub sans numéro de version")?;
    if !est_plus_recente(tag, locale) {
        return Ok(None);
    }
    let deb = release["assets"].as_array().and_then(|assets| {
        assets.iter().find_map(|a| {
            let nom = a["name"].as_str()?;
            if !nom.ends_with("_amd64.deb") {
                return None;
            }
            Some(PaquetDeb {
                nom: nom.to_string(),
                url: a["browser_download_url"].as_str()?.to_string(),
                sha256: a["digest"]
                    .as_str()
                    .and_then(|d| d.strip_prefix("sha256:"))
                    .map(str::to_string),
            })
        })
    });
    Ok(Some(VersionDisponible {
        version: tag.trim_start_matches('v').to_string(),
        page: release["html_url"].as_str().unwrap_or(PAGE_RELEASES).to_string(),
        deb,
    }))
}

/// Télécharge le paquet dans le répertoire temporaire et vérifie son empreinte SHA-256
pub fn telecharger(paquet: &PaquetDeb) -> Result<PathBuf, String> {
    let octets = agent()
        .get(&paquet.url)
        .call()
        .map_err(|e| format!("Téléchargement impossible : {e}"))?
        .body_mut()
        .with_config()
        .limit(TAILLE_MAX_DEB)
        .read_to_vec()
        .map_err(|e| format!("Téléchargement interrompu : {e}"))?;

    let dossier = std::env::temp_dir().join("freeoptical-maj");
    std::fs::create_dir_all(&dossier).map_err(|e| format!("{}: {e}", dossier.display()))?;
    let chemin = dossier.join(&paquet.nom);
    std::fs::write(&chemin, &octets).map_err(|e| format!("{}: {e}", chemin.display()))?;
    // apt lit le fichier local sous l'utilisateur _apt : dossier et paquet lisibles par tous
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dossier, std::fs::Permissions::from_mode(0o755));
        let _ = std::fs::set_permissions(&chemin, std::fs::Permissions::from_mode(0o644));
    }

    if let Some(attendu) = &paquet.sha256 {
        let sortie = Command::new("sha256sum")
            .arg(&chemin)
            .output()
            .map_err(|e| format!("sha256sum introuvable : {e}"))?;
        let obtenu = String::from_utf8_lossy(&sortie.stdout);
        let obtenu = obtenu.split_whitespace().next().unwrap_or("");
        if !obtenu.eq_ignore_ascii_case(attendu) {
            let _ = std::fs::remove_file(&chemin);
            return Err(format!(
                "Paquet corrompu (SHA-256 {obtenu} au lieu de {attendu}), installation annulée."
            ));
        }
    }
    Ok(chemin)
}

/// Installe le paquet avec apt-get sous pkexec (fenêtre de mot de passe administrateur)
pub fn installer(chemin: &Path) -> Result<(), String> {
    let sortie = Command::new("/usr/bin/pkexec")
        .arg("/usr/bin/apt-get")
        .args(["install", "-y"])
        .arg(chemin)
        .output()
        .map_err(|e| format!("pkexec impossible à lancer : {e}"))?;
    if sortie.status.success() {
        let _ = std::fs::remove_file(chemin);
        return Ok(());
    }
    // pkexec : 126 = authentification refusée ou annulée, 127 = non autorisé
    match sortie.status.code() {
        Some(126) | Some(127) => Err("Installation annulée (mot de passe non fourni).".to_string()),
        _ => {
            let erreur = String::from_utf8_lossy(&sortie.stderr);
            let derniere = erreur
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("erreur inconnue");
            Err(format!("Échec de l'installation : {derniere}"))
        }
    }
}

/// Relance FreeOptical (le binaire installé s'il existe) avec les mêmes arguments
pub fn relancer() -> Result<(), String> {
    let binaire = if Path::new(BINAIRE_INSTALLE).exists() {
        PathBuf::from(BINAIRE_INSTALLE)
    } else {
        std::env::current_exe().map_err(|e| e.to_string())?
    };
    Command::new(binaire)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Relance impossible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparaison_des_versions() {
        assert_eq!(parser_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parser_version("0.2"), Some((0, 2, 0)));
        assert_eq!(parser_version("1.0.0-rc1"), Some((1, 0, 0)));
        assert_eq!(parser_version("v1.x"), None);
        assert!(est_plus_recente("v0.1.1", "0.1.0"));
        assert!(est_plus_recente("v0.10.0", "0.9.9"));
        assert!(!est_plus_recente("v0.1.0", "0.1.0"));
        assert!(!est_plus_recente("v0.0.9", "0.1.0"));
        assert!(!est_plus_recente("nightly", "0.1.0"));
    }

    #[test]
    fn lecture_d_une_release_github() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/jbandrivet/freeoptical/releases/tag/v0.2.0",
            "assets": [
                {"name": "notes.txt", "browser_download_url": "https://x/notes.txt"},
                {"name": "freeoptical_0.2.0_amd64.deb",
                 "browser_download_url": "https://x/freeoptical_0.2.0_amd64.deb",
                 "digest": "sha256:abcd"}
            ]
        }"#;
        let v = analyser_release(json, "0.1.0").unwrap().unwrap();
        assert_eq!(v.version, "0.2.0");
        let deb = v.deb.unwrap();
        assert_eq!(deb.nom, "freeoptical_0.2.0_amd64.deb");
        assert_eq!(deb.sha256.as_deref(), Some("abcd"));
        assert!(analyser_release(json, "0.2.0").unwrap().is_none());
    }

    /// Interroge le vrai dépôt et télécharge le paquet publié : `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn telechargement_reel_depuis_github() {
        let texte = agent()
            .get(API_DERNIERE_RELEASE)
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap();
        let v = analyser_release(&texte, "0.0.0").unwrap().unwrap();
        let deb = v.deb.expect("release sans paquet .deb");
        assert!(deb.sha256.is_some(), "GitHub ne fournit pas l'empreinte du paquet");
        let chemin = telecharger(&deb).unwrap();
        assert!(std::fs::metadata(&chemin).unwrap().len() > 1_000_000);
        std::fs::remove_file(chemin).unwrap();
        assert!(rechercher().is_ok());
    }
}
