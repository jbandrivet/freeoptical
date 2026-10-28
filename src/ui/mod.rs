//! Module UI : Interface Terminal (TUI) style MS-DOS avec `ratatui`, en français, anglais ou allemand.

pub mod analysis_view;
pub mod app;
pub mod lde;
pub mod ondes_view;
pub mod theme;
pub mod viewport_2d;
pub mod viewport_3d;

use crate::{
    core::types::OpticalSystem,
    ui::{
        analysis_view::{dessiner_aberration_spherique, dessiner_spot_diagram},
        app::{ActiveView, AppState, items_menu, titres_menus},
        lde::dessiner_editeur_lentilles,
        theme::{
            DOS_BAR_BG, DOS_BAR_FG, DOS_CYAN, DOS_MODAL_BG, DOS_TEXT, DOS_WHITE,
            DOS_YELLOW, bloc_dos, style_barre_dos, style_fond_dos, style_menu_actif,
        },
        viewport_2d::dessiner_coupe_2d,
        viewport_3d::dessiner_filaire_3d,
    },
};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
};
use std::{io, time::Duration};

/// Lance la boucle interactive du terminal TUI style MS-DOS
pub fn executer_tui(systeme_initial: OpticalSystem) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::nouveau(systeme_initial);
    let res = boucle_principale(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

fn boucle_principale(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> Result<()> {
    while !app.doit_quitter {
        app.preparer_vue();
        terminal.draw(|f| dessiner_interface(f, app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    gerer_clavier(app, key.code, key.modifiers);
                }
            }
        }
    }
    Ok(())
}

/// Dessine l'ensemble de l'écran MS-DOS (Barre de menu, LDE, Panneau inférieur, Barre de statut, Menus déroulants)
pub fn dessiner_interface(f: &mut Frame, app: &AppState) {
    let taille = f.area();
    f.render_widget(Block::default().style(style_fond_dos()), taille);

    let zones = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // Barre de menu supérieure DOS
            Constraint::Length(12),     // Éditeur de Données de Lentilles (LDE) + Bilan Paraxial
            Constraint::Min(10),        // Zone graphique / analytique 2D & 3D (Étape 5)
            Constraint::Length(1),      // Barre de raccourcis et de statut en bas
        ])
        .split(taille);

    dessiner_barre_menu(f, zones[0], app);
    dessiner_editeur_lentilles(f, zones[1], app);
    dessiner_zone_inferieure(f, zones[2], app);
    dessiner_barre_statut(f, zones[3], app);

    // Si un menu déroulant est ouvert, on l'affiche en surimpression (popup DOS)
    if let Some(menu_idx) = app.menu_ouvert {
        dessiner_popup_menu(f, zones[0], menu_idx, app.item_menu_selectionne);
    }
}

fn bandeau_version() -> String {
    format!(" ◄ FREEOPTICAL v{} ► ", crate::maj::VERSION_COURANTE)
}

fn dessiner_barre_menu(f: &mut Frame, zone: Rect, app: &AppState) {
    let mut spans = vec![Span::styled(
        bandeau_version(),
        Style::default()
            .bg(Color::Rgb(0, 128, 128))
            .fg(DOS_WHITE)
            .add_modifier(Modifier::BOLD),
    )];

    for (idx, titre) in titres_menus().iter().enumerate() {
        let est_actif = app.menu_ouvert == Some(idx);
        let style = if est_actif {
            style_menu_actif()
        } else {
            style_barre_dos()
        };
        spans.push(Span::styled(format!("  {}  ", titre), style));
    }

    spans.push(Span::styled(
        tr!(
            "  [F1-F8:Vues  C:Champ  W:λ  F10/M:Menu  F12:Langue]",
            "  [F1-F8:Views  C:Field  W:λ  F10/M:Menu  F12:Language]",
            "  [F1-F8:Ansichten  C:Feld  W:λ  F10/M:Menü  F12:Sprache]"
        ),
        Style::default().bg(DOS_BAR_BG).fg(Color::Rgb(60, 60, 60)),
    ));

    let barre = Paragraph::new(Line::from(spans)).style(style_barre_dos());
    f.render_widget(barre, zone);
}

fn dessiner_zone_inferieure(f: &mut Frame, zone: Rect, app: &AppState) {
    let bloc = bloc_dos(app.vue_active.titre(), false);
    let interieur = bloc.inner(zone);
    f.render_widget(bloc, zone);

    match app.vue_active {
        ActiveView::Coupe2D => dessiner_coupe_2d(f, interieur, app),
        ActiveView::Filaire3D => dessiner_filaire_3d(f, interieur, app),
        ActiveView::SpotDiagram => dessiner_spot_diagram(f, interieur, app),
        ActiveView::AberrationSpherique => dessiner_aberration_spherique(f, interieur, app),
        ActiveView::CatalogueVerres => dessiner_catalogue_verres(f, interieur, app),
        ActiveView::FrontOnde => ondes_view::dessiner_front_onde(f, interieur, app),
        ActiveView::Psf => ondes_view::dessiner_psf(f, interieur, app),
        ActiveView::Ftm => ondes_view::dessiner_ftm(f, interieur, app),
    }
}

fn dessiner_catalogue_verres(f: &mut Frame, zone: Rect, app: &AppState) {
    let mut lignes = vec![
        Line::from(Span::styled(
            tr!("MATÉRIAU       TYPE        INDICE nd (587.6nm)   ABBE Vd     INDICE nF (486.1nm)   INDICE nC (656.3nm)", "MATERIAL       TYPE        INDEX nd (587.6nm)    ABBE Vd     INDEX nF (486.1nm)    INDEX nC (656.3nm)", "MATERIAL       TYP         INDEX nd (587.6nm)    ABBE Vd     INDEX nF (486.1nm)    INDEX nC (656.3nm)"),
            Style::default().fg(DOS_YELLOW).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "══════════════════════════════════════════════════════════════════════════════════════════════════════",
            Style::default().fg(DOS_TEXT),
        )),
    ];

    for mat in app.systeme.catalogue.lister() {
        let nd = mat.indice_refraction(587.5618);
        let nf = mat.indice_refraction(486.1327);
        let nc = mat.indice_refraction(656.2725);
        let (type_str, vd_str) = match &mat.kind {
            crate::core::glass::MediumKind::Air => (tr!("Milieu", "Medium", "Medium"), "   -    ".to_string()),
            crate::core::glass::MediumKind::Miroir => (tr!("Réflecteur", "Reflector", "Reflektor"), "   -    ".to_string()),
            crate::core::glass::MediumKind::Verre { vd, .. } => (tr!("Verre Opt.", "Opt. glass", "Opt. Glas"), format!("{:>8.2}", vd)),
        };
        lignes.push(Line::from(vec![
            Span::styled(format!("{:<14} ", mat.nom), Style::default().fg(DOS_CYAN).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:<11} ", type_str), Style::default().fg(DOS_TEXT)),
            Span::styled(format!("{:>19.5}   ", nd), Style::default().fg(DOS_WHITE)),
            Span::styled(format!("{}    ", vd_str), Style::default().fg(DOS_YELLOW)),
            Span::styled(format!("{:>19.5}   ", nf), Style::default().fg(DOS_CYAN)),
            Span::styled(format!("{:>19.5}", nc), Style::default().fg(DOS_TEXT)),
        ]));
    }

    f.render_widget(Paragraph::new(lignes), zone);
}

fn dessiner_barre_statut(f: &mut Frame, zone: Rect, app: &AppState) {
    let raccourcis = tr!(" Flèches:Nav | Entrée:Éditer | A:Ajouter | Suppr:Effacer | T:Type | O:Optimiser DLS | Q:Quitter ", " Arrows:Nav | Enter:Edit | A:Add | Del:Delete | T:Type | O:Optimize DLS | Q:Quit ", " Pfeile:Nav | Eingabe:Bearb. | A:Neu | Entf:Löschen | T:Typ | O:DLS optim. | Q:Ende ");
    let ligne = Line::from(vec![
        Span::styled(
            raccourcis,
            Style::default()
                .bg(DOS_BAR_BG)
                .fg(DOS_BAR_FG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" │ {}", app.statut_message),
            Style::default().bg(Color::Rgb(0, 90, 140)).fg(DOS_WHITE),
        ),
    ]);
    f.render_widget(Paragraph::new(ligne).style(style_barre_dos()), zone);
}

fn dessiner_popup_menu(f: &mut Frame, barre_haut: Rect, menu_idx: usize, item_sel: usize) {
    let items = items_menu(menu_idx);
    // Le menu s'ouvre sous son titre : bandeau de version puis titres de largeur variable
    let x_offset = (bandeau_version().chars().count()
        + titres_menus()[..menu_idx].iter().map(|t| t.chars().count() + 4).sum::<usize>()) as u16;
    let largeur = (items.iter().map(|t| t.chars().count()).max().unwrap_or(20) + 4) as u16;
    let hauteur = (items.len() as u16) + 2;

    let popup_rect = Rect {
        x: barre_haut.x.saturating_add(x_offset).min(f.area().width.saturating_sub(largeur)),
        y: barre_haut.y + 1,
        width: largeur.min(f.area().width),
        height: hauteur.min(f.area().height.saturating_sub(2)),
    };

    f.render_widget(Clear, popup_rect);

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, texte)| {
            let style = if i == item_sel {
                style_menu_actif()
            } else {
                Style::default().bg(DOS_MODAL_BG).fg(DOS_WHITE)
            };
            ListItem::new(format!(" {} ", texte)).style(style)
        })
        .collect();

    let bloc = Block::default()
        .title(format!(" {} ", titres_menus()[menu_idx]))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .style(Style::default().bg(DOS_MODAL_BG).fg(DOS_YELLOW));

    f.render_widget(List::new(list_items).block(bloc), popup_rect);
}

fn gerer_clavier(app: &mut AppState, code: KeyCode, modifiers: KeyModifiers) {
    // 1. Mode édition d'une cellule dans le LDE
    if let Some(mut texte) = app.mode_saisie.take() {
        match code {
            KeyCode::Esc => {
                app.statut_message = tr!("Édition annulée.", "Edit cancelled.", "Bearbeitung abgebrochen.").to_string();
            }
            KeyCode::Enter => {
                app.appliquer_saisie(&texte);
            }
            KeyCode::Backspace => {
                texte.pop();
                app.mode_saisie = Some(texte);
            }
            KeyCode::Char(c) => {
                texte.push(c);
                app.mode_saisie = Some(texte);
            }
            _ => {
                app.mode_saisie = Some(texte);
            }
        }
        return;
    }

    // 2. Navigation dans le menu supérieur déroulant s'il est ouvert
    if let Some(menu_idx) = app.menu_ouvert {
        let nb_items = items_menu(menu_idx).len();
        match code {
            KeyCode::Esc | KeyCode::F(10) => {
                app.menu_ouvert = None;
            }
            KeyCode::Left => {
                let nouv = if menu_idx == 0 {
                    titres_menus().len() - 1
                } else {
                    menu_idx - 1
                };
                app.menu_ouvert = Some(nouv);
                app.item_menu_selectionne = 0;
            }
            KeyCode::Right => {
                let nouv = (menu_idx + 1) % titres_menus().len();
                app.menu_ouvert = Some(nouv);
                app.item_menu_selectionne = 0;
            }
            KeyCode::Up => {
                app.item_menu_selectionne = if app.item_menu_selectionne == 0 {
                    nb_items - 1
                } else {
                    app.item_menu_selectionne - 1
                };
            }
            KeyCode::Down => {
                app.item_menu_selectionne = (app.item_menu_selectionne + 1) % nb_items;
            }
            KeyCode::Enter => {
                app.executer_action_menu(menu_idx, app.item_menu_selectionne);
            }
            _ => {}
        }
        return;
    }

    // 3. Mode normal (LDE + Raccourcis globaux)
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        app.doit_quitter = true;
        return;
    }

    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => app.doit_quitter = true,
        KeyCode::F(10) | KeyCode::Char('m') | KeyCode::Char('M') => {
            app.menu_ouvert = Some(0);
            app.item_menu_selectionne = 0;
        }
        KeyCode::F(1) | KeyCode::Char('1') => app.vue_active = ActiveView::Coupe2D,
        KeyCode::F(2) | KeyCode::Char('2') => app.vue_active = ActiveView::Filaire3D,
        KeyCode::F(3) | KeyCode::Char('3') => app.vue_active = ActiveView::SpotDiagram,
        KeyCode::F(4) | KeyCode::Char('4') => app.vue_active = ActiveView::AberrationSpherique,
        KeyCode::F(5) | KeyCode::Char('5') => app.vue_active = ActiveView::CatalogueVerres,
        KeyCode::F(6) | KeyCode::Char('6') => app.vue_active = ActiveView::FrontOnde,
        KeyCode::F(7) | KeyCode::Char('7') => app.vue_active = ActiveView::Psf,
        KeyCode::F(8) | KeyCode::Char('8') => app.vue_active = ActiveView::Ftm,
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.ondes.champ_suivant(&app.systeme);
        }
        KeyCode::Char('w') | KeyCode::Char('W') => {
            app.ondes.lambda_suivante(&app.systeme);
        }
        KeyCode::F(12) => app.changer_langue(crate::i18n::langue().suivante()),
        KeyCode::Up => {
            if app.ligne_selectionnee > 0 {
                app.ligne_selectionnee -= 1;
            }
        }
        KeyCode::Down => {
            if app.ligne_selectionnee + 1 < app.systeme.surfaces.len() {
                app.ligne_selectionnee += 1;
            }
        }
        KeyCode::Left => {
            if app.colonne_selectionnee > 0 {
                app.colonne_selectionnee -= 1;
            }
        }
        KeyCode::Right | KeyCode::Tab => {
            app.colonne_selectionnee = (app.colonne_selectionnee + 1) % 7;
        }
        KeyCode::Enter | KeyCode::Char('e') | KeyCode::Char('E') => {
            app.mode_saisie = Some(app.valeur_cellule_courante());
        }
        KeyCode::Char('i') | KeyCode::Char('I') | KeyCode::Insert => {
            app.inserer_surface_courante();
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            app.ajouter_surface_avant_image();
        }
        KeyCode::Delete | KeyCode::Char('x') | KeyCode::Char('X') => {
            app.supprimer_surface_courante();
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            app.basculer_type_surface();
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            app.definir_diaphragme();
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            app.lancer_optimisation_rapide();
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            app.mise_au_point_paraxiale();
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            app.recalculer();
            app.statut_message = tr!("Calcul séquentiel mis à jour.", "Sequential computation updated.", "Sequentielle Berechnung aktualisiert.").to_string();
        }
        // Contrôles de rotation 3D (utilisés en Vue 2 : Filaire 3D)
        KeyCode::Char('h') | KeyCode::Char('H') => app.angle_3d_yaw_deg -= 5.0,
        KeyCode::Char('l') | KeyCode::Char('L') => app.angle_3d_yaw_deg += 5.0,
        KeyCode::Char('j') | KeyCode::Char('J') => app.angle_3d_pitch_deg -= 5.0,
        KeyCode::Char('k') | KeyCode::Char('K') => app.angle_3d_pitch_deg += 5.0,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_rendu_tui_toutes_les_vues() {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = AppState::nouveau(OpticalSystem::doublet_achromatique_defaut());

        for vue in ActiveView::TOUTES {
            app.vue_active = vue;
            app.preparer_vue();
            terminal.draw(|f| dessiner_interface(f, &app)).unwrap();
        }
    }
}

