//! Fenêtre de l'interféromètre de Bath : simulation de l'interférogramme d'un miroir testé au
//! centre de courbure, réduction (conique mesurée, erreur de surface, Strehl) et analyse d'une
//! photo d'interférogramme.

use super::ondes::palette;
use super::*;
use crate::core::bath::{
    LAMBDA_VISUELLE_NM, ParametresBath, ReductionBath, SigneCarte, SimulationBath, reduire_carte, simuler_bath,
    simuler_et_reduire,
};
use crate::core::franges::{CarteFranges, Disque, ImageGris, OptionsFranges, detecter_disque, extraire_phase};
use crate::core::onde::nom_zernike;
use eframe::egui::{ColorImage, TextureHandle, TextureOptions};
use std::sync::Arc;

/// Termes de Zernike de surface proposés dans les défauts simulés (Noll)
const DEFAUTS_ZERNIKE: [usize; 6] = [6, 5, 8, 7, 10, 11];
/// Extensions des photos lisibles
const EXTENSIONS_IMAGES: [&str; 6] = ["png", "jpg", "jpeg", "bmp", "tif", "tiff"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeBath {
    Simulation,
    Photo,
}

/// Réglages qui déterminent l'extraction de phase d'une photo
#[derive(Debug, Clone, Copy, PartialEq)]
struct CleExtraction {
    disque: Disque,
    marge_pct: f64,
    rho_obstruction: f64,
    taille: usize,
}

#[derive(Default)]
struct TexturesBath {
    gauche: Option<(u64, TextureHandle)>,
    residu: Option<(u64, TextureHandle)>,
}

pub struct EtatBath {
    pub ouverte: bool,
    pub mode: ModeBath,
    pub params: ParametresBath,
    pub couleur_laser: bool,
    /// Interférogramme seul (rapide, suit les curseurs)
    apercu: Option<(ParametresBath, Result<SimulationBath, String>)>,
    /// Simulation et réduction complètes
    simulation: Option<(ParametresBath, Result<(SimulationBath, ReductionBath), String>)>,
    pub chemin_export: String,
    // Photo
    pub dossier: String,
    chemin_photo: Option<PathBuf>,
    photo: Option<Arc<ImageGris>>,
    pub disque: Option<Disque>,
    pub marge_pct: f64,
    pub signe: SigneCarte,
    extraction: Option<(CleExtraction, Result<CarteFranges, String>)>,
    reduction_photo: Option<(ParametresBath, SigneCarte, Result<ReductionBath, String>)>,
    recepteur: Option<(CleExtraction, mpsc::Receiver<Result<CarteFranges, String>>)>,
    pub message: String,
    generation: u64,
    textures: TexturesBath,
}

impl Default for EtatBath {
    fn default() -> Self {
        Self {
            ouverte: false,
            mode: ModeBath::Simulation,
            params: ParametresBath::default(),
            couleur_laser: true,
            apercu: None,
            simulation: None,
            chemin_export: "interferogramme_bath.png".to_string(),
            dossier: std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default(),
            chemin_photo: None,
            photo: None,
            disque: None,
            marge_pct: 3.0,
            signe: SigneCarte::Auto,
            extraction: None,
            reduction_photo: None,
            recepteur: None,
            message: String::new(),
            generation: 0,
            textures: TexturesBath::default(),
        }
    }
}

pub fn est_image(chemin: &Path) -> bool {
    chemin
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS_IMAGES.contains(&e.to_ascii_lowercase().as_str()))
}

/// Champ numérique avec libellé, sur une ligne de grille
fn champ(ui: &mut egui::Ui, libelle: &str, valeur: &mut f64, vitesse: f64, plage: std::ops::RangeInclusive<f64>, decimales: usize) -> bool {
    ui.label(libelle);
    let r = ui.add(egui::DragValue::new(valeur).speed(vitesse).range(plage).max_decimals(decimales)).changed();
    ui.end_row();
    r
}

fn titre(ui: &mut egui::Ui, texte: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(texte).monospace().strong().color(COL_DOS_YELLOW));
}

impl EtatBath {
    /// Ouvre une photo d'interférogramme et repère le disque du miroir
    pub fn ouvrir_photo(&mut self, chemin: &Path) {
        match ImageGris::charger(chemin) {
            Ok(img) => {
                self.disque = detecter_disque(&img);
                self.message = match self.disque {
                    Some(d) => trf!(
                        "{} : {} × {} px, miroir repéré (centre {:.1} ; {:.1}, rayon {:.1} px).",
                        "{}: {} × {} px, mirror found (centre {:.1}; {:.1}, radius {:.1} px).",
                        "{}: {} × {} px, Spiegel gefunden (Mitte {:.1}; {:.1}, Radius {:.1} px).",
                        chemin.file_name().unwrap_or_default().to_string_lossy(),
                        img.largeur,
                        img.hauteur,
                        d.cx,
                        d.cy,
                        d.rayon
                    ),
                    None => tr!(
                        "Miroir non repéré : donnez son centre et son rayon.",
                        "Mirror not found: enter its centre and radius.",
                        "Spiegel nicht gefunden: Mitte und Radius eingeben."
                    )
                    .to_string(),
                };
                if let Some(dossier) = chemin.parent() {
                    self.dossier = dossier.display().to_string();
                }
                self.photo = Some(Arc::new(img));
                self.chemin_photo = Some(chemin.to_path_buf());
                self.extraction = None;
                self.reduction_photo = None;
                self.recepteur = None;
                self.mode = ModeBath::Photo;
                self.ouverte = true;
                self.generation += 1;
            }
            Err(e) => self.message = e,
        }
    }

    fn cle_extraction(&self) -> Option<CleExtraction> {
        Some(CleExtraction {
            disque: self.disque?,
            marge_pct: self.marge_pct,
            rho_obstruction: self.params.rho_obstruction(),
            taille: self.params.taille,
        })
    }

    /// Lance l'extraction de phase de la photo dans un fil séparé
    fn lancer_analyse(&mut self, ctx: &Context) {
        let (Some(img), Some(cle)) = (self.photo.clone(), self.cle_extraction()) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let opt = OptionsFranges { disque: cle.disque, marge_pct: cle.marge_pct, rho_obstruction: cle.rho_obstruction, taille: cle.taille };
            let _ = tx.send(extraire_phase(&img, &opt));
            ctx.request_repaint();
        });
        self.recepteur = Some((cle, rx));
        self.message = tr!("Analyse des franges en cours…", "Analysing the fringes…", "Streifen werden ausgewertet…").to_string();
    }

    /// Met à jour les calculs : aperçu des franges en continu, réduction quand la souris est lâchée
    fn mettre_a_jour(&mut self, ctx: &Context) {
        let glisse = ctx.input(|i| i.pointer.any_down());
        match self.mode {
            ModeBath::Simulation => {
                if self.apercu.as_ref().is_none_or(|a| a.0 != self.params) {
                    self.apercu = Some((self.params.clone(), simuler_bath(&self.params)));
                    self.generation += 1;
                }
                if !glisse && self.simulation.as_ref().is_none_or(|s| s.0 != self.params) {
                    self.simulation = Some((self.params.clone(), simuler_et_reduire(&self.params)));
                    self.generation += 1;
                }
            }
            ModeBath::Photo => {
                if let Some((cle, rx)) = &self.recepteur
                    && let Ok(r) = rx.try_recv()
                {
                    self.message = match &r {
                        Ok(c) => trf!(
                            "Franges analysées : porteuse de {:.1} franges, contraste {:.0} %.",
                            "Fringes analysed: carrier of {:.1} fringes, contrast {:.0} %.",
                            "Streifen ausgewertet: Träger mit {:.1} Streifen, Kontrast {:.0} %.",
                            c.porteuse_franges.0.hypot(c.porteuse_franges.1),
                            c.contraste * 100.0
                        ),
                        Err(e) => e.clone(),
                    };
                    self.extraction = Some((*cle, r));
                    self.recepteur = None;
                    self.reduction_photo = None;
                    self.generation += 1;
                }
                let carte_a_jour = matches!((&self.extraction, self.cle_extraction()), (Some((c, Ok(_))), Some(k)) if *c == k);
                if carte_a_jour
                    && !glisse
                    && self.reduction_photo.as_ref().is_none_or(|r| r.0 != self.params || r.1 != self.signe)
                    && let Some((_, Ok(carte))) = &self.extraction
                {
                    let r = reduire_carte(&carte.opd_ondes, &self.params, self.signe);
                    self.reduction_photo = Some((self.params.clone(), self.signe, r));
                    self.generation += 1;
                }
            }
        }
    }

    /// Paramètres du miroir pris dans le système optique (premier miroir courbe)
    fn reprendre_miroir(&mut self, sys: &OpticalSystem) -> bool {
        let Some(s) = sys.surfaces.iter().find(|s| s.verre == "MIROIR" && s.rayon.is_finite()) else {
            return false;
        };
        self.params.diametre_mm = 2.0 * s.semi_diametre;
        self.params.rayon_courbure_mm = s.rayon.abs();
        self.params.conique = s.conique;
        self.params.conique_visee = s.conique;
        self.params.diametre_obstruction_mm = 2.0 * s.rayon_obstruction;
        true
    }

    fn reduction_courante(&self) -> Option<&Result<ReductionBath, String>> {
        match self.mode {
            ModeBath::Simulation => None,
            ModeBath::Photo => self.reduction_photo.as_ref().map(|r| &r.2),
        }
    }
}

impl FreeOpticalGuiApp {
    pub(super) fn dessiner_fenetre_bath(&mut self, ctx: &Context) {
        if !self.bath.ouverte {
            return;
        }
        self.bath.mettre_a_jour(ctx);
        let mut ouverte = true;
        egui::Window::new(RichText::new(tr!(" Interféromètre de Bath ", " Bath interferometer ", " Bath-Interferometer ")).monospace().strong())
            .open(&mut ouverte)
            .collapsible(false)
            .resizable(true)
            .default_size([1180.0, 760.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let b = &mut self.bath;
                    ui.selectable_value(&mut b.mode, ModeBath::Simulation, RichText::new(tr!(" Simulation ", " Simulation ", " Simulation ")).monospace().strong());
                    ui.selectable_value(&mut b.mode, ModeBath::Photo, RichText::new(tr!(" Analyse d'une photo ", " Photo analysis ", " Fotoauswertung ")).monospace().strong());
                    ui.separator();
                    if ui.button(tr!("Reprendre le miroir du système", "Use the system's mirror", "Spiegel des Systems übernehmen")).clicked() {
                        let sys = self.systeme.clone();
                        let b = &mut self.bath;
                        b.message = if b.reprendre_miroir(&sys) {
                            tr!("Miroir repris du système optique.", "Mirror taken from the optical system.", "Spiegel aus dem optischen System übernommen.").to_string()
                        } else {
                            tr!("Le système n'a pas de miroir courbe.", "The system has no curved mirror.", "Das System hat keinen gekrümmten Spiegel.").to_string()
                        };
                    }
                });
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(370.0);
                        egui::ScrollArea::vertical().id_salt("bath_parametres").show(ui, |ui| self.panneau_parametres_bath(ui));
                    });
                    ui.separator();
                    ui.vertical(|ui| self.panneau_resultats_bath(ui));
                });
            });
        if !ouverte {
            self.bath.ouverte = false;
        }
    }

    fn panneau_parametres_bath(&mut self, ui: &mut egui::Ui) {
        let simulation = self.bath.mode == ModeBath::Simulation;
        let b = &mut self.bath;
        let p = &mut b.params;

        titre(ui, tr!("MIROIR", "MIRROR", "SPIEGEL"));
        egui::Grid::new("bath_miroir").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
            champ(ui, tr!("Diamètre utile (mm)", "Clear diameter (mm)", "Nutzbarer Durchmesser (mm)"), &mut p.diametre_mm, 0.5, 5.0..=5000.0, 2);
            champ(ui, tr!("Rayon de courbure (mm)", "Radius of curvature (mm)", "Krümmungsradius (mm)"), &mut p.rayon_courbure_mm, 1.0, 10.0..=1e6, 2);
            ui.label(tr!("Rapport d'ouverture", "Focal ratio", "Öffnungsverhältnis"));
            ui.label(RichText::new(format!("F/{:.2}", p.rapport_ouverture())).monospace().color(COL_DOS_CYAN));
            ui.end_row();
            champ(ui, tr!("Conique visée k", "Target conic k", "Ziel-Konik k"), &mut p.conique_visee, 0.01, -10.0..=10.0, 4);
            if simulation {
                champ(ui, tr!("Conique réelle du miroir k", "Actual mirror conic k", "Tatsächliche Konik k"), &mut p.conique, 0.01, -10.0..=10.0, 4);
            }
            champ(ui, tr!("Obstruction centrale Ø (mm)", "Central obstruction Ø (mm)", "Zentrale Obstruktion Ø (mm)"), &mut p.diametre_obstruction_mm, 0.5, 0.0..=4000.0, 1);
        });

        titre(ui, tr!("MONTAGE", "SET-UP", "AUFBAU"));
        egui::Grid::new("bath_montage").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
            champ(ui, tr!("Longueur d'onde du laser (nm)", "Laser wavelength (nm)", "Laserwellenlänge (nm)"), &mut p.longueur_onde_nm, 0.5, 350.0..=1100.0, 1);
            champ(ui, tr!("Écart source – référence (mm)", "Source – reference separation (mm)", "Abstand Quelle – Referenz (mm)"), &mut p.ecart_mm, 0.1, 0.0..=50.0, 2);
            champ(ui, tr!("Direction de l'écart (°)", "Separation direction (°)", "Richtung des Abstands (°)"), &mut p.angle_ecart_deg, 1.0, -180.0..=180.0, 1);
            ui.label(tr!("Astigmatisme de Bath", "Bath astigmatism", "Bath-Astigmatismus"));
            ui.label(RichText::new(format!("{:.4} λ PV", p.astigmatisme_bath_theorique_ondes())).monospace().color(COL_DOS_CYAN));
            ui.end_row();
        });

        if simulation {
            titre(ui, tr!("RÉGLAGES (FRANGES)", "ADJUSTMENTS (FRINGES)", "JUSTIERUNG (STREIFEN)"));
            egui::Grid::new("bath_reglages").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
                champ(ui, tr!("Basculement X (franges)", "Tilt X (fringes)", "Kippung X (Streifen)"), &mut p.basculement_x_franges, 0.1, -60.0..=60.0, 2);
                champ(ui, tr!("Basculement Y (franges)", "Tilt Y (fringes)", "Kippung Y (Streifen)"), &mut p.basculement_y_franges, 0.1, -60.0..=60.0, 2);
                champ(ui, tr!("Défocalisation (franges)", "Defocus (fringes)", "Defokussierung (Streifen)"), &mut p.defocalisation_franges, 0.05, -40.0..=40.0, 2);
                let (tx, ty) = p.decalage_mm();
                ui.label(tr!("Déplacement équivalent", "Equivalent shift", "Entsprechende Verschiebung"));
                ui.label(
                    RichText::new(format!("X {:+.3} Y {:+.3} Z {:+.3} mm", tx, ty, p.defocalisation_mm()))
                        .monospace()
                        .color(COL_DOS_CYAN),
                );
                ui.end_row();
            });
            if ui
                .button(tr!("Mise au point : franges les plus droites", "Focus: straightest fringes", "Fokus: geradeste Streifen"))
                .on_hover_text(tr!(
                    "Défocalisation qui compense la partie en ρ² de la sphérique attendue, comme on le fait au réglage",
                    "Defocus balancing the ρ² part of the expected spherical aberration, as done when adjusting",
                    "Defokussierung, die den ρ²-Anteil der erwarteten sphärischen Aberration ausgleicht, wie beim Justieren"
                ))
                .clicked()
            {
                p.defocalisation_franges = p.defocalisation_franges_droites();
            }

            titre(ui, tr!("DÉFAUTS DE SURFACE (nm, RMS de Zernike)", "SURFACE ERRORS (nm, Zernike RMS)", "OBERFLÄCHENFEHLER (nm, Zernike-RMS)"));
            egui::Grid::new("bath_defauts").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
                for j in DEFAUTS_ZERNIKE {
                    let mut v = p.defauts.zernike_nm.iter().find(|z| z.0 == j).map_or(0.0, |z| z.1);
                    if champ(ui, &format!("Z{j} {}", nom_zernike(j)), &mut v, 0.5, -2000.0..=2000.0, 1) {
                        p.defauts.zernike_nm.retain(|z| z.0 != j);
                        if v != 0.0 {
                            p.defauts.zernike_nm.push((j, v));
                        }
                    }
                }
                champ(ui, tr!("Zone : rayon (%)", "Zone: radius (%)", "Zone: Radius (%)"), &mut p.defauts.zone_rayon_pct, 0.5, 0.0..=100.0, 1);
                champ(ui, tr!("Zone : hauteur (nm)", "Zone: height (nm)", "Zone: Höhe (nm)"), &mut p.defauts.zone_hauteur_nm, 0.5, -2000.0..=2000.0, 1);
                champ(ui, tr!("Zone : largeur (%)", "Zone: width (%)", "Zone: Breite (%)"), &mut p.defauts.zone_largeur_pct, 0.5, 0.0..=100.0, 1);
                champ(ui, tr!("Bord : début (%)", "Edge: start (%)", "Rand: Beginn (%)"), &mut p.defauts.bord_debut_pct, 0.5, 0.0..=99.0, 1);
                champ(ui, tr!("Bord : écart au bord (nm, < 0 rabattu)", "Edge: deviation (nm, < 0 turned down)", "Rand: Abweichung (nm, < 0 abfallend)"), &mut p.defauts.bord_hauteur_nm, 0.5, -2000.0..=2000.0, 1);
            });
            if ui.button(tr!("Miroir parfait", "Perfect mirror", "Perfekter Spiegel")).clicked() {
                p.defauts = Default::default();
                p.conique = p.conique_visee;
            }
        }

        titre(ui, tr!("RÉDUCTION", "REDUCTION", "AUSWERTUNG"));
        ui.checkbox(
            &mut p.retirer_astigmatisme,
            tr!("Retirer aussi l'astigmatisme mesuré", "Also remove measured astigmatism", "Auch gemessenen Astigmatismus entfernen"),
        )
        .on_hover_text(tr!(
            "L'astigmatisme de Bath est toujours déduit. Cochez si l'astigmatisme restant vient du support (vérifiez en tournant le miroir).",
            "Bath astigmatism is always subtracted. Tick if the remaining astigmatism comes from the support (check by rotating the mirror).",
            "Der Bath-Astigmatismus wird immer abgezogen. Ankreuzen, wenn der verbleibende Astigmatismus von der Lagerung kommt (durch Drehen des Spiegels prüfen)."
        ));
        egui::Grid::new("bath_reduction").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
            ui.label(tr!("Points sur le diamètre", "Points across the diameter", "Punkte über den Durchmesser"));
            egui::ComboBox::from_id_salt("bath_taille").selected_text(p.taille.to_string()).show_ui(ui, |ui| {
                for t in [128, 200, 256, 320] {
                    ui.selectable_value(&mut p.taille, t, t.to_string());
                }
            });
            ui.end_row();
        });

        if simulation {
            titre(ui, tr!("EXPORT", "EXPORT", "EXPORT"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut b.chemin_export).desired_width(220.0));
                if ui.button(tr!("Enregistrer (PNG)", "Save (PNG)", "Speichern (PNG)")).clicked() {
                    b.message = match &b.apercu {
                        Some((_, Ok(sim))) => {
                            let chemin = PathBuf::from(b.chemin_export.trim());
                            match ImageGris::depuis_grille(&sim.intensite, sim.taille).enregistrer_png(&chemin) {
                                Ok(()) => trf!("Interférogramme enregistré dans {}.", "Interferogram saved to {}.", "Interferogramm in {} gespeichert.", chemin.display()),
                                Err(e) => e,
                            }
                        }
                        _ => tr!("Rien à enregistrer.", "Nothing to save.", "Nichts zu speichern.").to_string(),
                    };
                }
            });
            return;
        }

        // Photo
        titre(ui, tr!("PHOTO DE L'INTERFÉROGRAMME", "INTERFEROGRAM PHOTO", "FOTO DES INTERFEROGRAMMS"));
        ui.label(
            RichText::new(tr!(
                "Caméra au point sur le miroir, 10 à 20 franges de basculement, mise au point réglée pour des franges aussi droites que possible. On peut aussi glisser l'image sur la fenêtre.",
                "Camera focused on the mirror, 10 to 20 tilt fringes, focus adjusted for fringes as straight as possible. You can also drop the image onto the window.",
                "Kamera auf den Spiegel fokussiert, 10 bis 20 Kippstreifen, Fokus für möglichst gerade Streifen eingestellt. Das Bild kann auch auf das Fenster gezogen werden."
            ))
            .color(COL_DOS_GRAY),
        );
        ui.horizontal(|ui| {
            ui.label(tr!("Dossier :", "Folder:", "Ordner:"));
            ui.add(egui::TextEdit::singleline(&mut b.dossier).desired_width(280.0));
        });
        let mut images: Vec<PathBuf> = std::fs::read_dir(b.dossier.trim())
            .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| est_image(p)).collect())
            .unwrap_or_default();
        images.sort();
        let mut a_ouvrir = None;
        egui::ScrollArea::vertical().id_salt("bath_images").max_height(110.0).show(ui, |ui| {
            if images.is_empty() {
                ui.label(RichText::new(tr!("Aucune image dans ce dossier.", "No image in this folder.", "Kein Bild in diesem Ordner.")).color(COL_DOS_ORANGE));
            }
            for f in &images {
                let nom = f.file_name().unwrap_or_default().to_string_lossy();
                let choisi = b.chemin_photo.as_ref() == Some(f);
                if ui.selectable_label(choisi, RichText::new(nom).monospace()).clicked() {
                    a_ouvrir = Some(f.clone());
                }
            }
        });
        if let Some(f) = a_ouvrir {
            b.ouvrir_photo(&f);
        }
        if let Some(img) = b.photo.clone() {
            titre(ui, tr!("DISQUE DU MIROIR (pixels)", "MIRROR DISC (pixels)", "SPIEGELSCHEIBE (Pixel)"));
            let mut d = b.disque.unwrap_or(Disque { cx: img.largeur as f64 / 2.0, cy: img.hauteur as f64 / 2.0, rayon: img.largeur.min(img.hauteur) as f64 * 0.4 });
            egui::Grid::new("bath_disque").num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
                champ(ui, tr!("Centre X", "Centre X", "Mitte X"), &mut d.cx, 0.25, 0.0..=img.largeur as f64, 2);
                champ(ui, tr!("Centre Y", "Centre Y", "Mitte Y"), &mut d.cy, 0.25, 0.0..=img.hauteur as f64, 2);
                champ(ui, tr!("Rayon", "Radius", "Radius"), &mut d.rayon, 0.25, 8.0..=img.largeur.max(img.hauteur) as f64, 2);
                champ(ui, tr!("Bord exclu (% du rayon)", "Excluded edge (% of radius)", "Ausgeschlossener Rand (% des Radius)"), &mut b.marge_pct, 0.1, 0.0..=30.0, 1);
            });
            b.disque = Some(d);
            ui.horizontal(|ui| {
                if ui.button(tr!("Repérer le miroir", "Find the mirror", "Spiegel finden")).clicked() {
                    b.disque = detecter_disque(&img).or(b.disque);
                }
                let en_cours = b.recepteur.is_some();
                if ui.add_enabled(!en_cours, egui::Button::new(RichText::new(tr!(" Analyser les franges ", " Analyse the fringes ", " Streifen auswerten ")).strong())).clicked() {
                    b.lancer_analyse(ui.ctx());
                }
                if en_cours {
                    ui.spinner();
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr!("Signe de la carte :", "Map sign:", "Vorzeichen der Karte:"));
                ui.radio_value(&mut b.signe, SigneCarte::Auto, tr!("auto", "auto", "auto"))
                    .on_hover_text(tr!(
                        "Une seule photo ne distingue pas bosse et creux : « auto » garde le signe qui rapproche le miroir de la conique visée.",
                        "A single photo cannot tell a bump from a hollow: \"auto\" keeps the sign that brings the mirror closer to the target conic.",
                        "Ein einzelnes Foto unterscheidet Buckel und Mulde nicht: „auto“ behält das Vorzeichen, das den Spiegel näher an die Ziel-Konik bringt."
                    ));
                ui.radio_value(&mut b.signe, SigneCarte::Normal, tr!("normal", "normal", "normal"));
                ui.radio_value(&mut b.signe, SigneCarte::Inverse, tr!("inversé", "inverted", "invertiert"));
            });
            let perimee = matches!((&b.extraction, b.cle_extraction()), (Some((c, _)), Some(k)) if *c != k);
            if perimee && b.recepteur.is_none() {
                ui.label(
                    RichText::new(tr!("Réglages modifiés : relancez l'analyse.", "Settings changed: run the analysis again.", "Einstellungen geändert: Auswertung erneut starten."))
                        .color(COL_DOS_ORANGE),
                );
            }
        }
    }

    /// Image carrée d'une grille (lignes de py = −1 à +1)
    fn texture_grille(ctx: &Context, nom: &str, taille: usize, couleur: impl Fn(usize) -> Option<Color32>) -> TextureHandle {
        let mut image = ColorImage::new([taille, taille], vec![COL_DOS_CANVAS; taille * taille]);
        for j in 0..taille {
            for i in 0..taille {
                if let Some(c) = couleur(j * taille + i) {
                    image.pixels[(taille - 1 - j) * taille + i] = c;
                }
            }
        }
        ctx.load_texture(nom, image, TextureOptions::LINEAR)
    }

    fn panneau_resultats_bath(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let b = &mut self.bath;
        let generation = b.generation;
        let cote = ((ui.available_width() - 120.0) / 2.0).clamp(180.0, 420.0);

        // Image de gauche : interférogramme simulé ou photo
        let laser = b.couleur_laser;
        let teinte = move |v: f32| {
            let v = v.clamp(0.0, 1.0);
            if laser {
                Color32::from_rgb((v * 255.0) as u8, (v * v * 60.0) as u8, (v * v * 40.0) as u8)
            } else {
                let g = (v * 255.0) as u8;
                Color32::from_rgb(g, g, g)
            }
        };
        if b.textures.gauche.as_ref().is_none_or(|t| t.0 != generation) {
            let tex = match b.mode {
                ModeBath::Simulation => match &b.apercu {
                    Some((_, Ok(sim))) => {
                        let opd = &sim.opd_ondes;
                        Some(Self::texture_grille(&ctx, "bath_franges", sim.taille, |k| opd[k].map(|_| teinte(sim.intensite[k]))))
                    }
                    _ => None,
                },
                ModeBath::Photo => b.photo.as_ref().map(|img| {
                    let pixels = img.pixels.iter().map(|&v| teinte(v)).collect();
                    ctx.load_texture("bath_photo", ColorImage::new([img.largeur, img.hauteur], pixels), TextureOptions::LINEAR)
                }),
            };
            b.textures.gauche = tex.map(|t| (generation, t));
        }

        // Carte d'erreur par rapport à la conique visée
        let reduction: Option<Result<ReductionBath, String>> = match b.mode {
            ModeBath::Simulation => b.simulation.as_ref().map(|s| s.1.as_ref().map(|r| r.1.clone()).map_err(|e| e.clone())),
            ModeBath::Photo => b.reduction_courante().cloned(),
        };
        let echelle_residu = reduction.as_ref().and_then(|r| r.as_ref().ok()).map(|r| {
            let (mini, maxi) = r.residu_ondes.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, c), &v| (a.min(v), c.max(v)));
            // Échelle d'au moins ±0,01 λ : un miroir parfait ne montre pas le bruit de calcul
            let m = mini.abs().max(maxi.abs()).max(0.01);
            (-m, m)
        });
        if b.textures.residu.as_ref().is_none_or(|t| t.0 != generation) {
            b.textures.residu = match (&reduction, echelle_residu) {
                (Some(Ok(r)), Some((mini, maxi))) => {
                    let res = &r.residu_ondes;
                    Some((generation, Self::texture_grille(&ctx, "bath_residu", r.taille, |k| res[k].map(|w| palette((w - mini) / (maxi - mini))))))
                }
                _ => None,
            };
        }

        ui.horizontal_top(|ui| {
            // Gauche
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(match b.mode {
                            ModeBath::Simulation => tr!("INTERFÉROGRAMME", "INTERFEROGRAM", "INTERFEROGRAMM"),
                            ModeBath::Photo => tr!("PHOTO", "PHOTO", "FOTO"),
                        })
                        .monospace()
                        .strong()
                        .color(COL_DOS_YELLOW),
                    );
                    if ui.checkbox(&mut b.couleur_laser, tr!("teinte du laser", "laser tint", "Laserfarbe")).changed() {
                        b.generation += 1;
                    }
                });
                let (rect, reponse) = ui.allocate_exact_size(Vec2::splat(cote), Sense::drag());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
                if let Some((_, tex)) = &b.textures.gauche {
                    match (b.mode, &b.photo) {
                        (ModeBath::Photo, Some(img)) => {
                            // Photo entière, proportions gardées ; cercle du miroir glissable
                            let echelle = (cote / img.largeur as f32).min(cote / img.hauteur as f32);
                            let taille = Vec2::new(img.largeur as f32 * echelle, img.hauteur as f32 * echelle);
                            let zone = egui::Rect::from_center_size(rect.center(), taille);
                            painter.image(tex.id(), zone, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            if let Some(d) = b.disque.as_mut() {
                                if reponse.dragged() {
                                    let delta = reponse.drag_delta() / echelle;
                                    d.cx += delta.x as f64;
                                    d.cy += delta.y as f64;
                                }
                                let centre = zone.min + Vec2::new(d.cx as f32, d.cy as f32) * echelle;
                                let r = d.rayon as f32 * echelle;
                                painter.circle_stroke(centre, r, Stroke::new(1.5, COL_DOS_GREEN));
                                painter.circle_stroke(centre, r * (1.0 - b.marge_pct as f32 / 100.0), Stroke::new(1.0, COL_DOS_YELLOW));
                                let ro = b.params.rho_obstruction() as f32;
                                if ro > 0.0 {
                                    painter.circle_stroke(centre, r * ro, Stroke::new(1.0, COL_DOS_YELLOW));
                                }
                                painter.line_segment([centre - Vec2::new(6.0, 0.0), centre + Vec2::new(6.0, 0.0)], Stroke::new(1.0, COL_DOS_GREEN));
                                painter.line_segment([centre - Vec2::new(0.0, 6.0), centre + Vec2::new(0.0, 6.0)], Stroke::new(1.0, COL_DOS_GREEN));
                            }
                        }
                        _ => {
                            painter.image(tex.id(), rect.shrink(2.0), egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                            // Direction de l'écart source – référence
                            let a = b.params.angle_ecart_deg.to_radians() as f32;
                            let c = rect.right_bottom() - Vec2::new(24.0, 24.0);
                            let u = Vec2::new(a.cos(), -a.sin()) * 14.0;
                            painter.arrow(c - u, u * 2.0, Stroke::new(1.5, COL_DOS_CYAN));
                            painter.text(c - Vec2::new(20.0, 0.0), Align2::RIGHT_CENTER, "s", FontId::new(12.0, FontFamily::Monospace), COL_DOS_CYAN);
                        }
                    }
                } else if b.mode == ModeBath::Photo {
                    painter.text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        tr!("Choisissez une photo", "Choose a photo", "Foto auswählen"),
                        FontId::new(13.0, FontFamily::Monospace),
                        COL_DOS_GRAY,
                    );
                }
            });
            // Droite : carte d'erreur
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(tr!("ÉCART À LA CONIQUE VISÉE (front d'onde)", "DEVIATION FROM TARGET CONIC (wavefront)", "ABWEICHUNG VON DER ZIEL-KONIK (Wellenfront)"))
                        .monospace()
                        .strong()
                        .color(COL_DOS_YELLOW),
                );
                let (rect, _) = ui.allocate_exact_size(Vec2::new(cote + 80.0, cote), Sense::hover());
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
                if let (Some((_, tex)), Some((mini, maxi))) = (&b.textures.residu, echelle_residu) {
                    let carre = egui::Rect::from_min_size(rect.min, Vec2::splat(cote)).shrink(2.0);
                    painter.image(tex.id(), carre, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    painter.circle_stroke(carre.center(), carre.width() * 0.5, Stroke::new(1.0, COL_DOS_GRAY));
                    let barre = egui::Rect::from_min_size(Pos2::new(carre.right() + 14.0, carre.top() + 10.0), Vec2::new(14.0, carre.height() - 30.0));
                    let nb = 64;
                    for k in 0..nb {
                        let t0 = k as f32 / nb as f32;
                        let y1 = barre.bottom() - t0 * barre.height();
                        let y0 = barre.bottom() - (t0 + 1.0 / nb as f32) * barre.height();
                        painter.rect_filled(egui::Rect::from_min_max(Pos2::new(barre.left(), y0), Pos2::new(barre.right(), y1 + 0.5)), 0.0, palette(t0 as f64));
                    }
                    let police = FontId::new(11.0, FontFamily::Monospace);
                    painter.text(Pos2::new(barre.right() + 3.0, barre.top()), Align2::LEFT_CENTER, format!("{maxi:+.3}"), police.clone(), COL_DOS_WHITE);
                    painter.text(Pos2::new(barre.right() + 3.0, barre.bottom()), Align2::LEFT_CENTER, format!("{mini:+.3}"), police.clone(), COL_DOS_WHITE);
                    painter.text(Pos2::new(barre.center().x, barre.bottom() + 8.0), Align2::CENTER_TOP, "λ", police, COL_DOS_GRAY);
                }
            });
        });

        ui.add_space(4.0);
        if !b.message.is_empty() {
            ui.label(RichText::new(&b.message).monospace().color(COL_DOS_CYAN));
        }
        let lambda = b.params.longueur_onde_nm;
        match &reduction {
            None => {
                if b.mode == ModeBath::Photo && b.photo.is_some() && b.extraction.is_none() && b.recepteur.is_none() {
                    ui.label(RichText::new(tr!("Cliquez sur « Analyser les franges ».", "Click \"Analyse the fringes\".", "Auf „Streifen auswerten“ klicken.")).monospace().color(COL_DOS_GRAY));
                }
                if let Some((_, Err(e))) = &b.extraction {
                    Self::afficher_erreur_onde(ui, e);
                }
            }
            Some(Err(e)) => Self::afficher_erreur_onde(ui, e),
            Some(Ok(r)) => {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(430.0);
                        let ligne = |ui: &mut egui::Ui, libelle: String, valeur: String, couleur: Color32| {
                            ui.label(RichText::new(libelle).monospace());
                            ui.label(RichText::new(valeur).monospace().strong().color(couleur));
                            ui.end_row();
                        };
                        egui::Grid::new("bath_bilan").num_columns(2).spacing([10.0, 2.0]).show(ui, |ui| {
                            let correction = if r.conique_visee.abs() > 1e-9 { format!("  ({:.0} %)", 100.0 * r.conique_mesuree / r.conique_visee) } else { String::new() };
                            ligne(ui, tr!("Conique visée / mesurée", "Target / measured conic", "Ziel- / gemessene Konik").to_string(), format!("{:+.4} / {:+.4}{correction}", r.conique_visee, r.conique_mesuree), COL_DOS_YELLOW);
                            ligne(
                                ui,
                                trf!("Front d'onde PV / RMS ({:.1} nm)", "Wavefront PV / RMS ({:.1} nm)", "Wellenfront PV / RMS ({:.1} nm)", lambda),
                                format!("{:.4} / {:.4} λ", r.pv_ondes, r.rms_ondes),
                                COL_DOS_WHITE,
                            );
                            let fraction = if r.pv_ondes_550 > 1e-3 { format!("  (λ/{:.1})", 1.0 / r.pv_ondes_550) } else { String::new() };
                            ligne(
                                ui,
                                trf!("Front d'onde PV / RMS ({:.0} nm)", "Wavefront PV / RMS ({:.0} nm)", "Wellenfront PV / RMS ({:.0} nm)", LAMBDA_VISUELLE_NM),
                                format!("{:.4} / {:.4} λ{fraction}", r.pv_ondes_550, r.rms_ondes_550),
                                COL_DOS_YELLOW,
                            );
                            ligne(ui, tr!("Surface PV / RMS", "Surface PV / RMS", "Oberfläche PV / RMS").to_string(), format!("{:.1} / {:.1} nm", r.pv_surface_nm, r.rms_surface_nm), COL_DOS_WHITE);
                            let ok = r.strehl_550 >= 0.8;
                            ligne(ui, trf!("Strehl ({:.0} nm)", "Strehl ({:.0} nm)", "Strehl ({:.0} nm)", LAMBDA_VISUELLE_NM), format!("{:.3}", r.strehl_550), if ok { COL_DOS_GREEN } else { COL_DOS_ORANGE });
                            ligne(ui, tr!("Sphérique attendue Z11", "Expected spherical Z11", "Erwartete Sphärische Z11").to_string(), format!("{:+.4} λ", r.spherique_visee_ondes), COL_DOS_GRAY);
                            ligne(ui, tr!("Astigmatisme de Bath déduit", "Bath astigmatism subtracted", "Abgezogener Bath-Astigmatismus").to_string(), format!("{:.4} λ PV", r.astigmatisme_bath_ondes), COL_DOS_GRAY);
                            if r.signe < 0.0 {
                                ligne(ui, tr!("Signe de la carte", "Map sign", "Vorzeichen der Karte").to_string(), tr!("inversé", "inverted", "invertiert").to_string(), COL_DOS_ORANGE);
                            }
                        });
                        ui.label(
                            RichText::new(tr!(
                                "Piston, basculements et défocalisation retirés ; le front d'onde au foyer sur une étoile a la même erreur (deux fois la surface).",
                                "Piston, tilts and defocus removed; the wavefront at focus on a star has the same error (twice the surface).",
                                "Kolben, Kippungen und Defokus entfernt; die Wellenfront im Fokus an einem Stern hat denselben Fehler (zweimal die Oberfläche)."
                            ))
                            .color(COL_DOS_GRAY),
                        );
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(tr!("ZERNIKE DE L'ÉCART (ondes)", "ZERNIKE OF THE DEVIATION (waves)", "ZERNIKE DER ABWEICHUNG (Wellen)"))
                                .monospace()
                                .strong()
                                .color(COL_DOS_YELLOW),
                        );
                        egui::ScrollArea::vertical().id_salt("bath_zernike").max_height(190.0).show(ui, |ui| {
                            egui::Grid::new("bath_grille_zernike").striped(true).spacing([12.0, 1.0]).show(ui, |ui| {
                                for (k, c) in r.zernike_residu_ondes.iter().enumerate().skip(4) {
                                    if c.abs() < 0.002 {
                                        continue;
                                    }
                                    let couleur = if c.abs() >= 0.05 { COL_DOS_YELLOW } else { COL_DOS_GRAY };
                                    ui.label(RichText::new(format!("Z{:<2}", k + 1)).monospace().color(COL_DOS_CYAN));
                                    ui.label(RichText::new(format!("{:+.4}", c)).monospace().color(couleur));
                                    ui.label(RichText::new(nom_zernike(k + 1)).monospace().color(COL_DOS_WHITE));
                                    ui.end_row();
                                }
                            });
                        });
                    });
                });
            }
        }
    }
}
