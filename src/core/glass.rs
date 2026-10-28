use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// Longueurs d'onde de référence de Fraunhofer (en nanomètres)
pub const LAMBDA_F_NM: f64 = 486.1327; // Raie F (Hydrogène bleu)
pub const LAMBDA_D_NM: f64 = 587.5618; // Raie d (Hélium jaune)
pub const LAMBDA_C_NM: f64 = 656.2725; // Raie C (Hydrogène rouge)

/// Formule de dispersion d'un verre de catalogue, numérotée comme dans les fichiers `.agf`
/// (longueur d'onde en µm) : 1 Schott, 2 Sellmeier 1, 3 Herzberger, 4 Sellmeier 2, 5 Conrady,
/// 6 Sellmeier 3, 7 Handbook 1, 8 Handbook 2, 9 Sellmeier 4, 10 Extended, 11 Sellmeier 5,
/// 12 Extended 2, 13 Extended 3
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormuleDispersion {
    pub numero: u32,
    pub coefficients: Vec<f64>,
}

impl FormuleDispersion {
    pub fn sellmeier(b1: f64, c1: f64, b2: f64, c2: f64, b3: f64, c3: f64) -> Self {
        Self { numero: 2, coefficients: vec![b1, c1, b2, c2, b3, c3] }
    }

    /// Indice à la longueur d'onde `l` (µm) ; `None` si la formule est inconnue ou hors domaine
    pub fn indice(&self, l: f64) -> Option<f64> {
        let c = |i: usize| self.coefficients.get(i).copied().unwrap_or(0.0);
        let l2 = l * l;
        let sellmeier = |termes: usize| -> f64 {
            (0..termes).map(|k| c(2 * k) * l2 / (l2 - c(2 * k + 1))).sum::<f64>()
        };
        let puissances = |p: &[i32]| -> f64 { p.iter().enumerate().map(|(k, &e)| c(k) * l.powi(e)).sum() };
        let n2 = match self.numero {
            1 => puissances(&[0, 2, -2, -4, -6, -8]),
            2 => 1.0 + sellmeier(3),
            3 => {
                let ll = 1.0 / (l2 - 0.028);
                let n = c(0) + c(1) * ll + c(2) * ll * ll + c(3) * l2 + c(4) * l2 * l2 + c(5) * l2 * l2 * l2;
                return n.is_finite().then_some(n);
            }
            4 => 1.0 + c(0) + c(1) * l2 / (l2 - c(2) * c(2)) + c(3) / (l2 - c(4) * c(4)),
            5 => {
                let n = c(0) + c(1) / l + c(2) / l.powf(3.5);
                return n.is_finite().then_some(n);
            }
            6 => 1.0 + sellmeier(4),
            7 => c(0) + c(1) / (l2 - c(2)) - c(3) * l2,
            8 => c(0) + c(1) * l2 / (l2 - c(2)) - c(3) * l2,
            9 => c(0) + c(1) * l2 / (l2 - c(2)) + c(3) * l2 / (l2 - c(4)),
            10 => puissances(&[0, 2, -2, -4, -6, -8, -10, -12]),
            11 => 1.0 + sellmeier(5),
            12 => puissances(&[0, 2, -2, -4, -6, -8, 4, 6]),
            13 => puissances(&[0, 2, 4, -2, -4, -6, -8, -10, -12]),
            _ => return None,
        };
        (n2 > 0.0 && n2.is_finite()).then(|| n2.sqrt())
    }
}

/// Nature physique d'un milieu optique
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MediumKind {
    /// Vide ou Air ambiant (n = 1.0)
    Air,
    /// Surface réfléchissante (Miroir)
    Miroir,
    /// Verre optique : indice nd et nombre d'Abbe Vd, et formule de dispersion du catalogue
    /// quand elle est connue (sinon modèle de Cauchy à partir de nd et Vd)
    Verre {
        nom: String,
        nd: f64,
        vd: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        formule: Option<FormuleDispersion>,
    },
}

/// Définition d'un matériau optique dans le catalogue
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub nom: String,
    pub kind: MediumKind,
    /// Catalogue d'origine (« Schott », nom du fichier .agf…), vide pour un verre modèle
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub catalogue: String,
}

impl Material {
    pub fn air() -> Self {
        Self { nom: "AIR".to_string(), kind: MediumKind::Air, catalogue: String::new() }
    }

    pub fn miroir() -> Self {
        Self { nom: "MIROIR".to_string(), kind: MediumKind::Miroir, catalogue: String::new() }
    }

    /// Verre modèle défini par nd et Vd (dispersion de Cauchy)
    pub fn verre(nom: impl Into<String>, nd: f64, vd: f64) -> Self {
        let nom_str = nom.into().to_uppercase();
        Self {
            nom: nom_str.clone(),
            kind: MediumKind::Verre { nom: nom_str, nd, vd, formule: None },
            catalogue: String::new(),
        }
    }

    /// Verre de catalogue défini par sa formule de dispersion ; nd et Vd en sont déduits
    pub fn verre_formule(nom: impl Into<String>, formule: FormuleDispersion, catalogue: impl Into<String>) -> Self {
        let nom_str = nom.into().to_uppercase();
        let n = |l_nm: f64| formule.indice(l_nm * 1e-3).unwrap_or(1.0);
        let nd = n(LAMBDA_D_NM);
        let dn = n(LAMBDA_F_NM) - n(LAMBDA_C_NM);
        let vd = if dn.abs() > 1e-12 { (nd - 1.0) / dn } else { 0.0 };
        Self {
            nom: nom_str.clone(),
            kind: MediumKind::Verre { nom: nom_str, nd, vd, formule: Some(formule) },
            catalogue: catalogue.into(),
        }
    }

    /// Indique si le milieu correspond à un miroir (réflexion spéculaire)
    pub fn est_miroir(&self) -> bool {
        matches!(self.kind, MediumKind::Miroir)
    }

    /// Calcule l'indice de réfraction `n(λ)` (λ en nm) : formule du catalogue si elle existe,
    /// sinon loi de Cauchy n(λ) = A + B / λ² ajustée sur nd et Vd
    pub fn indice_refraction(&self, longueur_onde_nm: f64) -> f64 {
        match &self.kind {
            MediumKind::Air | MediumKind::Miroir => 1.0,
            MediumKind::Verre { nd, vd, formule, .. } => {
                if let Some(n) = formule.as_ref().and_then(|f| f.indice(longueur_onde_nm * 1e-3)) {
                    return n;
                }
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

    /// Nombre d'Abbe Vd (0 pour l'air et les miroirs)
    pub fn abbe(&self) -> f64 {
        match &self.kind {
            MediumKind::Verre { vd, .. } => *vd,
            _ => 0.0,
        }
    }
}

/// Verres intégrés : coefficients de Sellmeier des catalogues Schott (et Malitson pour la silice
/// fondue et la fluorine), avec leurs anciens noms en alias
fn verres_integres() -> &'static HashMap<String, Material> {
    static VERRES: OnceLock<HashMap<String, Material>> = OnceLock::new();
    VERRES.get_or_init(|| {
        let table: [(&[&str], [f64; 6], &str); 14] = [
            (&["N-BK7", "BK7"], [1.03961212, 0.00600069867, 0.231792344, 0.0200179144, 1.01046945, 103.560653], "Schott"),
            (&["N-K5", "K5"], [1.08511833, 0.00661099503, 0.199562005, 0.024110866, 0.930511663, 111.982777], "Schott"),
            (&["F2", "N-F2"], [1.34533359, 0.00997743871, 0.209073176, 0.0470450767, 0.937357162, 111.886764], "Schott"),
            (&["SF2"], [1.40301821, 0.0105795466, 0.231767504, 0.0493226978, 0.939056586, 112.405955], "Schott"),
            (&["N-SF11", "SF11"], [1.73759695, 0.013188707, 0.313747346, 0.0623068142, 1.89878101, 155.23629], "Schott"),
            (&["N-LAK9", "LAK9"], [1.46231905, 0.00724270156, 0.344399589, 0.0243353131, 1.15508372, 85.4686868], "Schott"),
            (&["N-SK16", "SK16"], [1.34317774, 0.00704687339, 0.241144399, 0.0229005, 0.994317969, 92.7508526], "Schott"),
            (&["N-BAF10", "BAF10"], [1.5851495, 0.00926681282, 0.143559385, 0.0424489805, 1.08521269, 105.613573], "Schott"),
            (&["N-FK51A", "FK51A"], [0.971247817, 0.00472301995, 0.216901417, 0.0153575612, 0.904651666, 168.68133], "Schott"),
            (&["N-PK52A", "PK52A"], [1.029607, 0.00516800155, 0.1880506, 0.0166658798, 0.736488165, 138.964129], "Schott"),
            (&["N-SF5", "SF5"], [1.52481889, 0.011254756, 0.187085527, 0.0588995392, 1.42729015, 129.141675], "Schott"),
            (&["N-SF6", "SF6"], [1.77931763, 0.0133714182, 0.338149866, 0.0617533621, 2.08734474, 174.01759], "Schott"),
            (&["SILICE", "FUSED_SILICA", "F_SILICA"], [0.6961663, 0.0046791482, 0.4079426, 0.0135120631, 0.8974794, 97.9340025], "Malitson"),
            (&["CAF2", "FLUORINE", "FLUORITE"], [0.5675888, 0.00252642999, 0.4710914, 0.0100783328, 3.8484723, 1200.55597], "Malitson"),
        ];
        let mut verres = HashMap::new();
        for (noms, c, origine) in table {
            let formule = FormuleDispersion::sellmeier(c[0], c[1], c[2], c[3], c[4], c[5]);
            for nom in noms {
                verres.insert(nom.to_string(), Material::verre_formule(*nom, formule.clone(), origine));
            }
        }
        verres
    })
}

/// Verres des catalogues `.agf` importés par l'utilisateur (communs à tous les systèmes)
fn catalogue_utilisateur() -> &'static RwLock<HashMap<String, Material>> {
    static CATALOGUE: OnceLock<RwLock<HashMap<String, Material>>> = OnceLock::new();
    CATALOGUE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Dossier des catalogues importés : `~/.config/freeoptical/verres`
pub fn dossier_catalogues() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("freeoptical").join("verres"))
}

/// Lit un catalogue Zemax `.agf` (ANSI ou UTF-16) et retourne ses verres
pub fn lire_agf(octets: &[u8], origine: &str) -> Result<Vec<Material>, String> {
    let texte = if octets.starts_with(&[0xFF, 0xFE]) || (octets.len() > 1 && octets[1] == 0 && octets[0] != 0) {
        let mots: Vec<u16> = octets
            .chunks_exact(2)
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect();
        String::from_utf16_lossy(&mots).trim_start_matches('\u{feff}').to_string()
    } else {
        octets.iter().map(|&o| o as char).collect()
    };
    let mut verres = Vec::new();
    let mut courant: Option<(String, u32, f64, f64)> = None;
    let terminer = |c: Option<(String, u32, f64, f64)>, coefficients: Vec<f64>, verres: &mut Vec<Material>| {
        if let Some((nom, numero, nd, vd)) = c {
            let formule = FormuleDispersion { numero, coefficients };
            let mut m = Material::verre_formule(&nom, formule.clone(), origine);
            // Formule inconnue ou sans coefficients : verre modèle d'après nd et Vd du catalogue
            if formule.indice(0.5875618).is_none_or(|n| (n - nd).abs() > 1e-3) && nd > 1.0 {
                m = Material::verre(&nom, nd, vd);
                m.catalogue = origine.to_string();
            }
            verres.push(m);
        }
    };
    let mut coefficients_courants = Vec::new();
    for ligne in texte.lines() {
        let mots: Vec<&str> = ligne.split_whitespace().collect();
        match mots.first().copied() {
            Some("NM") if mots.len() >= 3 => {
                terminer(courant.take(), std::mem::take(&mut coefficients_courants), &mut verres);
                let numero = mots[2].parse::<f64>().unwrap_or(0.0) as u32;
                let nd = mots.get(4).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                let vd = mots.get(5).and_then(|v| v.parse().ok()).unwrap_or(0.0);
                courant = Some((mots[1].to_string(), numero, nd, vd));
            }
            Some("CD") if courant.is_some() => {
                coefficients_courants = mots[1..].iter().filter_map(|v| v.parse().ok()).collect();
            }
            _ => {}
        }
    }
    terminer(courant.take(), coefficients_courants, &mut verres);
    if verres.is_empty() {
        return Err(tr!(
            "aucun verre trouvé (lignes NM / CD d'un catalogue .agf attendues)",
            "no glass found (NM / CD lines of an .agf catalogue expected)",
            "kein Glas gefunden (NM-/CD-Zeilen eines .agf-Katalogs erwartet)"
        )
        .to_string());
    }
    Ok(verres)
}

/// Importe un catalogue `.agf` dans le catalogue commun ; s'il le faut, le copie dans
/// `~/.config/freeoptical/verres` pour les prochains démarrages. Retourne le nombre de verres.
pub fn importer_catalogue_agf(chemin: &Path, conserver: bool) -> Result<usize, String> {
    let octets = std::fs::read(chemin).map_err(|e| format!("{} : {e}", chemin.display()))?;
    let origine = chemin
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let verres = lire_agf(&octets, &origine)?;
    let nb = verres.len();
    if let Ok(mut cat) = catalogue_utilisateur().write() {
        for m in verres {
            cat.insert(m.nom.clone(), m);
        }
    }
    if conserver {
        if let Some(dossier) = dossier_catalogues() {
            let cible = dossier.join(chemin.file_name().unwrap_or_default());
            if cible != chemin {
                std::fs::create_dir_all(&dossier).map_err(|e| e.to_string())?;
                std::fs::copy(chemin, &cible).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(nb)
}

/// Charge au démarrage les catalogues déjà importés
pub fn charger_catalogues_utilisateur() -> usize {
    let Some(dossier) = dossier_catalogues() else { return 0 };
    let Ok(entrees) = std::fs::read_dir(dossier) else { return 0 };
    entrees
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("agf")))
        .filter_map(|p| importer_catalogue_agf(&p, false).ok())
        .sum()
}

/// Ne garde, à la relecture d'un fichier, que les verres propres au système : les verres intégrés
/// enregistrés par les anciennes versions (modèle nd/Vd) sont remplacés par leur formule
fn sans_verres_integres<'de, D: serde::Deserializer<'de>>(d: D) -> Result<HashMap<String, Material>, D::Error> {
    let mut verres = HashMap::<String, Material>::deserialize(d)?;
    verres.retain(|nom, _| !verres_integres().contains_key(nom) && nom != "AIR" && nom != "MIROIR");
    Ok(verres)
}

/// Catalogue de verres d'un système : verres propres au système, puis catalogues importés,
/// puis verres intégrés
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlassCatalog {
    #[serde(default, deserialize_with = "sans_verres_integres")]
    verres: HashMap<String, Material>,
}

impl GlassCatalog {
    /// Catalogue standard (verres intégrés et catalogues importés)
    pub fn standard() -> Self {
        Self::default()
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
        if let Some(mat) = catalogue_utilisateur().read().ok().and_then(|c| c.get(&cle).cloned()) {
            return Some(mat);
        }
        if let Some(mat) = verres_integres().get(&cle) {
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

    /// Liste triée de tous les matériaux disponibles (AIR et MIROIR en tête)
    pub fn lister(&self) -> Vec<Material> {
        let mut tous: HashMap<String, Material> = verres_integres().clone();
        if let Ok(c) = catalogue_utilisateur().read() {
            tous.extend(c.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        tous.extend(self.verres.iter().map(|(k, v)| (k.clone(), v.clone())));
        let mut liste: Vec<Material> = tous.into_values().collect();
        liste.sort_by(|a, b| a.nom.cmp(&b.nom));
        let mut resultat = vec![Material::air(), Material::miroir()];
        resultat.extend(liste);
        resultat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verres_integres_conformes_aux_fiches() {
        // nd et Vd des fiches Schott / Malitson
        let fiches = [
            ("N-BK7", 1.51680, 64.17),
            ("N-K5", 1.52249, 59.48),
            ("F2", 1.62004, 36.37),
            ("SF2", 1.64769, 33.85),
            ("N-SF11", 1.78472, 25.68),
            ("N-LAK9", 1.69100, 54.71),
            ("N-SK16", 1.62041, 60.32),
            ("N-BAF10", 1.67003, 47.11),
            ("N-FK51A", 1.48656, 84.47),
            ("N-PK52A", 1.49700, 81.61),
            ("N-SF5", 1.67270, 32.25),
            ("N-SF6", 1.80518, 25.36),
            ("SILICE", 1.45846, 67.82),
            ("CAF2", 1.43385, 94.99),
        ];
        let cat = GlassCatalog::standard();
        for (nom, nd, vd) in fiches {
            let m = cat.resoudre(nom);
            let n = m.indice_refraction(LAMBDA_D_NM);
            assert!((n - nd).abs() < 5e-5, "{nom} : nd = {n}");
            assert!((m.abbe() - vd).abs() < 0.15, "{nom} : Vd = {}", m.abbe());
        }
    }

    #[test]
    fn lecture_catalogue_agf() {
        let agf = "CC Catalogue d'essai\r\nNM N-BK7 2 517642.251 1.5168 64.17 0 0 0\r\nGC \r\nED 7.1 0 2.51 0\r\n\
CD 1.03961212 6.00069867E-003 2.31792344E-001 2.00179144E-002 1.01046945 1.03560653E+002 0 0 0 0\r\n\
LD 0.3 2.5\r\nNM MODELE 99 0 1.6 40 0 0 0\r\nCD\r\n";
        let verres = lire_agf(agf.as_bytes(), "essai").unwrap();
        assert_eq!(verres.len(), 2);
        assert!((verres[0].indice_refraction(LAMBDA_D_NM) - 1.5168).abs() < 2e-5);
        assert_eq!(verres[0].catalogue, "essai");
        // Formule inconnue : verre modèle nd / Vd du catalogue
        assert!((verres[1].indice_refraction(LAMBDA_D_NM) - 1.6).abs() < 1e-9);
        assert!((verres[1].abbe() - 40.0).abs() < 1e-9);
        // Même catalogue en UTF-16 (format des catalogues récents)
        let mut utf16 = vec![0xFF, 0xFE];
        for u in agf.encode_utf16() {
            utf16.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(lire_agf(&utf16, "essai").unwrap().len(), 2);
    }

    #[test]
    fn formules_de_dispersion() {
        // Schott (formule 1) équivalente au Sellmeier de N-BK7 à 1e-5 près sur le visible
        let schott = FormuleDispersion {
            numero: 1,
            coefficients: vec![2.2718929, -1.0108077e-2, 1.0592509e-2, 2.0816965e-4, -7.6472538e-6, 4.9240991e-7],
        };
        let n = schott.indice(0.5875618).unwrap();
        assert!((n - 1.5168).abs() < 2e-4, "{n}");
        assert!(FormuleDispersion { numero: 42, coefficients: vec![] }.indice(0.5).is_none());
    }
}
