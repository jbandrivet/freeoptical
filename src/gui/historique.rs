//! Annuler / Rétablir (Ctrl+Z, Ctrl+Y) : l'historique garde des copies du système (JSON).
//!
//! Plutôt que de demander à chaque endroit qui modifie le système (tableau des surfaces,
//! menus, optimiseur, fenêtres) d'enregistrer son changement, l'état est comparé à la fin de
//! chaque image : toute différence devient une étape. Pendant qu'un réglage est en cours
//! (bouton de la souris enfoncé, champ de saisie actif), les changements s'accumulent et ne
//! forment qu'une étape quand le geste se termine.

use crate::core::types::OpticalSystem;

/// Nombre d'étapes gardées (les plus anciennes sont oubliées)
pub const ETAPES_MAX: usize = 200;

#[derive(Debug, Default)]
pub struct Historique {
    /// États précédents, le plus récent en dernier
    annuler: Vec<String>,
    /// États défaits, le plus récent en dernier
    refaire: Vec<String>,
    /// Dernier état retenu (celui qu'une nouvelle modification ferait passer dans `annuler`)
    reference: String,
}

fn empreinte(sys: &OpticalSystem) -> String {
    serde_json::to_string(sys).unwrap_or_default()
}

impl Historique {
    /// Oublie tout et part de l'état donné (nouveau fichier, exemple chargé)
    pub fn reinitialiser(&mut self, sys: &OpticalSystem) {
        self.annuler.clear();
        self.refaire.clear();
        self.reference = empreinte(sys);
    }

    /// À appeler à la fin de chaque image : retient une modification terminée.
    /// `geste_en_cours` : l'utilisateur est en train de régler une valeur (attendre la fin).
    pub fn suivre(&mut self, sys: &OpticalSystem, geste_en_cours: bool) {
        if geste_en_cours {
            return;
        }
        self.valider(sys);
    }

    /// Retient tout de suite l'état courant s'il a changé ; vrai s'il a changé
    fn valider(&mut self, sys: &OpticalSystem) -> bool {
        let e = empreinte(sys);
        if e == self.reference {
            return false;
        }
        self.annuler.push(std::mem::replace(&mut self.reference, e));
        if self.annuler.len() > ETAPES_MAX {
            self.annuler.remove(0);
        }
        self.refaire.clear();
        true
    }

    pub fn peut_annuler(&self, sys: &OpticalSystem) -> bool {
        !self.annuler.is_empty() || empreinte(sys) != self.reference
    }

    pub fn peut_refaire(&self) -> bool {
        !self.refaire.is_empty()
    }

    pub fn nb_annulables(&self) -> usize {
        self.annuler.len()
    }

    /// Revient à l'état précédent ; une modification pas encore retenue est d'abord retenue
    /// (pour pouvoir la rétablir)
    pub fn annuler(&mut self, sys: &OpticalSystem) -> Option<OpticalSystem> {
        self.valider(sys);
        let precedent = self.annuler.pop()?;
        let restaure = serde_json::from_str(&precedent).ok()?;
        self.refaire.push(std::mem::replace(&mut self.reference, precedent));
        Some(restaure)
    }

    /// Rétablit l'état défait le plus récent
    pub fn refaire(&mut self, sys: &OpticalSystem) -> Option<OpticalSystem> {
        // Une modification faite après « Annuler » efface ce qui pouvait être rétabli
        if self.valider(sys) {
            return None;
        }
        let suivant = self.refaire.pop()?;
        let restaure = serde_json::from_str(&suivant).ok()?;
        self.annuler.push(std::mem::replace(&mut self.reference, suivant));
        Some(restaure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annuler_et_retablir() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        let e0 = sys.surfaces[0].epaisseur;
        let mut h = Historique::default();
        h.reinitialiser(&sys);
        assert!(!h.peut_annuler(&sys) && !h.peut_refaire());

        // Un réglage à la souris : plusieurs changements pendant le geste = une seule étape
        for k in 1..=5 {
            sys.surfaces[0].epaisseur = e0 + k as f64 * 0.1;
            h.suivre(&sys, true);
        }
        h.suivre(&sys, false);
        sys.nom = "renommé".into();
        h.suivre(&sys, false);
        assert_eq!(h.nb_annulables(), 2);

        // Annuler deux fois ramène à l'état d'origine
        sys = h.annuler(&sys).unwrap();
        assert_eq!(sys.nom, OpticalSystem::doublet_achromatique_defaut().nom);
        assert!((sys.surfaces[0].epaisseur - (e0 + 0.5)).abs() < 1e-12);
        sys = h.annuler(&sys).unwrap();
        assert_eq!(sys.surfaces[0].epaisseur, e0);
        assert!(h.annuler(&sys).is_none());

        // Rétablir refait dans l'ordre
        sys = h.refaire(&sys).unwrap();
        assert!((sys.surfaces[0].epaisseur - (e0 + 0.5)).abs() < 1e-12);
        sys = h.refaire(&sys).unwrap();
        assert_eq!(sys.nom, "renommé");
        assert!(h.refaire(&sys).is_none());

        // Une modification après « Annuler » efface ce qui pouvait être rétabli
        sys = h.annuler(&sys).unwrap();
        sys.surfaces[1].semi_diametre += 1.0;
        h.suivre(&sys, false);
        assert!(!h.peut_refaire());
        assert!(h.refaire(&sys).is_none());

        // Une modification pas encore retenue (geste en cours) est annulable, puis rétablissable
        sys.surfaces[2].epaisseur += 3.0;
        h.suivre(&sys, true);
        let modifie = sys.surfaces[2].epaisseur;
        sys = h.annuler(&sys).unwrap();
        assert_eq!(sys.surfaces[2].epaisseur, modifie - 3.0);
        sys = h.refaire(&sys).unwrap();
        assert_eq!(sys.surfaces[2].epaisseur, modifie);
    }

    #[test]
    fn nombre_d_etapes_borne() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        let mut h = Historique::default();
        h.reinitialiser(&sys);
        for k in 0..ETAPES_MAX + 30 {
            sys.surfaces[0].epaisseur = 1.0 + k as f64;
            h.suivre(&sys, false);
        }
        assert_eq!(h.nb_annulables(), ETAPES_MAX);
    }
}
