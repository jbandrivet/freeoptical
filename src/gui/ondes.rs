//! Vues graphiques des analyses ondulatoires : carte de front d'onde et Zernike, PSF, FTM.

use super::*;
use crate::core::onde::{ftm_ideale_circulaire, nom_zernike};
use eframe::egui::{ColorImage, TextureHandle, TextureOptions};

/// Couleurs des champs dans les courbes de FTM
const COULEURS_CHAMPS: [Color32; 6] = [
    COL_DOS_CYAN,
    COL_DOS_GREEN,
    COL_DOS_ORANGE,
    COL_DOS_MAGENTA,
    COL_DOS_YELLOW,
    COL_DOS_RED,
];

/// Images des analyses ondulatoires, reconstruites quand les résultats changent
#[derive(Default)]
pub struct TexturesOndes {
    front: Option<(u64, TextureHandle)>,
    psf: Option<(u64, bool, usize, TextureHandle)>,
}

/// Palette continue bleu → cyan → vert → jaune → rouge, `t` dans [0, 1]
fn palette(t: f64) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let (r, g, b) = if t < 0.25 {
        (0.0, 4.0 * t, 1.0)
    } else if t < 0.5 {
        (0.0, 1.0, 1.0 - 4.0 * (t - 0.25))
    } else if t < 0.75 {
        (4.0 * (t - 0.5), 1.0, 0.0)
    } else {
        (1.0, 1.0 - 4.0 * (t - 0.75), 0.0)
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Palette de la PSF : même dégradé, assombri vers le noir pour les faibles intensités
fn palette_psf(t: f64) -> Color32 {
    let c = palette(t);
    let b = (t * 5.0).clamp(0.0, 1.0) as f32;
    Color32::from_rgb((c.r() as f32 * b) as u8, (c.g() as f32 * b) as u8, (c.b() as f32 * b) as u8)
}

pub(super) fn libelle_champ(u: &str, c: (f64, f64)) -> String {
    if c.0.abs() < 1e-12 {
        format!("{:.2}{u}", c.1)
    } else {
        format!("({:.2}{u}, {:.2}{u})", c.0, c.1)
    }
}

impl FreeOpticalGuiApp {
    /// Sélecteurs de champ et de longueur d'onde communs aux trois analyses
    pub(super) fn selecteurs_ondes(&mut self, ui: &mut egui::Ui, poly_permis: bool) {
        let sys = &self.systeme;
        let mut champ = self.ondes.champ.min(sys.champs_deg.len().saturating_sub(1));
        let mut lambda = self.ondes.lambda;
        ui.horizontal_wrapped(|ui| {
            if !sys.champs_deg.is_empty() {
                ui.label(RichText::new(tr!("Champ :", "Field:", "Feld:")).monospace());
                egui::ComboBox::from_id_salt("choix_champ_ondes")
                    .selected_text(libelle_champ(sys.unite_champ(), sys.champs_deg[champ]))
                    .show_ui(ui, |ui| {
                        for (i, &c) in sys.champs_deg.iter().enumerate() {
                            ui.selectable_value(&mut champ, i, libelle_champ(sys.unite_champ(), c));
                        }
                    });
            }
            ui.label(RichText::new(tr!("Longueur d'onde :", "Wavelength:", "Wellenlänge:")).monospace());
            let libelle_aucune = if poly_permis {
                tr!("Polychromatique", "Polychromatic", "Polychromatisch").to_string()
            } else {
                trf!("Référence ({:.1} nm)", "Reference ({:.1} nm)", "Referenz ({:.1} nm)", sys.longueur_onde_ref_nm)
            };
            let selection = match lambda.and_then(|i| sys.longueurs_onde.get(i)) {
                Some(w) => format!("{:.1} nm", w.valeur_nm),
                None => libelle_aucune.clone(),
            };
            egui::ComboBox::from_id_salt("choix_lambda_ondes")
                .selected_text(selection)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut lambda, None, libelle_aucune);
                    for (i, w) in sys.longueurs_onde.iter().enumerate() {
                        ui.selectable_value(&mut lambda, Some(i), format!("{:.1} nm  {}", w.valeur_nm, w.etiquette));
                    }
                });
        });
        if champ != self.ondes.champ || lambda != self.ondes.lambda {
            self.ondes.champ = champ;
            self.ondes.lambda = lambda;
            self.ondes.invalider();
        }
    }

    pub(super) fn afficher_erreur_onde(ui: &mut egui::Ui, e: &str) {
        ui.add_space(20.0);
        ui.label(
            RichText::new(trf!("Calcul impossible : {}", "Cannot compute: {}", "Berechnung unmöglich: {}", e))
                .monospace()
                .color(COL_DOS_RED),
        );
    }

    pub(super) fn dessiner_front_onde(&mut self, ui: &mut egui::Ui) {
        let unite = self.systeme.unite_champ();
        self.selecteurs_ondes(ui, false);
        let generation = self.ondes.generation;
        let carte = match self.ondes.front_onde(&self.systeme) {
            Ok(c) => c.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        let valides: Vec<f64> = carte.valeurs_ondes.iter().flatten().copied().collect();
        let mini = valides.iter().copied().fold(f64::INFINITY, f64::min);
        let maxi = valides.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let etendue = (maxi - mini).max(1e-9);

        if self.textures_ondes.front.as_ref().is_none_or(|t| t.0 != generation) {
            let n = carte.taille;
            let mut image = ColorImage::new([n, n], vec![Color32::BLACK; n * n]);
            for j in 0..n {
                for i in 0..n {
                    if let Some(w) = carte.valeurs_ondes[j * n + i] {
                        image.pixels[(n - 1 - j) * n + i] = palette((w - mini) / etendue);
                    }
                }
            }
            let tex = ui.ctx().load_texture("carte_front_onde", image, TextureOptions::NEAREST);
            self.textures_ondes.front = Some((generation, tex));
        }
        let tex = self.textures_ondes.front.as_ref().map(|t| t.1.clone()).unwrap();

        ui.horizontal_top(|ui| {
            let hauteur = ui.available_height().min(ui.available_width() * 0.55).max(120.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(hauteur + 90.0, hauteur), Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
            let carre = egui::Rect::from_min_size(rect.min + Vec2::new(4.0, 4.0), Vec2::splat(hauteur - 8.0));
            painter.image(tex.id(), carre, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            painter.circle_stroke(carre.center(), carre.width() * 0.5, Stroke::new(1.0, COL_DOS_GRAY));
            // Échelle de couleurs
            let barre = egui::Rect::from_min_size(
                Pos2::new(carre.right() + 14.0, carre.top() + 10.0),
                Vec2::new(16.0, carre.height() - 20.0),
            );
            let nb = 64;
            for k in 0..nb {
                let t0 = k as f32 / nb as f32;
                let y1 = barre.bottom() - t0 * barre.height();
                let y0 = barre.bottom() - (t0 + 1.0 / nb as f32) * barre.height();
                painter.rect_filled(
                    egui::Rect::from_min_max(Pos2::new(barre.left(), y0), Pos2::new(barre.right(), y1 + 0.5)),
                    0.0,
                    palette(t0 as f64),
                );
            }
            let police = FontId::new(11.0, FontFamily::Monospace);
            painter.text(Pos2::new(barre.right() + 4.0, barre.top()), Align2::LEFT_CENTER, format!("{maxi:+.3}"), police.clone(), COL_DOS_WHITE);
            painter.text(Pos2::new(barre.right() + 4.0, barre.bottom()), Align2::LEFT_CENTER, format!("{mini:+.3}"), police.clone(), COL_DOS_WHITE);
            painter.text(Pos2::new(barre.center().x, barre.bottom() + 12.0), Align2::CENTER_TOP, tr!("ondes", "waves", "Wellen"), police, COL_DOS_GRAY);

            ui.vertical(|ui| {
                let ligne = |ui: &mut egui::Ui, libelle: &str, valeur: String, couleur: Color32| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(libelle).monospace());
                        ui.label(RichText::new(valeur).monospace().strong().color(couleur));
                    });
                };
                ligne(ui, tr!("Champ           :", "Field           :", "Feld            :"), libelle_champ(unite, carte.champ_deg), COL_DOS_WHITE);
                ligne(ui, tr!("Longueur d'onde :", "Wavelength      :", "Wellenlänge     :"), format!("{:.1} nm", carte.longueur_onde_nm), COL_DOS_WHITE);
                ligne(ui, tr!("Crête-à-creux   :", "Peak-to-valley  :", "Spitze-Tal      :"), format!("{:.4} λ", carte.pv_ondes), COL_DOS_YELLOW);
                ligne(ui, tr!("Écart RMS       :", "RMS             :", "RMS             :"), format!("{:.4} λ", carte.rms_ondes), COL_DOS_YELLOW);
                let strehl_ok = carte.strehl_marechal >= 0.8;
                ligne(
                    ui,
                    tr!("Strehl (Maréchal):", "Strehl (Maréchal):", "Strehl (Maréchal):"),
                    format!("{:.4}", carte.strehl_marechal),
                    if strehl_ok { COL_DOS_GREEN } else { COL_DOS_ORANGE },
                );
                ui.label(
                    RichText::new(if strehl_ok {
                        tr!("[OK] Critère de Maréchal respecté (Strehl ≥ 0,8)", "[OK] Maréchal criterion met (Strehl ≥ 0.8)", "[OK] Maréchal-Kriterium erfüllt (Strehl ≥ 0,8)")
                    } else {
                        tr!("[!] Critère de Maréchal non atteint (Strehl < 0,8)", "[!] Maréchal criterion not met (Strehl < 0.8)", "[!] Maréchal-Kriterium nicht erfüllt (Strehl < 0,8)")
                    })
                    .monospace()
                    .color(if strehl_ok { COL_DOS_GREEN } else { COL_DOS_ORANGE }),
                );
                ui.label(
                    RichText::new(trf!(
                        "{} rayons — OPD rapportée à la sphère de référence (rayon principal, pupille de sortie)",
                        "{} rays — OPD referred to the reference sphere (chief ray, exit pupil)",
                        "{} Strahlen — OPD bezogen auf die Referenzkugel (Hauptstrahl, Austrittspupille)",
                        carte.nb_points
                    ))
                    .monospace()
                    .color(COL_DOS_GRAY),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(trf!(
                        "COEFFICIENTS DE ZERNIKE (Noll, ondes) — résidu {:.1e} λ",
                        "ZERNIKE COEFFICIENTS (Noll, waves) — residual {:.1e} λ",
                        "ZERNIKE-KOEFFIZIENTEN (Noll, Wellen) — Rest {:.1e} λ",
                        carte.residu_zernike_ondes
                    ))
                    .monospace()
                    .strong()
                    .color(COL_DOS_YELLOW),
                );
                egui::ScrollArea::vertical().id_salt("zernike").show(ui, |ui| {
                    egui::Grid::new("grille_zernike").striped(true).spacing([14.0, 2.0]).show(ui, |ui| {
                        for (k, c) in carte.zernike_ondes.iter().enumerate() {
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

    pub(super) fn dessiner_psf(&mut self, ui: &mut egui::Ui) {
        self.selecteurs_ondes(ui, true);
        let mut log = self.psf_log;
        let mut zoom = self.psf_zoom;
        ui.horizontal(|ui| {
            ui.checkbox(&mut log, tr!("Échelle logarithmique (4 décades)", "Logarithmic scale (4 decades)", "Logarithmische Skala (4 Dekaden)"));
            ui.label(RichText::new(tr!("Demi-largeur :", "Half width:", "Halbe Breite:")).monospace());
            ui.add(egui::Slider::new(&mut zoom, 2.0..=30.0).suffix(tr!(" × r Airy", " × Airy r", " × Airy-r")));
        });
        self.psf_log = log;
        self.psf_zoom = zoom;
        let generation = self.ondes.generation;
        let (psf, _) = match self.ondes.psf(&self.systeme) {
            Ok(r) => r.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        ui.label(
            RichText::new(trf!(
                "Strehl = {:.4}   |   r Airy = {:.2} µm   |   F/# de travail = {:.2}   |   pas = {:.3} µm   |   λ = {} nm",
                "Strehl = {:.4}   |   Airy r = {:.2} µm   |   working F/# = {:.2}   |   step = {:.3} µm   |   λ = {} nm",
                "Strehl = {:.4}   |   Airy-r = {:.2} µm   |   Arbeitsblende F/# = {:.2}   |   Schritt = {:.3} µm   |   λ = {} nm",
                psf.strehl,
                psf.rayon_airy_um,
                psf.f_travail,
                psf.pas_um,
                psf.longueurs_onde_nm.iter().map(|l| format!("{l:.1}")).collect::<Vec<_>>().join(" / ")
            ))
            .monospace()
            .strong()
            .color(COL_DOS_YELLOW),
        );

        let m = psf.taille;
        let demi = ((zoom as f64 * psf.rayon_airy_um / psf.pas_um).ceil() as usize).clamp(4, m / 2);
        let a_refaire = self
            .textures_ondes
            .psf
            .as_ref()
            .is_none_or(|t| t.0 != generation || t.1 != log || t.2 != demi);
        if a_refaire {
            let cote = 2 * demi;
            let pic = psf.intensite.iter().copied().fold(1e-300, f64::max);
            let mut image = ColorImage::new([cote, cote], vec![Color32::BLACK; cote * cote]);
            for r in 0..cote {
                for c in 0..cote {
                    let j = m / 2 + demi - 1 - r;
                    let i = m / 2 - demi + c;
                    let v = psf.intensite[j * m + i] / pic;
                    let t = if log { (v.max(1e-12).log10() + 4.0) / 4.0 } else { v };
                    image.pixels[r * cote + c] = palette_psf(t);
                }
            }
            let tex = ui.ctx().load_texture("psf", image, TextureOptions::LINEAR);
            self.textures_ondes.psf = Some((generation, log, demi, tex));
        }
        let tex = self.textures_ondes.psf.as_ref().map(|t| t.3.clone()).unwrap();

        let dispo = ui.available_size();
        let (resp, painter) = ui.allocate_painter(dispo, Sense::hover());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
        let cote = (rect.height().min(rect.width()) - 40.0).max(60.0);
        let carre = egui::Rect::from_center_size(rect.center(), Vec2::splat(cote));
        painter.image(tex.id(), carre, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        let demi_um = demi as f64 * psf.pas_um;
        let r_airy = (psf.rayon_airy_um / demi_um) as f32 * cote * 0.5;
        painter.circle_stroke(carre.center(), r_airy, Stroke::new(1.0, COL_DOS_WHITE));
        let police = FontId::new(11.0, FontFamily::Monospace);
        painter.text(carre.left_bottom() + Vec2::new(0.0, 4.0), Align2::LEFT_TOP, format!("-{demi_um:.1} µm"), police.clone(), COL_DOS_GRAY);
        painter.text(carre.right_bottom() + Vec2::new(0.0, 4.0), Align2::RIGHT_TOP, format!("+{demi_um:.1} µm"), police.clone(), COL_DOS_GRAY);
        painter.text(
            carre.center_top() - Vec2::new(0.0, 4.0),
            Align2::CENTER_BOTTOM,
            tr!("cercle blanc : premier anneau noir d'Airy", "white circle: first dark Airy ring", "weißer Kreis: erster dunkler Airy-Ring"),
            police,
            COL_DOS_WHITE,
        );
    }

    pub(super) fn dessiner_ftm(&mut self, ui: &mut egui::Ui) {
        let unite = self.systeme.unite_champ();
        self.selecteurs_ondes(ui, true);
        let champs = self.systeme.champs_deg.clone();
        let resultats = self.ondes.ftm_champs(&self.systeme).clone();
        let Some(reference) = resultats.iter().flatten().next().cloned() else {
            let e = resultats.iter().find_map(|r| r.as_ref().err()).cloned().unwrap_or_default();
            Self::afficher_erreur_onde(ui, &e);
            return;
        };
        let f_max = reference.frequence_coupure_cyc_mm;
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(trf!(
                    "Coupure {:.1} cycles/mm   —   trait plein : tangentielle, tirets : sagittale, blanc : limite de diffraction",
                    "Cut-off {:.1} cycles/mm   —   solid: tangential, dashed: sagittal, white: diffraction limit",
                    "Grenzfrequenz {:.1} Perioden/mm   —   durchgezogen: tangential, gestrichelt: sagittal, weiß: Beugungsgrenze",
                    f_max
                ))
                .monospace()
                .strong()
                .color(COL_DOS_YELLOW),
            );
        });
        ui.horizontal_wrapped(|ui| {
            for (i, r) in resultats.iter().enumerate() {
                let c = COULEURS_CHAMPS[i % COULEURS_CHAMPS.len()];
                let texte = match r {
                    Ok(f) => {
                        let k = f.frequences_cyc_mm.len() / 2;
                        trf!(
                            "■ {} : T {:.2} / S {:.2} à {:.0} c/mm",
                            "■ {}: T {:.2} / S {:.2} at {:.0} c/mm",
                            "■ {}: T {:.2} / S {:.2} bei {:.0} P/mm",
                            libelle_champ(unite, f.champ_deg),
                            f.tangentielle[k],
                            f.sagittale[k],
                            f.frequences_cyc_mm[k]
                        )
                    }
                    Err(e) => format!("■ {} : {}", libelle_champ(unite, champs.get(i).copied().unwrap_or_default()), e),
                };
                ui.label(RichText::new(texte).monospace().color(c));
            }
        });

        let dispo = ui.available_size();
        let (resp, painter) = ui.allocate_painter(dispo, Sense::hover());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
        let zone = egui::Rect::from_min_max(rect.min + Vec2::new(52.0, 14.0), rect.max - Vec2::new(18.0, 34.0));
        let vers = |f: f64, m: f64| {
            Pos2::new(
                zone.left() + (f / f_max) as f32 * zone.width(),
                zone.bottom() - m.clamp(0.0, 1.0) as f32 * zone.height(),
            )
        };
        let police = FontId::new(11.0, FontFamily::Monospace);
        for k in 0..=5 {
            let m = k as f64 / 5.0;
            tracer_segment(&painter, vers(0.0, m), vers(f_max, m), 0.6, Color32::from_rgb(45, 45, 70));
            painter.text(vers(0.0, m) - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, format!("{m:.1}"), police.clone(), COL_DOS_GRAY);
            let f = f_max * k as f64 / 5.0;
            tracer_segment(&painter, vers(f, 0.0), vers(f, 1.0), 0.6, Color32::from_rgb(45, 45, 70));
            painter.text(vers(f, 0.0) + Vec2::new(0.0, 6.0), Align2::CENTER_TOP, format!("{f:.0}"), police.clone(), COL_DOS_GRAY);
        }
        painter.text(
            Pos2::new(zone.center().x, rect.bottom() - 4.0),
            Align2::CENTER_BOTTOM,
            tr!("fréquence spatiale (cycles/mm)", "spatial frequency (cycles/mm)", "Ortsfrequenz (Perioden/mm)"),
            police,
            COL_DOS_GRAY,
        );

        // Limite de diffraction de la pupille réelle, et pupille circulaire pleine en pointillé gris
        let ideale: Vec<Pos2> = (0..=100)
            .map(|k| {
                let nu = k as f64 / 100.0;
                vers(nu * f_max, ftm_ideale_circulaire(nu))
            })
            .collect();
        painter.add(Shape::dashed_line(&ideale, Stroke::new(0.8, COL_DOS_GRAY), 2.0, 4.0));
        let limite: Vec<Pos2> = reference
            .frequences_cyc_mm
            .iter()
            .zip(&reference.limite_diffraction)
            .map(|(&f, &m)| vers(f, m))
            .collect();
        tracer_polyligne(&painter, &limite, 1.4, COL_DOS_WHITE);
        for (i, r) in resultats.iter().enumerate() {
            let Ok(f) = r else { continue };
            let c = COULEURS_CHAMPS[i % COULEURS_CHAMPS.len()];
            let t: Vec<Pos2> = f.frequences_cyc_mm.iter().zip(&f.tangentielle).map(|(&x, &m)| vers(x, m)).collect();
            let s: Vec<Pos2> = f.frequences_cyc_mm.iter().zip(&f.sagittale).map(|(&x, &m)| vers(x, m)).collect();
            tracer_polyligne(&painter, &t, 1.6, c);
            painter.add(Shape::dashed_line(&s, Stroke::new(1.6, c), 6.0, 4.0));
        }
    }
}
