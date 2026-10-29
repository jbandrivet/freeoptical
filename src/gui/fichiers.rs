//! Fenêtre Ouvrir / Enregistrer sous : navigateur de dossiers intégré (sans dépendance au
//! sélecteur du bureau), raccourcis vers les dossiers personnels, dernier dossier mémorisé.

use super::*;

/// Mode de la fenêtre de fichiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModeFichiers {
    #[default]
    Ouvrir,
    Enregistrer,
}

/// Fichier de préférence du dernier dossier : `~/.config/freeoptical/dossier`
fn fichier_dossier() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("freeoptical").join("dossier"))
}

fn dossier_personnel() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Dossier utilisateur XDG (`DESKTOP`, `DOWNLOAD`, `DOCUMENTS`) d'après `~/.config/user-dirs.dirs`
fn dossier_xdg(cle: &str) -> Option<PathBuf> {
    let maison = dossier_personnel()?;
    let texte = std::fs::read_to_string(maison.join(".config").join("user-dirs.dirs")).ok()?;
    let ligne = texte.lines().find(|l| l.trim_start().starts_with(&format!("XDG_{cle}_DIR=")))?;
    let valeur = ligne.split_once('=')?.1.trim().trim_matches('"');
    let chemin = PathBuf::from(valeur.replace("$HOME", &maison.to_string_lossy()));
    chemin.is_dir().then_some(chemin)
}

/// Dossier proposé au premier affichage : le dernier utilisé, sinon Téléchargements, sinon le
/// dossier personnel
pub fn dossier_initial() -> String {
    fichier_dossier()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .map(|d| PathBuf::from(d.trim()))
        .filter(|d| d.is_dir())
        .or_else(|| dossier_xdg("DOWNLOAD"))
        .or_else(dossier_personnel)
        .or_else(|| std::env::current_dir().ok())
        .map(|d| d.display().to_string())
        .unwrap_or_default()
}

fn memoriser_dossier(dossier: &Path) {
    // Les tests ne touchent pas aux préférences de l'utilisateur
    if cfg!(test) {
        return;
    }
    if let Some(f) = fichier_dossier() {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(f, dossier.display().to_string());
    }
}

fn est_json(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

/// Nature d'un fichier que FreeOptical sait ouvrir
fn nature(p: &Path) -> Option<&'static str> {
    if est_json(p) {
        Some("JSON")
    } else if est_fichier_zemax(p) {
        Some(tr!("Zemax séquentiel", "sequential Zemax", "Zemax sequentiell"))
    } else if est_fichier_teloptik(p) {
        Some("TDESIGN / LENSDES / RAYTRACE")
    } else if est_catalogue_agf(p) {
        Some(tr!("catalogue de verres", "glass catalogue", "Glaskatalog"))
    } else {
        None
    }
}

impl FreeOpticalGuiApp {
    /// Ouvre la fenêtre de fichiers dans le mode demandé
    pub fn afficher_fichiers(&mut self, mode: ModeFichiers) {
        self.mode_fichiers = mode;
        self.bilan_import = None;
        self.confirmer_remplacement = None;
        if mode == ModeFichiers::Enregistrer {
            self.nom_enregistrement = self
                .chemin_courant
                .as_ref()
                .and_then(|c| c.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| {
                    let nom: String = self.systeme.nom.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
                    format!("{}.json", nom.trim_matches('_'))
                });
        }
        self.fenetre_import = true;
    }

    /// Enregistre le système dans `chemin` (JSON) ; retourne vrai en cas de succès
    pub fn enregistrer_sous(&mut self, chemin: &Path) -> bool {
        let resultat = serde_json::to_string_pretty(&self.systeme)
            .map_err(|e| e.to_string())
            .and_then(|json| std::fs::write(chemin, json).map_err(|e| e.to_string()));
        match resultat {
            Ok(()) => {
                self.statut_message = trf!("Système enregistré dans {}.", "System saved to {}.", "System in {} gespeichert.", chemin.display());
                self.chemin_courant = Some(chemin.to_path_buf());
                self.marquer_enregistre();
                if let Some(d) = chemin.parent() {
                    memoriser_dossier(d);
                }
                true
            }
            Err(e) => {
                self.statut_message = trf!("ÉCHEC de l'enregistrement dans {} : {}", "FAILED to save to {}: {}", "FEHLER beim Speichern in {}: {}", chemin.display(), e);
                self.bilan_import = Some((false, self.statut_message.clone(), Vec::new()));
                false
            }
        }
    }

    /// Enregistre dans le fichier courant, ou demande un nom s'il n'y en a pas
    pub fn enregistrer(&mut self) {
        match self.chemin_courant.clone() {
            Some(c) => {
                self.enregistrer_sous(&c);
            }
            None => self.afficher_fichiers(ModeFichiers::Enregistrer),
        }
    }

    pub(super) fn dessiner_fenetre_fichiers(&mut self, ctx: &Context) {
        if !self.fenetre_import {
            return;
        }
        let mode = self.mode_fichiers;
        let mut ouverte = true;
        let mut a_ouvrir: Option<PathBuf> = None;
        let mut a_enregistrer: Option<PathBuf> = None;
        let mut aller: Option<PathBuf> = None;
        let mut fermer = false;
        let titre = match mode {
            ModeFichiers::Ouvrir => tr!(" Ouvrir un système ou un catalogue de verres ", " Open a system or a glass catalogue ", " System oder Glaskatalog öffnen "),
            ModeFichiers::Enregistrer => tr!(" Enregistrer le système sous… ", " Save the system as… ", " System speichern unter… "),
        };
        egui::Window::new(RichText::new(titre).monospace().strong())
            .open(&mut ouverte)
            .collapsible(false)
            .resizable(true)
            .default_width(620.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if mode == ModeFichiers::Ouvrir {
                    ui.label(tr!(
                        "Systèmes FreeOptical .json, Zemax séquentiels .zmx, fichiers .TDS, .RLD et .DES des programmes du livre « Telescope Optics » (Rutten et van Venrooij), catalogues de verres Zemax .agf. On peut aussi glisser un fichier sur la fenêtre.",
                        "FreeOptical .json systems, sequential Zemax .zmx, .TDS, .RLD and .DES files from the programs of the book \"Telescope Optics\" (Rutten and van Venrooij), Zemax .agf glass catalogues. You can also drop a file onto the window.",
                        "FreeOptical-Systeme .json, sequentielle Zemax-Dateien .zmx, .TDS-, .RLD- und .DES-Dateien der Programme aus dem Buch „Telescope Optics“ (Rutten und van Venrooij), Zemax-Glaskataloge .agf. Eine Datei kann auch auf das Fenster gezogen werden."
                    ));
                    ui.add_space(4.0);
                }
                // Raccourcis
                ui.horizontal_wrapped(|ui| {
                    let raccourcis = [
                        (tr!("Dossier personnel", "Home", "Persönlicher Ordner"), dossier_personnel()),
                        (tr!("Bureau", "Desktop", "Schreibtisch"), dossier_xdg("DESKTOP")),
                        (tr!("Téléchargements", "Downloads", "Downloads"), dossier_xdg("DOWNLOAD")),
                        (tr!("Documents", "Documents", "Dokumente"), dossier_xdg("DOCUMENTS")),
                    ];
                    for (nom, d) in raccourcis {
                        if let Some(d) = d {
                            if ui.button(nom).clicked() {
                                aller = Some(d);
                            }
                        }
                    }
                });
                // Dossier courant
                let dossier = PathBuf::from(self.dossier_import.trim());
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(dossier.parent().is_some(), egui::Button::new(tr!("Dossier parent", "Parent folder", "Übergeordneter Ordner")))
                        .clicked()
                    {
                        aller = dossier.parent().map(Path::to_path_buf);
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.dossier_import).desired_width(500.0).font(FontId::new(13.0, FontFamily::Monospace)));
                });
                // Contenu : sous-dossiers puis fichiers reconnus (fichiers et dossiers cachés exclus)
                let (mut dossiers, mut fichiers): (Vec<PathBuf>, Vec<PathBuf>) = (Vec::new(), Vec::new());
                let lisible = match std::fs::read_dir(&dossier) {
                    Ok(it) => {
                        for e in it.flatten() {
                            let p = e.path();
                            if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                                continue;
                            }
                            if p.is_dir() {
                                dossiers.push(p);
                            } else if match mode {
                                ModeFichiers::Ouvrir => nature(&p).is_some(),
                                ModeFichiers::Enregistrer => est_json(&p),
                            } {
                                fichiers.push(p);
                            }
                        }
                        true
                    }
                    Err(_) => false,
                };
                let cle = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
                dossiers.sort_by_key(cle);
                fichiers.sort_by_key(cle);
                ui.add_space(4.0);
                egui::Frame::new().fill(COL_DOS_CANVAS).inner_margin(Margin::same(6)).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    egui::ScrollArea::vertical().max_height(300.0).auto_shrink([false, true]).show(ui, |ui| {
                        if !lisible {
                            ui.label(RichText::new(tr!("Dossier introuvable ou illisible.", "Folder not found or unreadable.", "Ordner nicht gefunden oder nicht lesbar.")).color(COL_DOS_ORANGE));
                        }
                        for f in &fichiers {
                            let nom = f.file_name().unwrap_or_default().to_string_lossy();
                            let texte = format!("{nom}   ({})", nature(f).unwrap_or(""));
                            if ui.selectable_label(false, RichText::new(texte).monospace().color(COL_DOS_WHITE)).clicked() {
                                match mode {
                                    ModeFichiers::Ouvrir => a_ouvrir = Some(f.clone()),
                                    ModeFichiers::Enregistrer => {
                                        self.nom_enregistrement = nom.into_owned();
                                        self.confirmer_remplacement = None;
                                    }
                                }
                            }
                        }
                        // Les fichiers d'abord (dans un dossier de téléchargements, les sous-dossiers les
                        // repousseraient hors de vue), puis les sous-dossiers
                        if !fichiers.is_empty() && !dossiers.is_empty() {
                            ui.separator();
                        }
                        for d in &dossiers {
                            let nom = d.file_name().unwrap_or_default().to_string_lossy();
                            if ui.selectable_label(false, RichText::new(format!("{nom}/")).monospace().color(COL_DOS_CYAN)).clicked() {
                                aller = Some(d.clone());
                            }
                        }
                        if lisible && dossiers.is_empty() && fichiers.is_empty() {
                            ui.label(RichText::new(match mode {
                                ModeFichiers::Ouvrir => tr!("Aucun fichier reconnu dans ce dossier.", "No supported file in this folder.", "Keine unterstützte Datei in diesem Ordner."),
                                ModeFichiers::Enregistrer => tr!("Dossier vide.", "Empty folder.", "Leerer Ordner."),
                            }).color(COL_DOS_GRAY));
                        }
                    });
                });
                if mode == ModeFichiers::Enregistrer {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(tr!("Nom :", "Name:", "Name:"));
                        if ui
                            .add(egui::TextEdit::singleline(&mut self.nom_enregistrement).desired_width(380.0).font(FontId::new(13.0, FontFamily::Monospace)))
                            .changed()
                        {
                            self.confirmer_remplacement = None;
                        }
                        let mut nom = self.nom_enregistrement.trim().to_string();
                        if !nom.is_empty() && !nom.to_lowercase().ends_with(".json") {
                            nom.push_str(".json");
                        }
                        let cible = dossier.join(&nom);
                        let remplacer = self.confirmer_remplacement.as_ref() == Some(&cible);
                        let libelle = if remplacer {
                            tr!("Remplacer le fichier", "Replace the file", "Datei ersetzen")
                        } else {
                            tr!("Enregistrer", "Save", "Speichern")
                        };
                        if ui.add_enabled(!nom.is_empty() && lisible, egui::Button::new(libelle)).clicked() {
                            if cible.exists() && !remplacer {
                                self.confirmer_remplacement = Some(cible);
                            } else {
                                a_enregistrer = Some(cible);
                            }
                        }
                    });
                    if self.confirmer_remplacement.is_some() {
                        ui.label(RichText::new(tr!("Ce fichier existe déjà : cliquez à nouveau pour le remplacer.", "This file already exists: click again to replace it.", "Diese Datei existiert bereits: zum Ersetzen erneut klicken.")).color(COL_DOS_ORANGE));
                    }
                }
                if let Some((reussi, msg, lignes)) = &self.bilan_import {
                    ui.separator();
                    let couleur = if *reussi { COL_DOS_GREEN } else { COL_DOS_RED };
                    ui.label(RichText::new(msg).color(couleur));
                    egui::ScrollArea::vertical().id_salt("avertissements_import").max_height(180.0).show(ui, |ui| {
                        for l in lignes {
                            ui.label(RichText::new(format!("• {l}")).color(COL_DOS_YELLOW));
                        }
                    });
                }
                ui.add_space(6.0);
                if ui.button(tr!("Fermer", "Close", "Schließen")).clicked() {
                    fermer = true;
                }
            });
        if let Some(d) = aller {
            self.dossier_import = d.display().to_string();
            self.confirmer_remplacement = None;
        }
        if let Some(f) = a_ouvrir {
            if let Some(d) = f.parent() {
                memoriser_dossier(d);
            }
            self.bilan_import = None;
            let reussi = self.ouvrir_fichier(&f);
            // Ouverture sans remarque : la fenêtre se ferme ; sinon le bilan reste affiché
            if reussi && self.bilan_import.as_ref().is_none_or(|b| b.0 && b.2.is_empty()) {
                fermer = true;
            }
        }
        if let Some(c) = a_enregistrer {
            if self.enregistrer_sous(&c) {
                fermer = true;
                // Enregistrement demandé en quittant : on quitte maintenant
                if self.quitter_apres_enregistrement {
                    self.fermer_maintenant = true;
                }
            }
        }
        if !ouverte || fermer {
            self.fenetre_import = false;
            self.quitter_apres_enregistrement = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enregistrer_puis_rouvrir() {
        let dossier = std::env::temp_dir().join(format!("freeoptical-fichiers-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let mut app = FreeOpticalGuiApp::nouveau(OpticalSystem::doublet_achromatique_defaut());
        // Rien à enregistrer au démarrage ; une modification est détectée, l'enregistrement l'efface
        assert!(!app.est_modifie());
        app.systeme.surfaces[0].epaisseur += 0.5;
        assert!(app.est_modifie());
        let chemin = dossier.join("doublet.json");
        assert!(app.enregistrer_sous(&chemin));
        assert!(!app.est_modifie());
        assert_eq!(app.chemin_courant.as_deref(), Some(chemin.as_path()));
        // Un exemple chargé oublie le fichier courant ; la réouverture le retrouve
        app.charger_systeme(OpticalSystem::nouveau("vierge", 587.5618), "");
        assert!(app.chemin_courant.is_none());
        assert!(app.ouvrir_fichier(&chemin));
        assert!(!app.est_modifie());
        assert_eq!(app.systeme.surfaces.len(), 4);
        assert_eq!(app.chemin_courant.as_deref(), Some(chemin.as_path()));
        // « Enregistrer » réécrit le fichier courant
        app.systeme.nom = "modifié".into();
        app.enregistrer();
        assert!(std::fs::read_to_string(&chemin).unwrap().contains("modifié"));
        // Fichier invalide : refusé, bilan en erreur, fichier courant inchangé
        let faux = dossier.join("faux.json");
        std::fs::write(&faux, "{ pas du json").unwrap();
        assert!(!app.ouvrir_fichier(&faux));
        assert!(app.bilan_import.as_ref().is_some_and(|b| !b.0));
        assert_eq!(app.chemin_courant.as_deref(), Some(chemin.as_path()));
        // Formats reconnus
        assert!(nature(&chemin).is_some() && nature(Path::new("a.ZMX")).is_some() && nature(Path::new("a.agf")).is_some());
        assert!(nature(Path::new("photo.png")).is_none());
        std::fs::remove_dir_all(&dossier).unwrap();
    }
}
