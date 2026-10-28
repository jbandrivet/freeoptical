use crate::ui::{
    app::AppState,
    theme::{DOS_BG, DOS_CYAN, DOS_GREEN, DOS_RED, DOS_TEXT, DOS_WHITE, DOS_YELLOW, bloc_dos},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Paragraph,
        canvas::{Canvas, Circle as CanvasCircle, Line as CanvasLine, Points},
    },
};

/// Dessine la vue Spot Diagram (Diagramme de points au plan image + Cercle d'Airy + Métriques RMS)
pub fn dessiner_spot_diagram(f: &mut Frame, zone: Rect, app: &AppState) {
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(66), Constraint::Percentage(34)])
        .split(zone);

    let spot = &app.spot;
    let cx = spot.centroide_mm.0;
    let cy = spot.centroide_mm.1;
    let airy_mm = spot.rayon_airy_um * 1e-3;
    let geo_mm = spot.rayon_geo_um * 1e-3;
    let demi_echelle = (geo_mm.max(airy_mm) * 1.35).max(0.005);

    // Regroupement des impacts par bande spectrale (Bleu F, Jaune/Vert d, Rouge C)
    let mut pts_bleu = Vec::new();
    let mut pts_vert = Vec::new();
    let mut pts_rouge = Vec::new();

    for p in &spot.points {
        let coord = (p.x_mm - cx, p.y_mm - cy);
        if p.longueur_onde_nm < 520.0 {
            pts_bleu.push(coord);
        } else if p.longueur_onde_nm <= 610.0 {
            pts_vert.push(coord);
        } else {
            pts_rouge.push(coord);
        }
    }

    let canvas = Canvas::default()
        .background_color(DOS_BG)
        .marker(Marker::Braille)
        .x_bounds([-demi_echelle, demi_echelle])
        .y_bounds([-demi_echelle, demi_echelle])
        .paint(move |ctx| {
            // Axes de référence centrés sur le centroïde
            ctx.draw(&CanvasLine {
                x1: -demi_echelle,
                y1: 0.0,
                x2: demi_echelle,
                y2: 0.0,
                color: DOS_TEXT,
            });
            ctx.draw(&CanvasLine {
                x1: 0.0,
                y1: -demi_echelle,
                x2: 0.0,
                y2: demi_echelle,
                color: DOS_TEXT,
            });

            // Cercle de diffraction d'Airy en jaune vif
            if airy_mm > 0.0 {
                ctx.draw(&CanvasCircle {
                    x: 0.0,
                    y: 0.0,
                    radius: airy_mm,
                    color: DOS_YELLOW,
                });
            }

            // Nuages d'impacts polychromatiques
            ctx.draw(&Points {
                coords: &pts_bleu,
                color: DOS_CYAN,
            });
            ctx.draw(&Points {
                coords: &pts_rouge,
                color: DOS_RED,
            });
            ctx.draw(&Points {
                coords: &pts_vert,
                color: DOS_GREEN,
            });
        });

    f.render_widget(canvas, decoupe[0]);

    // Panneau de métriques détaillées du Spot Diagram à droite
    let ratio_airy = if spot.rayon_airy_um > 1e-6 {
        spot.rayon_rms_um / spot.rayon_airy_um
    } else {
        0.0
    };

    let texte_stats = vec![
        Line::from(Span::styled(
            tr!("MÉTRIQUES SPOT DIAGRAM", "SPOT DIAGRAM METRICS", "SPOTDIAGRAMM-KENNWERTE"),
            Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
        )),
        Line::from("──────────────────────────────"),
        Line::from(vec![
            Span::styled(tr!("Rayons transmis : ", "Rays transmitted: ", "Durchgel. Strahl: "), Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("{}", spot.nb_rayons_valides),
                Style::default().fg(DOS_WHITE).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(tr!("Centroïde (X,Y) : ", "Centroid (X,Y)  : ", "Schwerpunkt X,Y : "), Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("({:.3}, {:.3}) mm", cx, cy),
                Style::default().fg(DOS_WHITE),
            ),
        ]),
        Line::from(vec![
            Span::styled(tr!("Rayon RMS       : ", "RMS radius      : ", "RMS-Radius      : "), Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("{:.2} µm ({:.4} mm)", spot.rayon_rms_um, spot.rayon_rms_mm),
                Style::default().fg(DOS_GREEN).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(tr!("Rayon GEO Max   : ", "GEO max radius  : ", "GEO max. Radius : "), Style::default().fg(DOS_TEXT)),
            Span::styled(format!("{:.2} µm", spot.rayon_geo_um), Style::default().fg(DOS_CYAN)),
        ]),
        Line::from(vec![
            Span::styled(tr!("Rayon d'Airy    : ", "Airy radius     : ", "Airy-Radius     : "), Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("{:.2} µm", spot.rayon_airy_um),
                Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(tr!("Ratio RMS/Airy  : ", "RMS/Airy ratio  : ", "Verh. RMS/Airy  : "), Style::default().fg(DOS_TEXT)),
            Span::styled(format!("{:.2}x", ratio_airy), Style::default().fg(DOS_WHITE)),
        ]),
        Line::from(vec![
            Span::styled(tr!("Échelle Graphe  : ", "Plot scale      : ", "Diagrammmaßstab : "), Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("±{:.1} µm", demi_echelle * 1000.0),
                Style::default().fg(DOS_TEXT),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            tr!("LÉGENDE SPECTRALE :", "SPECTRAL LEGEND:", "SPEKTRALLEGENDE:"),
            Style::default().fg(DOS_YELLOW),
        )),
        Line::from(Span::styled(tr!(" • Cyan  : Raie F (486.1 nm)", " • Cyan  : F line (486.1 nm)", " • Cyan  : F-Linie (486.1 nm)"), Style::default().fg(DOS_CYAN))),
        Line::from(Span::styled(tr!(" • Vert  : Raie d (587.6 nm)", " • Green : d line (587.6 nm)", " • Grün  : d-Linie (587.6 nm)"), Style::default().fg(DOS_GREEN))),
        Line::from(Span::styled(tr!(" • Rouge : Raie C (656.3 nm)", " • Red   : C line (656.3 nm)", " • Rot   : C-Linie (656.3 nm)"), Style::default().fg(DOS_RED))),
        Line::from(Span::styled(tr!(" ○ Jaune : Disque d'Airy", " ○ Yellow: Airy disk", " ○ Gelb  : Airy-Scheibchen"), Style::default().fg(DOS_YELLOW))),
    ];

    let bloc_stats = bloc_dos(tr!("STATISTIQUES IMAGE", "IMAGE STATISTICS", "BILDSTATISTIK"), false);
    f.render_widget(Paragraph::new(texte_stats).block(bloc_stats), decoupe[1]);
}

/// Dessine la courbe d'Aberration Sphérique Longitudinale (LSA : Hauteur Pupillaire Py en fonction de ΔZ)
pub fn dessiner_aberration_spherique(f: &mut Frame, zone: Rect, app: &AppState) {
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(66), Constraint::Percentage(34)])
        .split(zone);

    let lsa = &app.lsa;
    let max_dz = lsa
        .echantillons
        .iter()
        .map(|s| s.delta_z_mm.abs())
        .fold(0.2_f64, f64::max)
        * 1.25;

    let echantillons_clones = lsa.echantillons.clone();

    let canvas = Canvas::default()
        .background_color(DOS_BG)
        .marker(Marker::Braille)
        .x_bounds([-max_dz, max_dz])
        .y_bounds([0.0, 1.05])
        .paint(move |ctx| {
            // Axe vertical ΔZ = 0 (Plan image) et axe horizontal Py = 0
            ctx.draw(&CanvasLine {
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 1.05,
                color: DOS_YELLOW,
            });
            ctx.draw(&CanvasLine {
                x1: -max_dz,
                y1: 0.0,
                x2: max_dz,
                y2: 0.0,
                color: DOS_TEXT,
            });

            // Regroupement et tracé par longueur d'onde
            for &(lambda_min, lambda_max, couleur) in &[
                (0.0, 520.0, DOS_CYAN),
                (520.0, 610.0, DOS_GREEN),
                (610.0, 2000.0, DOS_RED),
            ] {
                let mut courbe: Vec<(f64, f64)> = echantillons_clones
                    .iter()
                    .filter(|s| s.longueur_onde_nm >= lambda_min && s.longueur_onde_nm < lambda_max)
                    .map(|s| (s.delta_z_mm, s.pupille_py))
                    .collect();
                courbe.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                for fen in courbe.windows(2) {
                    ctx.draw(&CanvasLine {
                        x1: fen[0].0,
                        y1: fen[0].1,
                        x2: fen[1].0,
                        y2: fen[1].1,
                        color: couleur,
                    });
                }
            }
        });

    f.render_widget(canvas, decoupe[0]);

    let mut lignes_table = vec![
        Line::from(Span::styled(
            tr!("ABERRATION SPHÉRIQUE (LSA)", "SPHERICAL ABERRATION (LSA)", "SPHÄRISCHE ABERRATION (LSA)"),
            Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(tr!("LSA Crête-à-Crête : ", "Peak-to-valley LSA: ", "LSA Spitze-Tal    : "), Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("{:.4} mm", lsa.lsa_max_mm),
                Style::default().fg(DOS_GREEN).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(tr!("Échelle ΔZ        : ", "ΔZ scale          : ", "ΔZ-Maßstab        : "), Style::default().fg(DOS_TEXT)),
            Span::styled(format!("±{:.3} mm", max_dz), Style::default().fg(DOS_WHITE)),
        ]),
        Line::from("──────────────────────────────"),
        Line::from(Span::styled(
            tr!("  Pupille Py   │   ΔZ (λ réf) ", "  Pupil Py     │   ΔZ (λ ref) ", "  Pupille Py   │   ΔZ (λ Ref) "),
            Style::default().fg(DOS_CYAN).add_modifier(Modifier::BOLD),
        )),
    ];

    let lambda_ref = app.systeme.longueur_onde_ref_nm;
    for s in lsa
        .echantillons
        .iter()
        .filter(|e| (e.longueur_onde_nm - lambda_ref).abs() < 1.0)
        .step_by(2)
    {
        lignes_table.push(Line::from(vec![
            Span::styled(format!("    {:>5.2}      │ ", s.pupille_py), Style::default().fg(DOS_WHITE)),
            Span::styled(format!("{:>+9.4} mm", s.delta_z_mm), Style::default().fg(DOS_GREEN)),
        ]));
    }

    let bloc_lsa = bloc_dos(tr!("TABLEAU LSA (ΔZ)", "LSA TABLE (ΔZ)", "LSA-TABELLE (ΔZ)"), false);
    f.render_widget(Paragraph::new(lignes_table).block(bloc_lsa), decoupe[1]);
}
