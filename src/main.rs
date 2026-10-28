pub mod core;
pub mod gui;
pub mod maj;
pub mod mcp;
pub mod ui;

use anyhow::Result;
use clap::Parser;
use core::types::OpticalSystem;

/// FreeOptical - Logiciel open-source de conception et de simulation optique séquentielle
#[derive(Parser, Debug)]
#[command(
    name = "freeoptical",
    version,
    about = "Logiciel de conception optique séquentielle (Exécutable Graphique Autonome, TUI DOS & Serveur MCP)"
)]
pub struct Cli {
    /// Démarre FreeOptical en mode serveur MCP (Model Context Protocol) sur stdio
    #[arg(long)]
    pub mcp: bool,

    /// Démarre FreeOptical en mode Terminal (TUI) au lieu de la fenêtre graphique autonome
    #[arg(long)]
    pub tui: bool,

    /// Fichier système optique à charger au démarrage : .json, ou .TDS / .RLD / .DES
    /// (programmes TDESIGN, LENSDES et RAYTRACE de Rutten et van Venrooij)
    #[arg(short, long)]
    pub fichier: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();

    let systeme = if let Some(chemin) = &args.fichier {
        let chemin = std::path::Path::new(chemin);
        if core::teloptik::est_fichier_teloptik(chemin) {
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
        OpticalSystem::nouveau("Nouveau Système Optique", core::glass::LAMBDA_D_NM)
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
