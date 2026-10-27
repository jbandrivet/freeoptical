use crate::ui::{
    app::AppState,
    theme::{
        DOS_BG, DOS_CYAN, DOS_GREEN, DOS_RED, DOS_TEXT, DOS_WHITE, DOS_YELLOW, bloc_dos,
        style_cellule_active, style_entete_tableau, style_ligne_selectionnee,
    },
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

/// Noms des colonnes de l'Éditeur de Données de Lentilles (LDE)
pub const COLONNES_LDE: [&str; 7] = [
    "Type",
    "Commentaire",
    "Rayon (mm)",
    "Épaisseur (mm)",
    "Verre",
    "Demi-Diam. (mm)",
    "Conique (k)",
];

/// Affiche l'Éditeur de Données de Lentilles (LDE) et le panneau récapitulatif paraxial à droite
pub fn dessiner_editeur_lentilles(f: &mut Frame, zone: Rect, app: &AppState) {
    let decoupe = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(zone);

    dessiner_tableau_lde(f, decoupe[0], app);
    dessiner_panneau_paraxial(f, decoupe[1], app);
}

fn dessiner_tableau_lde(f: &mut Frame, zone: Rect, app: &AppState) {
    let mut cellules_entete = vec![
        Cell::from("Surf#").style(style_entete_tableau()),
    ];
    for (col_idx, titre) in COLONNES_LDE.iter().enumerate() {
        let style = if col_idx == app.colonne_selectionnee {
            style_entete_tableau().fg(DOS_CYAN)
        } else {
            style_entete_tableau()
        };
        cellules_entete.push(Cell::from(*titre).style(style));
    }
    let entete = Row::new(cellules_entete).height(1);

    let nb_surfaces = app.systeme.surfaces.len();
    let mut lignes = Vec::with_capacity(nb_surfaces);

    for (idx, surf) in app.systeme.surfaces.iter().enumerate() {
        let est_ligne_active = idx == app.ligne_selectionnee;

        // Étiquette de surface (STOP, IMG, ou numéro)
        let tag_surf = if idx + 1 == nb_surfaces {
            format!("{:>2} IMG", idx + 1)
        } else if surf.est_diaphragme {
            format!("{:>2} STP", idx + 1)
        } else {
            format!("{:>2}    ", idx + 1)
        };

        let rayon_str = if !surf.rayon.is_finite() || surf.rayon.abs() > 1e8 {
            "INFINI".to_string()
        } else {
            format!("{:>10.3}", surf.rayon)
        };

        let valeurs = [
            surf.type_surface.libelle_court().to_string(),
            if surf.commentaire.is_empty() {
                "-".to_string()
            } else {
                surf.commentaire.clone()
            },
            rayon_str,
            format!("{:>10.3}", surf.epaisseur),
            surf.verre.clone(),
            format!("{:>10.3}", surf.semi_diametre),
            format!("{:>8.3}", surf.conique),
        ];

        let style_base = if est_ligne_active {
            style_ligne_selectionnee()
        } else {
            Style::default().bg(DOS_BG).fg(DOS_TEXT)
        };

        let mut cellules = Vec::with_capacity(8);
        let style_num = if surf.est_diaphragme {
            style_base.fg(DOS_YELLOW).add_modifier(Modifier::BOLD)
        } else {
            style_base.fg(DOS_CYAN)
        };
        cellules.push(Cell::from(tag_surf).style(style_num));

        for (col_idx, texte) in valeurs.iter().enumerate() {
            let est_cellule_active = est_ligne_active && col_idx == app.colonne_selectionnee;
            if est_cellule_active {
                if let Some(saisie) = &app.mode_saisie {
                    cellules.push(
                        Cell::from(format!("{}█", saisie)).style(
                            Style::default()
                                .bg(DOS_RED)
                                .fg(DOS_WHITE)
                                .add_modifier(Modifier::BOLD),
                        ),
                    );
                } else {
                    cellules.push(Cell::from(texte.clone()).style(style_cellule_active()));
                }
            } else {
                cellules.push(Cell::from(texte.clone()).style(style_base));
            }
        }

        lignes.push(Row::new(cellules).height(1));
    }

    let titre_lde = format!(
        "ÉDITEUR DE DONNÉES DE LENTILLES (LDE) - {}",
        app.systeme.nom
    );

    let largeurs = [
        Constraint::Length(7),
        Constraint::Length(11),
        Constraint::Min(16),
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Length(10),
        Constraint::Length(15),
        Constraint::Length(11),
    ];

    let table = Table::new(lignes, largeurs)
        .header(entete)
        .block(bloc_dos(&titre_lde, app.menu_ouvert.is_none()))
        .column_spacing(1);

    f.render_widget(table, zone);
}

fn dessiner_panneau_paraxial(f: &mut Frame, zone: Rect, app: &AppState) {
    let p = &app.paraxial;
    let s = &app.spot;
    let lsa = &app.lsa;

    let focale_txt = if p.focale_effl_mm.is_finite() {
        format!("{:.3} mm", p.focale_effl_mm)
    } else {
        "INFINIE (Afocal)".to_string()
    };

    let bfl_txt = if p.tirage_bfl_mm.is_finite() {
        format!("{:.3} mm", p.tirage_bfl_mm)
    } else {
        "INFINI".to_string()
    };

    let qualite_diffraction = if s.rayon_rms_um <= s.rayon_airy_um && s.rayon_airy_um > 0.0 {
        Span::styled(
            "LIMITÉ PAR LA DIFFRACTION",
            Style::default().fg(DOS_GREEN).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "LIMITÉ PAR LES ABERRATIONS",
            Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
        )
    };

    let lignes = vec![
        Line::from(vec![
            Span::styled("λ Réf.       : ", Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("{:.2} nm", app.systeme.longueur_onde_ref_nm),
                Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Focale (EFFL): ", Style::default().fg(DOS_TEXT)),
            Span::styled(focale_txt, Style::default().fg(DOS_CYAN).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Tirage (BFL) : ", Style::default().fg(DOS_TEXT)),
            Span::styled(bfl_txt, Style::default().fg(DOS_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Ouverture F/#: ", Style::default().fg(DOS_TEXT)),
            Span::styled(format!("F/{:.2}", p.nombre_ouverture_fnum), Style::default().fg(DOS_WHITE)),
            Span::styled(
                format!(" (NA={:.3})", p.ouverture_numerique_image),
                Style::default().fg(DOS_TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Pup. Entrée  : ", Style::default().fg(DOS_TEXT)),
            Span::styled(format!("Ø {:.2} mm", p.diametre_pupille_entree_mm), Style::default().fg(DOS_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Pup. Sortie  : ", Style::default().fg(DOS_TEXT)),
            Span::styled(format!("Ø {:.2} mm", p.diametre_pupille_sortie_mm), Style::default().fg(DOS_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Spot RMS     : ", Style::default().fg(DOS_TEXT)),
            Span::styled(
                format!("{:.2} µm", s.rayon_rms_um),
                Style::default().fg(DOS_GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" (Airy {:.2} µm)", s.rayon_airy_um),
                Style::default().fg(DOS_TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Aberr. Sphér.: ", Style::default().fg(DOS_TEXT)),
            Span::styled(format!("ΔZ = {:.3} mm", lsa.lsa_max_mm), Style::default().fg(DOS_WHITE)),
        ]),
        Line::from(vec![
            Span::styled("Diagnostic   : ", Style::default().fg(DOS_TEXT)),
            qualite_diffraction,
        ]),
    ];

    let para = Paragraph::new(lignes).block(bloc_dos("BILAN OPTIQUE & PARAXIAL", false));
    f.render_widget(para, zone);
}
