use crate::{
    core::{glass::MediumKind, raytracer::tracer_eventail_meridional},
    ui::{
        app::AppState,
        theme::{DOS_BG, DOS_CYAN, DOS_GREEN, DOS_MAGENTA, DOS_TEXT, DOS_WHITE, DOS_YELLOW},
    },
};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        Paragraph,
        canvas::{Canvas, Line as CanvasLine},
    },
};

/// Dessine la coupe transversale 2D (plan méridien YZ) du système optique et le tracé des rayons
pub fn dessiner_coupe_2d(f: &mut Frame, zone: Rect, app: &AppState) {
    let sys = &app.systeme;
    let positions_z = sys.positions_z_sommets();
    let reperes = sys.reperes();
    let [z_min_sys, z_max_sys, y_min_sys, y_max_sys] = sys.boite_englobante();
    let axe_replie = sys.a_des_coupures().then(|| sys.points_axe());
    let max_sd = sys
        .surfaces
        .iter()
        .map(|s| s.semi_diametre)
        .fold(15.0_f64, f64::max);

    let z_min = z_min_sys - 18.0;
    let z_max = z_max_sys.max(z_min_sys + 20.0) + 8.0;
    let y_milieu = 0.5 * (y_min_sys + y_max_sys);
    let y_lim = (max_sd * 1.35).max(0.6 * (y_max_sys - y_min_sys));

    // Pré-calcul des éventails de rayons méridiens (champ sur l'axe 0° et champ hors-axe éventuel)
    let rayons_axe = tracer_eventail_meridional(
        sys,
        0.0,
        app.nb_rayons_2d,
        sys.longueur_onde_ref_nm,
    );
    let rayons_champ = if sys.champs_deg.len() > 1 && sys.champs_deg[1].1.abs() > 0.01 {
        Some(tracer_eventail_meridional(
            sys,
            sys.champs_deg[1].1,
            5,
            sys.longueur_onde_ref_nm,
        ))
    } else {
        None
    };

    let canvas = Canvas::default()
        .background_color(DOS_BG)
        .marker(Marker::Braille)
        .x_bounds([z_min, z_max])
        .y_bounds([y_milieu - y_lim, y_milieu + y_lim])
        .paint(move |ctx| {
            // 1. Axe optique (Y = 0), ou ligne brisée qui suit les ruptures de repère
            if let Some(axe) = &axe_replie {
                let mut prec = (z_min, 0.0);
                for o in axe {
                    ctx.draw(&CanvasLine { x1: prec.0, y1: prec.1, x2: o.z, y2: o.y, color: DOS_TEXT });
                    prec = (o.z, o.y);
                }
            } else {
                ctx.draw(&CanvasLine {
                    x1: z_min,
                    y1: 0.0,
                    x2: z_max,
                    y2: 0.0,
                    color: DOS_TEXT,
                });
            }

            // 2. Rayons hors-axe (en vert phosphore si présents)
            if let Some(ref eventail_hors_axe) = rayons_champ {
                for tr in eventail_hors_axe {
                    let mut p_prec = (tr.rayon_initial.origine.z, tr.rayon_initial.origine.y);
                    for hit in &tr.impacts {
                        let p_curr = (hit.point_global.z, hit.point_global.y);
                        ctx.draw(&CanvasLine {
                            x1: p_prec.0,
                            y1: p_prec.1,
                            x2: p_curr.0,
                            y2: p_curr.1,
                            color: DOS_GREEN,
                        });
                        p_prec = p_curr;
                    }
                }
            }

            // 3. Rayons sur l'axe (en cyan vif DOS)
            for tr in &rayons_axe {
                let mut p_prec = (tr.rayon_initial.origine.z, tr.rayon_initial.origine.y);
                for hit in &tr.impacts {
                    let p_curr = (hit.point_global.z, hit.point_global.y);
                    ctx.draw(&CanvasLine {
                        x1: p_prec.0,
                        y1: p_prec.1,
                        x2: p_curr.0,
                        y2: p_curr.1,
                        color: DOS_CYAN,
                    });
                    p_prec = p_curr;
                }
            }

            // 4. Profils 2D des dioptres et fermeture des bords de lentilles
            let nb_pas_profil = 36;
            for (idx, (surf, &z_sommet)) in sys.surfaces.iter().zip(positions_z.iter()).enumerate() {
                if surf.est_coupure() {
                    continue;
                }
                let rep = reperes[idx];
                let sd = surf.semi_diametre;
                let mat = sys.catalogue.resoudre(&surf.verre);
                let couleur_surf = if idx == app.ligne_selectionnee {
                    DOS_YELLOW
                } else if mat.est_miroir() {
                    DOS_MAGENTA
                } else {
                    DOS_WHITE
                };

                let mut prev_pt: Option<(f64, f64)> = None;
                for k in 0..=nb_pas_profil {
                    let y = -sd + 2.0 * sd * (k as f64) / (nb_pas_profil as f64);
                    if let Some(fleche) = surf.fleche(y.abs()) {
                        let g = rep.vers_global(0.0, y, fleche);
                        if let Some((zp, yp)) = prev_pt {
                            ctx.draw(&CanvasLine {
                                x1: zp,
                                y1: yp,
                                x2: g.z,
                                y2: g.y,
                                color: couleur_surf,
                            });
                        }
                        prev_pt = Some((g.z, g.y));
                    }
                }

                // Si le milieu suivant est un verre réfractif (lentille ou cube séparateur),
                // on relie les bords haut et bas avec la surface suivante
                if idx + 1 < sys.surfaces.len() && matches!(mat.kind, MediumKind::Verre { .. }) {
                    let surf_suiv = &sys.surfaces[idx + 1];
                    let z_suiv = positions_z[idx + 1];
                    let sd_suiv = surf_suiv.semi_diametre;
                    if let (Some(f1), Some(f2)) = (surf.fleche(sd), surf_suiv.fleche(sd_suiv)) {
                        // Bords supérieur et inférieur
                        for signe in [1.0, -1.0] {
                            let a = rep.vers_global(0.0, signe * sd, f1);
                            let b = reperes[idx + 1].vers_global(0.0, signe * sd_suiv, f2);
                            ctx.draw(&CanvasLine { x1: a.z, y1: a.y, x2: b.z, y2: b.y, color: DOS_WHITE });
                        }

                        // Si cette surface fait partie d'un Cube Séparateur, on trace la lame diagonale à 45°
                        let comm_up = surf.commentaire.to_uppercase();
                        let comm_suiv_up = surf_suiv.commentaire.to_uppercase();
                        if comm_up.contains("CUBE")
                            || comm_up.contains("SEPARAT")
                            || comm_up.contains("SÉPARAT")
                        {
                            // Si le cube est découpé en 2 demi-cubes (avec Hypoténuse au milieu),
                            // ou en un seul bloc, on trace la diagonale à 45° sur toute la largeur du cube
                            if comm_suiv_up.contains("HYPOT") && idx + 2 < sys.surfaces.len() {
                                let z_fin_cube = positions_z[idx + 2];
                                ctx.draw(&CanvasLine {
                                    x1: z_sommet,
                                    y1: -sd,
                                    x2: z_fin_cube,
                                    y2: sd,
                                    color: DOS_MAGENTA,
                                });
                            } else if !comm_up.contains("HYPOT") {
                                // Lame de (y = -sd, z min) à (y = +sd, z max), quel que soit le sens de parcours
                                ctx.draw(&CanvasLine {
                                    x1: z_sommet.min(z_suiv),
                                    y1: -sd,
                                    x2: z_sommet.max(z_suiv),
                                    y2: sd,
                                    color: DOS_MAGENTA,
                                });
                            }
                        }
                    }
                }
            }
        });

    f.render_widget(canvas, zone);

    // Légende superposée en haut à gauche de la vue 2D
    if zone.height > 2 && zone.width > 50 {
        let legende_rect = Rect {
            x: zone.x + 1,
            y: zone.y,
            width: zone.width.saturating_sub(2).min(78),
            height: 1,
        };
        let legende = Paragraph::new(Line::from(vec![
            Span::styled("── ", Style::default().fg(DOS_CYAN)),
            Span::styled(tr!("Rayons 0°  ", "Rays 0°  ", "Strahlen 0°  "), Style::default().fg(DOS_WHITE)),
            Span::styled("── ", Style::default().fg(DOS_GREEN)),
            Span::styled(tr!("Champ hors-axe  ", "Off-axis field  ", "Feld außeraxial  "), Style::default().fg(DOS_WHITE)),
            Span::styled("── ", Style::default().fg(DOS_YELLOW)),
            Span::styled(tr!("Surface active (LDE)", "Active surface (LDE)", "Aktive Fläche (LDE)"), Style::default().fg(DOS_WHITE)),
        ]));
        f.render_widget(legende, legende_rect);
    }
}
