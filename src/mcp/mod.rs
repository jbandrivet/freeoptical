//! Module MCP : Serveur Model Context Protocol permettant le pilotage autonome par une IA.

pub mod server;

use crate::{core::types::OpticalSystem, mcp::server::FreeOpticalMcpServer};
use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};

/// Démarre le serveur MCP FreeOptical sur l'entrée/sortie standard (stdio)
pub async fn demarrer_serveur_mcp(systeme_initial: OpticalSystem) -> Result<()> {
    let serveur = FreeOpticalMcpServer::nouveau(systeme_initial);
    let service = serveur.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
