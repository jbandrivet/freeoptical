//! Vues graphiques des aberrations géométriques : courbes de rayons, courbure de champ et
//! distorsion, coefficients de Seidel.

use super::ondes::libelle_champ;
use super::*;

/// Couleur d'une longueur d'onde (bleu, vert, rouge comme les autres vues)
fn couleur_lambda(l: f64) -> Color32 {
    if l < 520.0 {
        COL_DOS_CYAN
    } else if l <= 610.0 {
        COL_DOS_GREEN
    } else {
        COL_DOS_RED
    }
}

/// Une courbe : points (x, y), couleur, en tirets ou non
struct Courbe {
    points: Vec<(f64, f64)>,
    couleur: Color32,
    tirets: bool,
}

/// Graphe à axes : `x` de `x_min` à `x_max`, `y` de `y_min` à `y_max`, avec titre et unités
#[allow(clippy::too_many_arguments)]
fn graphe(
    ui: &mut egui::Ui,
    taille: Vec2,
    titre: &str,
    etiquette_x: &str,
    (x_min, x_max): (f64, f64),
    (y_min, y_max): (f64, f64),
    courbes: &[Courbe],
) {
    let (rect, _) = ui.allocate_exact_size(taille, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
    let zone = egui::Rect::from_min_max(rect.min + Vec2::new(58.0, 22.0), rect.max - Vec2::new(12.0, 30.0));
    let vers = |x: f64, y: f64| {
        Pos2::new(
            zone.left() + ((x - x_min) / (x_max - x_min)) as f32 * zone.width(),
            zone.bottom() - ((y - y_min) / (y_max - y_min)) as f32 * zone.height(),
        )
    };
    let police = FontId::new(10.5, FontFamily::Monospace);
    painter.text(rect.center_top() + Vec2::new(0.0, 4.0), Align2::CENTER_TOP, titre, FontId::new(11.5, FontFamily::Monospace), COL_DOS_YELLOW);
    for k in 0..=4 {
        let x = x_min + (x_max - x_min) * k as f64 / 4.0;
        let y = y_min + (y_max - y_min) * k as f64 / 4.0;
        tracer_segment(&painter, vers(x, y_min), vers(x, y_max), 0.5, Color32::from_rgb(45, 45, 70));
        tracer_segment(&painter, vers(x_min, y), vers(x_max, y), 0.5, Color32::from_rgb(45, 45, 70));
        painter.text(vers(x, y_min) + Vec2::new(0.0, 4.0), Align2::CENTER_TOP, format!("{x:.3}"), police.clone(), COL_DOS_GRAY);
        painter.text(vers(x_min, y) - Vec2::new(5.0, 0.0), Align2::RIGHT_CENTER, format!("{y:.3}"), police.clone(), COL_DOS_GRAY);
    }
    if x_min < 0.0 && x_max > 0.0 {
        tracer_segment(&painter, vers(0.0, y_min), vers(0.0, y_max), 1.0, COL_DOS_GRAY);
    }
    if y_min < 0.0 && y_max > 0.0 {
        tracer_segment(&painter, vers(x_min, 0.0), vers(x_max, 0.0), 1.0, COL_DOS_GRAY);
    }
    painter.text(rect.center_bottom() - Vec2::new(0.0, 2.0), Align2::CENTER_BOTTOM, etiquette_x, police, COL_DOS_GRAY);
    for c in courbes {
        let pts: Vec<Pos2> = c.points.iter().map(|&(x, y)| vers(x, y)).collect();
        if pts.len() < 2 {
            continue;
        }
        if c.tirets {
            painter.add(Shape::dashed_line(&pts, Stroke::new(1.5, c.couleur), 6.0, 4.0));
        } else {
            tracer_polyligne(&painter, &pts, 1.5, c.couleur);
        }
    }
}

/// Échelle symétrique arrondie couvrant toutes les valeurs
fn echelle(valeurs: impl Iterator<Item = f64>, minimum: f64) -> f64 {
    let m = valeurs.fold(0.0_f64, |a, v| a.max(v.abs())).max(minimum) * 1.1;
    let p = 10f64.powf(m.log10().floor());
    (m / p).ceil() * p
}

impl FreeOpticalGuiApp {
    pub(super) fn dessiner_courbes_rayons(&mut self, ui: &mut egui::Ui) {
        self.selecteurs_ondes(ui, true);
        let c = match self.ondes.courbes(&self.systeme) {
            Ok(c) => c.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        ui.label(
            RichText::new(trf!(
                "Champ {} — aberrations transverses par rapport au rayon principal ({:.1} nm) et différence de marche, par longueur d'onde",
                "Field {} — transverse aberrations relative to the chief ray ({:.1} nm) and optical path difference, per wavelength",
                "Feld {} — Queraberrationen bezogen auf den Hauptstrahl ({:.1} nm) und optische Wegdifferenz, je Wellenlänge",
                libelle_champ(c.champ_deg),
                self.systeme.longueur_onde_ref_nm
            ))
            .monospace()
            .color(COL_DOS_YELLOW),
        );
        let serie = |valeurs: &Vec<Vec<Option<f64>>>| -> Vec<Courbe> {
            valeurs
                .iter()
                .zip(&c.longueurs_onde_nm)
                .map(|(v, &l)| Courbe {
                    points: c.pupille.iter().zip(v).filter_map(|(&p, y)| y.map(|y| (p, y))).collect(),
                    couleur: couleur_lambda(l),
                    tirets: false,
                })
                .collect()
        };
        let e_tr = echelle(c.transverse_t_um.iter().chain(&c.transverse_s_um).flatten().flatten().copied(), 1.0);
        let e_opd = echelle(c.opd_t_ondes.iter().chain(&c.opd_s_ondes).flatten().flatten().copied(), 0.05);
        let dispo = ui.available_size();
        let taille = Vec2::new((dispo.x - 8.0) / 2.0, (dispo.y - 8.0) / 2.0);
        ui.horizontal(|ui| {
            graphe(ui, taille, tr!("ΔY (µm) — coupe tangentielle", "ΔY (µm) — tangential fan", "ΔY (µm) — tangentialer Fächer"), "py", (-1.0, 1.0), (-e_tr, e_tr), &serie(&c.transverse_t_um));
            graphe(ui, taille, tr!("ΔX (µm) — coupe sagittale", "ΔX (µm) — sagittal fan", "ΔX (µm) — sagittaler Fächer"), "px", (-1.0, 1.0), (-e_tr, e_tr), &serie(&c.transverse_s_um));
        });
        ui.horizontal(|ui| {
            graphe(ui, taille, tr!("OPD (ondes) — tangentielle", "OPD (waves) — tangential", "OPD (Wellen) — tangential"), "py", (-1.0, 1.0), (-e_opd, e_opd), &serie(&c.opd_t_ondes));
            graphe(ui, taille, tr!("OPD (ondes) — sagittale", "OPD (waves) — sagittal", "OPD (Wellen) — sagittal"), "px", (-1.0, 1.0), (-e_opd, e_opd), &serie(&c.opd_s_ondes));
        });
    }

    pub(super) fn dessiner_courbure_champ(&mut self, ui: &mut egui::Ui) {
        self.selecteurs_ondes(ui, false);
        let c = match self.ondes.courbure(&self.systeme) {
            Ok(c) => c.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        let k = c.angles_deg.len() - 1;
        ui.label(
            RichText::new(trf!(
                "À {:.2}° : foyer T {:+.4} mm, S {:+.4} mm (astigmatisme {:.4} mm), distorsion {:+.4} %   —   T trait plein, S tirets, comptés depuis le plan image dans le sens de la lumière",
                "At {:.2}°: T focus {:+.4} mm, S {:+.4} mm (astigmatism {:.4} mm), distortion {:+.4} %   —   T solid, S dashed, from the image plane along the light",
                "Bei {:.2}°: Fokus T {:+.4} mm, S {:+.4} mm (Astigmatismus {:.4} mm), Verzeichnung {:+.4} %   —   T durchgezogen, S gestrichelt, ab Bildebene in Lichtrichtung",
                c.angles_deg[k],
                c.foyer_t_mm[k].unwrap_or(f64::NAN),
                c.foyer_s_mm[k].unwrap_or(f64::NAN),
                (c.foyer_t_mm[k].unwrap_or(0.0) - c.foyer_s_mm[k].unwrap_or(0.0)).abs(),
                c.distorsion_pct[k].unwrap_or(f64::NAN)
            ))
            .monospace()
            .color(COL_DOS_YELLOW),
        );
        let theta_max = c.angles_deg[k].max(1e-6);
        let courbe = |v: &Vec<Option<f64>>, couleur, tirets| Courbe {
            points: v.iter().zip(&c.angles_deg).filter_map(|(x, &a)| x.map(|x| (x, a))).collect(),
            couleur,
            tirets,
        };
        let e_f = echelle(c.foyer_t_mm.iter().chain(&c.foyer_s_mm).flatten().copied(), 0.01);
        let e_d = echelle(c.distorsion_pct.iter().flatten().copied(), 0.01);
        let dispo = ui.available_size();
        let taille = Vec2::new((dispo.x - 8.0) / 2.0, dispo.y - 4.0);
        ui.horizontal(|ui| {
            graphe(
                ui,
                taille,
                tr!("Courbure de champ (mm) — angle de champ (°)", "Field curvature (mm) — field angle (°)", "Bildfeldwölbung (mm) — Feldwinkel (°)"),
                "Δz (mm)",
                (-e_f, e_f),
                (0.0, theta_max),
                &[courbe(&c.foyer_t_mm, COL_DOS_CYAN, false), courbe(&c.foyer_s_mm, COL_DOS_CYAN, true)],
            );
            graphe(
                ui,
                taille,
                tr!("Distorsion (%) — angle de champ (°)", "Distortion (%) — field angle (°)", "Verzeichnung (%) — Feldwinkel (°)"),
                "%",
                (-e_d, e_d),
                (0.0, theta_max),
                &[courbe(&c.distorsion_pct, COL_DOS_GREEN, false)],
            );
        });
    }

    pub(super) fn dessiner_seidel(&mut self, ui: &mut egui::Ui) {
        let s = match self.ondes.seidel(&self.systeme) {
            Ok(s) => s.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        ui.label(
            RichText::new(trf!(
                "Coefficients de Seidel (mm) — rayon marginal au bord de la pupille, rayon principal à {:.2}°, λ = {:.1} nm",
                "Seidel coefficients (mm) — marginal ray at the pupil edge, chief ray at {:.2}°, λ = {:.1} nm",
                "Seidel-Koeffizienten (mm) — Randstrahl am Pupillenrand, Hauptstrahl bei {:.2}°, λ = {:.1} nm",
                s.champ_max_deg,
                s.longueur_onde_nm
            ))
            .monospace()
            .color(COL_DOS_YELLOW),
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("grille_seidel").striped(true).spacing([18.0, 3.0]).show(ui, |ui| {
                for h in [
                    "Surf",
                    tr!("S I sphér.", "S I spher.", "S I sphär."),
                    "S II coma",
                    tr!("S III astig.", "S III astig.", "S III Astig."),
                    "S IV Petzval",
                    tr!("S V distor.", "S V distort.", "S V Verz."),
                    tr!("CL chrom. ax.", "CL axial col.", "CL Farblängs."),
                    tr!("CT chrom. lat.", "CT lateral col.", "CT Farbquer."),
                ] {
                    ui.label(RichText::new(h).monospace().strong().color(COL_DOS_YELLOW));
                }
                ui.end_row();
                let ligne = |ui: &mut egui::Ui, nom: String, l: &crate::core::aberrations::LigneSeidel, couleur: Color32| {
                    ui.label(RichText::new(nom).monospace().color(COL_DOS_CYAN));
                    for v in [l.s1, l.s2, l.s3, l.s4, l.s5, l.cl, l.ct] {
                        ui.label(RichText::new(format!("{v:+.6}")).monospace().color(couleur));
                    }
                    ui.end_row();
                };
                for l in &s.lignes {
                    ligne(ui, format!("#{:02}", l.surface), l, COL_DOS_WHITE);
                }
                ligne(ui, tr!("TOTAL", "TOTAL", "SUMME").to_string(), &s.total, COL_DOS_GREEN);
            });
            ui.add_space(8.0);
            ui.label(
                RichText::new(trf!(
                    "Front d'onde équivalent (ondes) : W040 sphérique {:+.4}   W131 coma {:+.4}   W222 astigmatisme {:+.4}   W220 courbure {:+.4}   W311 distorsion {:+.4}",
                    "Equivalent wavefront (waves): W040 spherical {:+.4}   W131 coma {:+.4}   W222 astigmatism {:+.4}   W220 curvature {:+.4}   W311 distortion {:+.4}",
                    "Äquivalente Wellenfront (Wellen): W040 sphärisch {:+.4}   W131 Koma {:+.4}   W222 Astigmatismus {:+.4}   W220 Wölbung {:+.4}   W311 Verzeichnung {:+.4}",
                    s.w040,
                    s.w131,
                    s.w222,
                    s.w220,
                    s.w311
                ))
                .monospace()
                .color(COL_DOS_WHITE),
            );
            ui.label(
                RichText::new(tr!(
                    "Formules de Welford (3e ordre). Les bascules, décentrements et réseaux ne sont pas pris en compte.",
                    "Welford formulas (third order). Tilts, decenters and gratings are not taken into account.",
                    "Formeln nach Welford (3. Ordnung). Kippungen, Dezentrierungen und Gitter werden nicht berücksichtigt."
                ))
                .monospace()
                .color(COL_DOS_GRAY),
            );
        });
    }
}
