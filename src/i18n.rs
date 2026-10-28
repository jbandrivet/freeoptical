//! Langue de l'interface : français, anglais ou allemand.
//!
//! Les textes sont écrits sur place dans les trois langues avec les macros [`tr!`] (texte fixe)
//! et [`trf!`] (comme `format!`) ; la langue courante est globale et se change à chaud.
//! Ordre de priorité au démarrage : option `--langue`, choix enregistré
//! (`~/.config/freeoptical/langue`), variables d'environnement `LC_ALL` / `LC_MESSAGES` / `LANG`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Langue {
    #[default]
    Fr,
    En,
    De,
}

impl Langue {
    pub const TOUTES: [Langue; 3] = [Langue::Fr, Langue::En, Langue::De];

    /// Nom de la langue écrit dans cette langue (pour les menus de choix)
    pub fn nom_natif(self) -> &'static str {
        match self {
            Langue::Fr => "Français",
            Langue::En => "English",
            Langue::De => "Deutsch",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Langue::Fr => "fr",
            Langue::En => "en",
            Langue::De => "de",
        }
    }

    /// Reconnaît un code de langue (`fr`, `en_US.UTF-8`, `de-AT`, `deutsch`…)
    pub fn depuis_code(code: &str) -> Option<Langue> {
        let c = code.trim().to_lowercase();
        if c.starts_with("fr") {
            Some(Langue::Fr)
        } else if c.starts_with("en") {
            Some(Langue::En)
        } else if c.starts_with("de") {
            Some(Langue::De)
        } else {
            None
        }
    }

    /// Langue suivante (touche de bascule du TUI)
    pub fn suivante(self) -> Langue {
        match self {
            Langue::Fr => Langue::En,
            Langue::En => Langue::De,
            Langue::De => Langue::Fr,
        }
    }
}

static LANGUE: AtomicU8 = AtomicU8::new(0);

pub fn langue() -> Langue {
    match LANGUE.load(Ordering::Relaxed) {
        1 => Langue::En,
        2 => Langue::De,
        _ => Langue::Fr,
    }
}

pub fn definir_langue(l: Langue) {
    let v = match l {
        Langue::Fr => 0,
        Langue::En => 1,
        Langue::De => 2,
    };
    LANGUE.store(v, Ordering::Relaxed);
}

/// Texte de la langue courante parmi les trois versions
pub fn choisir<'a>(fr: &'a str, en: &'a str, de: &'a str) -> &'a str {
    match langue() {
        Langue::Fr => fr,
        Langue::En => en,
        Langue::De => de,
    }
}

/// Texte fixe dans les trois langues : `tr!("Fermer", "Close", "Schließen")`
#[macro_export]
macro_rules! tr {
    ($fr:expr, $en:expr, $de:expr $(,)?) => {
        $crate::i18n::choisir($fr, $en, $de)
    };
}

/// `format!` avec une chaîne par langue (mêmes arguments, dans le même ordre) :
/// `trf!("{} surfaces", "{} surfaces", "{} Flächen", n)`
#[macro_export]
macro_rules! trf {
    ($fr:literal, $en:literal, $de:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::langue() {
            $crate::i18n::Langue::Fr => format!($fr $(, $arg)*),
            $crate::i18n::Langue::En => format!($en $(, $arg)*),
            $crate::i18n::Langue::De => format!($de $(, $arg)*),
        }
    };
}

fn fichier_preference() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("freeoptical").join("langue"))
}

/// Langue choisie et enregistrée lors d'une session précédente
pub fn preference_enregistree() -> Option<Langue> {
    let contenu = std::fs::read_to_string(fichier_preference()?).ok()?;
    Langue::depuis_code(&contenu)
}

/// Enregistre le choix de langue pour les prochains démarrages
pub fn enregistrer_preference(l: Langue) -> Result<(), String> {
    let chemin = fichier_preference().ok_or("HOME")?;
    if let Some(dossier) = chemin.parent() {
        std::fs::create_dir_all(dossier).map_err(|e| e.to_string())?;
    }
    std::fs::write(&chemin, format!("{}\n", l.code())).map_err(|e| e.to_string())
}

/// Langue du système d'après les variables d'environnement POSIX (anglais par défaut)
pub fn langue_du_systeme(variable: impl Fn(&str) -> Option<String>) -> Langue {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| variable(v))
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .and_then(|v| Langue::depuis_code(&v))
        .unwrap_or(Langue::En)
}

/// Fixe la langue de démarrage : option de la ligne de commande, sinon choix enregistré,
/// sinon langue du système
pub fn initialiser(option: Option<Langue>) {
    let l = option
        .or_else(preference_enregistree)
        .unwrap_or_else(|| langue_du_systeme(|v| std::env::var(v).ok()));
    definir_langue(l);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_de_langue() {
        assert_eq!(Langue::depuis_code("fr_FR.UTF-8"), Some(Langue::Fr));
        assert_eq!(Langue::depuis_code("en_GB"), Some(Langue::En));
        assert_eq!(Langue::depuis_code("de-AT"), Some(Langue::De));
        assert_eq!(Langue::depuis_code("Deutsch"), Some(Langue::De));
        assert_eq!(Langue::depuis_code("it_IT"), None);
        for l in Langue::TOUTES {
            assert_eq!(Langue::depuis_code(l.code()), Some(l));
        }
    }

    #[test]
    fn langue_d_apres_l_environnement() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |v: &str| pairs.iter().find(|(k, _)| *k == v).map(|(_, x)| x.to_string())
        };
        assert_eq!(langue_du_systeme(env(&[("LANG", "de_DE.UTF-8")])), Langue::De);
        assert_eq!(langue_du_systeme(env(&[("LC_ALL", "fr_BE"), ("LANG", "de_DE")])), Langue::Fr);
        assert_eq!(langue_du_systeme(env(&[("LC_ALL", "C"), ("LANG", "fr_FR")])), Langue::Fr);
        assert_eq!(langue_du_systeme(env(&[("LANG", "ja_JP")])), Langue::En);
        assert_eq!(langue_du_systeme(env(&[])), Langue::En);
    }
}
