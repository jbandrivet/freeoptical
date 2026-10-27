use crate::core::{
    analysis::{
        LongitudinalAberrationResult, SpotDiagramResult,
        calculer_aberration_spherique_longitudinale, calculer_spot_diagram,
    },
    glass::LAMBDA_D_NM,
    optimizer::{OptimizationTarget, OptimizationVariable, optimiser_systeme_dls},
    paraxial::{ParaxialData, calculer_paraxial},
    presets::{creer_interferometre_bath, creer_singlet_aspherique, creer_telescope_cassegrain},
    types::{OpticalSystem, Surface, SurfaceType},
};
use crate::ui::lde::COLONNES_LDE;

/// Vue graphique ou analytique active dans le panneau inférieur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    /// Tracé 2D en coupe transversale (lentilles + éventail de rayons)
    Coupe2D,
    /// Projection 3D isométrique filaire orientable
    Filaire3D,
    /// Diagramme de points (Spot Diagram polychromatique + tache d'Airy)
    SpotDiagram,
    /// Courbe d'aberration sphérique longitudinale (LSA)
    AberrationSpherique,
    /// Catalogue de verres et détails complets des surfaces
    CatalogueVerres,
}

impl ActiveView {
    pub fn titre(&self) -> &'static str {
        match self {
            ActiveView::Coupe2D => "COUPE TRANSVERSALE 2D & LANCER DE RAYONS",
            ActiveView::Filaire3D => "PROJECTION 3D ISOMÉTRIQUE FILAIRE (WIREFRAME)",
            ActiveView::SpotDiagram => "ANALYSE : SPOT DIAGRAM AU PLAN IMAGE & DISQUE D'AIRY",
            ActiveView::AberrationSpherique => "ANALYSE : ABERRATION SPHÉRIQUE LONGITUDINALE (LSA)",
            ActiveView::CatalogueVerres => "CATALOGUE DE VERRES OPTIQUES (nd / Vd) & MATÉRIAUX",
        }
    }
}

/// Définition des menus déroulants supérieurs (Fichier, Édition, Analyse, Outils)
pub const TITRES_MENUS: [&str; 4] = ["Fichier", "Édition", "Analyse", "Outils"];

pub const ITEMS_MENUS: [&[&str]; 4] = [
    &[
        "1. Nouveau système vierge",
        "2. Charger Doublet Fraunhofer 100mm (BK7/F2)",
        "3. Charger Singlet Asphérique F/2 (LAK9)",
        "4. Charger Télescope Cassegrain (Miroirs)",
        "5. Charger Interféromètre de Bath (632.8 nm)",
        "6. Sauvegarder dans systeme_optique.json",
        "7. Quitter FreeOptical (Q)",
    ],
    &[
        "1. Modifier la cellule active (Entrée)",
        "2. Insérer une surface avant (I / Ins)",
        "3. Ajouter une surface avant Plan Image (A)",
        "4. Supprimer la surface sélectionnée (Suppr / X)",
        "5. Changer le type de surface (T)",
        "6. Définir comme Diaphragme STOP (S)",
    ],
    &[
        "1. Coupe Transversale 2D (F1 / 1)",
        "2. Projection 3D Isométrique Filaire (F2 / 2)",
        "3. Spot Diagram Polychromatique (F3 / 3)",
        "4. Aberration Sphérique Longitudinale (F4 / 4)",
        "5. Catalogue de Verres & Dispersion (F5 / 5)",
    ],
    &[
        "1. Optimiser le système (DLS : Spot RMS minimal) (O)",
        "2. Mise au point paraxiale automatique (BFL) (P)",
        "3. Recalculer toute la simulation (R)",
    ],
];

/// État global de l'application TUI style DOS
pub struct AppState {
    pub systeme: OpticalSystem,
    pub paraxial: ParaxialData,
    pub spot: SpotDiagramResult,
    pub lsa: LongitudinalAberrationResult,
    pub ligne_selectionnee: usize,
    pub colonne_selectionnee: usize,
    pub mode_saisie: Option<String>,
    pub menu_ouvert: Option<usize>,
    pub item_menu_selectionne: usize,
    pub vue_active: ActiveView,
    pub angle_3d_yaw_deg: f64,
    pub angle_3d_pitch_deg: f64,
    pub nb_rayons_2d: usize,
    pub statut_message: String,
    pub doit_quitter: bool,
}

impl AppState {
    pub fn nouveau(systeme: OpticalSystem) -> Self {
        let paraxial = calculer_paraxial(&systeme);
        let spot = calculer_spot_diagram(&systeme, (0.0, 0.0), 4);
        let lsa = calculer_aberration_spherique_longitudinale(&systeme, 14);
        let statut_message = format!(
            "PRÊT | EFFL = {:.2} mm | BFL = {:.2} mm | Spot RMS = {:.2} µm | F10/M: Menu | Entrée: Éditer",
            paraxial.focale_effl_mm, paraxial.tirage_bfl_mm, spot.rayon_rms_um
        );

        Self {
            systeme,
            paraxial,
            spot,
            lsa,
            ligne_selectionnee: 0,
            colonne_selectionnee: 2, // Colonne Rayon par défaut
            mode_saisie: None,
            menu_ouvert: None,
            item_menu_selectionne: 0,
            vue_active: ActiveView::Coupe2D,
            angle_3d_yaw_deg: 35.0,
            angle_3d_pitch_deg: 22.0,
            nb_rayons_2d: 9,
            statut_message,
            doit_quitter: false,
        }
    }

    /// Recalcule immédiatement les données paraxiales, le Spot Diagram et l'aberration sphérique
    pub fn recalculer(&mut self) {
        self.paraxial = calculer_paraxial(&self.systeme);
        self.spot = calculer_spot_diagram(&self.systeme, (0.0, 0.0), 4);
        self.lsa = calculer_aberration_spherique_longitudinale(&self.systeme, 14);
        if self.ligne_selectionnee >= self.systeme.surfaces.len() {
            self.ligne_selectionnee = self.systeme.surfaces.len().saturating_sub(1);
        }
    }

    /// Retourne la valeur textuelle actuelle de la cellule sélectionnée pour pré-remplir la saisie
    pub fn valeur_cellule_courante(&self) -> String {
        let Some(surf) = self.systeme.surfaces.get(self.ligne_selectionnee) else {
            return String::new();
        };
        match self.colonne_selectionnee {
            0 => surf.type_surface.libelle_court().to_string(),
            1 => surf.commentaire.clone(),
            2 => {
                if !surf.rayon.is_finite() {
                    "INF".to_string()
                } else {
                    format!("{:.4}", surf.rayon)
                }
            }
            3 => format!("{:.4}", surf.epaisseur),
            4 => surf.verre.clone(),
            5 => format!("{:.4}", surf.semi_diametre),
            6 => format!("{:.4}", surf.conique),
            _ => String::new(),
        }
    }

    /// Valide la saisie clavier et met à jour la propriété correspondante de la surface sélectionnée
    pub fn appliquer_saisie(&mut self, texte: &str) {
        let val = texte.trim();
        let Some(surf) = self.systeme.surfaces.get_mut(self.ligne_selectionnee) else {
            return;
        };

        match self.colonne_selectionnee {
            0 => {
                surf.type_surface = SurfaceType::depuis_chaine(val);
                if surf.type_surface == SurfaceType::Plane {
                    surf.rayon = f64::INFINITY;
                }
            }
            1 => {
                surf.commentaire = val.to_string();
            }
            2 => {
                let v_up = val.to_uppercase();
                if v_up == "INF" || v_up == "INFINI" || v_up == "0" {
                    surf.rayon = f64::INFINITY;
                    surf.type_surface = SurfaceType::Plane;
                } else if let Ok(r) = val.replace(',', ".").parse::<f64>() {
                    surf.rayon = r;
                    if surf.type_surface == SurfaceType::Plane {
                        surf.type_surface = SurfaceType::Spherique;
                    }
                }
            }
            3 => {
                if let Ok(ep) = val.replace(',', ".").parse::<f64>() {
                    surf.epaisseur = ep.max(0.0);
                }
            }
            4 => {
                if !val.is_empty() {
                    surf.verre = val.to_uppercase();
                }
            }
            5 => {
                if let Ok(sd) = val.replace(',', ".").parse::<f64>() {
                    surf.semi_diametre = sd.max(0.5);
                }
            }
            6 => {
                if let Ok(k) = val.replace(',', ".").parse::<f64>() {
                    surf.conique = k;
                    if k.abs() > 1e-9 && surf.type_surface == SurfaceType::Spherique {
                        surf.type_surface = SurfaceType::Conique;
                    }
                }
            }
            _ => {}
        }

        let nom_col = COLONNES_LDE[self.colonne_selectionnee];
        self.recalculer();
        let verre = &self.systeme.surfaces[self.ligne_selectionnee].verre;
        self.statut_message = if self.colonne_selectionnee == 4 && !self.systeme.catalogue.est_connu(verre) {
            format!(
                "ATTENTION : verre '{}' inconnu, traité comme de l'AIR (catalogue : F5, ou syntaxe nd:Vd)",
                verre
            )
        } else {
            format!(
                "Surface #{} [{}] modifiée -> EFFL = {:.2} mm | Spot RMS = {:.2} µm",
                self.ligne_selectionnee + 1,
                nom_col,
                self.paraxial.focale_effl_mm,
                self.spot.rayon_rms_um
            )
        };
    }

    /// Fait défiler cycliquement le type de la surface active (Plane -> Sphérique -> Conique -> Asphérique)
    pub fn basculer_type_surface(&mut self) {
        if let Some(surf) = self.systeme.surfaces.get_mut(self.ligne_selectionnee) {
            surf.type_surface = match surf.type_surface {
                SurfaceType::Plane => {
                    if !surf.rayon.is_finite() {
                        surf.rayon = 100.0;
                    }
                    SurfaceType::Spherique
                }
                SurfaceType::Spherique => SurfaceType::Conique,
                SurfaceType::Conique => SurfaceType::Aspherique,
                SurfaceType::Aspherique => {
                    surf.rayon = f64::INFINITY;
                    SurfaceType::Plane
                }
            };
            let nouveau = surf.type_surface.libelle_court();
            self.recalculer();
            self.statut_message = format!(
                "Surface #{} convertie en type {}",
                self.ligne_selectionnee + 1,
                nouveau
            );
        }
    }

    /// Insère une nouvelle surface à la position courante
    pub fn inserer_surface_courante(&mut self) {
        let mut nouv = Surface::nouvelle(SurfaceType::Spherique, 100.0, 5.0, "BK7", 15.0);
        nouv.commentaire = "Nouvelle Surface".to_string();
        let idx = self.ligne_selectionnee.min(self.systeme.surfaces.len());
        self.systeme.surfaces.insert(idx, nouv);
        self.recalculer();
        self.statut_message = format!("Surface insérée à la position #{}.", idx + 1);
    }

    /// Ajoute une surface juste avant le plan image
    pub fn ajouter_surface_avant_image(&mut self) {
        let mut nouv = Surface::nouvelle(SurfaceType::Spherique, 100.0, 10.0, "AIR", 15.0);
        nouv.commentaire = "Dioptre Ajouté".to_string();
        let idx = self.systeme.surfaces.len().saturating_sub(1);
        self.systeme.surfaces.insert(idx, nouv);
        self.ligne_selectionnee = idx;
        self.recalculer();
        self.statut_message = format!("Surface ajoutée en position #{} (avant Plan Image).", idx + 1);
    }

    /// Supprime la surface sélectionnée (en conservant au moins 2 surfaces)
    pub fn supprimer_surface_courante(&mut self) {
        if self.systeme.surfaces.len() <= 2 {
            self.statut_message =
                "IMPOSSIBLE : Le système doit comporter au moins une surface et un plan image."
                    .to_string();
            return;
        }
        if self.ligne_selectionnee + 1 >= self.systeme.surfaces.len() {
            self.statut_message = "IMPOSSIBLE : le plan image ne peut pas être supprimé.".to_string();
            return;
        }
        self.systeme.surfaces.remove(self.ligne_selectionnee);
        if self.ligne_selectionnee >= self.systeme.surfaces.len() {
            self.ligne_selectionnee = self.systeme.surfaces.len() - 1;
        }
        self.recalculer();
        self.statut_message = "Surface supprimée.".to_string();
    }

    /// Définit la surface active comme le diaphragme d'ouverture (STOP)
    pub fn definir_diaphragme(&mut self) {
        for (i, s) in self.systeme.surfaces.iter_mut().enumerate() {
            s.est_diaphragme = i == self.ligne_selectionnee;
        }
        self.recalculer();
        self.statut_message = format!(
            "Diaphragme (STOP) défini sur la surface #{}.",
            self.ligne_selectionnee + 1
        );
    }

    /// Lance l'optimiseur Damped Least Squares (DLS) directement depuis l'interface TUI
    pub fn lancer_optimisation_rapide(&mut self) {
        let cible_focale = if self.paraxial.focale_effl_mm.is_finite() {
            self.paraxial.focale_effl_mm
        } else {
            100.0
        };

        let mut vars = Vec::new();
        let n = self.systeme.surfaces.len();
        for i in 0..n.saturating_sub(1) {
            // Les faces planes (cubes, lames) restent planes
            if self.systeme.surfaces[i].type_surface == SurfaceType::Plane {
                continue;
            }
            vars.push(OptimizationVariable {
                index_surface: i,
                parametre: "rayon".to_string(),
            });
        }
        if n >= 2 {
            vars.push(OptimizationVariable {
                index_surface: n - 2,
                parametre: "epaisseur".to_string(),
            });
        }

        let cibles = vec![
            OptimizationTarget {
                type_cible: "focale".to_string(),
                valeur_cible: cible_focale,
                poids: 1.0,
            },
            OptimizationTarget {
                type_cible: "spot_rms".to_string(),
                valeur_cible: 0.0,
                poids: 2.0,
            },
        ];

        let rapport = optimiser_systeme_dls(&mut self.systeme, &vars, &cibles, 15);
        self.recalculer();
        self.statut_message = rapport.resume;
    }

    /// Ajuste automatiquement l'épaisseur avant le plan image sur le tirage paraxial BFL
    pub fn mise_au_point_paraxiale(&mut self) {
        let bfl = self.paraxial.tirage_bfl_mm;
        let n = self.systeme.surfaces.len();
        // Le tirage doit être compté dans le sens de propagation (négatif après un nombre impair de miroirs)
        if bfl.is_finite() && bfl * self.systeme.sens_propagation_image() > 0.1 && n >= 2 {
            self.systeme.surfaces[n - 2].epaisseur = bfl;
            self.recalculer();
            self.statut_message = format!(
                "Mise au point paraxiale effectuée : Tirage BFL = {:.3} mm | Spot RMS = {:.2} µm",
                bfl, self.spot.rayon_rms_um
            );
        } else {
            self.statut_message =
                "Mise au point impossible : image paraxiale virtuelle ou à l'infini.".to_string();
        }
    }

    /// Exécute une action du menu déroulant supérieur
    pub fn executer_action_menu(&mut self, menu_idx: usize, item_idx: usize) {
        self.menu_ouvert = None;
        match (menu_idx, item_idx) {
            // Menu Fichier
            (0, 0) => {
                self.systeme = OpticalSystem::nouveau("Nouveau Système Optique", LAMBDA_D_NM);
                self.ligne_selectionnee = 0;
                self.recalculer();
                self.statut_message = "Nouveau système optique initialisé.".to_string();
            }
            (0, 1) => {
                self.systeme = OpticalSystem::doublet_achromatique_defaut();
                self.ligne_selectionnee = 0;
                self.recalculer();
                self.statut_message = "Doublet Achromatique Fraunhofer 100mm chargé.".to_string();
            }
            (0, 2) => {
                self.systeme = creer_singlet_aspherique();
                self.ligne_selectionnee = 0;
                self.recalculer();
                self.statut_message = "Singlet Asphérique F/2 (LAK9) chargé.".to_string();
            }
            (0, 3) => {
                self.systeme = creer_telescope_cassegrain();
                self.ligne_selectionnee = 0;
                self.recalculer();
                self.statut_message = "Télescope Cassegrain à miroirs chargé.".to_string();
            }
            (0, 4) => {
                self.systeme = creer_interferometre_bath();
                self.ligne_selectionnee = 0;
                self.recalculer();
                self.statut_message =
                    "Interféromètre de Bath (Laser He-Ne 632.8 nm) chargé.".to_string();
            }
            (0, 5) => {
                let resultat = serde_json::to_string_pretty(&self.systeme)
                    .map_err(|e| e.to_string())
                    .and_then(|json| {
                        std::fs::write("systeme_optique.json", json).map_err(|e| e.to_string())
                    });
                self.statut_message = match resultat {
                    Ok(()) => "Système sauvegardé dans 'systeme_optique.json'.".to_string(),
                    Err(e) => format!("ÉCHEC de la sauvegarde de 'systeme_optique.json' : {}", e),
                };
            }
            (0, 6) => {
                self.doit_quitter = true;
            }
            // Menu Édition
            (1, 0) => {
                self.mode_saisie = Some(self.valeur_cellule_courante());
            }
            (1, 1) => self.inserer_surface_courante(),
            (1, 2) => self.ajouter_surface_avant_image(),
            (1, 3) => self.supprimer_surface_courante(),
            (1, 4) => self.basculer_type_surface(),
            (1, 5) => self.definir_diaphragme(),
            // Menu Analyse
            (2, 0) => self.vue_active = ActiveView::Coupe2D,
            (2, 1) => self.vue_active = ActiveView::Filaire3D,
            (2, 2) => self.vue_active = ActiveView::SpotDiagram,
            (2, 3) => self.vue_active = ActiveView::AberrationSpherique,
            (2, 4) => self.vue_active = ActiveView::CatalogueVerres,
            // Menu Outils
            (3, 0) => self.lancer_optimisation_rapide(),
            (3, 1) => self.mise_au_point_paraxiale(),
            (3, 2) => {
                self.recalculer();
                self.statut_message = "Simulation optique recalculée avec succès.".to_string();
            }
            _ => {}
        }
    }
}
