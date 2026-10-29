//! Vue terminal de l'interféromètre de Bath : interférogramme simulé et écart à la conique visée
//! en demi-blocs colorés, bilan de la réduction, réglages au clavier.

use crate::core::bath::{LAMBDA_VISUELLE_NM, ParametresBath, ReductionBath, SimulationBath, simuler_et_reduire};
use crate::core::types::OpticalSystem;
use crate::ui::theme::{DOS_CYAN, DOS_GREEN, DOS_RED, DOS_TEXT, DOS_WHITE, DOS_YELLOW, bloc_dos};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// État de la vue Bath du TUI
pub struct BathTui {
    pub params: ParametresBath,
    resultat: Option<(ParametresBath, Result<(SimulationBath, ReductionBath), String>)>,
}

impl BathTui {
    /// Miroir pris dans le système (premier miroir courbe), sinon 200 mm F/6 parabolique
    pub fn depuis_systeme(sys: &OpticalSystem) -> Self {
        let mut params = ParametresBath { taille: 128, ..Default::default() };
        if let Some(s) = sys.surfaces.iter().find(|s| s.verre == "MIROIR" && s.rayon.is_finite()) {
            params.diametre_mm = 2.0 * s.semi_diametre;
            params.rayon_courbure_mm = s.rayon.abs();
            params.conique = s.conique;
            params.conique_visee = s.conique;
            params.diametre_obstruction_mm = 2.0 * s.rayon_obstruction;
            if params.valider().is_err() {
                params = ParametresBath { taille: 128, ..Default::default() };
            }
        }
        Self { params, resultat: None }
    }

    /// Recalcule si les réglages ont changé
    pub fn preparer(&mut self) {
        if self.resultat.as_ref().is_none_or(|r| r.0 != self.params) {
            self.resultat = Some((self.params.clone(), simuler_et_reduire(&self.params)));
        }
    }

    /// Réglages au clavier ; `false` si la touche n'est pas une touche de la vue
    pub fn touche(&mut self, code: KeyCode, sys: &OpticalSystem) -> bool {
        let p = &mut self.params;
        match code {
            KeyCode::Char('x') => p.basculement_x_franges -= 1.0,
            KeyCode::Char('X') => p.basculement_x_franges += 1.0,
            KeyCode::Char('y') => p.basculement_y_franges -= 1.0,
            KeyCode::Char('Y') => p.basculement_y_franges += 1.0,
            KeyCode::Char('d') => p.defocalisation_franges -= 0.5,
            KeyCode::Char('D') => p.defocalisation_franges += 0.5,
            KeyCode::Char('e') => p.ecart_mm = (p.ecart_mm - 0.5).max(0.0),
            KeyCode::Char('E') => p.ecart_mm += 0.5,
            KeyCode::Char('k') => p.conique -= 0.02,
            KeyCode::Char('K') => p.conique += 0.02,
            KeyCode::Char('v') => p.conique_visee -= 0.02,
            KeyCode::Char('V') => p.conique_visee += 0.02,
            KeyCode::Char('a') | KeyCode::Char('A') => p.retirer_astigmatisme = !p.retirer_astigmatisme,
            KeyCode::Char('f') | KeyCode::Char('F') => {
                p.defocalisation_franges = p.defocalisation_franges_droites();
            }
            KeyCode::Char('g') | KeyCode::Char('G') => *self = Self::depuis_systeme(sys),
            _ => return false,
        }
        true
    }
}

fn palette(t: f64) -> Color {
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
    Color::Rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

/// Grille (lignes de py = −1 à +1) dessinée en demi-blocs : deux pixels carrés par caractère
fn image_demi_blocs(taille: usize, hauteur: u16, couleur: impl Fn(usize) -> Option<Color>) -> Vec<Line<'static>> {
    let px = 2 * hauteur as usize;
    let noir = Color::Rgb(0, 0, 0);
    let valeur = |x: usize, y: usize| -> Color {
        let i = ((x as f64 + 0.5) / px as f64 * taille as f64) as usize;
        let j = ((1.0 - (y as f64 + 0.5) / px as f64) * taille as f64) as usize;
        couleur(j.min(taille - 1) * taille + i.min(taille - 1)).unwrap_or(noir)
    };
    (0..hauteur as usize)
        .map(|ligne| {
            Line::from(
                (0..px)
                    .map(|x| Span::styled("▀", Style::default().fg(valeur(x, 2 * ligne)).bg(valeur(x, 2 * ligne + 1))))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

fn ligne(libelle: &str, valeur: String, couleur: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(libelle.to_string(), Style::default().fg(DOS_TEXT)),
        Span::styled(valeur, Style::default().fg(couleur)),
    ])
}

pub fn dessiner(f: &mut Frame, zone: Rect, bath: &BathTui) {
    let Some((p, resultat)) = &bath.resultat else { return };
    let (sim, red) = match resultat {
        Ok(r) => (&r.0, &r.1),
        Err(e) => {
            f.render_widget(Paragraph::new(Span::styled(e.clone(), Style::default().fg(DOS_RED))), zone);
            return;
        }
    };
    // Deux images carrées (2h colonnes sur h lignes) et 52 colonnes pour le bilan
    const LARGEUR_BILAN: u16 = 52;
    let h = zone.height.saturating_sub(2).min(zone.width.saturating_sub(LARGEUR_BILAN + 4) / 4).max(4);
    let cote = 2 * h + 2;
    let place_cartes = zone.width >= 2 * cote + LARGEUR_BILAN;
    let mut x = zone.x;

    // Interférogramme (teinte du laser)
    let rect = Rect::new(x, zone.y, cote.min(zone.width), (h + 2).min(zone.height));
    let franges = image_demi_blocs(sim.taille, h, |k| {
        sim.opd_ondes[k].map(|_| {
            let v = sim.intensite[k] as f64;
            Color::Rgb((v * 255.0) as u8, (v * v * 60.0) as u8, (v * v * 40.0) as u8)
        })
    });
    f.render_widget(Paragraph::new(franges).block(bloc_dos(tr!("INTERFÉROGRAMME", "INTERFEROGRAM", "INTERFEROGRAMM"), false)), rect);
    x += rect.width;

    // Écart à la conique visée
    if place_cartes {
        let m = red.residu_ondes.iter().flatten().fold(0.01f64, |a, &v| a.max(v.abs()));
        let rect = Rect::new(x, zone.y, cote, (h + 2).min(zone.height));
        let carte = image_demi_blocs(red.taille, h, |k| red.residu_ondes[k].map(|w| palette((w + m) / (2.0 * m))));
        f.render_widget(
            Paragraph::new(carte).block(bloc_dos(&trf!("ÉCART ±{:.3} λ", "DEVIATION ±{:.3} λ", "ABWEICHUNG ±{:.3} λ", m), false)),
            rect,
        );
        x += cote;
    }

    let ok = red.strehl_550 >= 0.8;
    let correction = if red.conique_visee.abs() > 1e-9 { format!(" ({:.0} %)", 100.0 * red.conique_mesuree / red.conique_visee) } else { String::new() };
    let lignes = vec![
        ligne(tr!("Miroir      : ", "Mirror      : ", "Spiegel     : "), format!("Ø{:.0} R{:.0} F/{:.2}", p.diametre_mm, p.rayon_courbure_mm, p.rapport_ouverture()), DOS_WHITE),
        ligne(tr!("k réel/visé : ", "k real/goal : ", "k real/Ziel : "), format!("{:+.3} / {:+.3}", p.conique, p.conique_visee), DOS_WHITE),
        ligne(tr!("k mesuré    : ", "k measured  : ", "k gemessen  : "), format!("{:+.4}{correction}", red.conique_mesuree), DOS_YELLOW),
        ligne(tr!("Écart s     : ", "Separation  : ", "Abstand s   : "), format!("{:.1} mm, astig. {:.4} λ PV", p.ecart_mm, red.astigmatisme_bath_ondes), DOS_WHITE),
        ligne(
            tr!("Franges     : ", "Fringes     : ", "Streifen    : "),
            format!(
                "X {:+.0} Y {:+.0} {} {:+.1}",
                p.basculement_x_franges,
                p.basculement_y_franges,
                tr!("déf.", "defoc.", "Defok."),
                p.defocalisation_franges
            ),
            DOS_WHITE,
        ),
        Line::from(""),
        ligne(&trf!("PV/RMS {:.0} nm: ", "PV/RMS {:.0} nm: ", "PV/RMS {:.0} nm: ", p.longueur_onde_nm), format!("{:.4} / {:.4} λ", red.pv_ondes, red.rms_ondes), DOS_WHITE),
        ligne(&trf!("PV/RMS {:.0} nm: ", "PV/RMS {:.0} nm: ", "PV/RMS {:.0} nm: ", LAMBDA_VISUELLE_NM), format!("{:.4} / {:.4} λ", red.pv_ondes_550, red.rms_ondes_550), DOS_YELLOW),
        ligne(tr!("Surface     : ", "Surface     : ", "Oberfläche  : "), format!("{:.1} / {:.1} nm", red.pv_surface_nm, red.rms_surface_nm), DOS_WHITE),
        ligne(tr!("Strehl 550  : ", "Strehl 550  : ", "Strehl 550  : "), format!("{:.3}", red.strehl_550), if ok { DOS_GREEN } else { DOS_RED }),
        ligne(
            tr!("Astigmatisme: ", "Astigmatism : ", "Astigmatism.: "),
            if p.retirer_astigmatisme { tr!("retiré", "removed", "entfernt") } else { tr!("gardé", "kept", "behalten") }.to_string(),
            DOS_WHITE,
        ),
    ];
    let aide = [
        tr!("X/x Y/y basculement   D/d défocalisation", "X/x Y/y tilt   D/d defocus", "X/x Y/y Kippung   D/d Defokus"),
        tr!("F franges droites   E/e écart   A astigmatisme", "F straight fringes   E/e separation   A astigmatism", "F gerade Streifen   E/e Abstand   A Astigmatismus"),
        tr!("K/k conique réelle   V/v conique visée", "K/k actual conic   V/v target conic", "K/k reale Konik   V/v Ziel-Konik"),
        tr!("G miroir du système   B/Échap fermer", "G system's mirror   B/Esc close", "G Spiegel des Systems   B/Esc schließen"),
        tr!("Photos d'interférogrammes : interface graphique", "Interferogram photos: graphical interface", "Interferogramm-Fotos: grafische Oberfläche"),
    ];
    let mut lignes = lignes;
    lignes.push(Line::from(""));
    lignes.extend(aide.iter().map(|t| Line::from(Span::styled(t.to_string(), Style::default().fg(DOS_CYAN)))));
    let rect = Rect::new(x, zone.y, zone.right().saturating_sub(x), zone.height);
    f.render_widget(Paragraph::new(lignes).block(bloc_dos(tr!("RÉDUCTION", "REDUCTION", "AUSWERTUNG"), false)), rect);
}
