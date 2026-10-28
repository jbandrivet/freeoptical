use crate::{
    core::{glass::MediumKind, raytracer::tracer_grille_hexapolaire, types::Point3},
    ui::{
        app::AppState,
        theme::{DOS_BG, DOS_CYAN, DOS_GREEN, DOS_MAGENTA, DOS_WHITE, DOS_YELLOW},
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

/// Projette un point 3D `(x, y, z)` (centré autour de `z_centre`) en coordonnées 2D écran `(u, v)`
/// selon les angles d'observation isométriques `(yaw_deg, pitch_deg)`.
fn projeter_isometrique(
    p: Point3,
    z_centre: f64,
    yaw_rad: f64,
    pitch_rad: f64,
) -> (f64, f64) {
    let x = p.x;
    let y = p.y;
    let z = p.z - z_centre;

    // Rotation autour de l'axe vertical Y (Yaw / Lacet)
    let x1 = z * yaw_rad.cos() + x * yaw_rad.sin();
    let z1 = -z * yaw_rad.sin() + x * yaw_rad.cos();
    let y1 = y;

    // Rotation d'inclinaison (Pitch / Tangage)
    let u = x1;
    let v = y1 * pitch_rad.cos() - z1 * pitch_rad.sin();
    (u, v)
}

/// Dessine la projection 3D isométrique filaire (wireframe) orientable des lentilles et du faisceau 3D
pub fn dessiner_filaire_3d(f: &mut Frame, zone: Rect, app: &AppState) {
    let sys = &app.systeme;
    let positions_z = sys.positions_z_sommets();
    let (z_min_sys, z_max_sys) = sys.etendue_axiale();
    let z_max = (z_max_sys - z_min_sys).max(20.0);
    let z_centre = 0.5 * (z_min_sys + z_max_sys);

    let max_sd = sys
        .surfaces
        .iter()
        .map(|s| s.semi_diametre)
        .fold(15.0_f64, f64::max);

    let etendue_x = (z_max * 0.65).max(max_sd * 2.0);
    let etendue_y = (max_sd * 1.8).max(z_max * 0.30);

    let yaw_rad = app.angle_3d_yaw_deg.to_radians();
    let pitch_rad = app.angle_3d_pitch_deg.to_radians();

    // Faisceau 3D hexapolaire (1 + 6 + 12 = 19 rayons)
    let faisceau_3d = tracer_grille_hexapolaire(sys, 0.0, 0.0, 2, sys.longueur_onde_ref_nm);

    let canvas = Canvas::default()
        .background_color(DOS_BG)
        .marker(Marker::Braille)
        .x_bounds([-etendue_x, etendue_x])
        .y_bounds([-etendue_y, etendue_y])
        .paint(move |ctx| {
            // 1. Faisceau de rayons 3D (en cyan rétro)
            for tr in &faisceau_3d {
                let mut prev = projeter_isometrique(tr.rayon_initial.origine, z_centre, yaw_rad, pitch_rad);
                for hit in &tr.impacts {
                    let curr = projeter_isometrique(hit.point_global, z_centre, yaw_rad, pitch_rad);
                    ctx.draw(&CanvasLine {
                        x1: prev.0,
                        y1: prev.1,
                        x2: curr.0,
                        y2: curr.1,
                        color: DOS_CYAN,
                    });
                    prev = curr;
                }
            }

            // 2. Maillage filaire 3D (anneaux et méridiens) de chaque surface
            let nb_seg_cercle = 32;
            let nb_seg_meridien = 20;

            for (idx, (surf, &z_sommet)) in sys.surfaces.iter().zip(positions_z.iter()).enumerate() {
                let sd = surf.semi_diametre;
                let mat = sys.catalogue.resoudre(&surf.verre);
                let couleur = if idx == app.ligne_selectionnee {
                    DOS_YELLOW
                } else if mat.est_miroir() {
                    DOS_MAGENTA
                } else {
                    DOS_GREEN
                };

                let comm_up = surf.commentaire.to_uppercase();
                let est_cube = comm_up.contains("CUBE")
                    || comm_up.contains("SEPARAT")
                    || comm_up.contains("SÉPARAT")
                    || comm_up.contains("HYPOT");

                if est_cube {
                    // Rendu 3D d'une face carrée [-sd, +sd] x [-sd, +sd] pour un Cube Séparateur
                    let coins = [
                        Point3::new(-sd, -sd, z_sommet),
                        Point3::new(sd, -sd, z_sommet),
                        Point3::new(sd, sd, z_sommet),
                        Point3::new(-sd, sd, z_sommet),
                        Point3::new(-sd, -sd, z_sommet),
                    ];
                    for fen in coins.windows(2) {
                        let (u1, v1) = projeter_isometrique(fen[0], z_centre, yaw_rad, pitch_rad);
                        let (u2, v2) = projeter_isometrique(fen[1], z_centre, yaw_rad, pitch_rad);
                        ctx.draw(&CanvasLine {
                            x1: u1,
                            y1: v1,
                            x2: u2,
                            y2: v2,
                            color: couleur,
                        });
                    }
                } else {
                    // Anneau périphérique (r = SD) et anneau intermédiaire (r = 0.55 * SD) pour une lentille circulaire
                    for &facteur_r in &[1.0, 0.55] {
                        let r = sd * facteur_r;
                        let Some(z_sag) = surf.fleche(r) else {
                            continue;
                        };
                        let z_anneau = z_sommet + z_sag;

                        let mut pt_prec: Option<(f64, f64)> = None;
                        for k in 0..=nb_seg_cercle {
                            let theta =
                                2.0 * std::f64::consts::PI * (k as f64) / (nb_seg_cercle as f64);
                            let p3 = Point3::new(r * theta.cos(), r * theta.sin(), z_anneau);
                            let proj = projeter_isometrique(p3, z_centre, yaw_rad, pitch_rad);
                            if let Some((up, vp)) = pt_prec {
                                ctx.draw(&CanvasLine {
                                    x1: up,
                                    y1: vp,
                                    x2: proj.0,
                                    y2: proj.1,
                                    color: couleur,
                                });
                            }
                            pt_prec = Some(proj);
                        }
                    }

                    // Deux méridiens croisés (plan XZ et plan YZ) sur la calotte
                    for &(cos_a, sin_a) in &[(1.0, 0.0), (0.0, 1.0)] {
                        let mut pt_prec: Option<(f64, f64)> = None;
                        for k in 0..=nb_seg_meridien {
                            let r_signe = -sd + 2.0 * sd * (k as f64) / (nb_seg_meridien as f64);
                            if let Some(z_sag) = surf.fleche(r_signe.abs()) {
                                let p3 = Point3::new(
                                    r_signe * cos_a,
                                    r_signe * sin_a,
                                    z_sommet + z_sag,
                                );
                                let proj = projeter_isometrique(p3, z_centre, yaw_rad, pitch_rad);
                                if let Some((up, vp)) = pt_prec {
                                    ctx.draw(&CanvasLine {
                                        x1: up,
                                        y1: vp,
                                        x2: proj.0,
                                        y2: proj.1,
                                        color: couleur,
                                    });
                                }
                                pt_prec = Some(proj);
                            }
                        }
                    }
                }

                // Arêtes longitudinales pour relier les faces en verre (Cube Séparateur ou Lentille)
                if idx + 1 < sys.surfaces.len() && matches!(mat.kind, MediumKind::Verre { .. }) {
                    let surf_suiv = &sys.surfaces[idx + 1];
                    let z_suiv = positions_z[idx + 1];
                    let sd_suiv = surf_suiv.semi_diametre;
                    if est_cube {
                        // 4 arêtes longitudinales du cube + plan séparateur diagonal à 45° en magenta
                        for &(sx, sy) in &[(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                            let p_a = Point3::new(sx * sd, sy * sd, z_sommet);
                            let p_b = Point3::new(sx * sd_suiv, sy * sd_suiv, z_suiv);
                            let (u1, v1) = projeter_isometrique(p_a, z_centre, yaw_rad, pitch_rad);
                            let (u2, v2) = projeter_isometrique(p_b, z_centre, yaw_rad, pitch_rad);
                            ctx.draw(&CanvasLine {
                                x1: u1,
                                y1: v1,
                                x2: u2,
                                y2: v2,
                                color: DOS_WHITE,
                            });
                        }
                        // Diagonale séparatrice 3D à 45° à l'intérieur du cube
                        for &sx in &[-1.0, 1.0] {
                            let p_d1 = Point3::new(sx * sd, -sd, z_sommet);
                            let p_d2 = Point3::new(sx * sd_suiv, sd_suiv, z_suiv);
                            let (u1, v1) = projeter_isometrique(p_d1, z_centre, yaw_rad, pitch_rad);
                            let (u2, v2) = projeter_isometrique(p_d2, z_centre, yaw_rad, pitch_rad);
                            ctx.draw(&CanvasLine {
                                x1: u1,
                                y1: v1,
                                x2: u2,
                                y2: v2,
                                color: DOS_MAGENTA,
                            });
                        }
                    } else if let (Some(f1), Some(f2)) = (surf.fleche(sd), surf_suiv.fleche(sd_suiv))
                    {
                        for k in 0..8 {
                            let theta = 2.0 * std::f64::consts::PI * (k as f64) / 8.0;
                            let p_a = Point3::new(
                                sd * theta.cos(),
                                sd * theta.sin(),
                                z_sommet + f1,
                            );
                            let p_b = Point3::new(
                                sd_suiv * theta.cos(),
                                sd_suiv * theta.sin(),
                                z_suiv + f2,
                            );
                            let (u1, v1) = projeter_isometrique(p_a, z_centre, yaw_rad, pitch_rad);
                            let (u2, v2) = projeter_isometrique(p_b, z_centre, yaw_rad, pitch_rad);
                            ctx.draw(&CanvasLine {
                                x1: u1,
                                y1: v1,
                                x2: u2,
                                y2: v2,
                                color: DOS_WHITE,
                            });
                        }
                    }
                }
            }
        });

    f.render_widget(canvas, zone);

    // Indication des touches de rotation 3D
    if zone.height > 2 && zone.width > 50 {
        let info_rect = Rect {
            x: zone.x + 1,
            y: zone.y,
            width: zone.width.saturating_sub(2).min(80),
            height: 1,
        };
        let info = Paragraph::new(Line::from(vec![
            Span::styled(
                trf!(
                    "Rotation 3D [Touches H/L: Lacet {:.0}° | J/K: Tangage {:.0}°]  ", "3D rotation [Keys H/L: yaw {:.0}° | J/K: pitch {:.0}°]  ", "3D-Drehung [Tasten H/L: Gieren {:.0}° | J/K: Nicken {:.0}°]  ",
                    app.angle_3d_yaw_deg, app.angle_3d_pitch_deg
                ),
                Style::default().fg(DOS_YELLOW),
            ),
            Span::styled("── ", Style::default().fg(DOS_GREEN)),
            Span::styled(tr!("Maillage Lentilles  ", "Lens mesh  ", "Linsennetz  "), Style::default().fg(DOS_WHITE)),
            Span::styled("── ", Style::default().fg(DOS_CYAN)),
            Span::styled(tr!("Faisceau 3D", "3D beam", "3D-Strahlenbündel"), Style::default().fg(DOS_WHITE)),
        ]));
        f.render_widget(info, info_rect);
    }
}
