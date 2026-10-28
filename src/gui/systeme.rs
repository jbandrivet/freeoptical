//! Paramètres du système (objet, ouverture, champs, longueurs d'onde, afocal) et vue spectro.

use super::aberrations::{Courbe, echelle, graphe};
use super::*;
use crate::core::glass::{LAMBDA_C_NM, LAMBDA_F_NM};
use crate::core::spectre::couleur_spectrale;
use crate::core::types::{ApertureSpec, Wavelength};

impl FreeOpticalGuiApp {
    pub(super) fn dessiner_fenetre_systeme(&mut self, ctx: &Context) {
        if !self.fenetre_systeme {
            return;
        }
        let mut ouverte = true;
        let mut modifie = false;
        let sys = &mut self.systeme;
        egui::Window::new(
            RichText::new(tr!(" Paramètres du système ", " System settings ", " Systemeinstellungen ")).monospace().strong(),
        )
        .open(&mut ouverte)
        .collapsible(false)
        .resizable(true)
        .default_width(480.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(560.0).show(ui, |ui| {
                ui.label(RichText::new(tr!("OBJET", "OBJECT", "OBJEKT")).monospace().strong().color(COL_DOS_YELLOW));
                let mut fini = sys.objet_fini().is_some();
                ui.horizontal(|ui| {
                    modifie |= ui.radio_value(&mut fini, false, tr!("À l'infini (champs en degrés)", "At infinity (fields in degrees)", "Im Unendlichen (Felder in Grad)")).changed();
                    modifie |= ui.radio_value(&mut fini, true, tr!("À distance finie (champs en mm)", "At finite distance (fields in mm)", "In endlicher Entfernung (Felder in mm)")).changed();
                });
                if fini {
                    let mut d = sys.distance_objet_mm.filter(|d| *d > 0.0).unwrap_or(100.0);
                    ui.horizontal(|ui| {
                        ui.label(tr!("Distance de l'objet à la 1re surface (mm)", "Distance from object to 1st surface (mm)", "Abstand vom Objekt zur 1. Fläche (mm)"));
                        modifie |= ui.add(egui::DragValue::new(&mut d).speed(1.0).range(0.001..=1e9)).changed();
                    });
                    sys.distance_objet_mm = Some(d);
                } else {
                    sys.distance_objet_mm = None;
                }
                modifie |= ui
                    .checkbox(
                        &mut sys.afocal,
                        tr!(
                            "Espace image afocal (oculaire, jumelles, élargisseur de faisceau) : analyses en angles",
                            "Afocal image space (eyepiece, binoculars, beam expander): angular analyses",
                            "Afokaler Bildraum (Okular, Fernglas, Strahlaufweiter): Analysen in Winkeln"
                        ),
                    )
                    .changed();
                ui.separator();

                ui.label(RichText::new(tr!("OUVERTURE", "APERTURE", "ÖFFNUNG")).monospace().strong().color(COL_DOS_YELLOW));
                let (mut genre, mut valeur) = match sys.ouverture {
                    ApertureSpec::PupilleEntreeMm(v) => (0, v),
                    ApertureSpec::NombreOuverture(v) => (1, v),
                    ApertureSpec::OuvertureNumeriqueObjet(v) => (2, v),
                };
                ui.horizontal(|ui| {
                    modifie |= ui.radio_value(&mut genre, 0, tr!("Pupille d'entrée Ø (mm)", "Entrance pupil Ø (mm)", "Eintrittspupille Ø (mm)")).changed();
                    modifie |= ui.radio_value(&mut genre, 1, "F/#").changed();
                    modifie |= ui.radio_value(&mut genre, 2, tr!("Ouverture numérique objet", "Object-space NA", "Objektseitige NA")).changed();
                });
                modifie |= ui.add(egui::DragValue::new(&mut valeur).speed(0.01).range(1e-4..=1e6)).changed();
                sys.ouverture = match genre {
                    1 => ApertureSpec::NombreOuverture(valeur),
                    2 => ApertureSpec::OuvertureNumeriqueObjet(valeur.min(0.99)),
                    _ => ApertureSpec::PupilleEntreeMm(valeur),
                };
                ui.separator();

                let unite = sys.unite_champ();
                ui.label(
                    RichText::new(trf!("CHAMPS ({})", "FIELDS ({})", "FELDER ({})", unite)).monospace().strong().color(COL_DOS_YELLOW),
                );
                let mut retirer = None;
                egui::Grid::new("grille_champs").spacing([8.0, 3.0]).show(ui, |ui| {
                    for (i, c) in sys.champs_deg.iter_mut().enumerate() {
                        ui.label(format!("#{}", i + 1));
                        ui.label("X");
                        modifie |= ui.add(egui::DragValue::new(&mut c.0).speed(0.01)).changed();
                        ui.label("Y");
                        modifie |= ui.add(egui::DragValue::new(&mut c.1).speed(0.01)).changed();
                        if ui.small_button("−").clicked() {
                            retirer = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = retirer {
                    sys.champs_deg.remove(i);
                    modifie = true;
                }
                if ui.button(tr!("+ Ajouter un champ", "+ Add a field", "+ Feld hinzufügen")).clicked() {
                    let dernier = sys.champs_deg.last().copied().unwrap_or((0.0, 0.0));
                    sys.champs_deg.push((dernier.0, dernier.1 + 1.0));
                    modifie = true;
                }
                ui.separator();

                ui.label(RichText::new(tr!("LONGUEURS D'ONDE (nm, poids)", "WAVELENGTHS (nm, weight)", "WELLENLÄNGEN (nm, Gewicht)")).monospace().strong().color(COL_DOS_YELLOW));
                let mut retirer = None;
                let mut reference = sys.longueur_onde_ref_nm;
                egui::Grid::new("grille_lambdas").spacing([8.0, 3.0]).show(ui, |ui| {
                    for (i, w) in sys.longueurs_onde.iter_mut().enumerate() {
                        modifie |= ui.add(egui::DragValue::new(&mut w.valeur_nm).speed(0.5).range(100.0..=20000.0).suffix(" nm")).changed();
                        modifie |= ui.add(egui::DragValue::new(&mut w.poids).speed(0.05).range(0.0..=100.0)).changed();
                        if ui.radio((w.valeur_nm - reference).abs() < 1e-9, tr!("réf.", "ref.", "Ref.")).clicked() {
                            reference = w.valeur_nm;
                            modifie = true;
                        }
                        if ui.small_button("−").clicked() {
                            retirer = Some(i);
                        }
                        ui.end_row();
                    }
                });
                if let Some(i) = retirer {
                    if sys.longueurs_onde.len() > 1 {
                        sys.longueurs_onde.remove(i);
                        modifie = true;
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr!("+ Ajouter", "+ Add", "+ Hinzufügen")).clicked() {
                        let l = sys.longueurs_onde.last().map(|w| w.valeur_nm + 50.0).unwrap_or(LAMBDA_D_NM);
                        sys.longueurs_onde.push(Wavelength::new(l, 1.0, format!("{l:.1} nm")));
                        modifie = true;
                    }
                    if ui.button(tr!("Visible F, d, C", "Visible F, d, C", "Sichtbar F, d, C")).clicked() {
                        sys.longueurs_onde = Wavelength::visible_fdc();
                        reference = LAMBDA_D_NM;
                        modifie = true;
                    }
                    if ui.button(tr!("Raie d seule", "d line only", "Nur d-Linie")).clicked() {
                        sys.longueurs_onde = vec![Wavelength::new(LAMBDA_D_NM, 1.0, "d")];
                        reference = LAMBDA_D_NM;
                        modifie = true;
                    }
                    if ui.button(tr!("Spectro 400–700 nm", "Spectro 400–700 nm", "Spektro 400–700 nm")).clicked() {
                        sys.longueurs_onde = [400.0, LAMBDA_F_NM, LAMBDA_D_NM, LAMBDA_C_NM, 700.0]
                            .iter()
                            .map(|&l| Wavelength::new(l, 1.0, format!("{l:.1} nm")))
                            .collect();
                        reference = LAMBDA_D_NM;
                        modifie = true;
                    }
                });
                if !sys.longueurs_onde.iter().any(|w| (w.valeur_nm - reference).abs() < 1e-9) {
                    reference = sys.longueurs_onde.first().map(|w| w.valeur_nm).unwrap_or(LAMBDA_D_NM);
                }
                sys.longueur_onde_ref_nm = reference;
            });
        });
        if modifie {
            self.recalculer();
        }
        if !ouverte {
            self.fenetre_systeme = false;
        }
    }

    pub(super) fn dessiner_spectre(&mut self, ui: &mut egui::Ui) {
        let fini = self.systeme.objet_fini().is_some();
        let (mut lmin, mut lmax) = self.ondes.domaine_spectral(&self.systeme);
        let mut fente = if self.ondes.largeur_fente > 0.0 { self.ondes.largeur_fente } else if fini { 20.0 } else { 2.0 };
        let mut change = false;
        self.selecteurs_ondes(ui, true);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr!("Domaine :", "Range:", "Bereich:")).monospace());
            change |= ui.add(egui::DragValue::new(&mut lmin).speed(1.0).range(100.0..=20000.0).suffix(" nm")).changed();
            ui.label(tr!("à", "to", "bis"));
            change |= ui.add(egui::DragValue::new(&mut lmax).speed(1.0).range(100.0..=20000.0).suffix(" nm")).changed();
            ui.label(RichText::new(tr!("Largeur de fente :", "Slit width:", "Spaltbreite:")).monospace());
            let suffixe = if fini { " µm" } else { " ″" };
            change |= ui.add(egui::DragValue::new(&mut fente).speed(0.1).range(0.01..=1e5).suffix(suffixe)).changed();
        });
        if change {
            self.ondes.spectre_min_nm = lmin;
            self.ondes.spectre_max_nm = lmax.max(lmin + 1.0);
            self.ondes.largeur_fente = fente;
            self.ondes.invalider();
        }
        let s = match self.ondes.spectre(&self.systeme) {
            Ok(s) => s.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        let k = s.longueurs_onde_nm.len() / 2;
        ui.label(
            RichText::new(trf!(
                "Spectre {:.2} mm  |  à {:.1} nm : dispersion {:.3} nm/mm, image de fente {:.2} µm, tache RMS {:.2} µm, R = λ/Δλ ≈ {:.0}",
                "Spectrum {:.2} mm  |  at {:.1} nm: dispersion {:.3} nm/mm, slit image {:.2} µm, RMS spot {:.2} µm, R = λ/Δλ ≈ {:.0}",
                "Spektrum {:.2} mm  |  bei {:.1} nm: Dispersion {:.3} nm/mm, Spaltbild {:.2} µm, RMS-Spot {:.2} µm, R = λ/Δλ ≈ {:.0}",
                s.longueur_spectre_mm,
                s.longueurs_onde_nm[k],
                s.dispersion_nm_mm[k].unwrap_or(f64::NAN),
                s.image_fente_um.unwrap_or(f64::NAN),
                s.tache_rms_um[k].unwrap_or(f64::NAN),
                s.resolution[k].unwrap_or(f64::NAN)
            ))
            .monospace()
            .strong()
            .color(COL_DOS_YELLOW),
        );

        let dispo = ui.available_size();
        let taille_carte = Vec2::new(dispo.x * 0.5 - 6.0, dispo.y);
        let taille_courbe = Vec2::new(dispo.x * 0.5 - 6.0, dispo.y * 0.5 - 4.0);
        ui.horizontal(|ui| {
            // Carte du détecteur : position de chaque longueur d'onde, couleur spectrale
            let (rect, _) = ui.allocate_exact_size(taille_carte, Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
            let pts: Vec<(f64, f64, f64)> = s
                .positions_mm
                .iter()
                .zip(&s.longueurs_onde_nm)
                .filter_map(|(p, &l)| p.map(|p| (p.0, p.1, l)))
                .collect();
            let (cx, cy) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            let n = pts.len().max(1) as f64;
            let (cx, cy) = (cx / n, cy / n);
            let demi = pts.iter().map(|p| (p.0 - cx).abs().max((p.1 - cy).abs())).fold(0.5_f64, f64::max) * 1.15;
            let echelle_px = (rect.width().min(rect.height()) as f64 - 40.0) / (2.0 * demi);
            let vers = |x: f64, y: f64| {
                rect.center() + Vec2::new(((x - cx) * echelle_px) as f32, -((y - cy) * echelle_px) as f32)
            };
            let police = FontId::new(11.0, FontFamily::Monospace);
            for (i, &(x, y, l)) in pts.iter().enumerate() {
                let (r, g, b) = couleur_spectrale(l);
                let c = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
                painter.circle_filled(vers(x, y), 4.0, c);
                if i == 0 || i + 1 == pts.len() || i == pts.len() / 2 {
                    painter.text(vers(x, y) + Vec2::new(8.0, -8.0), Align2::LEFT_BOTTOM, format!("{l:.0} nm"), police.clone(), COL_DOS_WHITE);
                }
            }
            painter.text(
                rect.left_bottom() + Vec2::new(6.0, -6.0),
                Align2::LEFT_BOTTOM,
                trf!(
                    "détecteur : ±{:.2} mm autour de ({:.2}, {:.2})",
                    "detector: ±{:.2} mm around ({:.2}, {:.2})",
                    "Detektor: ±{:.2} mm um ({:.2}, {:.2})",
                    demi,
                    cx,
                    cy
                ),
                police,
                COL_DOS_GRAY,
            );
            ui.vertical(|ui| {
                let courbe = |v: &Vec<Option<f64>>, c: Color32| Courbe {
                    points: s.longueurs_onde_nm.iter().zip(v).filter_map(|(&l, y)| y.map(|y| (l, y))).collect(),
                    couleur: c,
                    tirets: false,
                };
                let (a, b) = (s.longueurs_onde_nm[0], *s.longueurs_onde_nm.last().unwrap_or(&1.0));
                let ed = echelle(s.dispersion_nm_mm.iter().flatten().copied(), 0.01);
                graphe(ui, taille_courbe, tr!("Dispersion (nm/mm)", "Dispersion (nm/mm)", "Dispersion (nm/mm)"), "λ (nm)", (a, b), (0.0, ed), &[courbe(&s.dispersion_nm_mm, COL_DOS_CYAN)]);
                let er = echelle(s.resolution.iter().flatten().copied(), 1.0);
                graphe(ui, taille_courbe, tr!("Pouvoir de résolution R = λ/Δλ", "Resolving power R = λ/Δλ", "Auflösungsvermögen R = λ/Δλ"), "λ (nm)", (a, b), (0.0, er), &[courbe(&s.resolution, COL_DOS_GREEN)]);
            });
        });
    }
}
