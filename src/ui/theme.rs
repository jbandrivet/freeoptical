use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders},
};

// Palette de couleurs authentique MS-DOS (Turbo Vision / Zemax DOS / Norton Commander)
pub const DOS_BG: Color = Color::Rgb(0, 0, 128); // Bleu foncé DOS (#000080)
pub const DOS_PANEL_BG: Color = Color::Rgb(0, 0, 155); // Bleu intermédiaire pour panneaux
pub const DOS_MODAL_BG: Color = Color::Rgb(0, 85, 170); // Bleu clair pour boîtes de dialogue
pub const DOS_BAR_BG: Color = Color::Rgb(192, 192, 192); // Gris clair pour barre de menu et statut
pub const DOS_BAR_FG: Color = Color::Rgb(0, 0, 0); // Noir sur gris clair
pub const DOS_TEXT: Color = Color::Rgb(220, 220, 220); // Gris clair DOS (#DCDCDC)
pub const DOS_WHITE: Color = Color::Rgb(255, 255, 255); // Blanc brillant
pub const DOS_YELLOW: Color = Color::Rgb(255, 255, 85); // Jaune vif DOS (#FFFF55)
pub const DOS_CYAN: Color = Color::Rgb(85, 255, 255); // Cyan vif DOS (#55FFFF)
pub const DOS_GREEN: Color = Color::Rgb(85, 255, 85); // Vert phosphore rétro (#55FF55)
pub const DOS_RED: Color = Color::Rgb(255, 85, 85); // Rouge clair DOS (#FF5555)
pub const DOS_MAGENTA: Color = Color::Rgb(255, 85, 255); // Magenta DOS (#FF55FF)

/// Style de base du fond d'écran DOS
pub fn style_fond_dos() -> Style {
    Style::default().bg(DOS_BG).fg(DOS_TEXT)
}

/// Style de la barre de menu supérieure et de la barre de statut
pub fn style_barre_dos() -> Style {
    Style::default().bg(DOS_BAR_BG).fg(DOS_BAR_FG)
}

/// Style d'un élément de menu sélectionné
pub fn style_menu_actif() -> Style {
    Style::default()
        .bg(Color::Rgb(0, 170, 170))
        .fg(DOS_BAR_FG)
        .add_modifier(Modifier::BOLD)
}

/// Style des en-têtes de colonnes du tableau LDE
pub fn style_entete_tableau() -> Style {
    Style::default()
        .bg(DOS_PANEL_BG)
        .fg(DOS_YELLOW)
        .add_modifier(Modifier::BOLD)
}

/// Style de la cellule active (curseur dans l'Éditeur de Données de Lentilles)
pub fn style_cellule_active() -> Style {
    Style::default()
        .bg(DOS_CYAN)
        .fg(Color::Rgb(0, 0, 64))
        .add_modifier(Modifier::BOLD)
}

/// Style de la ligne sélectionnée dans le tableau LDE
pub fn style_ligne_selectionnee() -> Style {
    Style::default()
        .bg(Color::Rgb(0, 60, 165))
        .fg(DOS_WHITE)
}

/// Crée un cadre MS-DOS à bordures doubles (`╔═══╗`)
pub fn bloc_dos<'a>(titre: &'a str, actif: bool) -> Block<'a> {
    let couleur_bord = if actif { DOS_YELLOW } else { DOS_TEXT };
    let style_titre = if actif {
        Style::default()
            .fg(DOS_YELLOW)
            .bg(DOS_BG)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DOS_WHITE).bg(DOS_BG)
    };

    Block::default()
        .title(format!(" {} ", titre))
        .title_style(style_titre)
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(couleur_bord).bg(DOS_BG))
        .style(style_fond_dos())
}
