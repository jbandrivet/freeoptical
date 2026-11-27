//! Plusieurs configurations d'un même système (éditeur multi-configuration de Zemax).
//!
//! Une configuration est une variante du système : autre mise au point, autre distance d'objet,
//! autre bande de longueurs d'onde, réseau tourné, miroir basculé… Chaque **opérande** désigne un
//! paramètre (épaisseur de la surface 5, champ 2, ouverture…) et porte une valeur par
//! configuration ; tout le reste du système est commun.
//!
//! Règle de cohérence : les surfaces et paramètres du système contiennent toujours les valeurs de
//! la configuration **active**. On modifie donc le système comme d'habitude ; `capturer_configuration`
//! recopie ses valeurs dans la colonne active, `appliquer_configuration` écrit une colonne dans le
//! système. Les analyses portent sur la configuration active ; l'optimiseur peut les évaluer toutes.

use crate::core::types::{ApertureSpec, OpticalSystem, SurfaceType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Paramètre piloté par un opérande de configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TypeOperande {
    /// Rayon de courbure de la surface `index` (mm, « inf » pour un plan)
    Rayon,
    /// Épaisseur après la surface `index` (mm)
    Epaisseur,
    /// Constante conique de la surface `index`
    Conique,
    /// Matériau après la surface `index` (nom du verre, AIR, MIROIR, « nd:Vd »)
    Verre,
    /// Demi-diamètre de la surface `index` (mm)
    DemiDiametre,
    /// Décentrement en X de la surface `index` (mm)
    DecentrementX,
    /// Décentrement en Y de la surface `index` (mm)
    DecentrementY,
    /// Inclinaison autour de X de la surface `index` (degrés)
    InclinaisonX,
    /// Inclinaison autour de Y de la surface `index` (degrés)
    InclinaisonY,
    /// Rotation autour de Z d'une rupture de repère `index` (degrés)
    InclinaisonZ,
    /// Rayon de l'obstruction centrale de la surface `index` (mm)
    Obstruction,
    /// Traits par mm du réseau de la surface `index`
    ReseauTraits,
    /// Ordre de diffraction du réseau de la surface `index`
    ReseauOrdre,
    /// Valeur de l'ouverture (pupille d'entrée en mm, F/# ou ouverture numérique, selon le genre choisi)
    Ouverture,
    /// Distance de l'objet à la 1re surface (mm, « inf » pour un objet à l'infini)
    DistanceObjet,
    /// Composante X du champ numéro `index` (degrés, ou mm pour un objet proche)
    ChampX,
    /// Composante Y du champ numéro `index`
    ChampY,
    /// Longueur d'onde numéro `index` (nm)
    LongueurOnde,
    /// Longueur d'onde de référence (nm)
    LongueurOndeRef,
}

impl TypeOperande {
    pub const TOUS: [TypeOperande; 19] = [
        TypeOperande::Rayon,
        TypeOperande::Epaisseur,
        TypeOperande::Conique,
        TypeOperande::Verre,
        TypeOperande::DemiDiametre,
        TypeOperande::DecentrementX,
        TypeOperande::DecentrementY,
        TypeOperande::InclinaisonX,
        TypeOperande::InclinaisonY,
        TypeOperande::InclinaisonZ,
        TypeOperande::Obstruction,
        TypeOperande::ReseauTraits,
        TypeOperande::ReseauOrdre,
        TypeOperande::Ouverture,
        TypeOperande::DistanceObjet,
        TypeOperande::ChampX,
        TypeOperande::ChampY,
        TypeOperande::LongueurOnde,
        TypeOperande::LongueurOndeRef,
    ];

    /// Vrai si `index` désigne une surface (sinon un champ, une longueur d'onde, ou rien)
    pub fn porte_sur_surface(self) -> bool {
        !matches!(
            self,
            TypeOperande::Ouverture
                | TypeOperande::DistanceObjet
                | TypeOperande::ChampX
                | TypeOperande::ChampY
                | TypeOperande::LongueurOnde
                | TypeOperande::LongueurOndeRef
        )
    }

    /// Vrai si `index` est utile (surface, champ ou longueur d'onde)
    pub fn a_un_index(self) -> bool {
        !matches!(self, TypeOperande::Ouverture | TypeOperande::DistanceObjet | TypeOperande::LongueurOndeRef)
    }

    /// Paramètre que l'optimiseur peut faire varier dans chaque configuration
    pub fn peut_varier(self) -> bool {
        matches!(
            self,
            TypeOperande::Rayon
                | TypeOperande::Epaisseur
                | TypeOperande::Conique
                | TypeOperande::DecentrementX
                | TypeOperande::DecentrementY
                | TypeOperande::InclinaisonX
                | TypeOperande::InclinaisonY
                | TypeOperande::InclinaisonZ
        )
    }

    /// Nom court affiché dans l'éditeur
    pub fn libelle(self) -> &'static str {
        match self {
            TypeOperande::Rayon => tr!("Rayon", "Radius", "Radius"),
            TypeOperande::Epaisseur => tr!("Épaisseur", "Thickness", "Dicke"),
            TypeOperande::Conique => tr!("Conique", "Conic", "Konik"),
            TypeOperande::Verre => tr!("Verre", "Glass", "Glas"),
            TypeOperande::DemiDiametre => tr!("Demi-diamètre", "Semi-diameter", "Halbdurchmesser"),
            TypeOperande::DecentrementX => tr!("Décentrement X", "Decenter X", "Dezentrierung X"),
            TypeOperande::DecentrementY => tr!("Décentrement Y", "Decenter Y", "Dezentrierung Y"),
            TypeOperande::InclinaisonX => tr!("Inclinaison X°", "Tilt X°", "Kippung X°"),
            TypeOperande::InclinaisonY => tr!("Inclinaison Y°", "Tilt Y°", "Kippung Y°"),
            TypeOperande::InclinaisonZ => tr!("Rotation Z°", "Rotation Z°", "Drehung Z°"),
            TypeOperande::Obstruction => tr!("Obstruction", "Obstruction", "Obstruktion"),
            TypeOperande::ReseauTraits => tr!("Réseau tr/mm", "Grating l/mm", "Gitter L/mm"),
            TypeOperande::ReseauOrdre => tr!("Ordre du réseau", "Grating order", "Gitterordnung"),
            TypeOperande::Ouverture => tr!("Ouverture", "Aperture", "Öffnung"),
            TypeOperande::DistanceObjet => tr!("Distance objet", "Object distance", "Objektabstand"),
            TypeOperande::ChampX => tr!("Champ X", "Field X", "Feld X"),
            TypeOperande::ChampY => tr!("Champ Y", "Field Y", "Feld Y"),
            TypeOperande::LongueurOnde => tr!("Longueur d'onde", "Wavelength", "Wellenlänge"),
            TypeOperande::LongueurOndeRef => tr!("λ de référence", "Reference λ", "Referenz-λ"),
        }
    }
}

/// Valeur d'un opérande dans une configuration : un nombre, ou un texte (nom de verre, « inf »)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ValeurConfig {
    Nombre(f64),
    Texte(String),
}

impl ValeurConfig {
    /// Nombre, l'infini étant écrit « inf » (le JSON n'a pas d'infini)
    pub fn depuis_nombre(v: f64) -> Self {
        if v.is_finite() { ValeurConfig::Nombre(v) } else { ValeurConfig::Texte("inf".into()) }
    }

    /// Valeur numérique : « inf », « infini », « ∞ » donnent l'infini ; la virgule est acceptée
    pub fn nombre(&self) -> Option<f64> {
        match self {
            ValeurConfig::Nombre(v) => Some(*v),
            ValeurConfig::Texte(t) => {
                let t = t.trim().to_lowercase().replace(',', ".");
                match t.as_str() {
                    "inf" | "infini" | "infinity" | "unendlich" | "∞" | "+inf" => Some(f64::INFINITY),
                    "-inf" => Some(f64::NEG_INFINITY),
                    _ => t.parse::<f64>().ok(),
                }
            }
        }
    }

    /// Texte affiché ou nom de verre
    pub fn texte(&self) -> String {
        match self {
            ValeurConfig::Nombre(v) => format!("{v}"),
            ValeurConfig::Texte(t) => t.clone(),
        }
    }
}

/// Ligne de l'éditeur multi-configuration : un paramètre et sa valeur dans chaque configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OperandeConfig {
    /// Paramètre piloté (`"epaisseur"`, `"rayon"`, `"verre"`, `"champ_y"`, `"distance_objet"`…)
    #[serde(rename = "type")]
    pub type_op: TypeOperande,
    /// Surface (0..N-1), champ ou longueur d'onde (0 = premier) concerné ; ignoré pour
    /// l'ouverture, la distance de l'objet et la longueur d'onde de référence
    #[serde(default)]
    pub index: usize,
    /// Une valeur par configuration (nombre, « inf » ou nom de verre) ; les valeurs manquantes
    /// prennent celle du système
    #[serde(default)]
    pub valeurs: Vec<ValeurConfig>,
    /// L'optimiseur fait varier ce paramètre séparément dans chaque configuration
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub variable: bool,
}

/// Configurations du système (vide : une seule configuration, le système tel quel)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Configurations {
    /// Nom de chaque configuration (« Infini », « Objet à 5 m »…) ; vide = une seule
    #[serde(default)]
    pub noms: Vec<String>,
    /// Configuration active (0 = première) : celle que contiennent les surfaces du système
    #[serde(default)]
    pub active: usize,
    /// Paramètres qui changent d'une configuration à l'autre
    #[serde(default)]
    pub operandes: Vec<OperandeConfig>,
}

impl Configurations {
    /// Nombre de configurations (au moins 1)
    pub fn nb(&self) -> usize {
        self.noms.len().max(1)
    }

    /// Rien à enregistrer : une configuration et aucun opérande
    pub fn est_vide(&self) -> bool {
        self.noms.len() <= 1 && self.operandes.is_empty()
    }

    /// Nom de la configuration `k` (« Config 2 » s'il n'est pas donné)
    pub fn nom(&self, k: usize) -> String {
        match self.noms.get(k) {
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => trf!("Config {}", "Config {}", "Konfig. {}", k + 1),
        }
    }
}

fn surface_absente(i: usize) -> String {
    trf!("surface {} absente", "surface {} missing", "Fläche {} fehlt", i + 1)
}

impl OpticalSystem {
    /// Nombre de configurations du système
    pub fn nb_configurations(&self) -> usize {
        self.configurations.nb()
    }

    /// Valeur actuelle, dans le système, du paramètre désigné par l'opérande
    pub fn lire_operande(&self, type_op: TypeOperande, index: usize) -> Result<ValeurConfig, String> {
        use TypeOperande as T;
        if type_op.porte_sur_surface() {
            let s = self.surfaces.get(index).ok_or_else(|| surface_absente(index))?;
            let v = match type_op {
                T::Rayon => s.rayon,
                T::Epaisseur => s.epaisseur,
                T::Conique => s.conique,
                T::Verre => return Ok(ValeurConfig::Texte(s.verre.clone())),
                T::DemiDiametre => s.semi_diametre,
                T::DecentrementX => s.decentrement[0],
                T::DecentrementY => s.decentrement[1],
                T::InclinaisonX => s.inclinaison_deg[0],
                T::InclinaisonY => s.inclinaison_deg[1],
                T::InclinaisonZ => s.inclinaison_z_deg,
                T::Obstruction => s.rayon_obstruction,
                T::ReseauTraits => s.reseau_traits_mm,
                T::ReseauOrdre => s.reseau_ordre as f64,
                _ => unreachable!(),
            };
            return Ok(ValeurConfig::depuis_nombre(v));
        }
        let v = match type_op {
            T::Ouverture => match self.ouverture {
                ApertureSpec::PupilleEntreeMm(v) | ApertureSpec::NombreOuverture(v) | ApertureSpec::OuvertureNumeriqueObjet(v) => v,
            },
            T::DistanceObjet => self.objet_fini().unwrap_or(f64::INFINITY),
            T::ChampX | T::ChampY => {
                let c = self.champs_deg.get(index).ok_or_else(|| trf!("champ {} absent", "field {} missing", "Feld {} fehlt", index + 1))?;
                if type_op == T::ChampX { c.0 } else { c.1 }
            }
            T::LongueurOnde => {
                self.longueurs_onde
                    .get(index)
                    .ok_or_else(|| trf!("longueur d'onde {} absente", "wavelength {} missing", "Wellenlänge {} fehlt", index + 1))?
                    .valeur_nm
            }
            T::LongueurOndeRef => self.longueur_onde_ref_nm,
            _ => unreachable!(),
        };
        Ok(ValeurConfig::depuis_nombre(v))
    }

    /// Écrit dans le système la valeur d'un opérande
    pub fn ecrire_operande(&mut self, type_op: TypeOperande, index: usize, valeur: &ValeurConfig) -> Result<(), String> {
        use TypeOperande as T;
        let invalide = || trf!("valeur « {} » invalide pour {}", "invalid value “{}” for {}", "ungültiger Wert „{}“ für {}", valeur.texte(), type_op.libelle());
        if type_op == T::Verre {
            let nom = valeur.texte().trim().to_string();
            if nom.is_empty() {
                return Err(invalide());
            }
            let connu = self.catalogue.est_connu(&nom);
            let s = self.surfaces.get_mut(index).ok_or_else(|| surface_absente(index))?;
            s.verre = nom.clone();
            return if connu { Ok(()) } else { Err(trf!("verre inconnu : {}", "unknown glass: {}", "unbekanntes Glas: {}", nom)) };
        }
        let v = valeur.nombre().ok_or_else(invalide)?;
        if type_op.porte_sur_surface() {
            let s = self.surfaces.get_mut(index).ok_or_else(|| surface_absente(index))?;
            let fini = || if v.is_finite() { Ok(v) } else { Err(invalide()) };
            match type_op {
                T::Rayon => {
                    if !v.is_finite() || v.abs() > 1e12 {
                        s.rayon = f64::INFINITY;
                    } else if v == 0.0 {
                        return Err(invalide());
                    } else {
                        s.rayon = v;
                        if s.type_surface == SurfaceType::Plane {
                            s.type_surface = SurfaceType::Spherique;
                        }
                    }
                }
                T::Epaisseur => s.epaisseur = fini()?,
                T::Conique => s.conique = fini()?,
                T::DemiDiametre => s.semi_diametre = fini()?.max(1e-6),
                T::DecentrementX => s.decentrement[0] = fini()?,
                T::DecentrementY => s.decentrement[1] = fini()?,
                T::InclinaisonX => s.inclinaison_deg[0] = fini()?,
                T::InclinaisonY => s.inclinaison_deg[1] = fini()?,
                T::InclinaisonZ => s.inclinaison_z_deg = fini()?,
                T::Obstruction => s.rayon_obstruction = fini()?.max(0.0),
                T::ReseauTraits => s.reseau_traits_mm = fini()?.max(0.0),
                T::ReseauOrdre => s.reseau_ordre = fini()?.round() as i32,
                _ => unreachable!(),
            }
            return Ok(());
        }
        match type_op {
            T::Ouverture => {
                if !(v.is_finite() && v > 0.0) {
                    return Err(invalide());
                }
                self.ouverture = match self.ouverture {
                    ApertureSpec::PupilleEntreeMm(_) => ApertureSpec::PupilleEntreeMm(v),
                    ApertureSpec::NombreOuverture(_) => ApertureSpec::NombreOuverture(v),
                    ApertureSpec::OuvertureNumeriqueObjet(_) => ApertureSpec::OuvertureNumeriqueObjet(v.min(0.99)),
                };
            }
            T::DistanceObjet => {
                self.distance_objet_mm = (v.is_finite() && v > 0.0).then_some(v);
            }
            T::ChampX | T::ChampY => {
                if !v.is_finite() {
                    return Err(invalide());
                }
                let c = self.champs_deg.get_mut(index).ok_or_else(|| trf!("champ {} absent", "field {} missing", "Feld {} fehlt", index + 1))?;
                if type_op == T::ChampX { c.0 = v } else { c.1 = v }
            }
            T::LongueurOnde => {
                if !(v.is_finite() && v > 0.0) {
                    return Err(invalide());
                }
                self.longueurs_onde
                    .get_mut(index)
                    .ok_or_else(|| trf!("longueur d'onde {} absente", "wavelength {} missing", "Wellenlänge {} fehlt", index + 1))?
                    .valeur_nm = v;
            }
            T::LongueurOndeRef => {
                if !(v.is_finite() && v > 0.0) {
                    return Err(invalide());
                }
                self.longueur_onde_ref_nm = v;
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Remet chaque opérande à `nb` valeurs (les manquantes prennent la valeur du système)
    fn completer_configurations(&mut self) {
        let nb = self.configurations.nb();
        if self.configurations.active >= nb {
            self.configurations.active = nb - 1;
        }
        for i in 0..self.configurations.operandes.len() {
            let (t, idx) = (self.configurations.operandes[i].type_op, self.configurations.operandes[i].index);
            let actuel = self.lire_operande(t, idx).ok();
            let op = &mut self.configurations.operandes[i];
            op.valeurs.truncate(nb);
            while op.valeurs.len() < nb {
                let v = actuel.clone().or_else(|| op.valeurs.last().cloned()).unwrap_or(ValeurConfig::Nombre(0.0));
                op.valeurs.push(v);
            }
        }
    }

    /// Recopie les valeurs du système dans la colonne de la configuration active
    pub fn capturer_configuration(&mut self) {
        if self.configurations.operandes.is_empty() {
            return;
        }
        self.completer_configurations();
        let k = self.configurations.active;
        for i in 0..self.configurations.operandes.len() {
            let (t, idx) = (self.configurations.operandes[i].type_op, self.configurations.operandes[i].index);
            if let Ok(v) = self.lire_operande(t, idx) {
                // Ne pas transformer « BK7 » en Texte identique ni 5 en 5.0 : comparer d'abord
                let op = &mut self.configurations.operandes[i];
                let meme = match (&op.valeurs[k], &v) {
                    (a, b) if a == b => true,
                    (a, ValeurConfig::Nombre(b)) => a.nombre() == Some(*b),
                    (a, ValeurConfig::Texte(b)) if t == TypeOperande::Verre => a.texte().trim() == b.trim(),
                    (a, b) => a.nombre().is_some() && a.nombre() == b.nombre(),
                };
                if !meme {
                    op.valeurs[k] = v;
                }
            }
        }
    }

    /// Écrit la configuration `k` dans le système et la rend active (sans recapturer l'ancienne :
    /// voir `changer_configuration`). Retourne les opérandes qui n'ont pas pu être appliqués.
    pub fn appliquer_configuration(&mut self, k: usize) -> Vec<String> {
        self.completer_configurations();
        let k = k.min(self.configurations.nb() - 1);
        self.configurations.active = k;
        let mut erreurs = Vec::new();
        for i in 0..self.configurations.operandes.len() {
            let op = self.configurations.operandes[i].clone();
            if let Some(v) = op.valeurs.get(k) {
                if let Err(e) = self.ecrire_operande(op.type_op, op.index, v) {
                    erreurs.push(trf!("opérande {} ({}) : {}", "operand {} ({}): {}", "Operand {} ({}): {}", i + 1, op.type_op.libelle(), e));
                }
            }
        }
        erreurs
    }

    /// Passe à la configuration `k` en gardant les modifications faites dans l'active
    pub fn changer_configuration(&mut self, k: usize) -> Vec<String> {
        self.capturer_configuration();
        self.appliquer_configuration(k)
    }

    /// Copie du système dans la configuration `k`, solves appliqués (le système reste intact)
    pub fn systeme_de_configuration(&self, k: usize) -> OpticalSystem {
        let mut s = self.clone();
        if s.configurations.nb() > 1 && k != s.configurations.active {
            s.changer_configuration(k);
        }
        crate::core::solves::appliquer_solves(&mut s);
        s
    }

    /// Ajoute une configuration, copie de la configuration `modele` ; retourne son numéro
    pub fn ajouter_configuration(&mut self, modele: usize, nom: Option<String>) -> usize {
        self.capturer_configuration();
        if self.configurations.noms.is_empty() {
            self.configurations.noms.push(self.configurations.nom(0));
        }
        self.completer_configurations();
        let modele = modele.min(self.configurations.nb() - 1);
        let k = self.configurations.noms.len();
        self.configurations.noms.push(nom.unwrap_or_else(|| self.configurations.nom(k)));
        for op in &mut self.configurations.operandes {
            let v = op.valeurs[modele].clone();
            op.valeurs.push(v);
        }
        k
    }

    /// Supprime la configuration `k` (il en reste toujours une)
    pub fn supprimer_configuration(&mut self, k: usize) {
        let nb = self.configurations.nb();
        if nb <= 1 || k >= nb {
            return;
        }
        self.capturer_configuration();
        self.configurations.noms.remove(k);
        for op in &mut self.configurations.operandes {
            if k < op.valeurs.len() {
                op.valeurs.remove(k);
            }
        }
        let active = self.configurations.active;
        if active == k {
            self.appliquer_configuration(k.min(self.configurations.nb() - 1));
        } else if active > k {
            self.configurations.active = active - 1;
        }
        if self.configurations.noms.len() == 1 && self.configurations.operandes.is_empty() {
            self.configurations.noms.clear();
        }
    }

    /// Ajoute un opérande : toutes les configurations prennent la valeur actuelle du système
    pub fn ajouter_operande(&mut self, type_op: TypeOperande, index: usize) -> Result<usize, String> {
        let v = self.lire_operande(type_op, index)?;
        let nb = self.configurations.nb();
        self.configurations.operandes.push(OperandeConfig { type_op, index, valeurs: vec![v; nb], variable: false });
        Ok(self.configurations.operandes.len() - 1)
    }

    /// Renumérote les opérandes après l'insertion d'une surface en `idx`
    pub fn surface_inseree(&mut self, idx: usize) {
        for op in &mut self.configurations.operandes {
            if op.type_op.porte_sur_surface() && op.index >= idx {
                op.index += 1;
            }
        }
    }

    /// Retire les opérandes de la surface supprimée en `idx` et renumérote les suivants
    pub fn surface_supprimee(&mut self, idx: usize) {
        self.configurations.operandes.retain(|op| !(op.type_op.porte_sur_surface() && op.index == idx));
        for op in &mut self.configurations.operandes {
            if op.type_op.porte_sur_surface() && op.index > idx {
                op.index -= 1;
            }
        }
    }

    /// Vrai si un opérande de configuration pilote ce paramètre de cette surface (il ne peut
    /// alors pas être une variable commune de l'optimiseur quand il y a plusieurs configurations)
    pub fn est_pilote_par_configuration(&self, type_op: TypeOperande, index: usize) -> bool {
        self.configurations.nb() > 1
            && self.configurations.operandes.iter().any(|op| op.type_op == type_op && op.index == index)
    }

    /// Vérifie les opérandes : retourne les problèmes (index absent, valeur illisible, verre inconnu)
    pub fn verifier_configurations(&self) -> Vec<String> {
        let mut erreurs = Vec::new();
        for k in 0..self.configurations.nb() {
            let mut s = self.clone();
            for e in s.appliquer_configuration(k) {
                erreurs.push(format!("{} — {e}", self.configurations.nom(k)));
            }
        }
        erreurs
    }
}

/// Bilan d'une configuration (pour comparer les configurations entre elles)
#[derive(Debug, Clone, Serialize)]
pub struct BilanConfiguration {
    pub numero: usize,
    pub nom: String,
    pub focale_mm: f64,
    pub f_nombre: f64,
    pub tirage_mm: f64,
    pub longueur_totale_mm: f64,
    /// Rayon RMS du spot de chaque champ (µm), toutes longueurs d'onde
    pub spot_rms_um: Vec<f64>,
    pub rayon_airy_um: f64,
}

/// Données paraxiales et spot de chaque champ, pour chaque configuration
pub fn bilan_configurations(sys: &OpticalSystem) -> Vec<BilanConfiguration> {
    (0..sys.nb_configurations())
        .map(|k| {
            let s = sys.systeme_de_configuration(k);
            let p = crate::core::paraxial::calculer_paraxial(&s);
            let champs = if s.champs_deg.is_empty() { vec![(0.0, 0.0)] } else { s.champs_deg.clone() };
            let mut airy = 0.0;
            let spot_rms_um = champs
                .iter()
                .map(|&c| {
                    let r = crate::core::analysis::calculer_spot_diagram(&s, c, 6);
                    airy = r.rayon_airy_um;
                    r.rayon_rms_um
                })
                .collect();
            BilanConfiguration {
                numero: k,
                nom: sys.configurations.nom(k),
                focale_mm: p.focale_effl_mm,
                f_nombre: p.nombre_ouverture_fnum,
                tirage_mm: p.tirage_bfl_mm,
                longueur_totale_mm: p.longueur_totale_mm,
                spot_rms_um,
                rayon_airy_um: airy,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doublet_deux_configs() -> OpticalSystem {
        let mut s = OpticalSystem::doublet_achromatique_defaut();
        s.ajouter_operande(TypeOperande::DistanceObjet, 0).unwrap();
        s.ajouter_operande(TypeOperande::Epaisseur, 2).unwrap();
        let k = s.ajouter_configuration(0, Some("5 m".into()));
        s.configurations.operandes[0].valeurs[k] = ValeurConfig::Nombre(5000.0);
        s.configurations.operandes[1].valeurs[k] = ValeurConfig::Nombre(95.7);
        s
    }

    #[test]
    fn changer_de_configuration_garde_les_modifications() {
        let mut s = doublet_deux_configs();
        assert_eq!(s.nb_configurations(), 2);
        assert_eq!(s.distance_objet_mm, None);
        let ep0 = s.surfaces[2].epaisseur;
        s.changer_configuration(1);
        assert_eq!(s.distance_objet_mm, Some(5000.0));
        assert_eq!(s.surfaces[2].epaisseur, 95.7);
        // Modification dans la config 2, puis retour à la 1 et de nouveau à la 2
        s.surfaces[2].epaisseur = 96.0;
        s.changer_configuration(0);
        assert_eq!(s.surfaces[2].epaisseur, ep0);
        assert_eq!(s.distance_objet_mm, None);
        s.changer_configuration(1);
        assert_eq!(s.surfaces[2].epaisseur, 96.0);
        // Paramètre non piloté : commun aux deux
        s.surfaces[0].rayon = 46.0;
        s.changer_configuration(0);
        assert_eq!(s.surfaces[0].rayon, 46.0);
        assert!(s.verifier_configurations().is_empty());
    }

    #[test]
    fn json_avec_infini_et_verre() {
        let mut s = doublet_deux_configs();
        s.ajouter_operande(TypeOperande::Verre, 1).unwrap();
        s.configurations.operandes[2].valeurs[1] = ValeurConfig::Texte("SF2".into());
        s.ajouter_operande(TypeOperande::Rayon, 3).unwrap(); // plan image : rayon infini
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"inf\""), "{json}");
        let mut r: OpticalSystem = serde_json::from_str(&json).unwrap();
        assert_eq!(r.configurations, s.configurations);
        r.changer_configuration(1);
        assert_eq!(r.surfaces[1].verre, "SF2");
        assert!(r.surfaces[3].rayon.is_infinite());
        // Un système sans configurations n'écrit rien de plus
        let simple = serde_json::to_string(&OpticalSystem::doublet_achromatique_defaut()).unwrap();
        assert!(!simple.contains("configurations"));
    }

    #[test]
    fn ajout_et_suppression_de_surfaces_et_configurations() {
        let mut s = doublet_deux_configs();
        s.surfaces.insert(1, crate::core::types::Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 1.0, "AIR", 10.0));
        s.surface_inseree(1);
        assert_eq!(s.configurations.operandes[1].index, 3);
        s.surfaces.remove(3);
        s.surface_supprimee(3);
        assert_eq!(s.configurations.operandes.len(), 1);
        // Supprimer la config active ramène à la précédente
        s.changer_configuration(1);
        s.supprimer_configuration(1);
        assert_eq!(s.nb_configurations(), 1);
        assert_eq!(s.distance_objet_mm, None);
    }

    #[test]
    fn valeurs_lues_en_texte() {
        assert_eq!(ValeurConfig::Texte("12,5".into()).nombre(), Some(12.5));
        assert_eq!(ValeurConfig::Texte("∞".into()).nombre(), Some(f64::INFINITY));
        assert_eq!(ValeurConfig::Texte("BK7".into()).nombre(), None);
        let mut s = OpticalSystem::doublet_achromatique_defaut();
        assert!(s.ecrire_operande(TypeOperande::Verre, 0, &ValeurConfig::Texte("PASUNVERRE".into())).is_err());
        assert!(s.ecrire_operande(TypeOperande::ChampY, 9, &ValeurConfig::Nombre(1.0)).is_err());
        assert!(s.ecrire_operande(TypeOperande::Epaisseur, 0, &ValeurConfig::Texte("abc".into())).is_err());
    }
}
