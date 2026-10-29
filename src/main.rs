#[macro_use]
pub mod i18n;
pub mod core;
pub mod gui;
pub mod maj;
pub mod mcp;
pub mod ui;

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches, Parser};
use core::types::OpticalSystem;
use i18n::Langue;

/// FreeOptical - Logiciel open-source de conception et de simulation optique séquentielle
/// (les textes d'aide sont fixés dans la langue courante par `commande_traduite`)
#[derive(Parser, Debug)]
#[command(name = "freeoptical", version)]
pub struct Cli {
    #[arg(long)]
    pub mcp: bool,

    #[arg(long)]
    pub tui: bool,

    #[arg(short, long)]
    pub fichier: Option<String>,

    #[arg(long, value_name = "fr|en|de")]
    pub langue: Option<String>,

    #[arg(long, value_name = "FICHIER.agf")]
    pub catalogue: Vec<String>,
}

/// Valeur de `--langue` lue avant l'analyse complète, pour afficher l'aide dans cette langue
fn langue_demandee(args: &[String]) -> Option<Langue> {
    args.iter().enumerate().find_map(|(i, a)| {
        if let Some(v) = a.strip_prefix("--langue=") {
            Langue::depuis_code(v)
        } else if a == "--langue" {
            args.get(i + 1).and_then(|v| Langue::depuis_code(v))
        } else {
            None
        }
    })
}

fn commande_traduite() -> clap::Command {
    let (utilisation, options) = match i18n::langue() {
        Langue::Fr => ("Utilisation :", "Options :"),
        Langue::En => ("Usage:", "Options:"),
        Langue::De => ("Aufruf:", "Optionen:"),
    };
    Cli::command()
        .help_template(format!(
            "{{about-with-newline}}\n{utilisation} {{usage}}\n\n{options}\n{{options}}\n"
        ))
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(
            clap::Arg::new("aide")
                .short('h')
                .long("help")
                .action(clap::ArgAction::Help)
                .help(tr!("Affiche cette aide", "Print this help", "Zeigt diese Hilfe")),
        )
        .arg(
            clap::Arg::new("version")
                .short('V')
                .long("version")
                .action(clap::ArgAction::Version)
                .help(tr!("Affiche la version", "Print the version", "Zeigt die Version")),
        )
        .about(tr!(
            "Logiciel de conception optique séquentielle (fenêtre graphique, TUI et serveur MCP)",
            "Sequential optical design software (graphical window, TUI and MCP server)",
            "Software für sequentielles Optikdesign (grafisches Fenster, TUI und MCP-Server)"
        ))
        .mut_arg("mcp", |a| {
            a.help(tr!(
                "Démarre FreeOptical en mode serveur MCP (Model Context Protocol) sur stdio",
                "Start FreeOptical as an MCP (Model Context Protocol) server on stdio",
                "Startet FreeOptical als MCP-Server (Model Context Protocol) über stdio"
            ))
        })
        .mut_arg("tui", |a| {
            a.help(tr!(
                "Démarre FreeOptical dans le terminal (TUI) au lieu de la fenêtre graphique",
                "Start FreeOptical in the terminal (TUI) instead of the graphical window",
                "Startet FreeOptical im Terminal (TUI) statt im grafischen Fenster"
            ))
        })
        .mut_arg("fichier", |a| {
            a.help(tr!(
                "Fichier système à charger : .json, .zmx (Zemax), ou .TDS / .RLD / .DES (TDESIGN, LENSDES, RAYTRACE)",
                "System file to load: .json, .zmx (Zemax), or .TDS / .RLD / .DES (TDESIGN, LENSDES, RAYTRACE)",
                "Zu ladende Systemdatei: .json, .zmx (Zemax) oder .TDS / .RLD / .DES (TDESIGN, LENSDES, RAYTRACE)"
            ))
        })
        .mut_arg("catalogue", |a| {
            a.help(tr!(
                "Importe un catalogue de verres Zemax (.agf), conservé pour les prochains démarrages",
                "Import a Zemax glass catalogue (.agf), kept for the next starts",
                "Importiert einen Zemax-Glaskatalog (.agf), der für spätere Starts erhalten bleibt"
            ))
        })
        .mut_arg("langue", |a| {
            a.help(tr!(
                "Langue de l'interface : fr, en ou de (sinon le choix enregistré, puis la langue du système)",
                "Interface language: fr, en or de (otherwise the saved choice, then the system language)",
                "Sprache der Oberfläche: fr, en oder de (sonst die gespeicherte Wahl, dann die Systemsprache)"
            ))
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    let arguments: Vec<String> = std::env::args().collect();
    i18n::initialiser(langue_demandee(&arguments));
    let args = Cli::from_arg_matches(&commande_traduite().get_matches_from(&arguments))?;
    if let Some(code) = &args.langue {
        if Langue::depuis_code(code).is_none() {
            anyhow::bail!(
                "{}",
                trf!(
                    "Langue « {} » inconnue : fr, en ou de.",
                    "Unknown language \"{}\": fr, en or de.",
                    "Unbekannte Sprache „{}“: fr, en oder de.",
                    code
                )
            );
        }
    }

    core::glass::charger_catalogues_utilisateur();
    for chemin in &args.catalogue {
        let nb = core::glass::importer_catalogue_agf(std::path::Path::new(chemin), true).map_err(anyhow::Error::msg)?;
        eprintln!("{}", trf!("{} : {} verres importés", "{}: {} glasses imported", "{}: {} Gläser importiert", chemin, nb));
    }

    let systeme = if let Some(chemin) = &args.fichier {
        let chemin = std::path::Path::new(chemin);
        if core::zemax::est_fichier_zemax(chemin) {
            let import = core::zemax::importer_fichier_zemax(chemin).map_err(anyhow::Error::msg)?;
            for avertissement in &import.avertissements {
                eprintln!("Import {} : {}", chemin.display(), avertissement);
            }
            import.systeme
        } else if core::teloptik::est_fichier_teloptik(chemin) {
            let import =
                core::teloptik::importer_fichier_teloptik(chemin).map_err(anyhow::Error::msg)?;
            // stderr : stdout est réservé au protocole MCP
            for avertissement in &import.avertissements {
                eprintln!("Import {} : {}", chemin.display(), avertissement);
            }
            import.systeme
        } else {
            let contenu = std::fs::read_to_string(chemin)?;
            serde_json::from_str::<OpticalSystem>(&contenu)?
        }
    } else {
        OpticalSystem::nouveau(
            tr!("Nouveau Système Optique", "New Optical System", "Neues optisches System"),
            core::glass::LAMBDA_D_NM,
        )
    };

    if args.mcp {
        mcp::demarrer_serveur_mcp(systeme).await?;
    } else if args.tui {
        ui::executer_tui(systeme)?;
    } else {
        gui::executer_gui_autonome(systeme)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_langue_lue_avant_l_analyse() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(langue_demandee(&a(&["freeoptical", "--langue", "de"])), Some(Langue::De));
        assert_eq!(langue_demandee(&a(&["freeoptical", "--langue=en", "--tui"])), Some(Langue::En));
        assert_eq!(langue_demandee(&a(&["freeoptical", "--tui"])), None);
        commande_traduite().debug_assert();
    }
}
