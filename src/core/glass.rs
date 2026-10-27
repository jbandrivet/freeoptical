use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Longueurs d'onde de référence de Fraunhofer (en nanomètres)
pub const LAMBDA_F_NM: f64 = 486.1327; // Raie F (Hydrogène bleu)
pub const LAMBDA_D_NM: f64 = 587.5618; // Raie d (Hélium jaune)
pub const LAMBDA_C_NM: f64 = 656.2725; // Raie C (Hydrogène rouge)

/// Nature physique d'un milieu optique
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MediumKind {
    /// Vide ou Air ambiant (n = 1.0)
    Air,
    /// Surface réfléchissante (Miroir)
    Miroir,
    /// Verre optique réfractif défini par son indice nd et son nombre d'Abbe Vd
    Verre {
        nom: String,
        nd: f64,
        vd: f64,
    },
}

/// Définition d'un matériau optique dans le catalogue
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub nom: String,
    pub kind: MediumKind,
}

impl Material {
    pub fn air() -> Self {
        Self {
            nom: "AIR".to_string(),
            kind: MediumKind::Air,
        }
    }

    pub fn miroir() -> Self {
        Self {
            nom: "MIROIR".to_string(),
            kind: MediumKind::Miroir,
        }
    }

    pub fn verre(nom: impl Into<String>, nd: f64, vd: f64) -> Self {
        let nom_str = nom.into().to_uppercase();
        Self {
            nom: nom_str.clone(),
            kind: MediumKind::Verre {
                nom: nom_str,
                nd,
                vd,
            },
        }
    }

    /// Indique si le milieu correspond à un miroir (réflexion spéculaire)
    pub fn est_miroir(&self) -> bool {
        matches!(self.kind, MediumKind::Miroir)
    }

    /// Calcule l'indice de réfraction `n(λ)` à la longueur d'onde donnée (en nm)
    /// à partir de `nd` et de la constante d'Abbe `Vd` via la loi de dispersion de Cauchy :
    /// n(λ) = A + B / λ²
    pub fn indice_refraction(&self, longueur_onde_nm: f64) -> f64 {
        match &self.kind {
            MediumKind::Air | MediumKind::Miroir => 1.0,
            MediumKind::Verre { nd, vd, .. } => {
                if *vd <= 1e-6 || longueur_onde_nm <= 100.0 {
                    return *nd;
                }
                // Dispersion principale : nF - nC = (nd - 1) / Vd
                let delta_n = (nd - 1.0) / vd;
                let inv_f2 = 1.0 / (LAMBDA_F_NM * LAMBDA_F_NM);
                let inv_c2 = 1.0 / (LAMBDA_C_NM * LAMBDA_C_NM);
                let inv_d2 = 1.0 / (LAMBDA_D_NM * LAMBDA_D_NM);
                let inv_l2 = 1.0 / (longueur_onde_nm * longueur_onde_nm);

                let b = delta_n / (inv_f2 - inv_c2);
                let a = nd - b * inv_d2;
                a + b * inv_l2
            }
        }
    }
}

/// Catalogue de verres optiques (Schott / standards classiques)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlassCatalog {
    verres: HashMap<String, Material>,
}

impl Default for GlassCatalog {
    fn default() -> Self {
        Self::standard()
    }
}

impl GlassCatalog {
    /// Initialise le catalogue avec les verres standards (BK7, F2, SF11, SILICE, LAK9, SK16, BAF10, FK51A...)
    pub fn standard() -> Self {
        let mut verres = HashMap::new();
        let standards = [
            Material::air(),
            Material::miroir(),
            Material::verre("BK7", 1.51680, 64.17),
            Material::verre("N-BK7", 1.51680, 64.17),
            Material::verre("F2", 1.62004, 36.37),
            Material::verre("N-F2", 1.62004, 36.37),
            Material::verre("SF11", 1.78472, 25.76),
            Material::verre("N-SF11", 1.78472, 25.76),
            Material::verre("SILICE", 1.45846, 67.82),
            Material::verre("FUSED_SILICA", 1.45846, 67.82),
            Material::verre("LAK9", 1.69100, 54.71),
            Material::verre("SK16", 1.62041, 60.32),
            Material::verre("BAF10", 1.67003, 47.11),
            Material::verre("FK51A", 1.48656, 84.47),
            Material::verre("SF2", 1.64769, 33.85),
            Material::verre("K5", 1.52249, 59.48),
        ];

        for mat in standards {
            verres.insert(mat.nom.clone(), mat);
        }

        Self { verres }
    }

    /// Ajoute ou met à jour un verre dans le catalogue
    pub fn ajouter_verre(&mut self, nom: impl Into<String>, nd: f64, vd: f64) {
        let mat = Material::verre(nom, nd, vd);
        self.verres.insert(mat.nom.clone(), mat);
    }

    /// Résout un matériau par son nom ou via la syntaxe paramétrique `"nd:Vd"` (ex: `"1.5168:64.17"`) ou `"nd"`.
    /// Un nom inconnu est traité comme de l'air (voir [`GlassCatalog::est_connu`] pour le détecter).
    pub fn resoudre(&self, spec: &str) -> Material {
        self.chercher(spec).unwrap_or_else(Material::air)
    }

    /// Indique si le nom de matériau est reconnu (catalogue, AIR, MIROIR ou syntaxe `"nd:Vd"`)
    pub fn est_connu(&self, spec: &str) -> bool {
        self.chercher(spec).is_some()
    }

    fn chercher(&self, spec: &str) -> Option<Material> {
        let cle = spec.trim().to_uppercase();
        if cle.is_empty() || cle == "AIR" || cle == "VIDE" {
            return Some(Material::air());
        }
        if cle == "MIROIR" || cle == "MIRROR" {
            return Some(Material::miroir());
        }
        if let Some(mat) = self.verres.get(&cle) {
            return Some(mat.clone());
        }
        if let Some(mat) = Self::standard().verres.get(&cle) {
            return Some(mat.clone());
        }
        // Support d'un verre modèle au format "1.5168:64.17" ou "1.5168"
        if let Some((nd_str, vd_str)) = cle.split_once(':') {
            if let (Ok(nd), Ok(vd)) = (nd_str.parse::<f64>(), vd_str.parse::<f64>()) {
                return Some(Material::verre(cle, nd, vd));
            }
        }
        if let Ok(nd) = cle.parse::<f64>() {
            return Some(Material::verre(cle, nd, 0.0));
        }
        None
    }

    /// Liste triée des matériaux disponibles dans le catalogue
    pub fn lister(&self) -> Vec<&Material> {
        let mut liste: Vec<&Material> = self.verres.values().collect();
        liste.sort_by(|a, b| a.nom.cmp(&b.nom));
        liste
    }
}
