//! Vues terminal des analyses ondulatoires : front d'onde et Zernike, PSF, FTM.

use crate::core::onde::{ftm_ideale_circulaire, nom_zernike};
use crate::ui::{
    app::AppState,
    theme::{DOS_BG, DOS_CYAN, DOS_GREEN, DOS_RED, DOS_TEXT, DOS_WHITE, DOS_YELLOW, bloc_dos},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph},
};

const COULEURS_CHAMPS: [Color; 6] = [
    DOS_CYAN,
    DOS_GREEN,
    Color::Rgb(255, 175, 45),
    Color::Rgb(255, 105, 255),
    DOS_YELLOW,
    DOS_RED,
];

fn libelle_champ(c: (f64, f64)) -> String {
    if c.0.abs() < 1e-12 {
        format!("{:.2}°", c.1)
    } else {
        format!("({:.2}°, {:.2}°)", c.0, c.1)
    }
}

fn ligne(libelle: &str, valeur: String, couleur: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(libelle.to_string(), Style::default().fg(DOS_TEXT)),
        Span::styled(valeur, Style::default().fg(couleur).add_modifier(Modifier::BOLD)),
    ])
}

fn aide_selection(app: &AppState, poly: bool) -> Line<'static> {
    let lambda = match app.ondes.lambda_nm(&app.systeme) {
        Some(l) => format!("{l:.1} nm"),
        None if poly => tr!("polychromatique", "polychromatic", "polychromatisch").to_string(),
        None => format!("{:.1} nm", app.systeme.longueur_onde_ref_nm),
    };
    Line::from(Span::styled(
        trf!(
            "Champ {} [C]  |  λ {} [W]",
            "Field {} [C]  |  λ {} [W]",
            "Feld {} [C]  |  λ {} [W]",
            libelle_champ(app.ondes.champ_deg(&app.systeme)),
            lambda
        ),
        Style::default().fg(DOS_CYAN),
    ))
}

fn erreur(f: &mut Frame, zone: Rect, e: &str) {
    let p = Paragraph::new(Line::from(Span::styled(
        trf!("Calcul impossible : {}", "Cannot compute: {}", "Berechnung unmöglich: {}", e),
        Style::default().fg(DOS_RED),
    )));
    f.render_widget(p, zone);
}

/// Statistiques du front d'onde et coefficients de Zernike
pub fn dessiner_front_onde(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.front_calcule() else { return };
    let carte = match resultat {
        Ok(c) => c,
        Err(e) => return erreur(f, zone, e),
    };
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(zone);

    let strehl_ok = carte.strehl_marechal >= 0.8;
    let stats = vec![
        aide_selection(app, false),
        Line::from(""),
        ligne(tr!("Crête-à-creux   : ", "Peak-to-valley  : ", "Spitze-Tal      : "), format!("{:.4} λ", carte.pv_ondes), DOS_YELLOW),
        ligne(tr!("Écart RMS       : ", "RMS             : ", "RMS             : "), format!("{:.4} λ", carte.rms_ondes), DOS_YELLOW),
        ligne(
            tr!("Strehl Maréchal : ", "Strehl Maréchal : ", "Strehl Maréchal : "),
            format!("{:.4}", carte.strehl_marechal),
            if strehl_ok { DOS_GREEN } else { DOS_RED },
        ),
        ligne(tr!("Rayons          : ", "Rays            : ", "Strahlen        : "), carte.nb_points.to_string(), DOS_WHITE),
        Line::from(""),
        Line::from(Span::styled(
            if strehl_ok {
                tr!("Critère de Maréchal respecté", "Maréchal criterion met", "Maréchal-Kriterium erfüllt")
            } else {
                tr!("Critère de Maréchal non atteint", "Maréchal criterion not met", "Maréchal-Kriterium nicht erfüllt")
            },
            Style::default().fg(if strehl_ok { DOS_GREEN } else { DOS_RED }),
        )),
    ];
    f.render_widget(
        Paragraph::new(stats).block(bloc_dos(tr!("FRONT D'ONDE (OPD)", "WAVEFRONT (OPD)", "WELLENFRONT (OPD)"), false)),
        decoupe[0],
    );

    let hauteur = decoupe[1].height.saturating_sub(2) as usize;
    let mut lignes = Vec::new();
    let nb = carte.zernike_ondes.len();
    let par_colonne = hauteur.max(1);
    for r in 0..par_colonne.min(nb) {
        let mut spans = Vec::new();
        for col in 0..2 {
            let k = col * par_colonne + r;
            if k >= nb {
                continue;
            }
            let c = carte.zernike_ondes[k];
            spans.push(Span::styled(format!("Z{:<2} ", k + 1), Style::default().fg(DOS_CYAN)));
            spans.push(Span::styled(
                format!("{:+8.4} ", c),
                Style::default().fg(if c.abs() >= 0.05 { DOS_YELLOW } else { DOS_TEXT }),
            ));
            spans.push(Span::styled(format!("{:<26}", nom_zernike(k + 1)), Style::default().fg(DOS_WHITE)));
        }
        lignes.push(Line::from(spans));
    }
    f.render_widget(
        Paragraph::new(lignes).block(bloc_dos(tr!("ZERNIKE (Noll, ondes)", "ZERNIKE (Noll, waves)", "ZERNIKE (Noll, Wellen)"), false)),
        decoupe[1],
    );
}

/// PSF en caractères, de plus en plus denses avec l'intensité
pub fn dessiner_psf(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.psf_calculee() else { return };
    let psf = match resultat {
        Ok((p, _)) => p,
        Err(e) => return erreur(f, zone, e),
    };
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(42)])
        .split(zone);

    let h = decoupe[0].height.saturating_sub(2).max(4) as usize;
    let w = (2 * h).min(decoupe[0].width.saturating_sub(2) as usize).max(8);
    let m = psf.taille;
    let demi = ((8.0 * psf.rayon_airy_um / psf.pas_um).ceil() as usize).clamp(4, m / 2) as f64;
    let pic = psf.intensite.iter().copied().fold(1e-300, f64::max);
    let niveaux: Vec<char> = " .:-=+*#%@".chars().collect();
    let mut lignes = Vec::with_capacity(h);
    for r in 0..h {
        let y = demi - (r as f64 + 0.5) * 2.0 * demi / h as f64;
        let mut spans = Vec::with_capacity(w);
        for c in 0..w {
            let x = -demi + (c as f64 + 0.5) * 2.0 * demi / w as f64;
            let i = ((m / 2) as f64 + x).floor().clamp(0.0, (m - 1) as f64) as usize;
            let j = ((m / 2) as f64 + y).floor().clamp(0.0, (m - 1) as f64) as usize;
            let v = psf.intensite[j * m + i] / pic;
            let t = ((v.max(1e-12).log10() + 3.0) / 3.0).clamp(0.0, 1.0);
            let k = (t * (niveaux.len() - 1) as f64).round() as usize;
            let couleur = if t > 0.8 { DOS_YELLOW } else if t > 0.5 { DOS_GREEN } else { DOS_CYAN };
            spans.push(Span::styled(niveaux[k].to_string(), Style::default().fg(couleur).bg(DOS_BG)));
        }
        lignes.push(Line::from(spans));
    }
    f.render_widget(
        Paragraph::new(lignes)
            .alignment(Alignment::Center)
            .block(bloc_dos(tr!("PSF (log, 3 décades)", "PSF (log, 3 decades)", "PSF (log, 3 Dekaden)"), false)),
        decoupe[0],
    );

    let infos = vec![
        aide_selection(app, true),
        Line::from(""),
        ligne("Strehl          : ", format!("{:.4}", psf.strehl), if psf.strehl >= 0.8 { DOS_GREEN } else { DOS_RED }),
        ligne(tr!("Rayon d'Airy    : ", "Airy radius     : ", "Airy-Radius     : "), format!("{:.2} µm", psf.rayon_airy_um), DOS_WHITE),
        ligne(tr!("F/# de travail  : ", "Working F/#     : ", "Arbeits-F/#     : "), format!("{:.2}", psf.f_travail), DOS_WHITE),
        ligne(tr!("Demi-largeur    : ", "Half width      : ", "Halbe Breite    : "), format!("{:.1} µm", demi * psf.pas_um), DOS_WHITE),
    ];
    f.render_widget(Paragraph::new(infos).block(bloc_dos(tr!("MÉTRIQUES PSF", "PSF METRICS", "PSF-KENNWERTE"), false)), decoupe[1]);
}

/// Courbes de FTM tangentielle et sagittale de tous les champs
pub fn dessiner_ftm(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultats) = app.ondes.ftm_calculees() else { return };
    let Some(premier) = resultats.iter().flatten().next() else {
        let e = resultats.iter().find_map(|r| r.as_ref().err()).cloned().unwrap_or_default();
        return erreur(f, zone, &e);
    };
    let f_max = premier.frequence_coupure_cyc_mm;
    let ideale: Vec<(f64, f64)> = (0..=60).map(|k| {
        let nu = k as f64 / 60.0;
        (nu * f_max, ftm_ideale_circulaire(nu))
    }).collect();
    let courbes: Vec<(Vec<(f64, f64)>, Vec<(f64, f64)>, String)> = resultats
        .iter()
        .flatten()
        .map(|r| {
            let t = r.frequences_cyc_mm.iter().copied().zip(r.tangentielle.iter().copied()).collect();
            let s = r.frequences_cyc_mm.iter().copied().zip(r.sagittale.iter().copied()).collect();
            (t, s, libelle_champ(r.champ_deg))
        })
        .collect();
    let mut series = vec![
        Dataset::default()
            .name(tr!("diffraction", "diffraction", "Beugung"))
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(DOS_WHITE))
            .data(&ideale),
    ];
    for (i, (t, s, nom)) in courbes.iter().enumerate() {
        let c = COULEURS_CHAMPS[i % COULEURS_CHAMPS.len()];
        series.push(
            Dataset::default()
                .name(format!("T {nom}"))
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(c))
                .data(t),
        );
        series.push(
            Dataset::default()
                .name(format!("S {nom}"))
                .marker(Marker::Dot)
                .graph_type(GraphType::Scatter)
                .style(Style::default().fg(c))
                .data(s),
        );
    }
    let etiquettes_x: Vec<Span> = (0..=4).map(|k| Span::raw(format!("{:.0}", f_max * k as f64 / 4.0))).collect();
    let titre = trf!(
        "FTM — {} (T trait, S points) — coupure {:.0} c/mm",
        "MTF — {} (T line, S dots) — cut-off {:.0} c/mm",
        "MTF — {} (T Linie, S Punkte) — Grenze {:.0} P/mm",
        match app.ondes.lambda_nm(&app.systeme) {
            Some(l) => format!("{l:.1} nm"),
            None => tr!("polychromatique [W]", "polychromatic [W]", "polychromatisch [W]").to_string(),
        },
        f_max
    );
    let graphe = Chart::new(series)
        .block(bloc_dos(&titre, false))
        .x_axis(
            Axis::default()
                .title(tr!("cycles/mm", "cycles/mm", "Perioden/mm"))
                .style(Style::default().fg(DOS_TEXT))
                .bounds([0.0, f_max])
                .labels(etiquettes_x),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(DOS_TEXT))
                .bounds([0.0, 1.0])
                .labels(vec![Span::raw("0"), Span::raw("0.5"), Span::raw("1")]),
        );
    f.render_widget(graphe, zone);
}
