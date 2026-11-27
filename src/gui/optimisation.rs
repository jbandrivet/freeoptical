//! Fenêtre d'optimisation : choix des variables, du critère et des contraintes (fonction de mérite).

use super::*;

/// Réglages de la fonction de mérite et des variables
#[derive(Debug, Clone)]
pub struct ReglagesOptimisation {
    pub rayons: bool,
    pub epaisseurs_air: bool,
    pub epaisseurs_verre: bool,
    pub tirage: bool,
    pub coniques: bool,
    pub aspheres: bool,
    /// 0 : spot RMS, 1 : front d'onde RMS (tous champs, toutes longueurs d'onde)
    pub critere: u8,
    pub focale_active: bool,
    pub focale_mm: f64,
    /// Plusieurs configurations : focale visée dans une seule (numéro) ou dans toutes (None)
    pub focale_config: Option<usize>,
    pub bord_min_mm: f64,
    pub centre_min_mm: f64,
    pub longueur_active: bool,
    pub longueur_max_mm: f64,
    pub distorsion_active: bool,
    pub distorsion_max_pct: f64,
    pub iterations: usize,
}

impl Default for ReglagesOptimisation {
    fn default() -> Self {
        Self {
            rayons: true,
            epaisseurs_air: false,
            epaisseurs_verre: false,
            tirage: true,
            coniques: false,
            aspheres: false,
            critere: 0,
            focale_active: true,
            focale_mm: 100.0,
            focale_config: None,
            bord_min_mm: 1.8,
            centre_min_mm: 2.0,
            longueur_active: false,
            longueur_max_mm: 200.0,
            distorsion_active: false,
            distorsion_max_pct: 1.0,
            iterations: 20,
        }
    }
}

impl FreeOpticalGuiApp {
    /// Ouvre la fenêtre d'optimisation, focale cible = focale actuelle
    pub fn ouvrir_optimisation(&mut self) {
        if self.paraxial.focale_effl_mm.is_finite() {
            self.reglages_optim.focale_mm = (self.paraxial.focale_effl_mm * 1000.0).round() / 1000.0;
        }
        self.reglages_optim.longueur_max_mm = self.paraxial.longueur_totale_mm.abs().max(1.0).round();
        self.fenetre_optim = true;
    }

    fn lancer_optimisation(&mut self) {
        let r = self.reglages_optim.clone();
        let n = self.systeme.surfaces.len();
        let mut vars = Vec::new();
        let var = |i: usize, p: &str| OptimizationVariable { index_surface: i, parametre: p.to_string() };
        for i in 0..n.saturating_sub(1) {
            let s = &self.systeme.surfaces[i];
            if r.rayons && s.type_surface != SurfaceType::Plane {
                vars.push(var(i, "rayon"));
            }
            if r.coniques && matches!(s.type_surface, SurfaceType::Conique | SurfaceType::Aspherique) {
                vars.push(var(i, "conique"));
            }
            if r.aspheres && s.type_surface == SurfaceType::Aspherique {
                vars.push(var(i, "a4"));
                vars.push(var(i, "a6"));
            }
            let verre = est_verre(&self.systeme, s);
            let epaisseur = if i + 2 == n { r.tirage } else if verre { r.epaisseurs_verre } else { r.epaisseurs_air };
            if epaisseur {
                vars.push(var(i, "epaisseur"));
            }
        }
        let vars_par_config = self
            .systeme
            .configurations
            .operandes
            .iter()
            .filter(|o| o.variable && o.type_op.peut_varier())
            .count()
            * if self.systeme.nb_configurations() > 1 { self.systeme.nb_configurations() } else { 0 };
        if vars.is_empty() && vars_par_config == 0 {
            self.statut_message = tr!("Aucune variable choisie.", "No variable selected.", "Keine Variable gewählt.").to_string();
            return;
        }
        let cible = |t: &str, v: f64| OptimizationTarget { type_cible: t.to_string(), valeur_cible: v, poids: 1.0, configuration: None };
        let mut cibles = vec![cible(if r.critere == 1 { "opd_rms_champs" } else { "spot_rms_champs" }, 0.0)];
        if r.focale_active {
            let mut c = cible("focale", r.focale_mm);
            c.configuration = r.focale_config.filter(|k| *k < self.systeme.nb_configurations());
            cibles.push(c);
        }
        cibles.push(cible("epaisseur_bord_min", r.bord_min_mm));
        cibles.push(cible("epaisseur_centre_min", r.centre_min_mm));
        if r.longueur_active {
            cibles.push(cible("longueur_totale_max", r.longueur_max_mm));
        }
        if r.distorsion_active {
            cibles.push(cible("distorsion_max", r.distorsion_max_pct));
        }
        let rapport = optimiser_systeme_dls(&mut self.systeme, &vars, &cibles, r.iterations);
        self.recalculer();
        self.statut_message = trf!(
            "{} — {} variables",
            "{} — {} variables",
            "{} — {} Variablen",
            rapport.resume,
            vars.len()
        );
    }

    pub(super) fn dessiner_fenetre_optimisation(&mut self, ctx: &Context) {
        if !self.fenetre_optim {
            return;
        }
        let mut ouverte = true;
        let mut lancer = false;
        let nb_configs = self.systeme.nb_configurations();
        let noms_configs: Vec<String> = (0..nb_configs).map(|k| self.systeme.configurations.nom(k)).collect();
        let r = &mut self.reglages_optim;
        egui::Window::new(RichText::new(tr!(" Optimisation (Damped Least Squares) ", " Optimization (Damped Least Squares) ", " Optimierung (Damped Least Squares) ")).monospace().strong())
            .open(&mut ouverte)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(RichText::new(tr!("VARIABLES", "VARIABLES", "VARIABLEN")).monospace().strong().color(COL_DOS_YELLOW));
                ui.checkbox(&mut r.rayons, tr!("Rayons de courbure", "Radii of curvature", "Krümmungsradien"));
                ui.checkbox(&mut r.epaisseurs_verre, tr!("Épaisseurs des lentilles", "Lens thicknesses", "Linsendicken"));
                ui.checkbox(&mut r.epaisseurs_air, tr!("Espaces d'air (et distances entre miroirs)", "Air spaces (and mirror separations)", "Luftabstände (und Spiegelabstände)"));
                ui.checkbox(&mut r.tirage, tr!("Tirage (distance au plan image)", "Back focus (distance to the image plane)", "Schnittweite (Abstand zur Bildebene)"));
                ui.checkbox(&mut r.coniques, tr!("Constantes coniques", "Conic constants", "Konische Konstanten"));
                ui.checkbox(&mut r.aspheres, tr!("Coefficients asphériques A4, A6", "Aspheric coefficients A4, A6", "Asphärenkoeffizienten A4, A6"));
                ui.separator();
                ui.label(RichText::new(tr!("CRITÈRE (tous les champs, toutes les longueurs d'onde)", "CRITERION (all fields, all wavelengths)", "KRITERIUM (alle Felder, alle Wellenlängen)")).monospace().strong().color(COL_DOS_YELLOW));
                ui.radio_value(&mut r.critere, 0, tr!("Spot RMS (rayons, autour du centroïde)", "RMS spot (rays, about the centroid)", "RMS-Spot (Strahlen, um den Schwerpunkt)"));
                ui.radio_value(&mut r.critere, 1, tr!("Front d'onde RMS (différence de marche)", "RMS wavefront (optical path difference)", "RMS-Wellenfront (optische Wegdifferenz)"));
                ui.separator();
                if nb_configs > 1 {
                    ui.label(
                        RichText::new(trf!(
                            "{} configurations : critère et contraintes comptent dans chacune. Les paramètres cochés « Var. » dans les configurations varient dans chacune ; les autres variables sont communes.",
                            "{} configurations: criterion and constraints count in each one. Parameters ticked “Var.” in the configurations vary in each one; the other variables are shared.",
                            "{} Konfigurationen: Kriterium und Nebenbedingungen zählen in jeder. Die in den Konfigurationen mit „Var.“ markierten Parameter variieren in jeder; die anderen Variablen sind gemeinsam.",
                            nb_configs
                        ))
                        .color(COL_DOS_CYAN),
                    );
                    ui.separator();
                }
                ui.label(RichText::new(tr!("CIBLES ET CONTRAINTES", "TARGETS AND CONSTRAINTS", "ZIELE UND NEBENBEDINGUNGEN")).monospace().strong().color(COL_DOS_YELLOW));
                egui::Grid::new("grille_optim").spacing([10.0, 4.0]).show(ui, |ui| {
                    ui.checkbox(&mut r.focale_active, tr!("Focale (mm)", "Focal length (mm)", "Brennweite (mm)"));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut r.focale_mm).speed(0.5));
                        if nb_configs > 1 {
                            let texte = match r.focale_config {
                                Some(k) if k < nb_configs => format!("{} : {}", k + 1, noms_configs[k]),
                                _ => tr!("toutes les configurations", "all configurations", "alle Konfigurationen").to_string(),
                            };
                            egui::ComboBox::from_id_salt("focale_config").selected_text(texte).width(170.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut r.focale_config, None, tr!("toutes les configurations", "all configurations", "alle Konfigurationen"));
                                for (k, n) in noms_configs.iter().enumerate() {
                                    ui.selectable_value(&mut r.focale_config, Some(k), format!("{} : {}", k + 1, n));
                                }
                            });
                        }
                    });
                    ui.end_row();
                    ui.label(tr!("Épaisseur au bord min. (mm)", "Min. edge thickness (mm)", "Min. Randdicke (mm)"));
                    ui.add(egui::DragValue::new(&mut r.bord_min_mm).speed(0.1).range(0.0..=100.0));
                    ui.end_row();
                    ui.label(tr!("Épaisseur au centre min. (mm)", "Min. centre thickness (mm)", "Min. Mittendicke (mm)"));
                    ui.add(egui::DragValue::new(&mut r.centre_min_mm).speed(0.1).range(0.0..=100.0));
                    ui.end_row();
                    ui.checkbox(&mut r.longueur_active, tr!("Longueur totale max. (mm)", "Max. total length (mm)", "Max. Baulänge (mm)"));
                    ui.add(egui::DragValue::new(&mut r.longueur_max_mm).speed(1.0).range(1.0..=100000.0));
                    ui.end_row();
                    ui.checkbox(&mut r.distorsion_active, tr!("Distorsion max. (%)", "Max. distortion (%)", "Max. Verzeichnung (%)"));
                    ui.add(egui::DragValue::new(&mut r.distorsion_max_pct).speed(0.05).range(0.0..=50.0));
                    ui.end_row();
                    ui.label(tr!("Itérations", "Iterations", "Iterationen"));
                    ui.add(egui::Slider::new(&mut r.iterations, 1..=100));
                    ui.end_row();
                });
                ui.add_space(6.0);
                if ui.button(RichText::new(tr!(" Optimiser ", " Optimize ", " Optimieren ")).strong()).clicked() {
                    lancer = true;
                }
            });
        if lancer {
            self.lancer_optimisation();
        }
        if !ouverte {
            self.fenetre_optim = false;
        }
    }
}
