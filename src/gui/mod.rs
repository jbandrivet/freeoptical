//! Module GUI : Exécutable graphique autonome natif (OpenGL / `eframe` + `egui`)
//! avec esthétique rétro MS-DOS / CAO Zemax, rendu vectoriel haute résolution 2D/3D
//! et contrôle interactif à la souris et au clavier.

use crate::{
    core::{
        analysis::{
            LongitudinalAberrationResult, SpotDiagramResult,
            calculer_aberration_spherique_longitudinale, calculer_spot_diagram,
        },
        glass::{LAMBDA_D_NM, MediumKind},
        optimizer::{OptimizationTarget, OptimizationVariable, optimiser_systeme_dls},
        paraxial::{ParaxialData, calculer_paraxial},
        presets::{creer_interferometre_bath, creer_singlet_aspherique, creer_telescope_cassegrain},
        teloptik::{est_fichier_teloptik, importer_fichier_teloptik},
        raytracer::{
            TracedRay, snell_descartes_3d, tracer_eventail_meridional, tracer_grille_hexapolaire,
        },
        types::{OpticalSystem, Point3, Surface, SurfaceType, Vec3},
    },
    maj::{self, VersionDisponible},
    ui::app::ActiveView,
};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, FontFamily, FontId, Frame as EguiFrame, Key,
    Margin, Pos2, RichText, Sense, Shape, Stroke, StrokeKind, Vec2,
};

// Palette MS-DOS / CAO Rétro haute lisibilité
const COL_DOS_BG: Color32 = Color32::from_rgb(0, 0, 115);
const COL_DOS_PANEL: Color32 = Color32::from_rgb(0, 18, 145);
const COL_DOS_CANVAS: Color32 = Color32::BLACK;
const COL_DOS_BAR: Color32 = Color32::from_rgb(192, 192, 192);
const COL_DOS_WHITE: Color32 = Color32::from_rgb(245, 245, 245);
const COL_DOS_GRAY: Color32 = Color32::from_rgb(195, 205, 220);
const COL_DOS_YELLOW: Color32 = Color32::from_rgb(255, 255, 85);
const COL_DOS_CYAN: Color32 = Color32::from_rgb(85, 255, 255);
const COL_DOS_GREEN: Color32 = Color32::from_rgb(85, 255, 85);
const COL_DOS_RED: Color32 = Color32::from_rgb(255, 90, 90);
const COL_DOS_ORANGE: Color32 = Color32::from_rgb(255, 175, 45);
const COL_DOS_MAGENTA: Color32 = Color32::from_rgb(255, 105, 255);
const COL_VERRE_FILL: Color32 = Color32::from_rgba_premultiplied(35, 130, 210, 75);
const COL_CUBE_FILL: Color32 = Color32::from_rgba_premultiplied(80, 150, 230, 65);

/// Calcule les segments 3D des rayons réfléchis à 90° par la lame semi-réfléchissante à 45°
/// de chaque Cube Séparateur traversé par un faisceau de rayons.
/// Retourne une liste de polylignes 3D `[Point_Impact_Diagonale, Point_Sortie_Face_Laterale, Point_Externe]`.
fn calculer_rayons_reflechis_cubes(
    sys: &OpticalSystem,
    positions_z: &[f64],
    rayons: &[TracedRay],
) -> Vec<Vec<Point3>> {
    let mut polylignes = Vec::new();

    for idx in 0..sys.surfaces.len().saturating_sub(1) {
        let surf = &sys.surfaces[idx];
        if !est_cube_separateur(sys, surf) {
            continue;
        }

        // La lame va de (y = -sd, z min) à (y = +sd, z max), même si le cube est traversé vers -Z
        let z1 = positions_z[idx].min(positions_z[idx + 1]);
        let z2 = positions_z[idx].max(positions_z[idx + 1]);
        let dz_cube = z2 - z1;
        if dz_cube < 1e-3 {
            continue;
        }
        let sd = surf.semi_diametre;
        let z_mid = 0.5 * (z1 + z2);

        // Plan diagonal passant par (0, -sd, z1) et (0, +sd, z2) :
        // Equation : (z - z_mid) * sd - y * (dz_cube / 2) = 0
        let n_diag = Vec3::new(0.0, -0.5 * dz_cube, sd).normalize();
        let mat_cube = sys.catalogue.resoudre(&surf.verre);

        for tr in rayons {
            let (Some(h1), Some(h2)) = (tr.impacts.get(idx), tr.impacts.get(idx + 1)) else {
                continue;
            };
            let p1 = h1.point_global;
            let p2 = h2.point_global;
            let seg = p2 - p1;
            let seg_len = seg.norm();
            if seg_len < 1e-6 {
                continue;
            }
            let dir_interne = seg / seg_len;

            // Intersection du rayon interne avec la lame diagonale à 45°
            let centre_cube = Point3::new(0.0, 0.0, z_mid);
            let denom = dir_interne.dot(&n_diag);
            if denom.abs() < 1e-9 {
                continue;
            }
            let t_diag = -(p1 - centre_cube).dot(&n_diag) / denom;
            if t_diag < 0.0 || t_diag > seg_len {
                continue;
            }
            let p_split = p1 + t_diag * dir_interne;

            // Direction réfléchie par la lame séparatrice à 45° (loi de la réflexion vectorielle 3D)
            let Some(dir_refl) = snell_descartes_3d(&dir_interne, &n_diag, 1.0, 1.0, true) else {
                continue;
            };

            // Intersection du rayon réfléchi avec la face latérale du cube (y = +sd ou y = -sd)
            if dir_refl.y.abs() < 1e-6 {
                continue;
            }
            let y_face = if dir_refl.y > 0.0 { sd } else { -sd };
            let t_lat = (y_face - p_split.y) / dir_refl.y;
            if t_lat <= 0.0 {
                continue;
            }
            let p_face_lat = p_split + t_lat * dir_refl;

            // Réfraction en sortie de la face latérale du cube (verre BK7 -> AIR)
            let normale_lat = Vec3::new(0.0, if dir_refl.y > 0.0 { 1.0 } else { -1.0 }, 0.0);
            let n_verre = mat_cube.indice_refraction(tr.longueur_onde_nm);
            let dir_sortie_air =
                snell_descartes_3d(&dir_refl, &normale_lat, n_verre, 1.0, false)
                    .unwrap_or(dir_refl);

            // Prolongement du faisceau réfléchi à l'extérieur du cube jusqu'au plan latéral (y = ±2.65 * sd)
            let y_cible_ext = y_face.signum() * sd * 2.65;
            let t_ext = if dir_sortie_air.y.abs() > 1e-6 {
                ((y_cible_ext - p_face_lat.y) / dir_sortie_air.y).max(0.1)
            } else {
                sd * 1.65
            };
            let p_externe = p_face_lat + t_ext * dir_sortie_air;

            polylignes.push(vec![p_split, p_face_lat, p_externe]);
        }
    }

    polylignes
}

/// Trace un segment en trait continu anticrénelé
fn tracer_segment(painter: &egui::Painter, p1: Pos2, p2: Pos2, epaisseur: f32, couleur: Color32) {
    painter.line_segment([p1, p2], Stroke::new(epaisseur, couleur));
}

/// Trace une polyligne en trait continu anticrénelé
fn tracer_polyligne(painter: &egui::Painter, pts: &[Pos2], epaisseur: f32, couleur: Color32) {
    if pts.len() >= 2 {
        painter.add(Shape::line(pts.to_vec(), Stroke::new(epaisseur, couleur)));
    }
}

/// Face d'entrée d'un cube séparateur 50/50 (repéré par son commentaire)
fn est_cube_separateur(sys: &OpticalSystem, surf: &Surface) -> bool {
    let comm_up = surf.commentaire.to_uppercase();
    (comm_up.contains("CUBE") || comm_up.contains("SEPARAT") || comm_up.contains("SÉPARAT"))
        && est_verre(sys, surf)
}

/// Vrai si le milieu qui suit la surface est un verre réfractif (lentille ou cube)
fn est_verre(sys: &OpticalSystem, surf: &Surface) -> bool {
    matches!(sys.catalogue.resoudre(&surf.verre).kind, MediumKind::Verre { .. })
}

/// Fichier de sauvegarde du système, dans le répertoire courant
const FICHIER_SYSTEME: &str = "systeme_optique.json";

/// Lance la fenêtre graphique autonome native de FreeOptical
pub fn executer_gui_autonome(systeme_initial: OpticalSystem) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!(
                "FreeOptical v{} — Conception & Simulation Optique Séquentielle",
                maj::VERSION_COURANTE
            ))
            .with_inner_size([1380.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            // L'app_id relie la fenêtre au lanceur freeoptical.desktop (icône sous Wayland)
            .with_app_id("freeoptical")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(
                    "../../assets/icons/freeoptical-256.png"
                ))
                .unwrap_or_default(),
            ),
        ..Default::default()
    };

    eframe::run_native(
        "FreeOptical",
        options,
        Box::new(|cc| {
            configurer_theme_dos(&cc.egui_ctx);
            Ok(Box::new(FreeOpticalGuiApp::nouveau(systeme_initial)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Erreur d'ouverture de la fenêtre graphique : {}", e))
}

fn configurer_theme_dos(ctx: &Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = COL_DOS_BG;
    visuals.window_fill = COL_DOS_PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(0, 0, 75);
    visuals.faint_bg_color = Color32::from_rgb(0, 30, 155);
    visuals.override_text_color = Some(COL_DOS_WHITE);
    visuals.selection.bg_fill = Color32::from_rgb(0, 150, 180);
    visuals.selection.stroke = Stroke::new(1.5, COL_DOS_YELLOW);
    visuals.widgets.noninteractive.bg_fill = COL_DOS_PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, COL_DOS_GRAY);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0, 40, 165);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, COL_DOS_CYAN);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0, 95, 195);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.5, COL_DOS_YELLOW);
    visuals.widgets.active.bg_fill = Color32::from_rgb(0, 130, 210);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, COL_DOS_YELLOW);
    ctx.set_visuals(visuals);
}

/// Étapes de la mise à jour en ligne
#[derive(Clone, Debug)]
pub enum EtatMaj {
    Recherche,
    AJour,
    Disponible(VersionDisponible),
    Installation(String),
    Installee(String),
    Erreur(String),
}

pub struct FreeOpticalGuiApp {
    pub systeme: OpticalSystem,
    pub paraxial: ParaxialData,
    pub spot: SpotDiagramResult,
    pub lsa: LongitudinalAberrationResult,
    pub ligne_selectionnee: usize,
    pub vue_active: ActiveView,
    pub yaw_3d_deg: f64,
    pub pitch_3d_deg: f64,
    pub zoom: f32,
    pub pan_offset: Vec2,
    pub nb_rayons_2d: usize,
    pub statut_message: String,
    pub fenetre_maj: bool,
    pub etat_maj: EtatMaj,
    recepteur_maj: Option<mpsc::Receiver<EtatMaj>>,
    pub fenetre_import: bool,
    pub dossier_import: String,
    /// Résultat du dernier import (message, avertissements) affiché dans la fenêtre d'import
    pub bilan_import: Option<(String, Vec<String>)>,
}

impl FreeOpticalGuiApp {
    pub fn nouveau(systeme: OpticalSystem) -> Self {
        let paraxial = calculer_paraxial(&systeme);
        let spot = calculer_spot_diagram(&systeme, (0.0, 0.0), 4);
        let lsa = calculer_aberration_spherique_longitudinale(&systeme, 16);
        let statut_message = format!(
            "PRÊT — Système : {} | EFFL = {:.2} mm | Spot RMS = {:.2} µm",
            systeme.nom, paraxial.focale_effl_mm, spot.rayon_rms_um
        );

        Self {
            systeme,
            paraxial,
            spot,
            lsa,
            ligne_selectionnee: 0,
            vue_active: ActiveView::Coupe2D,
            yaw_3d_deg: 32.0,
            pitch_3d_deg: 20.0,
            zoom: 1.0,
            pan_offset: Vec2::ZERO,
            nb_rayons_2d: 11,
            statut_message,
            fenetre_maj: false,
            etat_maj: EtatMaj::Recherche,
            recepteur_maj: None,
            fenetre_import: false,
            dossier_import: std::env::current_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
            bilan_import: None,
        }
    }

    pub fn recalculer(&mut self) {
        self.paraxial = calculer_paraxial(&self.systeme);
        self.spot = calculer_spot_diagram(&self.systeme, (0.0, 0.0), 4);
        self.lsa = calculer_aberration_spherique_longitudinale(&self.systeme, 16);
        if self.ligne_selectionnee >= self.systeme.surfaces.len() {
            self.ligne_selectionnee = self.systeme.surfaces.len().saturating_sub(1);
        }
    }

    pub fn charger_systeme(&mut self, sys: OpticalSystem, msg: impl Into<String>) {
        self.systeme = sys;
        self.ligne_selectionnee = 0;
        self.zoom = 1.0;
        self.pan_offset = Vec2::ZERO;
        self.recalculer();
        self.statut_message = msg.into();
    }

    /// Ouvre un fichier système : `.json` FreeOptical, ou `.TDS` / `.RLD` / `.DES` des programmes
    /// TDESIGN, LENSDES et RAYTRACE (Rutten et van Venrooij)
    pub fn ouvrir_fichier(&mut self, chemin: &Path) {
        if est_fichier_teloptik(chemin) {
            match importer_fichier_teloptik(chemin) {
                Ok(import) => {
                    let nom = import.systeme.nom.clone();
                    let msg = format!(
                        "'{}' importé depuis {} ({} avertissement(s)).",
                        nom,
                        chemin.display(),
                        import.avertissements.len()
                    );
                    self.charger_systeme(import.systeme, msg.clone());
                    let focale = self.paraxial.focale_effl_mm;
                    let mut lignes = import.avertissements;
                    lignes.insert(
                        0,
                        format!(
                            "Focale inscrite dans le fichier : {:.2} mm — focale paraxiale recalculée : {:.2} mm.",
                            import.focale_fichier_mm.abs(),
                            focale.abs()
                        ),
                    );
                    self.bilan_import = Some((msg, lignes));
                }
                Err(e) => {
                    self.statut_message = format!("ÉCHEC de l'import : {e}");
                    self.bilan_import = Some((self.statut_message.clone(), Vec::new()));
                }
            }
            return;
        }
        let resultat = std::fs::read_to_string(chemin)
            .map_err(|e| e.to_string())
            .and_then(|c| serde_json::from_str::<OpticalSystem>(&c).map_err(|e| e.to_string()));
        match resultat {
            Ok(sys) if sys.surfaces.len() >= 2 => {
                let msg = format!("Système '{}' chargé depuis {}.", sys.nom, chemin.display());
                self.charger_systeme(sys, msg);
            }
            Ok(_) => {
                self.statut_message = format!(
                    "Fichier {} invalide : il faut au moins une surface et un plan image.",
                    chemin.display()
                )
            }
            Err(e) => self.statut_message = format!("Ouverture de {} impossible : {e}", chemin.display()),
        }
    }

    fn dessiner_fenetre_import(&mut self, ctx: &Context) {
        if !self.fenetre_import {
            return;
        }
        let mut ouverte = true;
        let mut a_ouvrir: Option<PathBuf> = None;
        let mut fermer = false;
        egui::Window::new(
            RichText::new(" Importer TDESIGN / LENSDES / RAYTRACE ").monospace().strong(),
        )
        .open(&mut ouverte)
        .collapsible(false)
        .resizable(true)
        .default_width(560.0)
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(
                "Fichiers .TDS, .RLD et .DES des programmes du livre « Telescope Optics » \
                 (Rutten et van Venrooij). On peut aussi glisser un fichier sur la fenêtre.",
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Dossier :");
                ui.add(egui::TextEdit::singleline(&mut self.dossier_import).desired_width(440.0));
            });
            let dossier = PathBuf::from(self.dossier_import.trim());
            let mut fichiers: Vec<PathBuf> = std::fs::read_dir(&dossier)
                .map(|it| {
                    it.filter_map(|e| e.ok().map(|e| e.path()))
                        .filter(|p| est_fichier_teloptik(p))
                        .collect()
                })
                .unwrap_or_default();
            fichiers.sort_by_key(|p| p.file_name().map(|n| n.to_ascii_uppercase()));
            ui.add_space(4.0);
            if fichiers.is_empty() {
                ui.label(
                    RichText::new("Aucun fichier .TDS / .RLD / .DES dans ce dossier.").color(COL_DOS_ORANGE),
                );
            } else {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for f in &fichiers {
                            let nom = f.file_name().unwrap_or_default().to_string_lossy();
                            if ui.button(RichText::new(nom).monospace()).clicked() {
                                a_ouvrir = Some(f.clone());
                            }
                        }
                    });
                });
            }
            if let Some((msg, lignes)) = &self.bilan_import {
                ui.separator();
                let couleur = if msg.starts_with("ÉCHEC") { COL_DOS_RED } else { COL_DOS_GREEN };
                ui.label(RichText::new(msg).color(couleur));
                egui::ScrollArea::vertical()
                    .id_salt("avertissements_import")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for l in lignes {
                            ui.label(RichText::new(format!("• {l}")).color(COL_DOS_YELLOW));
                        }
                    });
            }
            ui.add_space(6.0);
            if ui.button("Fermer").clicked() {
                fermer = true;
            }
        });
        if let Some(f) = a_ouvrir {
            self.ouvrir_fichier(&f);
        }
        if !ouverte || fermer {
            self.fenetre_import = false;
        }
    }

    pub fn optimiser_dls(&mut self) {
        let cible_focale = if self.paraxial.focale_effl_mm.is_finite() {
            self.paraxial.focale_effl_mm
        } else {
            100.0
        };
        let mut vars = Vec::new();
        let n = self.systeme.surfaces.len();
        for i in 0..n.saturating_sub(1) {
            if self.systeme.surfaces[i].type_surface != SurfaceType::Plane {
                vars.push(OptimizationVariable {
                    index_surface: i,
                    parametre: "rayon".to_string(),
                });
            }
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
                poids: 2.5,
            },
        ];
        let rapport = optimiser_systeme_dls(&mut self.systeme, &vars, &cibles, 18);
        self.recalculer();
        self.statut_message = rapport.resume;
    }

    pub fn mise_au_point_paraxiale(&mut self) {
        let bfl = self.paraxial.tirage_bfl_mm;
        let n = self.systeme.surfaces.len();
        // Le tirage doit être compté dans le sens de propagation (négatif après un nombre impair de miroirs)
        if bfl.is_finite() && bfl * self.systeme.sens_propagation_image() > 0.1 && n >= 2 {
            self.systeme.surfaces[n - 2].epaisseur = bfl;
            self.recalculer();
            self.statut_message = format!(
                "Mise au point paraxiale BFL effectuée : Tirage = {:.3} mm | Spot RMS = {:.2} µm",
                bfl, self.spot.rayon_rms_um
            );
        } else {
            self.statut_message =
                "Mise au point impossible : image paraxiale virtuelle ou à l'infini.".to_string();
        }
    }

    /// Supprime la surface sélectionnée, sauf le plan image (dernière surface)
    pub fn supprimer_surface_selectionnee(&mut self) {
        let n = self.systeme.surfaces.len();
        if n <= 2 || self.ligne_selectionnee + 1 >= n {
            self.statut_message =
                "IMPOSSIBLE : le plan image et la dernière surface optique ne peuvent pas être supprimés."
                    .to_string();
            return;
        }
        self.systeme.surfaces.remove(self.ligne_selectionnee);
        self.recalculer();
        self.statut_message = "Surface supprimée.".to_string();
    }

    /// Exécute une étape de mise à jour hors du fil de l'interface (réseau, pkexec)
    fn lancer_tache_maj(
        &mut self,
        ctx: &Context,
        etat: EtatMaj,
        tache: impl FnOnce() -> EtatMaj + Send + 'static,
    ) {
        let (emetteur, recepteur) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = emetteur.send(tache());
            ctx.request_repaint();
        });
        self.etat_maj = etat;
        self.recepteur_maj = Some(recepteur);
        self.fenetre_maj = true;
    }

    pub fn rechercher_mise_a_jour(&mut self, ctx: &Context) {
        if self.recepteur_maj.is_some() {
            self.fenetre_maj = true;
            return;
        }
        self.lancer_tache_maj(ctx, EtatMaj::Recherche, || match maj::rechercher() {
            Ok(Some(v)) => EtatMaj::Disponible(v),
            Ok(None) => EtatMaj::AJour,
            Err(e) => EtatMaj::Erreur(e),
        });
    }

    fn installer_mise_a_jour(&mut self, ctx: &Context, v: VersionDisponible) {
        let Some(deb) = v.deb else { return };
        let version = v.version;
        self.lancer_tache_maj(ctx, EtatMaj::Installation(version.clone()), move || {
            match maj::telecharger(&deb).and_then(|chemin| maj::installer(&chemin)) {
                Ok(()) => EtatMaj::Installee(version),
                Err(e) => EtatMaj::Erreur(e),
            }
        });
    }

    fn dessiner_fenetre_maj(&mut self, ctx: &Context) {
        if let Some(recepteur) = &self.recepteur_maj {
            match recepteur.try_recv() {
                Ok(etat) => {
                    self.statut_message = match &etat {
                        EtatMaj::AJour => format!("FreeOptical v{} est à jour.", maj::VERSION_COURANTE),
                        EtatMaj::Disponible(v) => format!("Mise à jour disponible : v{}", v.version),
                        EtatMaj::Installee(v) => {
                            format!("FreeOptical v{v} installé : redémarrer pour l'utiliser.")
                        }
                        EtatMaj::Erreur(e) => format!("Mise à jour : {e}"),
                        _ => self.statut_message.clone(),
                    };
                    self.etat_maj = etat;
                    self.recepteur_maj = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.etat_maj = EtatMaj::Erreur("La tâche de mise à jour s'est interrompue.".into());
                    self.recepteur_maj = None;
                }
            }
        }
        if !self.fenetre_maj {
            return;
        }

        let mut ouverte = true;
        let mut action: Option<&str> = None;
        egui::Window::new(RichText::new(" Mise à jour de FreeOptical ").monospace().strong())
            .open(&mut ouverte)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(380.0);
                ui.label(
                    RichText::new(format!("Version installée : v{}", maj::VERSION_COURANTE))
                        .monospace(),
                );
                ui.add_space(6.0);
                match &self.etat_maj {
                    EtatMaj::Recherche => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Recherche de la dernière version sur GitHub…");
                        });
                    }
                    EtatMaj::AJour => {
                        ui.label(RichText::new("FreeOptical est à jour.").color(COL_DOS_GREEN));
                        ui.add_space(6.0);
                        if ui.button("Fermer").clicked() {
                            action = Some("fermer");
                        }
                    }
                    EtatMaj::Disponible(v) => {
                        ui.label(
                            RichText::new(format!("Nouvelle version disponible : v{}", v.version))
                                .strong()
                                .color(COL_DOS_YELLOW),
                        );
                        ui.add_space(6.0);
                        let automatique = v.deb.is_some() && maj::installation_automatique_possible();
                        if !automatique {
                            ui.label("Installation automatique impossible ici : téléchargez le paquet depuis la page de la version.");
                        } else {
                            ui.label("Le mot de passe administrateur sera demandé pour installer le paquet.");
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if automatique && ui.button("Installer").clicked() {
                                action = Some("installer");
                            }
                            if ui.button("Voir les nouveautés").clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(&v.page));
                            }
                            if ui.button("Plus tard").clicked() {
                                action = Some("fermer");
                            }
                        });
                    }
                    EtatMaj::Installation(v) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(format!("Téléchargement et installation de la v{v}…"));
                        });
                        ui.label("Saisissez le mot de passe administrateur dans la fenêtre qui s'ouvre.");
                    }
                    EtatMaj::Installee(v) => {
                        ui.label(
                            RichText::new(format!("FreeOptical v{v} est installé."))
                                .strong()
                                .color(COL_DOS_GREEN),
                        );
                        ui.label("Pensez à sauvegarder votre système avant de redémarrer.");
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Redémarrer FreeOptical").clicked() {
                                action = Some("relancer");
                            }
                            if ui.button("Plus tard").clicked() {
                                action = Some("fermer");
                            }
                        });
                    }
                    EtatMaj::Erreur(e) => {
                        ui.label(RichText::new(e).color(COL_DOS_RED));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button("Réessayer").clicked() {
                                action = Some("rechercher");
                            }
                            if ui.button("Page des versions").clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(maj::PAGE_RELEASES));
                            }
                            if ui.button("Fermer").clicked() {
                                action = Some("fermer");
                            }
                        });
                    }
                }
            });

        // Pendant le téléchargement et l'installation, la fenêtre reste ouverte
        if !ouverte && !matches!(self.etat_maj, EtatMaj::Installation(_)) {
            self.fenetre_maj = false;
        }
        match action {
            Some("fermer") => self.fenetre_maj = false,
            Some("rechercher") => self.rechercher_mise_a_jour(ctx),
            Some("installer") => {
                if let EtatMaj::Disponible(v) = self.etat_maj.clone() {
                    self.installer_mise_a_jour(ctx, v);
                }
            }
            Some("relancer") => match maj::relancer() {
                Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => self.etat_maj = EtatMaj::Erreur(e),
            },
            _ => {}
        }
    }
}

impl eframe::App for FreeOpticalGuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Raccourcis clavier globaux
        ctx.input(|i| {
            if i.key_pressed(Key::F1) {
                self.vue_active = ActiveView::Coupe2D;
            } else if i.key_pressed(Key::F2) {
                self.vue_active = ActiveView::Filaire3D;
            } else if i.key_pressed(Key::F3) {
                self.vue_active = ActiveView::SpotDiagram;
            } else if i.key_pressed(Key::F4) {
                self.vue_active = ActiveView::AberrationSpherique;
            } else if i.key_pressed(Key::F5) {
                self.vue_active = ActiveView::CatalogueVerres;
            }
        });

        // 1. Barre de menu supérieure style MS-DOS
        egui::Panel::top("barre_menu_dos")
            .frame(
                EguiFrame::new()
                    .fill(COL_DOS_BAR)
                    .inner_margin(Margin::symmetric(8, 4)),
            )
            .show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(Color32::BLACK);
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.label(
                        RichText::new(format!(" ◄ FREEOPTICAL v{} ► ", maj::VERSION_COURANTE))
                            .monospace()
                            .strong()
                            .color(COL_DOS_WHITE)
                            .background_color(Color32::from_rgb(0, 110, 120)),
                    );
                    ui.add_space(8.0);

                    ui.menu_button(RichText::new(" Fichier ").monospace().strong(), |ui| {
                        ui.visuals_mut().override_text_color = Some(COL_DOS_WHITE);
                        if ui.button("1. Nouveau système vierge").clicked() {
                            self.charger_systeme(
                                OpticalSystem::nouveau("Nouveau Système Optique", LAMBDA_D_NM),
                                "Nouveau système optique initialisé.",
                            );
                            ui.close();
                        }
                        if ui
                            .button("2. Charger Doublet Fraunhofer 100mm (BK7/F2)")
                            .clicked()
                        {
                            self.charger_systeme(
                                OpticalSystem::doublet_achromatique_defaut(),
                                "Doublet Achromatique Fraunhofer 100mm chargé.",
                            );
                            ui.close();
                        }
                        if ui
                            .button("3. Charger Interféromètre de Bath (Laser He-Ne 632.8 nm)")
                            .clicked()
                        {
                            self.charger_systeme(
                                creer_interferometre_bath(),
                                "Interféromètre de Bath (632.8 nm) avec Cube Séparateur chargé.",
                            );
                            ui.close();
                        }
                        if ui
                            .button("4. Charger Singlet Asphérique F/2 (LAK9)")
                            .clicked()
                        {
                            self.charger_systeme(
                                creer_singlet_aspherique(),
                                "Singlet Asphérique F/2 (LAK9) chargé.",
                            );
                            ui.close();
                        }
                        if ui
                            .button("5. Charger Télescope Cassegrain (Miroirs)")
                            .clicked()
                        {
                            self.charger_systeme(
                                creer_telescope_cassegrain(),
                                "Télescope Cassegrain à miroirs chargé.",
                            );
                            ui.close();
                        }
                        if ui
                            .button("6. Importer TDESIGN / LENSDES / RAYTRACE (.TDS, .RLD, .DES)…")
                            .clicked()
                        {
                            self.bilan_import = None;
                            self.fenetre_import = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .button("7. Sauvegarder dans systeme_optique.json")
                            .clicked()
                        {
                            self.statut_message = sauvegarder_systeme(&self.systeme);
                            ui.close();
                        }
                        if ui
                            .button("8. Recharger systeme_optique.json")
                            .clicked()
                        {
                            match charger_systeme_fichier() {
                                Ok((sys, msg)) => self.charger_systeme(sys, msg),
                                Err(msg) => self.statut_message = msg,
                            }
                            ui.close();
                        }
                        if ui.button("9. Quitter").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button(RichText::new(" Édition ").monospace().strong(), |ui| {
                        ui.visuals_mut().override_text_color = Some(COL_DOS_WHITE);
                        if ui.button("Insérer une surface avant la sélection").clicked() {
                            let mut s =
                                Surface::nouvelle(SurfaceType::Spherique, 100.0, 5.0, "BK7", 15.0);
                            s.commentaire = "Nouvelle Surface".to_string();
                            let idx = self.ligne_selectionnee.min(self.systeme.surfaces.len());
                            self.systeme.surfaces.insert(idx, s);
                            self.recalculer();
                            ui.close();
                        }
                        if ui
                            .button("Ajouter une surface avant le Plan Image")
                            .clicked()
                        {
                            let mut s =
                                Surface::nouvelle(SurfaceType::Spherique, 100.0, 10.0, "AIR", 15.0);
                            s.commentaire = "Dioptre Ajouté".to_string();
                            let idx = self.systeme.surfaces.len().saturating_sub(1);
                            self.systeme.surfaces.insert(idx, s);
                            self.ligne_selectionnee = idx;
                            self.recalculer();
                            ui.close();
                        }
                        if ui.button("Supprimer la surface sélectionnée").clicked() {
                            self.supprimer_surface_selectionnee();
                            ui.close();
                        }
                        if ui.button("Définir comme Diaphragme (STOP)").clicked() {
                            for (i, s) in self.systeme.surfaces.iter_mut().enumerate() {
                                s.est_diaphragme = i == self.ligne_selectionnee;
                            }
                            self.recalculer();
                            ui.close();
                        }
                    });

                    ui.menu_button(RichText::new(" Analyse ").monospace().strong(), |ui| {
                        ui.visuals_mut().override_text_color = Some(COL_DOS_WHITE);
                        if ui.button("F1 : Coupe Transversale 2D Vectorielle").clicked() {
                            self.vue_active = ActiveView::Coupe2D;
                            ui.close();
                        }
                        if ui
                            .button("F2 : Projection 3D Isométrique Filaire & Solide")
                            .clicked()
                        {
                            self.vue_active = ActiveView::Filaire3D;
                            ui.close();
                        }
                        if ui
                            .button("F3 : Spot Diagram Polychromatique & Airy")
                            .clicked()
                        {
                            self.vue_active = ActiveView::SpotDiagram;
                            ui.close();
                        }
                        if ui
                            .button("F4 : Aberration Sphérique Longitudinale (LSA)")
                            .clicked()
                        {
                            self.vue_active = ActiveView::AberrationSpherique;
                            ui.close();
                        }
                        if ui.button("F5 : Catalogue de Verres Optiques").clicked() {
                            self.vue_active = ActiveView::CatalogueVerres;
                            ui.close();
                        }
                    });

                    ui.menu_button(RichText::new(" Outils ").monospace().strong(), |ui| {
                        ui.visuals_mut().override_text_color = Some(COL_DOS_WHITE);
                        if ui
                            .button("Optimiser le système (Damped Least Squares - DLS)")
                            .clicked()
                        {
                            self.optimiser_dls();
                            ui.close();
                        }
                        if ui
                            .button("Mise au point paraxiale automatique (Tirage BFL)")
                            .clicked()
                        {
                            self.mise_au_point_paraxiale();
                            ui.close();
                        }
                        ui.separator();
                        if ui.button("Rechercher une mise à jour…").clicked() {
                            self.rechercher_mise_a_jour(&ctx);
                            ui.close();
                        }
                    });

                    ui.separator();
                    // Boutons d'accès direct aux vues et préréglages dans la barre supérieure
                    for (vue, label) in [
                        (ActiveView::Coupe2D, " [F1] Coupe 2D "),
                        (ActiveView::Filaire3D, " [F2] Vue 3D "),
                        (ActiveView::SpotDiagram, " [F3] Spot Diagram "),
                        (ActiveView::AberrationSpherique, " [F4] Aberr. Sphérique "),
                        (ActiveView::CatalogueVerres, " [F5] Verres "),
                    ] {
                        let actif = self.vue_active == vue;
                        let btn = egui::Button::new(
                            RichText::new(label)
                                .monospace()
                                .strong()
                                .color(if actif { COL_DOS_YELLOW } else { Color32::BLACK }),
                        )
                        .fill(if actif {
                            Color32::from_rgb(0, 70, 150)
                        } else {
                            Color32::from_rgb(170, 175, 185)
                        });
                        if ui.add(btn).clicked() {
                            self.vue_active = vue;
                        }
                    }

                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(" Optimiser DLS ")
                                    .monospace()
                                    .strong()
                                    .color(COL_DOS_GREEN),
                            )
                            .fill(Color32::from_rgb(0, 80, 110)),
                        )
                        .clicked()
                    {
                        self.optimiser_dls();
                    }
                });
            });

        // 2. Barre de statut inférieure style MS-DOS
        egui::Panel::bottom("barre_statut_dos")
            .frame(
                EguiFrame::new()
                    .fill(COL_DOS_BAR)
                    .inner_margin(Margin::symmetric(8, 3)),
            )
            .show(ui, |ui| {
                // Bouton de mise à jour calé à droite, toujours visible quelle que soit la largeur
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let maj_disponible =
                        matches!(self.etat_maj, EtatMaj::Disponible(_) | EtatMaj::Installee(_));
                    let libelle = match &self.etat_maj {
                        EtatMaj::Disponible(v) => format!(" ⬆ Mise à jour v{} ", v.version),
                        _ => " Mise à jour ".to_string(),
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(libelle).monospace().strong().color(
                                if maj_disponible { COL_DOS_YELLOW } else { COL_DOS_CYAN },
                            ))
                            .fill(if maj_disponible {
                                Color32::from_rgb(130, 60, 0)
                            } else {
                                Color32::from_rgb(0, 80, 110)
                            }),
                        )
                        .on_hover_text("Rechercher et installer la dernière version publiée sur GitHub")
                        .clicked()
                    {
                        self.rechercher_mise_a_jour(&ctx);
                    }
                    ui.separator();
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(
                                " Souris: Glisser=Pivoter/Déplacer | Molette=Zoom | Clic LDE=Éditer ",
                            )
                            .monospace()
                            .strong()
                            .color(Color32::BLACK),
                        );
                        ui.separator();
                        ui.add(
                            egui::Label::new(
                                RichText::new(&self.statut_message)
                                    .monospace()
                                    .strong()
                                    .color(Color32::from_rgb(0, 30, 120)),
                            )
                            .truncate(),
                        );
                    });
                });
            });

        // 3. Panneau supérieur : Éditeur de Données de Lentilles (LDE) + Bilan Paraxial
        egui::Panel::top("panneau_lde_haut")
            .resizable(true)
            .default_size(315.0)
            .min_size(175.0)
            .frame(
                EguiFrame::new()
                    .fill(COL_DOS_BG)
                    .inner_margin(Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                // Panneaux imbriqués plutôt que ui.columns : egui laisse un contenu trop large
                // déborder de sa colonne, et les deux cadres se chevauchaient
                egui::Panel::right("panneau_bilan")
                    .resizable(true)
                    .default_size(440.0)
                    .min_size(320.0)
                    .frame(EguiFrame::new().fill(COL_DOS_BG).inner_margin(Margin {
                        left: 8,
                        ..Default::default()
                    }))
                    .show(ui, |ui| self.dessiner_bilan_paraxial(ui));
                egui::CentralPanel::default()
                    .frame(EguiFrame::new().fill(COL_DOS_BG))
                    .show(ui, |ui| self.dessiner_lde_interactif(ui));
            });

        // 4. Zone centrale : Canevas Graphique Vectoriel Haute Résolution 2D / 3D / Analyses
        egui::CentralPanel::default()
            .frame(
                EguiFrame::new()
                    .fill(COL_DOS_BG)
                    .inner_margin(Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| {
                cadre_dos(ui, self.vue_active.titre(), |ui| match self.vue_active {
                    ActiveView::Coupe2D => self.dessiner_canevas_2d(ui),
                    ActiveView::Filaire3D => self.dessiner_canevas_3d(ui),
                    ActiveView::SpotDiagram => self.dessiner_canevas_spot(ui),
                    ActiveView::AberrationSpherique => self.dessiner_canevas_lsa(ui),
                    ActiveView::CatalogueVerres => self.dessiner_table_verres(ui),
                });
            });

        self.dessiner_fenetre_maj(&ctx);
        self.dessiner_fenetre_import(&ctx);

        // Glisser-déposer d'un fichier système (.json, .TDS, .RLD, .DES) sur la fenêtre
        let depose = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| {
            let p = f.path();
            (!p.as_os_str().is_empty()).then(|| p.to_path_buf())
        }));
        if let Some(chemin) = depose {
            self.ouvrir_fichier(&chemin);
            if est_fichier_teloptik(&chemin) {
                self.fenetre_import = true;
            }
        }
    }
}

/// Encadre un composant dans un panneau à double bordure jaune/blanche style MS-DOS
fn cadre_dos(ui: &mut egui::Ui, titre: &str, contenu: impl FnOnce(&mut egui::Ui)) {
    EguiFrame::new()
        .fill(COL_DOS_PANEL)
        .stroke(Stroke::new(2.0, COL_DOS_YELLOW))
        .corner_radius(CornerRadius::ZERO)
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(ui.available_height());
            ui.add(
                egui::Label::new(
                    RichText::new(format!("╔══ {} ══╗", titre))
                        .monospace()
                        .strong()
                        .color(COL_DOS_YELLOW),
                )
                .truncate(),
            );
            ui.separator();
            contenu(ui);
        });
}

impl FreeOpticalGuiApp {
    fn dessiner_lde_interactif(&mut self, ui: &mut egui::Ui) {
        let mut modifie = false;
        let titre = format!(
            "ÉDITEUR DE DONNÉES DE LENTILLES (LDE) — {}",
            self.systeme.nom
        );

        cadre_dos(ui, &titre, |ui| {
            ui.horizontal(|ui| {
                if ui.button("+ Insérer Surface").clicked() {
                    let mut s =
                        Surface::nouvelle(SurfaceType::Spherique, 100.0, 5.0, "BK7", 15.0);
                    s.commentaire = "Nouvelle Surface".to_string();
                    let idx = self.ligne_selectionnee.min(self.systeme.surfaces.len());
                    self.systeme.surfaces.insert(idx, s);
                    modifie = true;
                }
                if ui.button("- Supprimer Surface").clicked() {
                    self.supprimer_surface_selectionnee();
                }
                if ui.button("Définir STOP").clicked() {
                    for (i, s) in self.systeme.surfaces.iter_mut().enumerate() {
                        s.est_diaphragme = i == self.ligne_selectionnee;
                    }
                    modifie = true;
                }
            });
            ui.add_space(4.0);

            // Coefficients asphériques pairs de la surface sélectionnée
            let idx_sel = self
                .ligne_selectionnee
                .min(self.systeme.surfaces.len().saturating_sub(1));
            if let Some(surf) = self.systeme.surfaces.get_mut(idx_sel) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("Asphérique surf. #{:02} :", idx_sel + 1))
                            .monospace()
                            .strong()
                            .color(COL_DOS_YELLOW),
                    );
                    let sd = surf.semi_diametre.max(0.1);
                    for (i, nom) in ["A4", "A6", "A8", "A10"].into_iter().enumerate() {
                        let puissance = (2 * i + 4) as i32;
                        // On édite la contribution du terme à la flèche au bord (mm) : les coefficients
                        // (1e-7 à 1e-18) seraient sinon arrondis à 15 décimales par le glisser d'egui
                        let echelle = sd.powi(puissance);
                        let mut fleche_bord = surf.aspheriques[i] * echelle;
                        ui.label(RichText::new(nom).monospace().color(COL_DOS_CYAN));
                        let reponse = ui
                            .add(
                                egui::DragValue::new(&mut fleche_bord)
                                    .speed(1e-4)
                                    .custom_formatter(move |v, _| format!("{:+.4e}", v / echelle))
                                    .custom_parser(move |t| {
                                        t.trim()
                                            .replace(',', ".")
                                            .parse::<f64>()
                                            .ok()
                                            .map(|a| a * echelle)
                                    }),
                            )
                            .on_hover_text(format!(
                                "Coefficient {} (terme en r^{}) en mm^-{}.\nGlisser : 0,1 µm de flèche au bord par pixel.\nCliquer pour saisir (ex. 2.1e-7).\nFlèche ajoutée au bord : {:+.4} µm",
                                nom,
                                puissance,
                                puissance - 1,
                                fleche_bord * 1000.0
                            ));
                        if reponse.changed() {
                            surf.aspheriques[i] = fleche_bord / echelle;
                            // Les coefficients ne sont pris en compte que par le type ASPHERIQUE
                            if surf.type_surface != SurfaceType::Aspherique {
                                surf.type_surface = SurfaceType::Aspherique;
                            }
                            modifie = true;
                        }
                    }
                    if surf.type_surface != SurfaceType::Aspherique {
                        ui.label(
                            RichText::new("(inactifs : modifier un coefficient passe en ASPHERIQUE)")
                                .monospace()
                                .color(COL_DOS_GRAY),
                        );
                    }
                });
                ui.add_space(4.0);
            }

            egui::ScrollArea::both()
                .show(ui, |ui| {
                    egui::Grid::new("grille_lde_dos")
                        .striped(true)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            // En-têtes
                            for h in [
                                "Surf#",
                                "Type",
                                "Commentaire",
                                "Rayon (mm)",
                                "Épaisseur",
                                "Verre",
                                "Demi-Diam.",
                                "Conique k",
                            ] {
                                ui.label(
                                    RichText::new(h)
                                        .monospace()
                                        .strong()
                                        .color(COL_DOS_YELLOW),
                                );
                            }
                            ui.end_row();

                            let nb = self.systeme.surfaces.len();
                            let noms_verres: Vec<String> = self
                                .systeme
                                .catalogue
                                .lister()
                                .into_iter()
                                .map(|m| m.nom.clone())
                                .collect();
                            for idx in 0..nb {
                                let est_sel = idx == self.ligne_selectionnee;
                                let tag = if idx + 1 == nb {
                                    format!("#{:02} IMG", idx + 1)
                                } else if self.systeme.surfaces[idx].est_diaphragme {
                                    format!("#{:02} STP", idx + 1)
                                } else {
                                    format!("#{:02}    ", idx + 1)
                                };

                                if ui
                                    .selectable_label(
                                        est_sel,
                                        RichText::new(tag).monospace().strong().color(if est_sel {
                                            COL_DOS_YELLOW
                                        } else {
                                            COL_DOS_CYAN
                                        }),
                                    )
                                    .clicked()
                                {
                                    self.ligne_selectionnee = idx;
                                }

                                let surf = &mut self.systeme.surfaces[idx];

                                // Sélecteur de type de surface
                                egui::ComboBox::from_id_salt(format!("type_surf_{}", idx))
                                    .selected_text(
                                        RichText::new(surf.type_surface.libelle_court())
                                            .monospace(),
                                    )
                                    .width(95.0)
                                    .show_ui(ui, |ui| {
                                        for t in [
                                            SurfaceType::Plane,
                                            SurfaceType::Spherique,
                                            SurfaceType::Conique,
                                            SurfaceType::Aspherique,
                                        ] {
                                            if ui
                                                .selectable_value(
                                                    &mut surf.type_surface,
                                                    t,
                                                    t.libelle_court(),
                                                )
                                                .changed()
                                            {
                                                if t == SurfaceType::Plane {
                                                    surf.rayon = f64::INFINITY;
                                                } else if !surf.rayon.is_finite() {
                                                    surf.rayon = 100.0;
                                                }
                                                modifie = true;
                                            }
                                        }
                                    });

                                // Commentaire
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut surf.commentaire)
                                            .desired_width(155.0)
                                            .font(FontId::new(13.0, FontFamily::Monospace)),
                                    )
                                    .changed()
                                {
                                    modifie = true;
                                }

                                // Rayon (mm)
                                if surf.type_surface == SurfaceType::Plane
                                    || !surf.rayon.is_finite()
                                {
                                    if ui
                                        .button(
                                            RichText::new("  INFINI  ")
                                                .monospace()
                                                .color(COL_DOS_GRAY),
                                        )
                                        .on_hover_text("Cliquer pour passer en rayon fini (100 mm)")
                                        .clicked()
                                    {
                                        surf.type_surface = SurfaceType::Spherique;
                                        surf.rayon = 100.0;
                                        modifie = true;
                                    }
                                } else if ui
                                    .add(
                                        egui::DragValue::new(&mut surf.rayon)
                                            .speed(0.5)
                                            .max_decimals(3),
                                    )
                                    .changed()
                                {
                                    if surf.rayon.abs() < 0.5 {
                                        surf.rayon = if surf.rayon >= 0.0 { 0.5 } else { -0.5 };
                                    }
                                    modifie = true;
                                }

                                // Épaisseur (mm) : négative après un miroir. La plage doit l'inclure,
                                // car egui ramène la valeur affichée dans la plage (ce qui l'écraserait)
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut surf.epaisseur)
                                            .speed(0.25)
                                            .range(-5000.0..=5000.0)
                                            .max_decimals(3),
                                    )
                                    .changed()
                                {
                                    modifie = true;
                                }

                                // Sélecteur de Verre
                                egui::ComboBox::from_id_salt(format!("verre_surf_{}", idx))
                                    .selected_text(RichText::new(&surf.verre).monospace())
                                    .width(80.0)
                                    .show_ui(ui, |ui| {
                                        for v in &noms_verres {
                                            if ui
                                                .selectable_value(
                                                    &mut surf.verre,
                                                    v.clone(),
                                                    v.as_str(),
                                                )
                                                .changed()
                                            {
                                                modifie = true;
                                            }
                                        }
                                    });

                                // Demi-diamètre (mm)
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut surf.semi_diametre)
                                            .speed(0.2)
                                            .range(0.1..=5000.0)
                                            .max_decimals(2),
                                    )
                                    .changed()
                                {
                                    modifie = true;
                                }

                                // Constante conique k
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut surf.conique)
                                            .speed(0.02)
                                            .range(-100.0..=100.0)
                                            .max_decimals(3),
                                    )
                                    .changed()
                                {
                                    // Une sphère ignore la constante conique : on passe en type CONIQUE
                                    if surf.conique.abs() > 1e-9
                                        && surf.type_surface == SurfaceType::Spherique
                                    {
                                        surf.type_surface = SurfaceType::Conique;
                                    }
                                    modifie = true;
                                }

                                ui.end_row();
                            }
                        });
                });
        });

        if modifie {
            self.recalculer();
        }
    }

    fn dessiner_bilan_paraxial(&mut self, ui: &mut egui::Ui) {
        let p = &self.paraxial;
        let s = &self.spot;
        let lsa = &self.lsa;

        cadre_dos(ui, "BILAN OPTIQUE, PARAXIAL & ABERRATIONS", |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("grille_bilan_paraxial")
                    .spacing([18.0, 2.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Longueur d'onde λ réf :").monospace());
                        ui.label(
                            RichText::new(format!("{:.2} nm", self.systeme.longueur_onde_ref_nm))
                                .monospace()
                                .strong()
                                .color(COL_DOS_YELLOW),
                        );

                        ui.end_row();

                        ui.label(RichText::new("Pupille d'Entrée (ENPD):").monospace());
                        ui.label(
                            RichText::new(format!("Ø {:.2} mm", p.diametre_pupille_entree_mm))
                                .monospace()
                                .color(COL_DOS_WHITE),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Focale Effective (EFFL):").monospace());
                        ui.label(
                            RichText::new(if p.focale_effl_mm.is_finite() {
                                format!("{:.3} mm", p.focale_effl_mm)
                            } else {
                                "INFINIE (Afocal)".to_string()
                            })
                            .monospace()
                            .strong()
                            .color(COL_DOS_CYAN),
                        );

                        ui.end_row();

                        ui.label(RichText::new("Pupille de Sortie (EXPD):").monospace());
                        ui.label(
                            RichText::new(format!("Ø {:.2} mm", p.diametre_pupille_sortie_mm))
                                .monospace()
                                .color(COL_DOS_WHITE),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Tirage Paraxial (BFL)  :").monospace());
                        ui.label(
                            RichText::new(if p.tirage_bfl_mm.is_finite() {
                                format!("{:.3} mm", p.tirage_bfl_mm)
                            } else {
                                "INFINI".to_string()
                            })
                            .monospace()
                            .color(COL_DOS_WHITE),
                        );

                        ui.end_row();

                        ui.label(RichText::new("Ouverture F/# & NA     :").monospace());
                        ui.label(
                            RichText::new(if p.focale_effl_mm.is_finite() {
                                format!(
                                    "F/{:.2}  (NA = {:.3})",
                                    p.nombre_ouverture_fnum, p.ouverture_numerique_image
                                )
                            } else {
                                "— (afocal)".to_string()
                            })
                            .monospace()
                            .color(COL_DOS_WHITE),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Rayon Spot RMS (Image) :").monospace());
                        ui.label(
                            RichText::new(format!("{:.2} µm", s.rayon_rms_um))
                                .monospace()
                                .strong()
                                .color(COL_DOS_GREEN),
                        );

                        ui.end_row();

                        ui.label(RichText::new("Rayon Disque d'Airy    :").monospace());
                        ui.label(
                            RichText::new(if s.rayon_airy_um > 0.0 {
                                format!("{:.2} µm", s.rayon_airy_um)
                            } else {
                                "—".to_string()
                            })
                                .monospace()
                                .strong()
                                .color(COL_DOS_YELLOW),
                        );
                        ui.end_row();

                        ui.label(RichText::new("Aberr. Sphérique (LSA) :").monospace());
                        ui.label(
                            RichText::new(format!("ΔZ = {:.4} mm", lsa.lsa_max_mm))
                                .monospace()
                                .color(COL_DOS_WHITE),
                        );

                        ui.end_row();

                        ui.label(RichText::new("Longueur Totale Système:").monospace());
                        ui.label(
                            RichText::new(format!("{:.2} mm", p.longueur_totale_mm))
                                .monospace()
                                .color(COL_DOS_WHITE),
                        );
                        ui.end_row();
                    });

                ui.add_space(6.0);
                let diffraction_ok = s.rayon_rms_um <= s.rayon_airy_um && s.rayon_airy_um > 0.0;
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Diagnostic :").monospace());
                    if !p.focale_effl_mm.is_finite() {
                        ui.label(
                            RichText::new("AFOCAL : ajoutez des surfaces")
                                .monospace()
                                .strong()
                                .color(COL_DOS_CYAN),
                        );
                    } else if diffraction_ok {
                        ui.label(
                            RichText::new("[OK] LIMITÉ PAR LA DIFFRACTION")
                                .monospace()
                                .strong()
                                .color(COL_DOS_GREEN),
                        );
                    } else {
                        ui.label(
                            RichText::new("[!] LIMITÉ PAR LES ABERRATIONS")
                                .monospace()
                                .strong()
                                .color(COL_DOS_YELLOW),
                        );
                    }
                });
            });
        });
    }

    /// Dessine la coupe transversale 2D avec rendu en traits continus (lentilles, cubes séparateurs à 45°, rayons)
    fn dessiner_canevas_2d(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Rayons :").monospace());
            ui.add(egui::Slider::new(&mut self.nb_rayons_2d, 3..=25));
            ui.separator();
            if ui.button("Réinitialiser Vue").clicked() {
                self.zoom = 1.0;
                self.pan_offset = Vec2::ZERO;
            }
            // Légende limitée à ce que le système affiche réellement
            let sys = &self.systeme;
            let mut legende = vec!["Cyan = Champ sur l'axe".to_string()];
            if sys.champs_deg.len() > 1 && sys.champs_deg[1].1.abs() > 0.01 {
                legende.push(format!("Vert = Champ {:.1}°", sys.champs_deg[1].1));
            }
            if sys.surfaces.iter().any(|surf| est_cube_separateur(sys, surf)) {
                legende.push("Orange = Réfléchis 90° (Cube 50/50)".to_string());
                legende.push("Magenta = Lame 45° / Miroir".to_string());
            } else if sys
                .surfaces
                .iter()
                .any(|surf| sys.catalogue.resoudre(&surf.verre).est_miroir())
            {
                legende.push("Magenta = Miroir".to_string());
            }
            ui.label(
                RichText::new(format!("  [{}]", legende.join(" | ")))
                    .monospace()
                    .color(COL_DOS_GRAY),
            );
        });

        let dispo = ui.available_size();
        let (response, painter) = ui.allocate_painter(dispo, Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, COL_DOS_CYAN),
            StrokeKind::Inside,
        );

        // Gestion du déplacement (pan) et du zoom molette
        if response.dragged() {
            self.pan_offset += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.0015)).clamp(0.25, 8.0);
            }
        }

        let sys = &self.systeme;
        let positions_z = sys.positions_z_sommets();
        let (z_min_sys, z_max_sys) = sys.etendue_axiale();
        let max_sd = sys
            .surfaces
            .iter()
            .map(|s| s.semi_diametre)
            .fold(15.0_f64, f64::max);

        // Tient compte des surfaces placées en arrière (après un miroir) et du départ des rayons
        let z_min = z_min_sys - 18.0;
        let z_max = z_max_sys.max(z_min_sys + 20.0) + 12.0;
        let largeur_z = (z_max - z_min).max(10.0);
        let hauteur_y = (max_sd * 3.4).max(10.0);

        let echelle_x = (rect.width() as f64) / largeur_z;
        let echelle_y = (rect.height() as f64) / hauteur_y;
        // Échelle isotrope (respecte les proportions géométriques des cubes et rayons de courbure)
        let echelle = echelle_x.min(echelle_y) * (self.zoom as f64);

        let z_milieu = 0.5 * (z_min + z_max);
        let centre_ecran = rect.center() + self.pan_offset;

        let vers_ecran = |z: f64, y: f64| -> Pos2 {
            Pos2::new(
                centre_ecran.x + ((z - z_milieu) * echelle) as f32,
                centre_ecran.y - (y * echelle) as f32,
            )
        };

        // 1. Grille de fond et axe optique (Y = 0)
        tracer_segment(&painter, vers_ecran(z_min - 50.0, 0.0), vers_ecran(z_max + 50.0, 0.0), 1.08, Color32::from_rgb(100, 125, 180));

        // 2. Remplissage et contours des éléments en verre (Lentilles & Cubes Séparateurs)
        let nb_pts_courbe = 48;
        for (idx, (surf, &z_sommet)) in sys.surfaces.iter().zip(positions_z.iter()).enumerate() {
            let sd = surf.semi_diametre;
            let mat = sys.catalogue.resoudre(&surf.verre);

            // Points du profil de la surface courante (de -sd à +sd)
            let mut pts_surf = Vec::with_capacity(nb_pts_courbe + 1);
            for k in 0..=nb_pts_courbe {
                let y = -sd + 2.0 * sd * (k as f64) / (nb_pts_courbe as f64);
                if let Some(f_z) = surf.fleche(y.abs()) {
                    pts_surf.push(vers_ecran(z_sommet + f_z, y));
                }
            }

            // Si cette surface est suivie d'un milieu en verre, on dessine le corps de la lentille ou du cube
            if idx + 1 < sys.surfaces.len() && est_verre(sys, surf) {
                let surf_suiv = &sys.surfaces[idx + 1];
                let z_suiv = positions_z[idx + 1];
                let sd_suiv = surf_suiv.semi_diametre;

                let mut pts_suiv = Vec::with_capacity(nb_pts_courbe + 1);
                for k in (0..=nb_pts_courbe).rev() {
                    let y = -sd_suiv + 2.0 * sd_suiv * (k as f64) / (nb_pts_courbe as f64);
                    if let Some(f_z) = surf_suiv.fleche(y.abs()) {
                        pts_suiv.push(vers_ecran(z_suiv + f_z, y));
                    }
                }

                let comm_up = surf.commentaire.to_uppercase();
                let est_cube = comm_up.contains("CUBE")
                    || comm_up.contains("SEPARAT")
                    || comm_up.contains("SÉPARAT");

                // Remplissage par un maillage de bandes horizontales : convient aussi aux ménisques
                // et aux lentilles divergentes, sans joint visible entre les bandes
                let couleur_fond = if est_cube {
                    COL_CUBE_FILL
                } else {
                    COL_VERRE_FILL
                };
                let sd_bande = sd.min(sd_suiv);
                let nb_bandes = 48;
                let bord = |s: &Surface, z: f64, y: f64| s.fleche(y.abs()).map(|f| vers_ecran(z + f, y));
                let mut maillage = egui::Mesh::default();
                for k in 0..nb_bandes {
                    let ya = -sd_bande + 2.0 * sd_bande * (k as f64) / (nb_bandes as f64);
                    let yb = -sd_bande + 2.0 * sd_bande * ((k + 1) as f64) / (nb_bandes as f64);
                    if let (Some(a1), Some(b1), Some(b2), Some(a2)) = (
                        bord(surf, z_sommet, ya),
                        bord(surf, z_sommet, yb),
                        bord(surf_suiv, z_suiv, yb),
                        bord(surf_suiv, z_suiv, ya),
                    ) {
                        let i0 = maillage.vertices.len() as u32;
                        for p in [a1, b1, b2, a2] {
                            maillage.colored_vertex(p, couleur_fond);
                        }
                        maillage.add_triangle(i0, i0 + 1, i0 + 2);
                        maillage.add_triangle(i0, i0 + 2, i0 + 3);
                    }
                }
                painter.add(Shape::mesh(maillage));

                // Bords haut et bas
                if let (Some(&haut_g), Some(&bas_g), Some(&haut_d), Some(&bas_d)) = (
                    pts_surf.last(),
                    pts_surf.first(),
                    pts_suiv.first(),
                    pts_suiv.last(),
                ) {
                    tracer_segment(&painter, haut_g, haut_d, 1.44, COL_DOS_WHITE);
                    tracer_segment(&painter, bas_g, bas_d, 1.44, COL_DOS_WHITE);

                    // Si c'est un Cube Séparateur, on trace clairement la lame diagonale semi-réfléchissante à 45° !
                    if est_cube {
                        // Lame de (y = -sd, z min) à (y = +sd, z max), comme pour le calcul des reflets
                        let (lame_bas, lame_haut) = if z_suiv >= z_sommet {
                            (bas_g, haut_d)
                        } else {
                            (bas_d, haut_g)
                        };
                        tracer_segment(&painter, lame_bas, lame_haut, 1.56, COL_DOS_MAGENTA);
                        let centre_cube = Pos2::new(
                            0.5 * (bas_g.x + haut_d.x),
                            bas_g.y + 14.0,
                        );
                        painter.text(
                            centre_cube,
                            Align2::CENTER_TOP,
                            "CUBE SÉPARATEUR 50/50",
                            FontId::new(11.5, FontFamily::Monospace),
                            COL_DOS_YELLOW,
                        );

                        // Plan miroir/détecteur latéral en haut du bras réfléchi à 90°
                        let y_bras = sd * 2.65;
                        let m_g = vers_ecran(z_sommet + 0.1 * (z_suiv - z_sommet), y_bras);
                        let m_d = vers_ecran(z_suiv - 0.1 * (z_suiv - z_sommet), y_bras);
                        tracer_segment(&painter, m_g, m_d, 1.56, COL_DOS_MAGENTA);
                        painter.text(
                            Pos2::new(0.5 * (m_g.x + m_d.x), m_g.y - 5.0),
                            Align2::CENTER_BOTTOM,
                            "FAISCEAU RÉFLÉCHI 90° (50%)",
                            FontId::new(11.0, FontFamily::Monospace),
                            COL_DOS_ORANGE,
                        );
                    } else {
                        let centre_lentille = Pos2::new(
                            0.5 * (haut_g.x + haut_d.x),
                            haut_g.y.min(haut_d.y) - 6.0,
                        );
                        painter.text(
                            centre_lentille,
                            Align2::CENTER_BOTTOM,
                            &surf.verre,
                            FontId::new(11.5, FontFamily::Monospace),
                            COL_DOS_CYAN,
                        );
                    }
                }
            }

            // Tracé du dioptre lui-même
            let couleur_dioptre = if idx == self.ligne_selectionnee {
                COL_DOS_YELLOW
            } else if mat.est_miroir() {
                COL_DOS_MAGENTA
            } else {
                COL_DOS_WHITE
            };
            let epaisseur = if idx == self.ligne_selectionnee {
                1.8
            } else {
                1.45
            };
            tracer_polyligne(&painter, &pts_surf, epaisseur, couleur_dioptre);
        }

        // 3. Tracé des éventails de rayons lumineux (Champ hors-axe + Champ sur l'axe)
        if sys.champs_deg.len() > 1 && sys.champs_deg[1].1.abs() > 0.01 {
            let rayons_champ = tracer_eventail_meridional(
                sys,
                sys.champs_deg[1].1,
                7,
                sys.longueur_onde_ref_nm,
            );
            for tr in &rayons_champ {
                let mut pts = Vec::with_capacity(tr.impacts.len() + 1);
                pts.push(vers_ecran(
                    tr.rayon_initial.origine.z,
                    tr.rayon_initial.origine.y,
                ));
                for hit in &tr.impacts {
                    pts.push(vers_ecran(hit.point_global.z, hit.point_global.y));
                }
                tracer_polyligne(&painter, &pts, 1.2, COL_DOS_GREEN);
            }
        }

        let rayons_axe = tracer_eventail_meridional(
            sys,
            0.0,
            self.nb_rayons_2d,
            sys.longueur_onde_ref_nm,
        );
        for tr in &rayons_axe {
            let mut pts = Vec::with_capacity(tr.impacts.len() + 1);
            pts.push(vers_ecran(
                tr.rayon_initial.origine.z,
                tr.rayon_initial.origine.y,
            ));
            for hit in &tr.impacts {
                pts.push(vers_ecran(hit.point_global.z, hit.point_global.y));
            }
            tracer_polyligne(&painter, &pts, 1.32, COL_DOS_CYAN);
        }

        // 4. Tracé des rayons réfléchis à 90° par la lame séparatrice à 45° de chaque Cube Séparateur
        let polylignes_refl = calculer_rayons_reflechis_cubes(sys, &positions_z, &rayons_axe);
        for poly in &polylignes_refl {
            let pts_ecran: Vec<Pos2> = poly.iter().map(|p| vers_ecran(p.z, p.y)).collect();
            if let Some(&p_split_ecran) = pts_ecran.first() {
                painter.circle_filled(p_split_ecran, 2.5, COL_DOS_YELLOW);
            }
            if let Some(&p_ext_ecran) = pts_ecran.last() {
                painter.circle_filled(p_ext_ecran, 2.2, COL_DOS_ORANGE);
            }
            tracer_polyligne(&painter, &pts_ecran, 1.38, COL_DOS_ORANGE);
        }
    }

    /// Dessine la projection 3D isométrique filaire en traits continus (orientable à la souris)
    fn dessiner_canevas_3d(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Lacet (Yaw) :").monospace());
            ui.add(egui::Slider::new(&mut self.yaw_3d_deg, -180.0..=180.0).suffix("°"));
            ui.label(RichText::new("Tangage (Pitch) :").monospace());
            ui.add(egui::Slider::new(&mut self.pitch_3d_deg, -85.0..=85.0).suffix("°"));
            if ui.button("Réinitialiser Angle 3D").clicked() {
                self.yaw_3d_deg = 32.0;
                self.pitch_3d_deg = 20.0;
                self.zoom = 1.0;
            }
            if self.systeme.surfaces.iter().any(|surf| est_cube_separateur(&self.systeme, surf)) {
                ui.label(
                    RichText::new("  (Cyan = Transmis | Orange = Réfléchis 90° Cube 50/50)")
                        .monospace()
                        .color(COL_DOS_YELLOW),
                );
            }
        });

        let dispo = ui.available_size();
        let (response, painter) = ui.allocate_painter(dispo, Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, COL_DOS_GREEN),
            StrokeKind::Inside,
        );

        if response.dragged() {
            let delta = response.drag_delta();
            self.yaw_3d_deg += (delta.x as f64) * 0.45;
            self.pitch_3d_deg = (self.pitch_3d_deg + (delta.y as f64) * 0.45).clamp(-85.0, 85.0);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.1 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.0015)).clamp(0.25, 8.0);
            }
        }

        let sys = &self.systeme;
        let positions_z = sys.positions_z_sommets();
        let (z_min_sys, z_max_sys) = sys.etendue_axiale();
        let longueur = (z_max_sys - z_min_sys).max(20.0);
        let z_centre = 0.5 * (z_min_sys + z_max_sys);
        let max_sd = sys
            .surfaces
            .iter()
            .map(|s| s.semi_diametre)
            .fold(15.0_f64, f64::max);

        let echelle = ((rect.width().min(rect.height()) as f64)
            / (longueur * 1.15).max(max_sd * 3.2))
            * (self.zoom as f64);
        let centre = rect.center();
        let yaw = self.yaw_3d_deg.to_radians();
        let pitch = self.pitch_3d_deg.to_radians();

        let projeter = |p: Point3| -> Pos2 {
            let x = p.x;
            let y = p.y;
            let z = p.z - z_centre;
            let x1 = z * yaw.cos() + x * yaw.sin();
            let z1 = -z * yaw.sin() + x * yaw.cos();
            let u = x1;
            let v = y * pitch.cos() - z1 * pitch.sin();
            Pos2::new(
                centre.x + (u * echelle) as f32,
                centre.y - (v * echelle) as f32,
            )
        };

        // 1. Faisceau 3D hexapolaire (rayons transmis + rayons réfléchis à 90° dans les cubes séparateurs)
        let faisceau = tracer_grille_hexapolaire(sys, 0.0, 0.0, 2, sys.longueur_onde_ref_nm);
        for tr in &faisceau {
            let mut pts = Vec::with_capacity(tr.impacts.len() + 1);
            pts.push(projeter(tr.rayon_initial.origine));
            for hit in &tr.impacts {
                pts.push(projeter(hit.point_global));
            }
            tracer_polyligne(&painter, &pts, 1.2, COL_DOS_CYAN);
        }

        let polylignes_refl_3d = calculer_rayons_reflechis_cubes(sys, &positions_z, &faisceau);
        for poly in &polylignes_refl_3d {
            let pts_3d: Vec<Pos2> = poly.iter().map(|&p| projeter(p)).collect();
            if let Some(&p_split) = pts_3d.first() {
                painter.circle_filled(p_split, 2.2, COL_DOS_YELLOW);
            }
            tracer_polyligne(&painter, &pts_3d, 1.32, COL_DOS_ORANGE);
        }

        // 2. Maillage 3D des surfaces (Cubes Séparateurs carrés + Lentilles circulaires)
        let nb_cercle = 40;
        let nb_meridien = 24;
        for (idx, (surf, &z_sommet)) in sys.surfaces.iter().zip(positions_z.iter()).enumerate() {
            let sd = surf.semi_diametre;
            let mat = sys.catalogue.resoudre(&surf.verre);
            let couleur = if idx == self.ligne_selectionnee {
                COL_DOS_YELLOW
            } else if mat.est_miroir() {
                COL_DOS_MAGENTA
            } else {
                COL_DOS_GREEN
            };

            let comm_up = surf.commentaire.to_uppercase();
            let est_cube = comm_up.contains("CUBE")
                || comm_up.contains("SEPARAT")
                || comm_up.contains("SÉPARAT");

            if est_cube {
                // Face carrée 3D
                let coins = [
                    projeter(Point3::new(-sd, -sd, z_sommet)),
                    projeter(Point3::new(sd, -sd, z_sommet)),
                    projeter(Point3::new(sd, sd, z_sommet)),
                    projeter(Point3::new(-sd, sd, z_sommet)),
                    projeter(Point3::new(-sd, -sd, z_sommet)),
                ];
                tracer_polyligne(&painter, &coins, 1.44, couleur);
            } else {
                // Anneaux concentriques 3D
                for &f_r in &[1.0, 0.55] {
                    let r = sd * f_r;
                    if let Some(z_sag) = surf.fleche(r) {
                        let mut pts = Vec::with_capacity(nb_cercle + 1);
                        for k in 0..=nb_cercle {
                            let th = 2.0 * std::f64::consts::PI * (k as f64) / (nb_cercle as f64);
                            pts.push(projeter(Point3::new(
                                r * th.cos(),
                                r * th.sin(),
                                z_sommet + z_sag,
                            )));
                        }
                        tracer_polyligne(&painter, &pts, 1.32, couleur);
                    }
                }
                // Méridiens XZ et YZ
                for &(ca, sa) in &[(1.0, 0.0), (0.0, 1.0)] {
                    let mut pts = Vec::with_capacity(nb_meridien + 1);
                    for k in 0..=nb_meridien {
                        let r_s = -sd + 2.0 * sd * (k as f64) / (nb_meridien as f64);
                        if let Some(z_sag) = surf.fleche(r_s.abs()) {
                            pts.push(projeter(Point3::new(
                                r_s * ca,
                                r_s * sa,
                                z_sommet + z_sag,
                            )));
                        }
                    }
                    tracer_polyligne(&painter, &pts, 1.2, couleur);
                }
            }

            // Liaison longitudinale entre l'entrée et la sortie d'un composant en verre
            if idx + 1 < sys.surfaces.len() && est_verre(sys, surf) {
                let surf_suiv = &sys.surfaces[idx + 1];
                let z_suiv = positions_z[idx + 1];
                let sd_suiv = surf_suiv.semi_diametre;
                if est_cube {
                    for &(sx, sy) in &[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        tracer_segment(&painter, projeter(Point3::new(sx * sd, sy * sd, z_sommet)), projeter(Point3::new(sx * sd_suiv, sy * sd_suiv, z_suiv)), 1.38, COL_DOS_WHITE);
                    }
                    // Plan diagonal séparateur 3D à 45° en magenta
                    let d1 = projeter(Point3::new(-sd, -sd, z_sommet));
                    let d2 = projeter(Point3::new(sd, -sd, z_sommet));
                    let d3 = projeter(Point3::new(sd_suiv, sd_suiv, z_suiv));
                    let d4 = projeter(Point3::new(-sd_suiv, sd_suiv, z_suiv));
                    tracer_polyligne(&painter, &[d1, d2, d3, d4, d1], 1.44, COL_DOS_MAGENTA);
                    // Plan détecteur / miroir latéral 3D sur le bras réfléchi à 90° (y = +2.65 * sd)
                    let y_bras = sd * 2.65;
                    let z_m1 = z_sommet + 0.1 * (z_suiv - z_sommet);
                    let z_m2 = z_suiv - 0.1 * (z_suiv - z_sommet);
                    let x_m = sd * 0.8;
                    let m1 = projeter(Point3::new(-x_m, y_bras, z_m1));
                    let m2 = projeter(Point3::new(x_m, y_bras, z_m1));
                    let m3 = projeter(Point3::new(x_m, y_bras, z_m2));
                    let m4 = projeter(Point3::new(-x_m, y_bras, z_m2));
                    tracer_polyligne(&painter, &[m1, m2, m3, m4, m1], 1.32, COL_DOS_ORANGE);
                } else if let (Some(f1), Some(f2)) = (surf.fleche(sd), surf_suiv.fleche(sd_suiv)) {
                    for k in 0..8 {
                        let th = 2.0 * std::f64::consts::PI * (k as f64) / 8.0;
                        tracer_segment(&painter, projeter(Point3::new(sd * th.cos(), sd * th.sin(), z_sommet + f1)), projeter(Point3::new(
                                sd_suiv * th.cos(),
                                sd_suiv * th.sin(),
                                z_suiv + f2,
                            )), 1.2, COL_DOS_WHITE);
                    }
                }
            }
        }
    }

    /// Dessine le Spot Diagram polychromatique et le disque d'Airy en traits continus terminal
    fn dessiner_canevas_spot(&mut self, ui: &mut egui::Ui) {
        let spot = &self.spot;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Rayons transmis : {}  |  Rayon RMS = {:.2} µm  |  Rayon GEO = {:.2} µm  |  Disque d'Airy (jaune) = {:.2} µm",
                    spot.nb_rayons_valides, spot.rayon_rms_um, spot.rayon_geo_um, spot.rayon_airy_um
                ))
                .monospace()
                .strong()
                .color(COL_DOS_YELLOW),
            );
        });

        let dispo = ui.available_size();
        let (_resp, painter) = ui.allocate_painter(dispo, Sense::hover());
        let rect = _resp.rect;
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);

        let cote = rect.width().min(rect.height()) * 0.88;
        let centre = rect.center();
        let airy_mm = spot.rayon_airy_um * 1e-3;
        let geo_mm = spot.rayon_geo_um * 1e-3;
        let demi_echelle = (geo_mm.max(airy_mm) * 1.35).max(0.002);
        let echelle = (cote as f64 * 0.5) / demi_echelle;

        // Grille et axes centrés sur le centroïde
        tracer_segment(&painter, Pos2::new(centre.x - cote * 0.5, centre.y), Pos2::new(centre.x + cote * 0.5, centre.y), 1.08, Color32::from_rgb(80, 100, 150));
        tracer_segment(&painter, Pos2::new(centre.x, centre.y - cote * 0.5), Pos2::new(centre.x, centre.y + cote * 0.5), 1.08, Color32::from_rgb(80, 100, 150));

        // Cercle de diffraction d'Airy en traits continus
        if airy_mm > 0.0 {
            let r_px = (airy_mm * echelle) as f32;
            let nb_c = 64;
            let mut pts_airy = Vec::with_capacity(nb_c + 1);
            for k in 0..=nb_c {
                let th = 2.0 * std::f32::consts::PI * (k as f32) / (nb_c as f32);
                pts_airy.push(Pos2::new(
                    centre.x + r_px * th.cos(),
                    centre.y + r_px * th.sin(),
                ));
            }
            tracer_polyligne(&painter, &pts_airy, 1.44, COL_DOS_YELLOW);
        }

        // Impacts des rayons (carrés lumineux style terminal)
        let (cx, cy) = spot.centroide_mm;
        for pt in &spot.points {
            let px = centre.x + ((pt.x_mm - cx) * echelle) as f32;
            let py = centre.y - ((pt.y_mm - cy) * echelle) as f32;
            let col = if pt.longueur_onde_nm < 520.0 {
                COL_DOS_CYAN
            } else if pt.longueur_onde_nm <= 610.0 {
                COL_DOS_GREEN
            } else {
                COL_DOS_RED
            };
            painter.circle_filled(Pos2::new(px, py), 2.0, col);
        }
    }

    /// Dessine la courbe d'Aberration Sphérique Longitudinale (LSA) en traits continus
    fn dessiner_canevas_lsa(&mut self, ui: &mut egui::Ui) {
        let lsa = &self.lsa;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(format!(
                    "Aberration Sphérique Longitudinale Crête-à-Crête (LSA) = {:.4} mm   [Axe Horizontal : ΔZ (mm) | Axe Vertical : Hauteur Pupille Py]",
                    lsa.lsa_max_mm
                ))
                .monospace()
                .strong()
                .color(COL_DOS_YELLOW),
            );
        });

        let dispo = ui.available_size();
        let (resp, painter) = ui.allocate_painter(dispo, Sense::hover());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);

        let max_dz = lsa
            .echantillons
            .iter()
            .map(|s| s.delta_z_mm.abs())
            .fold(0.2_f64, f64::max)
            * 1.25;

        let marge = 40.0;
        let z_plot = rect.shrink(marge);
        let vers_pt = |dz: f64, py: f64| -> Pos2 {
            let x = z_plot.center().x + ((dz / max_dz) as f32) * (z_plot.width() * 0.5);
            let y = z_plot.bottom() - (py as f32) * z_plot.height();
            Pos2::new(x, y)
        };

        tracer_segment(&painter, vers_pt(0.0, 0.0), vers_pt(0.0, 1.0), 1.32, COL_DOS_YELLOW);
        tracer_segment(&painter, vers_pt(-max_dz, 0.0), vers_pt(max_dz, 0.0), 1.2, COL_DOS_GRAY);

        for &(l_min, l_max, col) in &[
            (0.0, 520.0, COL_DOS_CYAN),
            (520.0, 610.0, COL_DOS_GREEN),
            (610.0, 2000.0, COL_DOS_RED),
        ] {
            let mut pts: Vec<(f64, f64)> = lsa
                .echantillons
                .iter()
                .filter(|s| s.longueur_onde_nm >= l_min && s.longueur_onde_nm < l_max)
                .map(|s| (s.delta_z_mm, s.pupille_py))
                .collect();
            pts.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            let poly: Vec<Pos2> = pts.into_iter().map(|(dz, py)| vers_pt(dz, py)).collect();
            if poly.len() >= 2 {
                tracer_polyligne(&painter, &poly, 1.56, col);
            }
        }
    }

    fn dessiner_table_verres(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::both().show(ui, |ui| {
            egui::Grid::new("grille_catalogue_verres_gui")
                .striped(true)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for h in [
                        "Matériau",
                        "Indice nd (587.6 nm)",
                        "Nombre d'Abbe Vd",
                        "Indice nF (486.1 nm)",
                        "Indice nC (656.3 nm)",
                    ] {
                        ui.label(
                            RichText::new(h)
                                .monospace()
                                .strong()
                                .color(COL_DOS_YELLOW),
                        );
                    }
                    ui.end_row();

                    for mat in self.systeme.catalogue.lister() {
                        ui.label(
                            RichText::new(&mat.nom)
                                .monospace()
                                .strong()
                                .color(COL_DOS_CYAN),
                        );
                        ui.label(
                            RichText::new(format!("{:.5}", mat.indice_refraction(587.5618)))
                                .monospace(),
                        );
                        let vd_txt = match &mat.kind {
                            crate::core::glass::MediumKind::Verre { vd, .. } => format!("{:.2}", vd),
                            _ => "-".to_string(),
                        };
                        ui.label(RichText::new(vd_txt).monospace().color(COL_DOS_YELLOW));
                        ui.label(
                            RichText::new(format!("{:.5}", mat.indice_refraction(486.1327)))
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(format!("{:.5}", mat.indice_refraction(656.2725)))
                                .monospace(),
                        );
                        ui.end_row();
                    }
                });
        });
    }
}

/// Sauvegarde le système dans `systeme_optique.json` et retourne le message de statut
fn sauvegarder_systeme(sys: &OpticalSystem) -> String {
    let resultat = serde_json::to_string_pretty(sys)
        .map_err(|e| e.to_string())
        .and_then(|json| std::fs::write(FICHIER_SYSTEME, json).map_err(|e| e.to_string()));
    let chemin = std::env::current_dir()
        .map(|d| d.join(FICHIER_SYSTEME).display().to_string())
        .unwrap_or_else(|_| FICHIER_SYSTEME.to_string());
    match resultat {
        Ok(()) => format!("Système sauvegardé dans '{}'.", chemin),
        Err(e) => format!("ÉCHEC de la sauvegarde dans '{}' : {}", chemin, e),
    }
}

/// Relit `systeme_optique.json` depuis le répertoire courant
fn charger_systeme_fichier() -> Result<(OpticalSystem, String), String> {
    let contenu = std::fs::read_to_string(FICHIER_SYSTEME)
        .map_err(|e| format!("Lecture de '{}' impossible : {}", FICHIER_SYSTEME, e))?;
    let sys = serde_json::from_str::<OpticalSystem>(&contenu)
        .map_err(|e| format!("Fichier '{}' invalide : {}", FICHIER_SYSTEME, e))?;
    if sys.surfaces.len() < 2 {
        return Err(format!(
            "Fichier '{}' invalide : il faut au moins une surface et un plan image.",
            FICHIER_SYSTEME
        ));
    }
    let msg = format!("Système '{}' rechargé depuis '{}'.", sys.nom, FICHIER_SYSTEME);
    Ok((sys, msg))
}

#[cfg(test)]
mod tests {
    #[test]
    fn icone_de_fenetre_lisible() {
        let icone = eframe::icon_data::from_png_bytes(include_bytes!(
            "../../assets/icons/freeoptical-256.png"
        ))
        .unwrap();
        assert_eq!((icone.width, icone.height), (256, 256));
    }
}
