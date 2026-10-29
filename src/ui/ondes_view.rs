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

fn libelle_champ(u: &str, c: (f64, f64)) -> String {
    if c.0.abs() < 1e-12 {
        format!("{:.2}{u}", c.1)
    } else {
        format!("({:.2}{u}, {:.2}{u})", c.0, c.1)
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
            libelle_champ(app.systeme.unite_champ(), app.ondes.champ_deg(&app.systeme)),
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
            (t, s, libelle_champ(app.systeme.unite_champ(), r.champ_deg))
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

fn couleur_lambda(l: f64) -> Color {
    if l < 520.0 {
        DOS_CYAN
    } else if l <= 610.0 {
        DOS_GREEN
    } else {
        DOS_RED
    }
}

fn echelle(valeurs: impl Iterator<Item = f64>, minimum: f64) -> f64 {
    valeurs.fold(0.0_f64, |a, v| a.max(v.abs())).max(minimum) * 1.1
}

/// Graphe terminal d'une famille de courbes
fn graphe<'a>(titre: &'a str, axe_x: &'a str, bx: [f64; 2], by: [f64; 2], series: &'a [(Vec<(f64, f64)>, Color)]) -> Chart<'a> {
    let jeux = series
        .iter()
        .map(|(pts, c)| {
            Dataset::default()
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(*c))
                .data(pts)
        })
        .collect();
    let etiquette = |v: f64| Span::raw(format!("{v:.3}"));
    Chart::new(jeux)
        .block(bloc_dos(titre, false))
        .x_axis(
            Axis::default()
                .title(axe_x)
                .style(Style::default().fg(DOS_TEXT))
                .bounds(bx)
                .labels(vec![etiquette(bx[0]), etiquette(0.5 * (bx[0] + bx[1])), etiquette(bx[1])]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(DOS_TEXT))
                .bounds(by)
                .labels(vec![etiquette(by[0]), etiquette(0.5 * (by[0] + by[1])), etiquette(by[1])]),
        )
}

/// Courbes de rayons tangentielle et sagittale (aberrations transverses)
pub fn dessiner_courbes_rayons(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.courbes_calculees() else { return };
    let c = match resultat {
        Ok(c) => c,
        Err(e) => return erreur(f, zone, e),
    };
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(zone);
    let serie = |v: &Vec<Vec<Option<f64>>>| -> Vec<(Vec<(f64, f64)>, Color)> {
        v.iter()
            .zip(&c.longueurs_onde_nm)
            .map(|(v, &l)| (c.pupille.iter().zip(v).filter_map(|(&p, y)| y.map(|y| (p, y))).collect(), couleur_lambda(l)))
            .collect()
    };
    let e = echelle(c.transverse_t_um.iter().chain(&c.transverse_s_um).flatten().flatten().copied(), 1.0);
    let (t, s) = (serie(&c.transverse_t_um), serie(&c.transverse_s_um));
    let titre_t = trf!("ΔY (µm) tangentielle — champ {} [C]", "ΔY (µm) tangential — field {} [C]", "ΔY (µm) tangential — Feld {} [C]", libelle_champ(app.systeme.unite_champ(), c.champ_deg));
    let titre_s = tr!("ΔX (µm) sagittale", "ΔX (µm) sagittal", "ΔX (µm) sagittal");
    f.render_widget(graphe(&titre_t, "py", [-1.0, 1.0], [-e, e], &t), decoupe[0]);
    f.render_widget(graphe(titre_s, "px", [-1.0, 1.0], [-e, e], &s), decoupe[1]);
}

/// Courbure de champ (T et S) et distorsion
pub fn dessiner_courbure_champ(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.courbure_calculee() else { return };
    let c = match resultat {
        Ok(c) => c,
        Err(e) => return erreur(f, zone, e),
    };
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(zone);
    let points = |v: &Vec<Option<f64>>| -> Vec<(f64, f64)> {
        v.iter().zip(&c.angles_deg).filter_map(|(x, &a)| x.map(|x| (x, a))).collect()
    };
    let theta = c.angles_deg.last().copied().unwrap_or(1.0).max(1e-6);
    let ef = echelle(c.foyer_t_mm.iter().chain(&c.foyer_s_mm).flatten().copied(), 0.01);
    let ed = echelle(c.distorsion_pct.iter().flatten().copied(), 0.01);
    let courbure = [(points(&c.foyer_t_mm), DOS_CYAN), (points(&c.foyer_s_mm), DOS_YELLOW)];
    let distorsion = [(points(&c.distorsion_pct), DOS_GREEN)];
    f.render_widget(
        graphe(tr!("Courbure de champ (mm) — T cyan, S jaune", "Field curvature (mm) — T cyan, S yellow", "Bildfeldwölbung (mm) — T cyan, S gelb"), "Δz", [-ef, ef], [0.0, theta], &courbure),
        decoupe[0],
    );
    f.render_widget(graphe(tr!("Distorsion (%)", "Distortion (%)", "Verzeichnung (%)"), "%", [-ed, ed], [0.0, theta], &distorsion), decoupe[1]);
}

/// Tableau des coefficients de Seidel
pub fn dessiner_seidel(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.seidel_calcule() else { return };
    let s = match resultat {
        Ok(s) => s,
        Err(e) => return erreur(f, zone, e),
    };
    let mut lignes = vec![Line::from(Span::styled(
        tr!(
            "Surf      S I sphér.   S II coma  S III astig. S IV Petzval  S V distor.   CL chr.ax.   CT chr.lat.",
            "Surf      S I spher.   S II coma  S III astig. S IV Petzval  S V distort.  CL ax.col.   CT lat.col.",
            "Fläche    S I sphär.   S II Koma  S III Astig. S IV Petzval  S V Verz.     CL Farbl.    CT Farbq."
        ),
        Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
    ))];
    let mut ligne = |nom: String, l: &crate::core::aberrations::LigneSeidel, c: Color| {
        let mut txt = format!("{nom:<6}");
        for v in [l.s1, l.s2, l.s3, l.s4, l.s5, l.cl, l.ct] {
            txt.push_str(&format!(" {v:>+12.6}"));
        }
        lignes.push(Line::from(Span::styled(txt, Style::default().fg(c))));
    };
    for l in &s.lignes {
        ligne(format!("#{:02}", l.surface), l, DOS_WHITE);
    }
    ligne(tr!("TOTAL", "TOTAL", "SUMME").to_string(), &s.total, DOS_GREEN);
    lignes.push(Line::from(""));
    lignes.push(Line::from(Span::styled(
        format!(
            "W040 {:+.4}  W131 {:+.4}  W222 {:+.4}  W220 {:+.4}  W311 {:+.4} λ  ({:.2}°)",
            s.w040, s.w131, s.w222, s.w220, s.w311, s.champ_max_deg
        ),
        Style::default().fg(DOS_CYAN),
    )));
    f.render_widget(
        Paragraph::new(lignes).block(bloc_dos(tr!("SEIDEL (mm)", "SEIDEL (mm)", "SEIDEL (mm)"), false)),
        zone,
    );
}

/// Tableau du spectrographe : position, dispersion, tache et résolution par longueur d'onde
pub fn dessiner_spectre(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.spectre_calcule() else { return };
    let s = match resultat {
        Ok(s) => s,
        Err(e) => return erreur(f, zone, e),
    };
    let unite_fente = if app.systeme.objet_fini().is_some() { "µm" } else { "\"" };
    let mut lignes = vec![
        aide_selection(app, true),
        Line::from(Span::styled(
            trf!(
                "Spectre {:.2} mm — fente {} {} → image {:.2} µm",
                "Spectrum {:.2} mm — slit {} {} → image {:.2} µm",
                "Spektrum {:.2} mm — Spalt {} {} → Bild {:.2} µm",
                s.longueur_spectre_mm,
                s.largeur_fente,
                unite_fente,
                s.image_fente_um.unwrap_or(f64::NAN)
            ),
            Style::default().fg(DOS_YELLOW),
        )),
        Line::from(Span::styled(
            tr!(
                "   λ (nm)      X (mm)      Y (mm)    nm/mm   tache µm        R",
                "   λ (nm)      X (mm)      Y (mm)    nm/mm   spot µm         R",
                "   λ (nm)      X (mm)      Y (mm)    nm/mm   Spot µm         R"
            ),
            Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
        )),
    ];
    let hauteur = zone.height.saturating_sub(5).max(1) as usize;
    let pas = s.longueurs_onde_nm.len().div_ceil(hauteur).max(1);
    for k in (0..s.longueurs_onde_nm.len()).step_by(pas) {
        let (x, y) = s.positions_mm[k].unwrap_or((f64::NAN, f64::NAN));
        lignes.push(Line::from(Span::styled(
            format!(
                "{:>9.1} {:>11.4} {:>11.4} {:>8.3} {:>9.2} {:>8.0}",
                s.longueurs_onde_nm[k],
                x,
                y,
                s.dispersion_nm_mm[k].unwrap_or(f64::NAN),
                s.tache_rms_um[k].unwrap_or(f64::NAN),
                s.resolution[k].unwrap_or(f64::NAN)
            ),
            Style::default().fg(couleur_lambda(s.longueurs_onde_nm[k])),
        )));
    }
    f.render_widget(
        Paragraph::new(lignes).block(bloc_dos(tr!("SPECTROGRAPHE", "SPECTROGRAPH", "SPEKTROGRAPH"), false)),
        zone,
    );
}

/// Courbe tracée en ligne dans un graphique
fn serie<'a>(nom: String, couleur: Color, points: &'a [(f64, f64)]) -> Dataset<'a> {
    Dataset::default()
        .name(nom)
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(couleur))
        .data(points)
}

/// Axe gradué en 5 étiquettes
fn axe(titre: &str, a: f64, b: f64, decimales: usize) -> Axis<'static> {
    Axis::default()
        .title(titre.to_string())
        .style(Style::default().fg(DOS_TEXT))
        .bounds([a, b])
        .labels((0..=4).map(|k| Span::raw(format!("{:.*}", decimales, a + (b - a) * k as f64 / 4.0))).collect::<Vec<_>>())
}

/// Énergie encerclée géométrique, par diffraction et d'Airy
pub fn dessiner_energie(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(resultat) = app.ondes.energie_calculee() else { return };
    let e = match resultat {
        Ok(e) => e,
        Err(e) => return erreur(f, zone, e),
    };
    let parties = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(5)]).split(zone);
    let unite = if app.systeme.afocal { "µrad" } else { "µm" };
    let diff = |v: Option<f64>| v.map(|v| format!("{v:.3}")).unwrap_or_else(|| "—".into());
    let lignes = vec![
        aide_selection(app, false),
        ligne(
            tr!("50 % / 80 % géom. : ", "50% / 80% geom.: ", "50 % / 80 % geom.: "),
            format!("{:.3} / {:.3} {unite}", e.r50_geo_um, e.r80_geo_um),
            DOS_CYAN,
        ),
        ligne(
            tr!("50 % / 80 % diffr. : ", "50% / 80% diffr.: ", "50 % / 80 % Beug.: "),
            format!("{} / {} {unite}  (Airy {:.3} {unite})", diff(e.r50_diffraction_um), diff(e.r80_diffraction_um), e.rayon_airy_um),
            DOS_GREEN,
        ),
    ];
    f.render_widget(Paragraph::new(lignes), parties[0]);
    let pts = |v: &[f64]| e.rayons_um.iter().copied().zip(v.iter().copied()).collect::<Vec<_>>();
    let (geo, airy) = (pts(&e.geometrique), pts(&e.airy));
    let diffraction = e.diffraction.as_deref().map(pts).unwrap_or_default();
    let mut series = vec![
        serie(tr!("Airy", "Airy", "Airy").into(), DOS_WHITE, &airy),
        serie(tr!("géométrique", "geometric", "geometrisch").into(), DOS_CYAN, &geo),
    ];
    if !diffraction.is_empty() {
        series.push(serie(tr!("diffraction", "diffraction", "Beugung").into(), DOS_GREEN, &diffraction));
    }
    let r_max = e.rayons_um.last().copied().unwrap_or(1.0);
    let graphe = Chart::new(series)
        .block(bloc_dos(tr!("ÉNERGIE ENCERCLÉE", "ENCIRCLED ENERGY", "EINGESCHLOSSENE ENERGIE"), false))
        .x_axis(axe(unite, 0.0, r_max, 2))
        .y_axis(axe("", 0.0, 1.0, 2));
    f.render_widget(graphe, parties[1]);
}

/// Décalage focal chromatique et couleur latérale
pub fn dessiner_chromatique(f: &mut Frame, zone: Rect, app: &AppState) {
    let Some(r) = app.ondes.chromatique_calcule() else { return };
    let colonnes = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(zone);
    match &r.decalage {
        Ok(d) => {
            let pts: Vec<(f64, f64)> = d.longueurs_onde_nm.iter().copied().zip(d.decalage_um.iter().copied()).collect();
            let (a, b) = (d.longueurs_onde_nm[0], *d.longueurs_onde_nm.last().unwrap_or(&700.0));
            let m = d.decalage_um.iter().fold(d.profondeur_diffraction_um, |m, v| m.max(v.abs())) * 1.1;
            let p = d.profondeur_diffraction_um;
            let (haut, bas) = (vec![(a, p), (b, p)], vec![(a, -p), (b, -p)]);
            let titre = trf!(
                "DÉCALAGE FOCAL — étendue {:.1} µm, foyer ±{:.1} µm",
                "FOCAL SHIFT — range {:.1} µm, focus ±{:.1} µm",
                "FOKUSVERSCHIEBUNG — Bereich {:.1} µm, Fokus ±{:.1} µm",
                d.etendue_um,
                p
            );
            let graphe = Chart::new(vec![
                serie("±2λN²".into(), DOS_WHITE, &haut),
                serie(String::new(), DOS_WHITE, &bas),
                serie("Δz (µm)".into(), DOS_CYAN, &pts),
            ])
            .block(bloc_dos(&titre, false))
            .x_axis(axe("nm", a, b, 0))
            .y_axis(axe("µm", -m, m, 1));
            f.render_widget(graphe, colonnes[0]);
        }
        Err(e) => erreur(f, colonnes[0], e),
    }
    match &r.laterale {
        Ok(c) => {
            let courbes: Vec<(Vec<(f64, f64)>, f64)> = c
                .ecart_um
                .iter()
                .zip(&c.longueurs_onde_nm)
                .map(|(v, &l)| (c.fractions_champ.iter().zip(v).filter_map(|(&x, y)| y.map(|y| (x, y))).collect(), l))
                .collect();
            let m = c.ecart_um.iter().flatten().flatten().fold(c.rayon_airy_um, |m, v| m.max(v.abs())) * 1.1;
            let airy = (vec![(0.0, c.rayon_airy_um), (1.0, c.rayon_airy_um)], vec![(0.0, -c.rayon_airy_um), (1.0, -c.rayon_airy_um)]);
            let mut series = vec![serie("Airy".into(), DOS_WHITE, &airy.0), serie(String::new(), DOS_WHITE, &airy.1)];
            for (i, (p, l)) in courbes.iter().enumerate() {
                series.push(serie(format!("{l:.0} nm"), COULEURS_CHAMPS[i % COULEURS_CHAMPS.len()], p));
            }
            let titre = trf!(
                "COULEUR LATÉRALE — écart max {:.2} µm",
                "LATERAL COLOUR — max spread {:.2} µm",
                "FARBQUERFEHLER — max. {:.2} µm",
                c.ecart_max_um
            );
            let graphe = Chart::new(series)
                .block(bloc_dos(&titre, false))
                .x_axis(axe(tr!("champ", "field", "Feld"), 0.0, 1.0, 2))
                .y_axis(axe("µm", -m, m, 2));
            f.render_widget(graphe, colonnes[1]);
        }
        Err(e) => erreur(f, colonnes[1], e),
    }
}

/// Empreinte des faisceaux : points d'impact dans le repère local de la surface
pub fn dessiner_empreinte(f: &mut Frame, zone: Rect, app: &AppState) {
    use ratatui::widgets::canvas::{Canvas, Circle, Points};
    let Some(resultat) = app.ondes.empreinte_calculee() else { return };
    let e = match resultat {
        Ok(e) => e,
        Err(e) => return erreur(f, zone, e),
    };
    let parties = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(2), Constraint::Min(5)]).split(zone);
    let lignes = vec![
        Line::from(Span::styled(
            trf!(
                "Surface #{} [ ]  —  demi-diamètre {:.3} mm, utilisé {:.3} mm, nécessaire {:.3} mm",
                "Surface #{} [ ]  —  semi-diameter {:.3} mm, used {:.3} mm, needed {:.3} mm",
                "Fläche #{} [ ]  —  Halbdurchmesser {:.3} mm, genutzt {:.3} mm, nötig {:.3} mm",
                e.surface + 1,
                e.semi_diametre_mm,
                e.rayon_utile_mm,
                e.rayon_requis_mm
            ),
            Style::default().fg(DOS_CYAN),
        )),
        ligne(tr!("Rayons arrêtés ici ou avant : ", "Rays stopped here or before: ", "Hier oder davor gestoppte Strahlen: "), format!("{} / {}", e.nb_vignettes, e.nb_rayons), if e.nb_vignettes > 0 { DOS_RED } else { DOS_GREEN }),
    ];
    f.render_widget(Paragraph::new(lignes), parties[0]);
    let etendue = e.points.iter().map(|p| p.x_mm.abs().max(p.y_mm.abs())).fold(e.semi_diametre_mm, f64::max).max(1e-3) * 1.08;
    // Cellules du terminal deux fois plus hautes que larges : on élargit l'axe X
    let rapport = (parties[1].width as f64 / (2.0 * parties[1].height.max(1) as f64)).max(1.0);
    let nb_champs = app.systeme.champs_deg.len().max(1);
    let groupes: Vec<(Vec<(f64, f64)>, Color)> = (0..nb_champs)
        .map(|c| {
            (
                e.points.iter().filter(|p| p.champ == c && !p.vignette).map(|p| (p.x_mm, p.y_mm)).collect(),
                COULEURS_CHAMPS[c % COULEURS_CHAMPS.len()],
            )
        })
        .collect();
    let arretes: Vec<(f64, f64)> = e.points.iter().filter(|p| p.vignette).map(|p| (p.x_mm, p.y_mm)).collect();
    let canevas = Canvas::default()
        .block(bloc_dos(tr!("EMPREINTE (repère local, mm)", "FOOTPRINT (local frame, mm)", "FOOTPRINT (lokales System, mm)"), false))
        .background_color(DOS_BG)
        .marker(Marker::Braille)
        .x_bounds([-etendue * rapport, etendue * rapport])
        .y_bounds([-etendue, etendue])
        .paint(move |ctx| {
            ctx.draw(&Circle { x: 0.0, y: 0.0, radius: e.semi_diametre_mm, color: DOS_WHITE });
            if e.rayon_obstruction_mm > 0.0 {
                ctx.draw(&Circle { x: 0.0, y: 0.0, radius: e.rayon_obstruction_mm, color: DOS_TEXT });
            }
            for (pts, c) in &groupes {
                ctx.draw(&Points { coords: pts, color: *c });
            }
            ctx.draw(&Points { coords: &arretes, color: DOS_RED });
        });
    f.render_widget(canevas, parties[1]);
}
