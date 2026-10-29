//! Édition des solves et des reprises de la surface sélectionnée

use super::{COL_DOS_CYAN, COL_DOS_GRAY};
use crate::core::solves::ParametreSolve;
use crate::core::types::Solve;
use eframe::egui::{self, RichText};

/// Genre de solve proposé dans la liste (sans ses valeurs)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Genre {
    Aucun,
    HauteurMarginale,
    HauteurPrincipale,
    AngleMarginal,
    AnglePrincipal,
    NombreOuverture,
    Aplanetique,
    Reprise,
}

impl Genre {
    fn de(solve: &Option<Solve>) -> Self {
        match solve {
            None => Genre::Aucun,
            Some(Solve::HauteurMarginale { .. }) => Genre::HauteurMarginale,
            Some(Solve::HauteurPrincipale { .. }) => Genre::HauteurPrincipale,
            Some(Solve::AngleMarginal { .. }) => Genre::AngleMarginal,
            Some(Solve::AnglePrincipal { .. }) => Genre::AnglePrincipal,
            Some(Solve::NombreOuverture { .. }) => Genre::NombreOuverture,
            Some(Solve::Aplanetique) => Genre::Aplanetique,
            Some(Solve::Reprise { .. }) => Genre::Reprise,
        }
    }

    fn libelle(self) -> &'static str {
        match self {
            Genre::Aucun => tr!("fixe", "fixed", "fest"),
            Genre::HauteurMarginale => tr!("Hauteur marginale (mise au point)", "Marginal ray height (focus)", "Randstrahlhöhe (Fokus)"),
            Genre::HauteurPrincipale => tr!("Hauteur principale (pupille)", "Chief ray height (pupil)", "Hauptstrahlhöhe (Pupille)"),
            Genre::AngleMarginal => tr!("Angle marginal", "Marginal ray angle", "Randstrahlwinkel"),
            Genre::AnglePrincipal => tr!("Angle principal", "Chief ray angle", "Hauptstrahlwinkel"),
            Genre::NombreOuverture => tr!("Nombre d'ouverture F/#", "F number F/#", "Blendenzahl F/#"),
            Genre::Aplanetique => tr!("Aplanétique", "Aplanatic", "Aplanatisch"),
            Genre::Reprise => tr!("Reprise (pickup)", "Pickup", "Übernahme (Pickup)"),
        }
    }

    fn valeur_defaut(self, index_surface: usize) -> Option<Solve> {
        match self {
            Genre::Aucun => None,
            Genre::HauteurMarginale => Some(Solve::HauteurMarginale { hauteur_mm: 0.0, zone: 0.0 }),
            Genre::HauteurPrincipale => Some(Solve::HauteurPrincipale { hauteur_mm: 0.0 }),
            Genre::AngleMarginal => Some(Solve::AngleMarginal { angle: -0.1 }),
            Genre::AnglePrincipal => Some(Solve::AnglePrincipal { angle: 0.0 }),
            Genre::NombreOuverture => Some(Solve::NombreOuverture { f_nombre: 5.0 }),
            Genre::Aplanetique => Some(Solve::Aplanetique),
            Genre::Reprise => Some(Solve::Reprise { surface: index_surface.max(1), facteur: 1.0, decalage: 0.0 }),
        }
    }
}

/// Liste déroulante et valeurs d'un solve ; `index_surface` est le numéro affiché (à partir de 1)
/// de la surface éditée. Retourne vrai si le solve a changé.
pub fn editeur_solve(
    ui: &mut egui::Ui,
    id: &str,
    libelle: &str,
    solve: &mut Option<Solve>,
    parametre: ParametreSolve,
    index_surface: usize,
) -> bool {
    let genres: &[Genre] = match parametre {
        ParametreSolve::Epaisseur => &[Genre::Aucun, Genre::HauteurMarginale, Genre::HauteurPrincipale, Genre::Reprise],
        ParametreSolve::Rayon => &[
            Genre::Aucun,
            Genre::AngleMarginal,
            Genre::AnglePrincipal,
            Genre::NombreOuverture,
            Genre::Aplanetique,
            Genre::Reprise,
        ],
        ParametreSolve::Conique => &[Genre::Aucun, Genre::Reprise],
    };
    let mut modifie = false;
    ui.label(RichText::new(libelle).monospace().color(COL_DOS_CYAN));
    let actuel = Genre::de(solve);
    egui::ComboBox::from_id_salt(id).selected_text(actuel.libelle()).width(150.0).show_ui(ui, |ui| {
        for &g in genres {
            if ui.selectable_label(g == actuel, g.libelle()).clicked() && g != actuel {
                // Reprise : par défaut depuis la surface précédente
                *solve = g.valeur_defaut(index_surface.saturating_sub(1));
                modifie = true;
            }
        }
    });
    let valeur = |ui: &mut egui::Ui, nom: &str, v: &mut f64, pas: f64, aide: &str| -> bool {
        ui.label(RichText::new(nom).monospace().color(COL_DOS_GRAY));
        ui.add(egui::DragValue::new(v).speed(pas).max_decimals(5)).on_hover_text(aide).changed()
    };
    match solve {
        Some(Solve::HauteurMarginale { hauteur_mm, zone }) => {
            modifie |= valeur(ui, "h", hauteur_mm, 0.01, tr!("Hauteur visée sur la surface suivante (mm), 0 = foyer", "Target height on the next surface (mm), 0 = focus", "Zielhöhe auf der nächsten Fläche (mm), 0 = Fokus"));
            modifie |= valeur(ui, tr!("zone", "zone", "Zone"), zone, 0.01, tr!("0 : rayon paraxial ; sinon rayon réel à cette fraction de la pupille (0,7071 : meilleur foyer en sphérique)", "0: paraxial ray; otherwise real ray at this pupil fraction (0.7071: best focus with spherical aberration)", "0: paraxialer Strahl; sonst realer Strahl bei diesem Pupillenanteil (0,7071: bester Fokus bei sphärischer Aberration)"));
            *zone = zone.clamp(-1.0, 1.0);
        }
        Some(Solve::HauteurPrincipale { hauteur_mm }) => {
            modifie |= valeur(ui, "h", hauteur_mm, 0.01, tr!("Hauteur du rayon principal sur la surface suivante (mm), 0 = pupille", "Chief ray height on the next surface (mm), 0 = pupil", "Hauptstrahlhöhe auf der nächsten Fläche (mm), 0 = Pupille"));
        }
        Some(Solve::AngleMarginal { angle }) | Some(Solve::AnglePrincipal { angle }) => {
            modifie |= valeur(ui, "u'", angle, 0.001, tr!("Pente paraxiale après la surface (dy/dz)", "Paraxial slope after the surface (dy/dz)", "Paraxiale Steigung nach der Fläche (dy/dz)"));
        }
        Some(Solve::NombreOuverture { f_nombre }) => {
            modifie |= valeur(ui, "F/", f_nombre, 0.05, tr!("Nombre d'ouverture image obtenu après cette surface", "Image F number obtained after this surface", "Bildseitige Blendenzahl nach dieser Fläche"));
        }
        Some(Solve::Reprise { surface, facteur, decalage }) => {
            ui.label(RichText::new("#").monospace().color(COL_DOS_GRAY));
            if ui.add(egui::DragValue::new(surface).range(1..=999)).on_hover_text(tr!("Surface source (numéro affiché)", "Source surface (displayed number)", "Quellfläche (angezeigte Nummer)")).changed() {
                modifie = true;
            }
            modifie |= valeur(ui, "×", facteur, 0.01, tr!("Facteur appliqué à la valeur reprise", "Scale factor applied to the picked-up value", "Faktor für den übernommenen Wert"));
            modifie |= valeur(ui, "+", decalage, 0.01, tr!("Décalage ajouté", "Offset added", "Addierter Versatz"));
        }
        Some(Solve::Aplanetique) | None => {}
    }
    modifie
}
