use crate::core::{
    analysis::{
        LongitudinalAberrationResult, SpotDiagramResult,
        calculer_aberration_spherique_longitudinale, calculer_spot_diagram,
    },
    optimizer::{OptimizationReport, OptimizationTarget, OptimizationVariable, optimiser_systeme_dls},
    paraxial::{ParaxialData, calculer_paraxial},
    raytracer::{TracedRay, tracer_eventail_meridional},
    teloptik::importer_fichier_teloptik,
    types::{OpticalSystem, Surface, SurfaceType},
};
use rmcp::{
    handler::server::wrapper::Parameters,
    schemars::{self, JsonSchema},
    serde::{Deserialize, Serialize},
    tool, tool_router,
};
use std::sync::{Arc, Mutex};

/// Paramètres de l'outil MCP `creer_systeme(nom, longueur_onde_ref)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CreerSystemeParams {
    /// Nom du nouveau design optique
    pub nom: String,
    /// Longueur d'onde de référence en nanomètres (ex: 587.56)
    pub longueur_onde_ref: f64,
}

/// Paramètres de l'outil MCP `ajouter_surface(type, rayon, epaisseur, verre, semi_diametre)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AjouterSurfaceParams {
    /// Type géométrique de la surface : `"PLANE"`, `"SPHERIQUE"`, `"CONIQUE"`, ou `"ASPHERIQUE"`
    #[serde(rename = "type", alias = "type_surface")]
    pub r#type: String,
    /// Rayon de courbure en mm (0.0 ou très grand pour une surface plane)
    pub rayon: f64,
    /// Épaisseur axiale jusqu'à la surface suivante en mm
    pub epaisseur: f64,
    /// Matériau après la surface (`"AIR"`, `"BK7"`, `"F2"`, `"SF11"`, `"SILICE"`, `"MIROIR"`, ou `"nd:Vd"`)
    pub verre: String,
    /// Demi-diamètre mécanique utile en mm
    pub semi_diametre: f64,
}

/// Sous-structure des propriétés modifiables dans `modifier_surface(index, parametres)`
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct SurfacePatchParams {
    /// Nouveau type de surface (`"PLANE"`, `"SPHERIQUE"`, `"CONIQUE"`, `"ASPHERIQUE"`)
    #[serde(rename = "type", alias = "type_surface")]
    pub r#type: Option<String>,
    /// Nouveau rayon de courbure en mm
    pub rayon: Option<f64>,
    /// Nouvelle épaisseur en mm
    pub epaisseur: Option<f64>,
    /// Nouveau matériau optique (`"AIR"`, `"BK7"`, `"F2"`, `"MIROIR"`...)
    pub verre: Option<String>,
    /// Nouveau demi-diamètre en mm
    pub semi_diametre: Option<f64>,
    /// Nouvelle constante conique k
    pub conique: Option<f64>,
    /// Nouveaux coefficients asphériques pairs [A4, A6, A8, A10]
    pub aspheriques: Option<[f64; 4]>,
    /// Nouveau commentaire descriptif
    pub commentaire: Option<String>,
    /// Définit cette surface comme diaphragme (STOP)
    pub est_diaphragme: Option<bool>,
}

/// Paramètres de l'outil MCP `modifier_surface(index, parametres)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ModifierSurfaceParams {
    /// Index de la surface à modifier (0..N-1)
    pub index: usize,
    /// Dictionnaire des propriétés à mettre à jour sur cette surface
    pub parametres: SurfacePatchParams,
}

/// Paramètres de l'outil MCP `optimiser_systeme(variables, cibles)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct OptimiserSystemeParams {
    /// Liste des variables d'optimisation (index_surface et parametre : "rayon", "epaisseur", "conique")
    pub variables: Vec<OptimizationVariable>,
    /// Liste des cibles de la fonction de mérite ("focale", "spot_rms", "aberration_spherique", "tirage_bfl")
    pub cibles: Vec<OptimizationTarget>,
}

/// Paramètres de l'outil MCP `importer_teloptik(chemin)`
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ImporterTeloptikParams {
    /// Chemin absolu d'un fichier `.TDS` (TDESIGN), `.RLD` (LENSDES) ou `.DES` (RAYTRACE)
    pub chemin: String,
}

/// Données brutes retournées par `lancer_simulation()`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationOutput {
    pub systeme: OpticalSystem,
    pub paraxial: ParaxialData,
    pub rayons_meridiens: Vec<TracedRay>,
    pub aberration_spherique: LongitudinalAberrationResult,
}

/// Résultat retourné par `optimiser_systeme(variables, cibles)`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimisationOutput {
    pub rapport: OptimizationReport,
    pub systeme_optimise: OpticalSystem,
}

fn avertissement_verre(verre: &str) -> String {
    format!(
        "Verre '{}' inconnu : il est traité comme de l'AIR. Utiliser un nom du catalogue (BK7, F2, SF11, SILICE...), MIROIR, ou la syntaxe 'nd:Vd'.",
        verre
    )
}

/// Serveur MCP de conception et simulation optique FreeOptical
#[derive(Clone)]
pub struct FreeOpticalMcpServer {
    pub systeme: Arc<Mutex<OpticalSystem>>,
}

impl FreeOpticalMcpServer {
    pub fn nouveau(systeme_initial: OpticalSystem) -> Self {
        Self {
            systeme: Arc::new(Mutex::new(systeme_initial)),
        }
    }
}

#[tool_router(server_handler)]
impl FreeOpticalMcpServer {
    /// Initialise un nouveau design optique avec son nom et sa longueur d'onde de référence (en nm).
    #[tool(
        name = "creer_systeme",
        description = "Initialise un nouveau système optique séquentiel avec son nom et sa longueur d'onde de référence (en nm)."
    )]
    pub async fn creer_systeme(&self, Parameters(params): Parameters<CreerSystemeParams>) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let lambda = if params.longueur_onde_ref > 100.0 {
            params.longueur_onde_ref
        } else {
            587.5618
        };
        // Crée un système doté uniquement d'un plan image initial afin que `ajouter_surface`
        // empile les dioptres dans l'ordre exact de création avant le plan image
        let mut nouveau = OpticalSystem::nouveau(&params.nom, lambda);
        nouveau.surfaces = vec![Surface::plan_image(15.0)];
        *sys = nouveau;

        serde_json::to_string_pretty(&*sys).unwrap_or_default()
    }

    /// Ajoute une surface optique (lentille ou miroir) avant le plan image du système.
    #[tool(
        name = "ajouter_surface",
        description = "Ajoute une surface optique (PLANE, SPHERIQUE, CONIQUE, ASPHERIQUE) avec son rayon, épaisseur, verre et demi-diamètre."
    )]
    pub async fn ajouter_surface(
        &self,
        Parameters(params): Parameters<AjouterSurfaceParams>,
    ) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let type_surf = SurfaceType::depuis_chaine(&params.r#type);
        let mut surf = Surface::nouvelle(
            type_surf,
            params.rayon,
            params.epaisseur,
            &params.verre,
            params.semi_diametre,
        );
        if sys.surfaces.len() <= 1 {
            surf.est_diaphragme = true;
        }
        surf.commentaire = format!("Surface {}", sys.surfaces.len());

        // Insère juste avant le Plan Image final
        let pos_insertion = sys.surfaces.len().saturating_sub(1);
        let verre_connu = sys.catalogue.est_connu(&surf.verre);
        let verre = surf.verre.clone();
        sys.surfaces.insert(pos_insertion, surf);

        let paraxial = calculer_paraxial(&sys);
        let mut reponse = serde_json::json!({
            "statut": "Surface ajoutée avec succès",
            "index_insere": pos_insertion,
            "nb_surfaces": sys.surfaces.len(),
            "paraxial": paraxial,
        });
        if !verre_connu {
            reponse["avertissement"] = avertissement_verre(&verre).into();
        }
        serde_json::to_string_pretty(&reponse).unwrap_or_default()
    }

    /// Modifie les propriétés d'une surface existante à partir de son index (0..N-1).
    #[tool(
        name = "modifier_surface",
        description = "Modifie les propriétés (type, rayon, epaisseur, verre, semi_diametre, conique, aspheriques) d'une surface existante."
    )]
    pub async fn modifier_surface(
        &self,
        Parameters(params): Parameters<ModifierSurfaceParams>,
    ) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let nb = sys.surfaces.len();
        let Some(surf) = sys.surfaces.get_mut(params.index) else {
            return serde_json::json!({
                "erreur": format!("Index de surface {} hors limites (0..{})", params.index, nb.saturating_sub(1))
            })
            .to_string();
        };

        let p = params.parametres;
        let type_explicite = p.r#type.is_some();
        if let Some(t) = p.r#type {
            surf.type_surface = SurfaceType::depuis_chaine(&t);
            if surf.type_surface == SurfaceType::Plane {
                surf.rayon = f64::INFINITY;
            }
        }
        if let Some(r) = p.rayon {
            surf.rayon = if r.abs() < 1e-9 { f64::INFINITY } else { r };
            // Un rayon fini sur une surface plane la rend sphérique (sinon il serait ignoré)
            if !type_explicite && surf.rayon.is_finite() && surf.type_surface == SurfaceType::Plane {
                surf.type_surface = SurfaceType::Spherique;
            }
        }
        if let Some(ep) = p.epaisseur {
            surf.epaisseur = ep;
        }
        if let Some(v) = p.verre {
            surf.verre = v.to_uppercase();
        }
        if let Some(sd) = p.semi_diametre {
            surf.semi_diametre = sd.max(0.1);
        }
        if let Some(k) = p.conique {
            surf.conique = k;
            // Une sphère ignore la constante conique
            if !type_explicite && k.abs() > 1e-9 && surf.type_surface == SurfaceType::Spherique {
                surf.type_surface = SurfaceType::Conique;
            }
        }
        if let Some(asph) = p.aspheriques {
            surf.aspheriques = asph;
        }
        if let Some(comm) = p.commentaire {
            surf.commentaire = comm;
        }
        if let Some(stop) = p.est_diaphragme {
            surf.est_diaphragme = stop;
        }

        let surf_maj = surf.clone();
        let paraxial = calculer_paraxial(&sys);
        let mut reponse = serde_json::json!({
            "statut": "Surface modifiée avec succès",
            "index": params.index,
            "surface": surf_maj,
            "paraxial": paraxial,
        });
        if !sys.catalogue.est_connu(&surf_maj.verre) {
            reponse["avertissement"] = avertissement_verre(&surf_maj.verre).into();
        }
        serde_json::to_string_pretty(&reponse).unwrap_or_default()
    }

    /// Remplace le système courant par un fichier TDESIGN / LENSDES / RAYTRACE importé.
    #[tool(
        name = "importer_teloptik",
        description = "Importe un fichier .TDS (TDESIGN), .RLD (LENSDES) ou .DES (RAYTRACE) des programmes du livre « Telescope Optics » de Rutten et van Venrooij, et en fait le système courant. Retourne le système, ses données paraxiales, la focale inscrite dans le fichier et les éléments non repris (obstructions, couleurs supplémentaires…). Les systèmes basculés ou décentrés sont refusés."
    )]
    pub async fn importer_teloptik(
        &self,
        Parameters(params): Parameters<ImporterTeloptikParams>,
    ) -> String {
        match importer_fichier_teloptik(std::path::Path::new(&params.chemin)) {
            Ok(import) => {
                let mut sys = self.systeme.lock().unwrap();
                *sys = import.systeme;
                let reponse = serde_json::json!({
                    "systeme": &*sys,
                    "paraxial": calculer_paraxial(&sys),
                    "focale_fichier_mm": import.focale_fichier_mm,
                    "avertissements": import.avertissements,
                });
                serde_json::to_string_pretty(&reponse).unwrap_or_default()
            }
            Err(e) => serde_json::json!({ "erreur": e }).to_string(),
        }
    }

    /// Exécute le lancer de rayons séquentiel et retourne les données brutes (paraxial, rayons, LSA).
    #[tool(
        name = "lancer_simulation",
        description = "Exécute le ray tracing séquentiel et retourne les données brutes : paramètres paraxiaux, trajectoires 3D des rayons et aberration sphérique longitudinale."
    )]
    pub async fn lancer_simulation(&self) -> String {
        let sys = self.systeme.lock().unwrap();
        let paraxial = calculer_paraxial(&sys);
        let rayons_meridiens =
            tracer_eventail_meridional(&sys, 0.0, 9, sys.longueur_onde_ref_nm);
        let aberration_spherique = calculer_aberration_spherique_longitudinale(&sys, 12);

        let out = SimulationOutput {
            systeme: sys.clone(),
            paraxial,
            rayons_meridiens,
            aberration_spherique,
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }

    /// Calcule et retourne les coordonnées des points d'impact au plan image et le rayon RMS du Spot Diagram.
    #[tool(
        name = "obtenir_spot_diagram",
        description = "Retourne les coordonnées des points d'impact au plan image, le centroïde, le rayon RMS (µm et mm), le rayon géométrique maximal et le rayon d'Airy."
    )]
    pub async fn obtenir_spot_diagram(&self) -> String {
        let sys = self.systeme.lock().unwrap();
        let spot: SpotDiagramResult = calculer_spot_diagram(&sys, (0.0, 0.0), 4);
        serde_json::to_string_pretty(&spot).unwrap_or_default()
    }

    /// Lance l'algorithme d'optimisation Damped Least Squares (DLS) selon les variables et cibles fournies.
    #[tool(
        name = "optimiser_systeme",
        description = "Lance l'algorithme d'optimisation Damped Least Squares (DLS / Levenberg-Marquardt) sur les variables sélectionnées pour atteindre une focale ou minimiser le Spot RMS / l'aberration sphérique."
    )]
    pub async fn optimiser_systeme(
        &self,
        Parameters(params): Parameters<OptimiserSystemeParams>,
    ) -> String {
        let mut sys = self.systeme.lock().unwrap();
        let rapport = optimiser_systeme_dls(&mut sys, &params.variables, &params.cibles, 20);
        let out = OptimisationOutput {
            rapport,
            systeme_optimise: sys.clone(),
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_outils_mcp_workflow_complet() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());

        // 1. creer_systeme
        let res_creer = srv
            .creer_systeme(Parameters(CreerSystemeParams {
                nom: "Singlet Test MCP".to_string(),
                longueur_onde_ref: 587.56,
            }))
            .await;
        assert!(res_creer.contains("Singlet Test MCP"));

        // 2. ajouter_surface (Entrée et Sortie de la lentille BK7)
        srv.ajouter_surface(Parameters(AjouterSurfaceParams {
            r#type: "SPHERIQUE".to_string(),
            rayon: 60.0,
            epaisseur: 5.0,
            verre: "BK7".to_string(),
            semi_diametre: 15.0,
        }))
        .await;

        srv.ajouter_surface(Parameters(AjouterSurfaceParams {
            r#type: "SPHERIQUE".to_string(),
            rayon: -180.0,
            epaisseur: 85.0,
            verre: "AIR".to_string(),
            semi_diametre: 15.0,
        }))
        .await;

        // 3. modifier_surface
        let res_mod = srv
            .modifier_surface(Parameters(ModifierSurfaceParams {
                index: 0,
                parametres: SurfacePatchParams {
                    rayon: Some(65.0),
                    commentaire: Some("Entrée Singlet Modifiée".to_string()),
                    ..Default::default()
                },
            }))
            .await;
        assert!(res_mod.contains("Surface modifiée avec succès"));

        // 4. lancer_simulation
        let res_sim = srv.lancer_simulation().await;
        assert!(res_sim.contains("focale_effl_mm"));

        // 5. obtenir_spot_diagram
        let res_spot = srv.obtenir_spot_diagram().await;
        assert!(res_spot.contains("rayon_rms_um"));

        // 6. optimiser_systeme
        let res_opt = srv
            .optimiser_systeme(Parameters(OptimiserSystemeParams {
                variables: vec![
                    OptimizationVariable {
                        index_surface: 1,
                        parametre: "rayon".to_string(),
                    },
                    OptimizationVariable {
                        index_surface: 1,
                        parametre: "epaisseur".to_string(),
                    },
                ],
                cibles: vec![
                    OptimizationTarget {
                        type_cible: "focale".to_string(),
                        valeur_cible: 100.0,
                        poids: 1.0,
                    },
                    OptimizationTarget {
                        type_cible: "spot_rms".to_string(),
                        valeur_cible: 0.0,
                        poids: 1.0,
                    },
                ],
            }))
            .await;
        assert!(res_opt.contains("DLS terminé"));
    }

    #[tokio::test]
    async fn test_outil_mcp_import_teloptik() {
        let srv = FreeOpticalMcpServer::nouveau(OpticalSystem::doublet_achromatique_defaut());
        let chemin = std::env::temp_dir().join(format!("freeoptical_test_{}.TDS", std::process::id()));
        std::fs::write(
            &chemin,
            "\"Parabole\"\r\n1,1,200,-1000,0\r\n-2000,0,-1\r\n0,0,0,0,0\r\n-1\r\n-1\r\n\"N\",\"N\"\r\n",
        )
        .unwrap();
        let res = srv
            .importer_teloptik(Parameters(ImporterTeloptikParams {
                chemin: chemin.display().to_string(),
            }))
            .await;
        std::fs::remove_file(&chemin).ok();
        let json: serde_json::Value = serde_json::from_str(&res).unwrap();
        assert_eq!(json["systeme"]["nom"], "Parabole");
        let effl = json["paraxial"]["focale_effl_mm"].as_f64().unwrap();
        assert!((effl - 1000.0).abs() < 1e-6, "EFFL = {effl}");
        assert_eq!(srv.systeme.lock().unwrap().surfaces.len(), 2);

        let erreur = srv
            .importer_teloptik(Parameters(ImporterTeloptikParams {
                chemin: "/inexistant.TDS".to_string(),
            }))
            .await;
        assert!(erreur.contains("erreur"));
    }
}

