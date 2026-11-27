//! Éditeur des configurations multiples (comme l'éditeur multi-configuration de Zemax) et
//! choix de la configuration active.

use super::*;
use crate::core::configurations::{TypeOperande, ValeurConfig, bilan_configurations};

/// État de la fenêtre des configurations
#[derive(Debug, Clone)]
pub struct EtatConfigurations {
    pub ouverte: bool,
    /// Paramètre et numéro (à partir de 1) de l'opérande à ajouter
    nouveau_type: TypeOperande,
    nouvel_index: usize,
    /// Comparaison des configurations (focale, F/#, spot par champ), calculée à la demande
    bilan: Option<Vec<crate::core::configurations::BilanConfiguration>>,
}

impl Default for EtatConfigurations {
    fn default() -> Self {
        Self { ouverte: false, nouveau_type: TypeOperande::Epaisseur, nouvel_index: 1, bilan: None }
    }
}

/// Pas de glissement d'une cellule selon le paramètre
fn pas(t: TypeOperande) -> f64 {
    match t {
        TypeOperande::Rayon => 0.1,
        TypeOperande::Conique => 0.001,
        TypeOperande::LongueurOnde | TypeOperande::LongueurOndeRef | TypeOperande::DistanceObjet | TypeOperande::ReseauTraits => 1.0,
        TypeOperande::ReseauOrdre => 0.05,
        _ => 0.01,
    }
}

/// Désignation de ce que vise l'index : « S3 », « champ 2 », « λ 1 »
fn cible_operande(t: TypeOperande, index: usize) -> String {
    if t.porte_sur_surface() {
        format!("S{}", index + 1)
    } else if !t.a_un_index() {
        "—".into()
    } else if matches!(t, TypeOperande::ChampX | TypeOperande::ChampY) {
        trf!("champ {}", "field {}", "Feld {}", index + 1)
    } else {
        format!("λ {}", index + 1)
    }
}

impl FreeOpticalGuiApp {
    /// Rend active la configuration `k` (les modifications de l'ancienne sont gardées)
    pub fn changer_configuration(&mut self, k: usize) {
        let erreurs = self.systeme.changer_configuration(k);
        self.recalculer();
        let k = self.systeme.configurations.active;
        self.statut_message = if erreurs.is_empty() {
            trf!(
                "Configuration {}/{} : {} | EFFL = {:.2} mm | Spot RMS = {:.2} µm",
                "Configuration {}/{}: {} | EFFL = {:.2} mm | Spot RMS = {:.2} µm",
                "Konfiguration {}/{}: {} | EFFL = {:.2} mm | Spot RMS = {:.2} µm",
                k + 1,
                self.systeme.nb_configurations(),
                self.systeme.configurations.nom(k),
                self.paraxial.focale_effl_mm,
                self.spot.rayon_rms_um
            )
        } else {
            erreurs.join(" ; ")
        };
    }

    /// Passe à la configuration suivante (Ctrl+A, comme dans Zemax)
    pub fn configuration_suivante(&mut self) {
        let nb = self.systeme.nb_configurations();
        if nb > 1 {
            self.changer_configuration((self.systeme.configurations.active + 1) % nb);
        }
    }

    /// Liste déroulante de la configuration active (barre de menu), seulement s'il y en a plusieurs
    pub(super) fn selecteur_configuration(&mut self, ui: &mut egui::Ui) {
        let nb = self.systeme.nb_configurations();
        if nb <= 1 {
            return;
        }
        let active = self.systeme.configurations.active;
        let mut choisie = active;
        egui::ComboBox::from_id_salt("selecteur_config")
            .selected_text(
                RichText::new(format!("Config {}/{} : {}", active + 1, nb, self.systeme.configurations.nom(active)))
                    .monospace()
                    .strong()
                    .color(COL_DOS_YELLOW)
                    .background_color(Color32::from_rgb(120, 0, 120)),
            )
            .width(190.0)
            .show_ui(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(COL_DOS_WHITE);
                for k in 0..nb {
                    ui.selectable_value(&mut choisie, k, format!("{} : {}", k + 1, self.systeme.configurations.nom(k)));
                }
            })
            .response
            .on_hover_text(tr!("Configuration affichée et analysée (Ctrl+A : suivante)", "Configuration shown and analysed (Ctrl+A: next)", "Angezeigte und analysierte Konfiguration (Strg+A: nächste)"));
        if choisie != active {
            self.changer_configuration(choisie);
        }
    }

    pub(super) fn dessiner_fenetre_configurations(&mut self, ctx: &Context) {
        if !self.configs.ouverte {
            return;
        }
        let mut ouverte = true;
        let mut modifie = false;
        let mut activer: Option<usize> = None;
        let mut action_config: Option<(bool, usize)> = None; // (ajouter, k)
        let mut retirer_op: Option<usize> = None;
        let mut ajouter_op = false;
        let mut comparer = false;
        let noms_verres: Vec<String> = self.systeme.catalogue.lister().into_iter().map(|m| m.nom.clone()).collect();
        let problemes = self.systeme.verifier_configurations();
        let etat = &mut self.configs;
        let sys = &mut self.systeme;
        egui::Window::new(RichText::new(tr!(" Configurations multiples ", " Multiple configurations ", " Mehrfachkonfigurationen ")).monospace().strong())
            .open(&mut ouverte)
            .collapsible(false)
            .resizable(true)
            .default_width(640.0)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(tr!(
                        "Chaque ligne est un paramètre qui change d'une configuration à l'autre ; tout le reste est commun. Le LDE et les analyses montrent la configuration active.",
                        "Each row is a parameter that changes from one configuration to another; everything else is shared. The LDE and the analyses show the active configuration.",
                        "Jede Zeile ist ein Parameter, der sich von Konfiguration zu Konfiguration ändert; alles andere ist gemeinsam. LDE und Analysen zeigen die aktive Konfiguration."
                    ))
                    .color(COL_DOS_GRAY),
                );
                let nb = sys.nb_configurations();
                let active = sys.configurations.active;
                ui.horizontal(|ui| {
                    if ui.button(tr!("+ Configuration (copie de l'active)", "+ Configuration (copy of the active one)", "+ Konfiguration (Kopie der aktiven)")).clicked() {
                        action_config = Some((true, active));
                    }
                    if ui
                        .add_enabled(nb > 1, egui::Button::new(tr!("Supprimer la configuration active", "Delete the active configuration", "Aktive Konfiguration löschen")))
                        .clicked()
                    {
                        action_config = Some((false, active));
                    }
                    if ui
                        .add_enabled(nb > 1, egui::Button::new(tr!("Comparer les configurations", "Compare configurations", "Konfigurationen vergleichen")))
                        .clicked()
                    {
                        comparer = true;
                    }
                });
                ui.separator();

                egui::ScrollArea::both().max_height(380.0).show(ui, |ui| {
                    egui::Grid::new("grille_configs").striped(true).spacing([8.0, 4.0]).show(ui, |ui| {
                        // En-têtes : nom de chaque configuration (modifiable) et bouton pour l'activer
                        ui.label("");
                        ui.label(RichText::new(tr!("Paramètre", "Parameter", "Parameter")).monospace().strong().color(COL_DOS_YELLOW));
                        ui.label(RichText::new(tr!("Où", "Where", "Wo")).monospace().strong().color(COL_DOS_YELLOW));
                        ui.label(RichText::new("Var.").monospace().strong().color(COL_DOS_YELLOW)).on_hover_text(tr!(
                            "Variable de l'optimiseur, séparément dans chaque configuration",
                            "Optimizer variable, separately in each configuration",
                            "Optimierungsvariable, getrennt in jeder Konfiguration"
                        ));
                        for k in 0..nb {
                            if k >= sys.configurations.noms.len() {
                                let n = sys.configurations.nom(k);
                                sys.configurations.noms.push(n);
                            }
                            let est_active = k == active;
                            ui.vertical(|ui| {
                                if ui
                                    .selectable_label(
                                        est_active,
                                        RichText::new(format!("{} {}", if est_active { "▶" } else { " " }, k + 1))
                                            .monospace()
                                            .strong()
                                            .color(if est_active { COL_DOS_YELLOW } else { COL_DOS_CYAN }),
                                    )
                                    .on_hover_text(tr!("Rendre active", "Make active", "Aktivieren"))
                                    .clicked()
                                {
                                    activer = Some(k);
                                }
                                modifie |= ui
                                    .add(egui::TextEdit::singleline(&mut sys.configurations.noms[k]).desired_width(110.0))
                                    .changed();
                            });
                        }
                        ui.label("");
                        ui.end_row();

                        for i in 0..sys.configurations.operandes.len() {
                            let (t, index) = (sys.configurations.operandes[i].type_op, sys.configurations.operandes[i].index);
                            ui.label(RichText::new(format!("{}", i + 1)).monospace().color(COL_DOS_CYAN));
                            ui.label(RichText::new(t.libelle()).monospace());
                            ui.label(RichText::new(cible_operande(t, index)).monospace());
                            if t.peut_varier() {
                                modifie |= ui.checkbox(&mut sys.configurations.operandes[i].variable, "").changed();
                            } else {
                                ui.label("");
                            }
                            for k in 0..nb {
                                let Some(valeur) = sys.configurations.operandes[i].valeurs.get(k).cloned() else {
                                    ui.label("?");
                                    continue;
                                };
                                let mut nouvelle: Option<ValeurConfig> = None;
                                if t == TypeOperande::Verre {
                                    let mut v = valeur.texte();
                                    egui::ComboBox::from_id_salt(("verre_config", i, k))
                                        .selected_text(RichText::new(&v).monospace())
                                        .width(90.0)
                                        .show_ui(ui, |ui| {
                                            for n in &noms_verres {
                                                if ui.selectable_value(&mut v, n.clone(), n.as_str()).changed() {
                                                    nouvelle = Some(ValeurConfig::Texte(n.clone()));
                                                }
                                            }
                                        });
                                } else {
                                    let mut v = valeur.nombre().unwrap_or(0.0);
                                    let r = ui.add(
                                        egui::DragValue::new(&mut v)
                                            .speed(pas(t))
                                            .max_decimals(6)
                                            .custom_formatter(|v, _| if v.is_finite() { format!("{v:.4}") } else { "INF".into() })
                                            .custom_parser(|s| ValeurConfig::Texte(s.to_string()).nombre()),
                                    );
                                    if r.changed() {
                                        nouvelle = Some(ValeurConfig::depuis_nombre(v));
                                    }
                                    if !v.is_finite() {
                                        r.on_hover_text(tr!("Infini : saisir un nombre pour en sortir", "Infinite: type a number to leave it", "Unendlich: eine Zahl eingeben, um es zu verlassen"));
                                    }
                                }
                                if let Some(n) = nouvelle {
                                    if k == active {
                                        // Colonne active : le système change tout de suite
                                        let _ = sys.ecrire_operande(t, index, &n);
                                    }
                                    sys.configurations.operandes[i].valeurs[k] = n;
                                    modifie = true;
                                }
                            }
                            if ui.button("X").on_hover_text(tr!("Retirer ce paramètre", "Remove this parameter", "Diesen Parameter entfernen")).clicked() {
                                retirer_op = Some(i);
                            }
                            ui.end_row();
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr!("Ajouter :", "Add:", "Hinzufügen:")).monospace().strong().color(COL_DOS_YELLOW));
                    egui::ComboBox::from_id_salt("type_nouvel_operande")
                        .selected_text(etat.nouveau_type.libelle())
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for t in TypeOperande::TOUS {
                                ui.selectable_value(&mut etat.nouveau_type, t, t.libelle());
                            }
                        });
                    if etat.nouveau_type.a_un_index() {
                        let libelle = if etat.nouveau_type.porte_sur_surface() {
                            tr!("surface", "surface", "Fläche")
                        } else if matches!(etat.nouveau_type, TypeOperande::ChampX | TypeOperande::ChampY) {
                            tr!("champ", "field", "Feld")
                        } else {
                            "λ"
                        };
                        ui.label(libelle);
                        ui.add(egui::DragValue::new(&mut etat.nouvel_index).range(1..=999));
                    }
                    if ui.button(tr!("Ajouter", "Add", "Hinzufügen")).clicked() {
                        ajouter_op = true;
                    }
                });
                for p in &problemes {
                    ui.label(RichText::new(format!("(!) {p}")).monospace().color(COL_DOS_ORANGE));
                }

                if let Some(bilan) = &etat.bilan {
                    ui.separator();
                    ui.label(RichText::new(tr!("COMPARAISON", "COMPARISON", "VERGLEICH")).monospace().strong().color(COL_DOS_YELLOW));
                    egui::Grid::new("bilan_configs").striped(true).spacing([10.0, 3.0]).show(ui, |ui| {
                        for h in ["Config", "EFFL (mm)", "F/#", tr!("Tirage paraxial (mm)", "Paraxial BFL (mm)", "Parax. Schnittweite (mm)"), tr!("Longueur (mm)", "Length (mm)", "Länge (mm)"), tr!("Spot RMS par champ (µm)", "RMS spot per field (µm)", "RMS-Spot je Feld (µm)"), "Airy (µm)"] {
                            ui.label(RichText::new(h).monospace().strong().color(COL_DOS_CYAN));
                        }
                        ui.end_row();
                        for b in bilan {
                            ui.label(RichText::new(format!("{} {}", b.numero + 1, b.nom)).monospace());
                            ui.label(RichText::new(format!("{:.3}", b.focale_mm)).monospace());
                            ui.label(RichText::new(format!("{:.2}", b.f_nombre)).monospace());
                            ui.label(RichText::new(format!("{:.3}", b.tirage_mm)).monospace());
                            ui.label(RichText::new(format!("{:.2}", b.longueur_totale_mm)).monospace());
                            ui.label(RichText::new(b.spot_rms_um.iter().map(|v| format!("{v:.2}")).collect::<Vec<_>>().join(" / ")).monospace());
                            ui.label(RichText::new(format!("{:.2}", b.rayon_airy_um)).monospace());
                            ui.end_row();
                        }
                    });
                }
            });

        if modifie {
            self.configs.bilan = None;
            self.recalculer();
        }
        if let Some(k) = activer {
            self.changer_configuration(k);
        }
        match action_config {
            Some((true, modele)) => {
                let k = self.systeme.ajouter_configuration(modele, None);
                self.configs.bilan = None;
                self.statut_message = trf!(
                    "Configuration {} ajoutée (copie de la {}). Ajouter les paramètres qui changent, puis leurs valeurs.",
                    "Configuration {} added (copy of {}). Add the parameters that change, then their values.",
                    "Konfiguration {} hinzugefügt (Kopie von {}). Die sich ändernden Parameter und dann ihre Werte hinzufügen.",
                    k + 1,
                    modele + 1
                );
            }
            Some((false, k)) => {
                self.systeme.supprimer_configuration(k);
                self.configs.bilan = None;
                self.recalculer();
            }
            None => {}
        }
        if let Some(i) = retirer_op {
            self.systeme.configurations.operandes.remove(i);
            self.configs.bilan = None;
        }
        if ajouter_op {
            let t = self.configs.nouveau_type;
            let index = if t.a_un_index() { self.configs.nouvel_index.saturating_sub(1) } else { 0 };
            match self.systeme.ajouter_operande(t, index) {
                Ok(_) => self.configs.bilan = None,
                Err(e) => self.statut_message = e,
            }
        }
        if comparer {
            self.systeme.capturer_configuration();
            self.configs.bilan = Some(bilan_configurations(&self.systeme));
        }
        if !ouverte {
            self.configs.ouverte = false;
        }
    }

    /// Ouvre la fenêtre des configurations, numéro proposé = surface sélectionnée dans le LDE
    pub fn ouvrir_configurations(&mut self) {
        self.configs.ouverte = true;
        if self.configs.nouveau_type.porte_sur_surface() {
            self.configs.nouvel_index = self.ligne_selectionnee + 1;
        }
    }
}
