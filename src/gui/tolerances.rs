//! Fenêtre de tolérancement : éditeur des tolérances (enregistrées avec le système), réglages,
//! analyse en arrière-plan (sensibilités, Monte-Carlo), résultats et pire tirage.

use super::*;
use crate::core::tolerance::{
    self as ct, Compensateur, Critere, Distribution, ReglagesTolerance, ResultatTolerance, Tolerance, TypeTolerance,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Analyse en cours dans un fil séparé
pub struct TacheTolerance {
    progression: Arc<AtomicUsize>,
    arret: Arc<AtomicBool>,
    total: usize,
    recepteur: mpsc::Receiver<Result<ResultatTolerance, String>>,
}

/// État de la fenêtre
pub struct EtatTolerance {
    pub ouverte: bool,
    pub reglages: ReglagesTolerance,
    /// Fréquence de la FTM quand ce critère est choisi (cycles/mm)
    pub frequence_ftm: f64,
    tache: Option<TacheTolerance>,
    resultat: Option<Result<ResultatTolerance, String>>,
    /// Système et tolérances de la dernière analyse (pour reconstruire le pire tirage)
    analyse: Option<(OpticalSystem, Vec<Tolerance>, ReglagesTolerance)>,
    /// Système nominal mis de côté pendant l'examen du pire tirage : système, fichier, empreinte
    nominal: Option<(OpticalSystem, Option<PathBuf>, String)>,
}

impl Default for EtatTolerance {
    fn default() -> Self {
        Self {
            ouverte: false,
            reglages: ReglagesTolerance::default(),
            frequence_ftm: 30.0,
            tache: None,
            resultat: None,
            analyse: None,
            nominal: None,
        }
    }
}

/// Borne de tolérance lisible (au plus 5 décimales, zéros inutiles retirés)
fn borne(v: f64) -> String {
    let t = format!("{v:+.5}");
    let t = t.trim_end_matches('0').trim_end_matches('.');
    if t == "+" || t == "-" { "0".into() } else { t.to_string() }
}

/// Valeur du critère lisible
fn val(critere: Critere, v: f64) -> String {
    if !v.is_finite() {
        return tr!("échec", "failed", "Fehler").to_string();
    }
    match critere {
        Critere::SpotRms => format!("{v:.3}"),
        _ => format!("{v:.4}"),
    }
}

impl FreeOpticalGuiApp {
    pub fn ouvrir_tolerancement(&mut self) {
        self.tolerance.ouverte = true;
    }


    fn lancer_tolerancement(&mut self, ctx: &Context) {
        let mut reglages = self.tolerance.reglages.clone();
        if let Critere::Ftm { .. } = reglages.critere {
            reglages.critere = Critere::Ftm { frequence: self.tolerance.frequence_ftm };
        }
        let sys = self.systeme.clone();
        let ops = sys.tolerances.clone();
        let progression = Arc::new(AtomicUsize::new(0));
        let arret = Arc::new(AtomicBool::new(false));
        let (emetteur, recepteur) = mpsc::channel();
        let (p, a, c) = (progression.clone(), arret.clone(), ctx.clone());
        let (s2, o2, r2) = (sys.clone(), ops.clone(), reglages.clone());
        std::thread::spawn(move || {
            let _ = emetteur.send(ct::tolerancer(&s2, &o2, &r2, &p, &a));
            c.request_repaint();
        });
        self.tolerance.tache = Some(TacheTolerance { progression, arret, total: ct::nb_calculs(&ops, &reglages), recepteur });
        self.tolerance.resultat = None;
        self.tolerance.analyse = Some((sys, ops, reglages));
    }

    /// Remplace le système affiché par le pire tirage (le nominal est gardé pour y revenir)
    fn examiner_pire_tirage(&mut self) {
        let Some((sys, ops, reglages)) = self.tolerance.analyse.clone() else { return };
        let Some(Ok(r)) = &self.tolerance.resultat else { return };
        let Some(mc) = &r.monte_carlo else { return };
        let pire = ct::systeme_du_tirage(&sys, &ops, &reglages, mc.index_pire);
        if self.tolerance.nominal.is_none() {
            self.tolerance.nominal = Some((self.systeme.clone(), self.chemin_courant.clone(), self.systeme_enregistre.clone()));
        }
        let msg = trf!(
            "Pire tirage Monte-Carlo (n° {}) affiché : « Revenir au système nominal » dans la fenêtre de tolérancement.",
            "Worst Monte Carlo trial (#{}) shown: “Back to the nominal system” in the tolerancing window.",
            "Schlechtester Monte-Carlo-Lauf (Nr. {}) angezeigt: „Zurück zum Nennsystem“ im Toleranzfenster.",
            mc.index_pire + 1
        );
        self.charger_systeme(pire, msg);
        // Le pire tirage n'est pas un fichier : pas de question à la fermeture pour lui seul
        self.systeme_enregistre = serde_json::to_string(&self.systeme).unwrap_or_default();
    }

    fn revenir_au_nominal(&mut self) {
        if let Some((sys, chemin, empreinte)) = self.tolerance.nominal.take() {
            self.charger_systeme(sys, tr!("Système nominal rétabli.", "Nominal system restored.", "Nennsystem wiederhergestellt."));
            self.chemin_courant = chemin;
            self.systeme_enregistre = empreinte;
        }
    }

    pub(super) fn dessiner_fenetre_tolerancement(&mut self, ctx: &Context) {
        if !self.tolerance.ouverte {
            return;
        }
        // Fin du calcul en arrière-plan
        if let Some(t) = &self.tolerance.tache {
            if let Ok(r) = t.recepteur.try_recv() {
                self.statut_message = match &r {
                    Ok(r) => trf!(
                        "Tolérancement terminé : {} nominal {}, estimé {}.",
                        "Tolerancing done: {} nominal {}, estimated {}.",
                        "Tolerierung beendet: {} nominal {}, geschätzt {}.",
                        r.critere.libelle(),
                        val(r.critere, r.nominal),
                        val(r.critere, r.critere_estime)
                    ),
                    Err(e) => e.clone(),
                };
                self.tolerance.resultat = Some(r);
                self.tolerance.tache = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        let mut ouverte = true;
        let mut action = 0; // 1 lancer, 2 arrêter, 3 pire tirage, 4 nominal
        let en_cours = self.tolerance.tache.is_some();
        let nb_surfaces = self.systeme.surfaces.len();
        egui::Window::new(RichText::new(tr!(" Tolérancement ", " Tolerancing ", " Tolerierung ")).monospace().strong())
            .open(&mut ouverte)
            .collapsible(false)
            .resizable(true)
            .default_width(820.0)
            .default_height(640.0)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // --- Éditeur des tolérances ---
                    ui.label(
                        RichText::new(tr!(
                            "TOLÉRANCES (enregistrées avec le système ; surfaces numérotées comme dans l'éditeur)",
                            "TOLERANCES (saved with the system; surfaces numbered as in the editor)",
                            "TOLERANZEN (mit dem System gespeichert; Flächen wie im Editor nummeriert)"
                        ))
                        .monospace()
                        .strong()
                        .color(COL_DOS_YELLOW),
                    );
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!en_cours, |ui| {
                            if ui.button(tr!("Tolérances par défaut", "Default tolerances", "Standardtoleranzen")).on_hover_text(tr!(
                                "2 franges de puissance, ±0,05 mm d'épaisseur, ±0,01 de conique, lentilles décentrées de ±0,02 mm et inclinées de ±1', faces inclinées de ±1', miroirs décentrés de ±0,05 mm et inclinés de ±1', indice ±0,0005, constringence ±0,5 %",
                                "2 fringes of power, ±0.05 mm thickness, ±0.01 conic, lenses decentered ±0.02 mm and tilted ±1', faces tilted ±1', mirrors decentered ±0.05 mm and tilted ±1', index ±0.0005, Abbe number ±0.5%",
                                "2 Ringe Passe, ±0,05 mm Dicke, ±0,01 Konik, Linsen ±0,02 mm dezentriert und ±1' gekippt, Flächen ±1' gekippt, Spiegel ±0,05 mm dezentriert und ±1' gekippt, Brechzahl ±0,0005, Abbe-Zahl ±0,5 %"
                            )).clicked() {
                                self.systeme.tolerances = ct::tolerances_par_defaut(&self.systeme);
                            }
                            if ui.button(tr!("+ Ajouter", "+ Add", "+ Hinzufügen")).clicked() {
                                self.systeme.tolerances.push(Tolerance::nouvelle(TypeTolerance::Franges, 1, 0, 1.0));
                            }
                            if ui.button(tr!("Tout effacer", "Clear all", "Alle löschen")).clicked() {
                                self.systeme.tolerances.clear();
                            }
                        });
                    });
                    if self.systeme.tolerances.is_empty() {
                        ui.label(RichText::new(tr!(
                            "Aucune tolérance : « Tolérances par défaut » propose un jeu complet, modifiable ensuite.",
                            "No tolerance: “Default tolerances” suggests a full set, editable afterwards.",
                            "Keine Toleranz: „Standardtoleranzen“ schlägt einen vollständigen, danach änderbaren Satz vor."
                        )).color(COL_DOS_GRAY));
                    }
                    let mut supprimer = None;
                    egui::ScrollArea::vertical().id_salt("liste_tolerances").max_height(220.0).show(ui, |ui| {
                        egui::Grid::new("grille_tolerances").striped(true).spacing([8.0, 3.0]).show(ui, |ui| {
                            for h in [tr!("Type", "Type", "Typ"), "Surf.", tr!("à", "to", "bis"), "Min", "Max", "", ""] {
                                ui.label(RichText::new(h).monospace().strong().color(COL_DOS_CYAN));
                            }
                            ui.end_row();
                            for (k, t) in self.systeme.tolerances.iter_mut().enumerate() {
                                egui::ComboBox::from_id_salt(("type_tol", k))
                                    .selected_text(format!("{} {}", t.type_tolerance.code(), t.type_tolerance.libelle()))
                                    .width(230.0)
                                    .show_ui(ui, |ui| {
                                        for ty in TypeTolerance::TOUS {
                                            ui.selectable_value(&mut t.type_tolerance, ty, format!("{} {}", ty.code(), ty.libelle()));
                                        }
                                    });
                                ui.add(egui::DragValue::new(&mut t.surface).range(1..=nb_surfaces.max(1)));
                                if t.type_tolerance.est_element() {
                                    if t.derniere < t.surface {
                                        t.derniere = t.surface;
                                    }
                                    ui.add(egui::DragValue::new(&mut t.derniere).range(t.surface..=nb_surfaces.max(1)));
                                } else {
                                    ui.label("");
                                }
                                ui.add(egui::DragValue::new(&mut t.min).speed(0.001).max_decimals(5));
                                ui.add(egui::DragValue::new(&mut t.max).speed(0.001).max_decimals(5));
                                ui.label(RichText::new(t.type_tolerance.unite()).color(COL_DOS_GRAY));
                                if ui.small_button("×").clicked() {
                                    supprimer = Some(k);
                                }
                                ui.end_row();
                            }
                        });
                    });
                    if let Some(k) = supprimer {
                        self.systeme.tolerances.remove(k);
                    }
                    for a in ct::verifier(&self.systeme, &self.systeme.tolerances, &self.tolerance.reglages) {
                        ui.label(RichText::new(format!("(!) {a}")).color(COL_DOS_ORANGE));
                    }
                    ui.separator();

                    // --- Réglages ---
                    let r = &mut self.tolerance.reglages;
                    ui.add_enabled_ui(!en_cours, |ui| {
                        egui::Grid::new("reglages_tol").spacing([12.0, 4.0]).show(ui, |ui| {
                            ui.label(tr!("Critère", "Criterion", "Kriterium"));
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut r.critere, Critere::SpotRms, tr!("tache RMS", "RMS spot", "RMS-Fleck"));
                                ui.radio_value(&mut r.critere, Critere::FrontOndeRms, tr!("front d'onde RMS", "RMS wavefront", "RMS-Wellenfront"));
                                let ftm = matches!(r.critere, Critere::Ftm { .. });
                                if ui.radio(ftm, tr!("FTM à", "MTF at", "MTF bei")).clicked() {
                                    r.critere = Critere::Ftm { frequence: self.tolerance.frequence_ftm };
                                }
                                ui.add(egui::DragValue::new(&mut self.tolerance.frequence_ftm).range(1.0..=2000.0).suffix(" c/mm"));
                            });
                            ui.end_row();
                            ui.label(tr!("Compensateur", "Compensator", "Kompensator"));
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut r.compensateur, Compensateur::MeilleureMiseAuPoint, tr!("meilleure mise au point", "best focus", "bester Fokus"));
                                ui.radio_value(&mut r.compensateur, Compensateur::MiseAuPointParaxiale, tr!("foyer paraxial", "paraxial focus", "paraxialer Fokus"));
                                ui.radio_value(&mut r.compensateur, Compensateur::Aucun, tr!("aucun", "none", "keiner"));
                            });
                            ui.end_row();
                            ui.label(tr!("Monte-Carlo", "Monte Carlo", "Monte-Carlo"));
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut r.nb_tirages).range(0..=5000).suffix(tr!(" tirages", " trials", " Läufe")));
                                ui.radio_value(&mut r.distribution, Distribution::Normale, tr!("loi normale (±2σ)", "normal (±2σ)", "Normalverteilung (±2σ)"));
                                ui.radio_value(&mut r.distribution, Distribution::Uniforme, tr!("loi uniforme", "uniform", "Gleichverteilung"));
                                ui.label(tr!("graine", "seed", "Startwert"));
                                ui.add(egui::DragValue::new(&mut r.graine));
                            });
                            ui.end_row();
                        });
                    });
                    if matches!(self.tolerance.reglages.critere, Critere::Ftm { .. }) {
                        ui.label(RichText::new(tr!(
                            "La FTM demande une PSF par champ et par calcul : comptez quelques secondes à une minute.",
                            "The MTF needs one PSF per field and per evaluation: allow a few seconds to a minute.",
                            "Die MTF braucht eine PSF je Feld und Rechnung: einige Sekunden bis eine Minute."
                        )).color(COL_DOS_GRAY));
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if let Some(t) = &self.tolerance.tache {
                            let fait = t.progression.load(Ordering::Relaxed);
                            ui.add(egui::ProgressBar::new(fait as f32 / t.total.max(1) as f32).desired_width(400.0).text(format!("{fait} / {}", t.total)));
                            if ui.button(tr!("Arrêter", "Stop", "Anhalten")).clicked() {
                                action = 2;
                            }
                        } else if ui
                            .add_enabled(!self.systeme.tolerances.is_empty(), egui::Button::new(RichText::new(tr!(" Lancer l'analyse ", " Run the analysis ", " Analyse starten ")).strong()))
                            .clicked()
                        {
                            action = 1;
                        }
                        if self.tolerance.nominal.is_some() && ui.button(tr!("Revenir au système nominal", "Back to the nominal system", "Zurück zum Nennsystem")).clicked() {
                            action = 4;
                        }
                    });

                    // --- Résultats ---
                    match &self.tolerance.resultat {
                        None => {}
                        Some(Err(e)) => {
                            ui.label(RichText::new(e).color(COL_DOS_RED));
                        }
                        Some(Ok(res)) => {
                            ui.separator();
                            let c = res.critere;
                            let sens = if c.plus_grand_meilleur() { "−" } else { "+" };
                            ui.label(
                                RichText::new(trf!(
                                    "{} : nominal {} — dégradation estimée (RSS) {}{} → {}",
                                    "{}: nominal {} — estimated change (RSS) {}{} → {}",
                                    "{}: nominal {} — geschätzte Änderung (RSS) {}{} → {}",
                                    c.libelle(),
                                    val(c, res.nominal),
                                    sens,
                                    val(c, res.degradation_rss),
                                    val(c, res.critere_estime)
                                ))
                                .monospace()
                                .strong()
                                .color(COL_DOS_YELLOW),
                            );
                            for a in &res.avertissements {
                                ui.label(RichText::new(format!("(!) {a}")).color(COL_DOS_ORANGE));
                            }
                            ui.label(RichText::new(tr!(
                                "SENSIBILITÉS (chaque tolérance seule, de la plus pénalisante à la moins pénalisante)",
                                "SENSITIVITIES (each tolerance alone, from worst to least harmful)",
                                "EMPFINDLICHKEITEN (jede Toleranz allein, von der schlimmsten zur harmlosesten)"
                            )).monospace().strong().color(COL_DOS_CYAN));
                            egui::ScrollArea::vertical().id_salt("sensibilites").max_height(220.0).show(ui, |ui| {
                                egui::Grid::new("grille_sens").striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
                                    for h in [
                                        tr!("Tolérance", "Tolerance", "Toleranz"),
                                        tr!("Bornes", "Limits", "Grenzen"),
                                        tr!("Critère au min", "Criterion at min", "Kriterium bei Min"),
                                        tr!("Critère au max", "Criterion at max", "Kriterium bei Max"),
                                        tr!("Pire écart", "Worst change", "Schlimmste Änderung"),
                                        tr!("Mise au point (mm)", "Refocus (mm)", "Nachfokus (mm)"),
                                    ] {
                                        ui.label(RichText::new(h).monospace().strong());
                                    }
                                    ui.end_row();
                                    let pire = res.lignes.first().map(|l| l.pire()).unwrap_or(0.0).max(1e-12);
                                    for l in &res.lignes {
                                        let couleur = if l.pire() > 0.5 * pire { COL_DOS_ORANGE } else { COL_DOS_WHITE };
                                        ui.label(RichText::new(&l.libelle).monospace().color(couleur));
                                        ui.label(RichText::new(format!("{} / {} {}", borne(l.tolerance.min), borne(l.tolerance.max), l.tolerance.type_tolerance.unite())).monospace());
                                        ui.label(RichText::new(val(c, l.critere_min)).monospace());
                                        ui.label(RichText::new(val(c, l.critere_max)).monospace());
                                        ui.label(RichText::new(val(c, l.pire())).monospace().color(couleur));
                                        ui.label(RichText::new(format!("{:+.4} / {:+.4}", l.compensation_min, l.compensation_max)).monospace().color(COL_DOS_GRAY));
                                        ui.end_row();
                                    }
                                });
                            });
                            if let Some(mc) = &res.monte_carlo {
                                ui.separator();
                                ui.label(
                                    RichText::new(trf!(
                                        "MONTE-CARLO ({} tirages) : moyenne {} ± {} — meilleur {} — pire {}{}",
                                        "MONTE CARLO ({} trials): mean {} ± {} — best {} — worst {}{}",
                                        "MONTE-CARLO ({} Läufe): Mittel {} ± {} — bester {} — schlechtester {}{}",
                                        mc.valeurs.len(),
                                        val(c, mc.moyenne),
                                        val(c, mc.ecart_type),
                                        val(c, mc.meilleur),
                                        val(c, mc.pire),
                                        if mc.nb_echecs > 0 { trf!(" — {} échecs", " — {} failures", " — {} Fehlschläge", mc.nb_echecs) } else { String::new() }
                                    ))
                                    .monospace()
                                    .strong()
                                    .color(COL_DOS_CYAN),
                                );
                                let mieux = if c.plus_grand_meilleur() { "≥" } else { "≤" };
                                ui.label(RichText::new(
                                    mc.centiles.iter().map(|(p, v)| format!("{p:.0} % {mieux} {}", val(c, *v))).collect::<Vec<_>>().join("   ")
                                ).monospace());
                                ui.label(RichText::new(trf!(
                                    "Mise au point de compensation : {:+.4} mm en moyenne, jusqu'à {:.4} mm",
                                    "Compensating refocus: {:+.4} mm on average, up to {:.4} mm",
                                    "Ausgleichsfokus: {:+.4} mm im Mittel, bis {:.4} mm",
                                    mc.compensation_moyenne,
                                    mc.compensation_max
                                )).color(COL_DOS_GRAY));
                                histogramme(ui, c, res.nominal, mc);
                                if ui.button(tr!("Examiner le pire tirage", "Inspect the worst trial", "Schlechtesten Lauf ansehen")).clicked() {
                                    action = 3;
                                }
                            }
                        }
                    }
                });
            });
        match action {
            1 => self.lancer_tolerancement(ctx),
            2 => {
                if let Some(t) = &self.tolerance.tache {
                    t.arret.store(true, Ordering::Relaxed);
                }
            }
            3 => self.examiner_pire_tirage(),
            4 => self.revenir_au_nominal(),
            _ => {}
        }
        if !ouverte {
            if let Some(t) = &self.tolerance.tache {
                t.arret.store(true, Ordering::Relaxed);
            }
            self.tolerance.ouverte = false;
        }
    }
}

/// Histogramme des tirages, avec le nominal (cyan) et la moyenne (jaune)
fn histogramme(ui: &mut egui::Ui, c: Critere, nominal: f64, mc: &ct::MonteCarlo) {
    let v: Vec<f64> = mc.valeurs.iter().copied().filter(|x| x.is_finite()).collect();
    if v.len() < 2 {
        return;
    }
    let (mut a, mut b) = v.iter().fold((nominal, nominal), |(a, b), &x| (a.min(x), b.max(x)));
    if b - a < 1e-12 {
        b = a + 1e-6;
    }
    let marge = 0.05 * (b - a);
    a -= marge;
    b += marge;
    let nb = 30;
    let mut classes = vec![0usize; nb];
    for x in &v {
        classes[(((x - a) / (b - a) * nb as f64) as usize).min(nb - 1)] += 1;
    }
    let max = *classes.iter().max().unwrap_or(&1) as f32;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(760.0), 150.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, COL_DOS_CANVAS);
    let zone = egui::Rect::from_min_max(rect.min + Vec2::new(8.0, 8.0), rect.max - Vec2::new(8.0, 22.0));
    let x = |t: f64| zone.left() + ((t - a) / (b - a)) as f32 * zone.width();
    for (k, &n) in classes.iter().enumerate() {
        let x0 = zone.left() + zone.width() * k as f32 / nb as f32;
        let h = zone.height() * n as f32 / max;
        p.rect_filled(egui::Rect::from_min_max(Pos2::new(x0 + 1.0, zone.bottom() - h), Pos2::new(x0 + zone.width() / nb as f32 - 1.0, zone.bottom())), 0.0, Color32::from_rgb(70, 110, 190));
    }
    for (t, couleur) in [(nominal, COL_DOS_CYAN), (mc.moyenne, COL_DOS_YELLOW)] {
        tracer_segment(&p, Pos2::new(x(t), zone.top()), Pos2::new(x(t), zone.bottom()), 1.5, couleur);
    }
    let police = FontId::new(10.5, FontFamily::Monospace);
    p.text(Pos2::new(zone.left(), rect.bottom() - 4.0), Align2::LEFT_BOTTOM, val(c, a), police.clone(), COL_DOS_GRAY);
    p.text(Pos2::new(zone.right(), rect.bottom() - 4.0), Align2::RIGHT_BOTTOM, val(c, b), police.clone(), COL_DOS_GRAY);
    p.text(
        Pos2::new(zone.center().x, rect.bottom() - 4.0),
        Align2::CENTER_BOTTOM,
        trf!("{}   (cyan : nominal, jaune : moyenne)", "{}   (cyan: nominal, yellow: mean)", "{}   (cyan: nominal, gelb: Mittel)", c.libelle()),
        police,
        COL_DOS_GRAY,
    );
}
