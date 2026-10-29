use crate::core::glass::{GlassCatalog, LAMBDA_C_NM, LAMBDA_D_NM, LAMBDA_F_NM};
use nalgebra::{Point3 as NaPoint3, Vector3 as NaVector3};
use serde::{Deserialize, Serialize};

/// Alias pour un vecteur 3D en double précision (cosinus directeurs, normales)
pub type Vec3 = NaVector3<f64>;

/// Alias pour un point 3D dans l'espace optique (en millimètres, axe optique = Z)
pub type Point3 = NaPoint3<f64>;

/// Longueur d'onde d'analyse (en nanomètres) avec son poids relatif
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wavelength {
    /// Longueur d'onde en nanomètres (ex: 587.56 nm)
    pub valeur_nm: f64,
    /// Poids relatif pour le calcul polychromatique (ex: 1.0)
    pub poids: f64,
    /// Nom ou raie spectrale associée (ex: "Raie d (Jaune)")
    pub etiquette: String,
}

impl Wavelength {
    pub fn new(valeur_nm: f64, poids: f64, etiquette: impl Into<String>) -> Self {
        Self {
            valeur_nm,
            poids,
            etiquette: etiquette.into(),
        }
    }

    /// Triplet trichromatique standard Fraunhofer (F, d, C) dans le visible
    pub fn visible_fdc() -> Vec<Self> {
        vec![
            Self::new(LAMBDA_F_NM, 1.0, tr!("F (Bleu 486.1 nm)", "F (Blue 486.1 nm)", "F (Blau 486.1 nm)")),
            Self::new(LAMBDA_D_NM, 1.0, tr!("d (Jaune 587.6 nm)", "d (Yellow 587.6 nm)", "d (Gelb 587.6 nm)")),
            Self::new(LAMBDA_C_NM, 1.0, tr!("C (Rouge 656.3 nm)", "C (Red 656.3 nm)", "C (Rot 656.3 nm)")),
        ]
    }
}

/// Définition de l'ouverture du système optique (Pupille / Ouverture Numérique)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ApertureSpec {
    /// Diamètre de la pupille d'entrée (Entrance Pupil Diameter - EPD) en mm
    PupilleEntreeMm(f64),
    /// Ouverture numérique dans l'espace objet (NA = n * sin(θ))
    OuvertureNumeriqueObjet(f64),
    /// Nombre d'ouverture image paraxial (F/# = EFFL / EPD)
    NombreOuverture(f64),
}

impl Default for ApertureSpec {
    fn default() -> Self {
        Self::PupilleEntreeMm(20.0)
    }
}

/// Type géométrique d'une surface optique
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SurfaceType {
    /// Surface plane (rayon de courbure infini)
    Plane,
    /// Surface sphérique classique (définie par son rayon R)
    #[default]
    Spherique,
    /// Surface conique de révolution (rayon R + constante conique k)
    Conique,
    /// Surface asphérique paire (rayon R, conique k, termes polynomiaux A4, A6, A8, A10)
    Aspherique,
    /// Lentille idéale (paraxiale) de focale donnée, sans aberration : plan qui fait converger
    /// tout faisceau parallèle en un point de son plan focal
    Ideale,
    /// Rupture de repère (Coordinate Break de Zemax) : surface fictive, sans action sur les
    /// rayons, qui décentre puis incline le repère de toutes les surfaces suivantes ; son
    /// épaisseur est comptée le long du nouvel axe
    CoupureRepere,
}

impl SurfaceType {
    pub fn libelle_court(&self) -> &'static str {
        match self {
            SurfaceType::Plane => tr!("PLANE", "FLAT", "PLAN"),
            SurfaceType::Spherique => tr!("SPHERIQUE", "SPHERICAL", "SPHÄRISCH"),
            SurfaceType::Conique => tr!("CONIQUE", "CONIC", "KONISCH"),
            SurfaceType::Aspherique => tr!("ASPHERIQUE", "ASPHERIC", "ASPHÄRISCH"),
            SurfaceType::Ideale => tr!("IDEALE", "IDEAL", "IDEAL"),
            SurfaceType::CoupureRepere => "COORDBRK",
        }
    }

    pub fn depuis_chaine(s: &str) -> Self {
        match s.trim().to_uppercase().as_str() {
            "PLANE" | "PLAN" | "FLAT" => SurfaceType::Plane,
            "CONIQUE" | "CONIC" | "KONISCH" => SurfaceType::Conique,
            "IDEALE" | "IDEAL" | "PARAXIAL" | "PARAXIALE" => SurfaceType::Ideale,
            "COORDBRK" | "COUPURE" | "RUPTURE" | "COORDINATE_BREAK" => SurfaceType::CoupureRepere,
            "ASPHERIQUE" | "ASPHERIC" | "EVEN_ASPHERE" | "ASPHÄRISCH" | "ASPHAERISCH" => {
                SurfaceType::Aspherique
            }
            _ => SurfaceType::Spherique,
        }
    }
}

/// Sérialisation/désérialisation robuste des rayons de courbure (supporte `f64::INFINITY` <-> `null` ou `0.0`)
pub mod serde_rayon_inf {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(val: &f64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if val.is_finite() {
            serializer.serialize_some(val)
        } else {
            serializer.serialize_none()
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<f64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<f64>::deserialize(deserializer)?;
        match opt {
            Some(v) if v.is_finite() && v.abs() > 1e-9 => Ok(v),
            _ => Ok(f64::INFINITY),
        }
    }
}

fn rayon_infini() -> f64 {
    f64::INFINITY
}

fn semi_diametre_defaut() -> f64 {
    10.0
}

fn verre_air() -> String {
    "AIR".to_string()
}

/// Représentation d'une surface dans l'Éditeur de Données de Lentilles (LDE).
/// Les champs facultatifs peuvent être omis dans un fichier JSON écrit à la main.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    /// Commentaire ou désignation (ex: "Entrée Crown", "Diaphragme", "Plan Image")
    #[serde(default)]
    pub commentaire: String,
    /// Type de profil géométrique
    #[serde(default)]
    pub type_surface: SurfaceType,
    /// Rayon de courbure en mm (`f64::INFINITY` pour une surface plane)
    #[serde(with = "serde_rayon_inf", default = "rayon_infini")]
    pub rayon: f64,
    /// Épaisseur axiale (en mm) jusqu'à la surface suivante le long de l'axe Z
    /// (négative quand la lumière se propage vers -Z, après un nombre impair de miroirs)
    #[serde(default)]
    pub epaisseur: f64,
    /// Nom du matériau situé après la surface ("AIR", "BK7", "F2", "MIROIR", etc.)
    #[serde(default = "verre_air")]
    pub verre: String,
    /// Demi-diamètre utile (ouverture mécanique en mm)
    #[serde(default = "semi_diametre_defaut")]
    pub semi_diametre: f64,
    /// Constante conique k (0 = sphère, -1 = paraboloïde, < -1 = hyperboloïde)
    #[serde(default)]
    pub conique: f64,
    /// Coefficients d'asphéricité pairs [A4, A6, A8, A10]
    #[serde(default)]
    pub aspheriques: [f64; 4],
    /// Indique si cette surface porte le diaphragme d'ouverture (STOP)
    #[serde(default)]
    pub est_diaphragme: bool,
    /// Décentrement du sommet (x, y) en mm, dans le repère global
    #[serde(default, skip_serializing_if = "est_nul2")]
    pub decentrement: [f64; 2],
    /// Inclinaison de la surface autour de son sommet : rotation autour de X puis de Y (degrés)
    #[serde(default, skip_serializing_if = "est_nul2")]
    pub inclinaison_deg: [f64; 2],
    /// Rayon de l'obstruction centrale (mm) : les rayons qui passent plus près de l'axe local
    /// sont arrêtés (secondaire, araignée centrale, trou du primaire…)
    #[serde(default, skip_serializing_if = "est_nul")]
    pub rayon_obstruction: f64,
    /// Réseau de diffraction gravé sur la surface : traits par mm (0 = pas de réseau) ;
    /// les traits sont parallèles à l'axe X local, la dispersion se fait selon Y
    #[serde(default, skip_serializing_if = "est_nul")]
    pub reseau_traits_mm: f64,
    /// Ordre de diffraction utilisé
    #[serde(default, skip_serializing_if = "est_nul_i")]
    pub reseau_ordre: i32,
    /// Focale d'une lentille idéale (type `Ideale`), en mm
    #[serde(default, skip_serializing_if = "est_nul")]
    pub focale_ideale_mm: f64,
    /// Rupture de repère : inclinaison appliquée avant le décentrement (ordre 1 de Zemax, qui
    /// sert à défaire une rupture précédente) ; par défaut décentrement puis inclinaison
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inclinaison_avant_decentrement: bool,
    /// Rupture de repère : rotation autour de Z (degrés), après X et Y
    #[serde(default, skip_serializing_if = "est_nul")]
    pub inclinaison_z_deg: f64,
    /// Calcul automatique (solve) du rayon de courbure
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solve_rayon: Option<Solve>,
    /// Calcul automatique (solve) de l'épaisseur
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solve_epaisseur: Option<Solve>,
    /// Reprise (pickup) de la constante conique
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solve_conique: Option<Solve>,
}

fn est_nul(v: &f64) -> bool {
    *v == 0.0
}
fn est_nul2(v: &[f64; 2]) -> bool {
    v[0] == 0.0 && v[1] == 0.0
}
fn est_nul_i(v: &i32) -> bool {
    *v == 0
}

impl Surface {
    /// Crée une surface sphérique ou plane standard
    pub fn nouvelle(
        type_surface: SurfaceType,
        rayon: f64,
        epaisseur: f64,
        verre: impl Into<String>,
        semi_diametre: f64,
    ) -> Self {
        let rayon_effectif = if matches!(type_surface, SurfaceType::Plane | SurfaceType::Ideale | SurfaceType::CoupureRepere)
            || rayon.abs() < 1e-9
        {
            f64::INFINITY
        } else {
            rayon
        };
        Self {
            commentaire: String::new(),
            type_surface,
            rayon: rayon_effectif,
            epaisseur,
            verre: verre.into().to_uppercase(),
            semi_diametre: semi_diametre.max(0.1),
            conique: 0.0,
            aspheriques: [0.0; 4],
            est_diaphragme: false,
            decentrement: [0.0; 2],
            inclinaison_deg: [0.0; 2],
            rayon_obstruction: 0.0,
            reseau_traits_mm: 0.0,
            reseau_ordre: 0,
            focale_ideale_mm: 0.0,
            inclinaison_avant_decentrement: false,
            inclinaison_z_deg: 0.0,
            solve_rayon: None,
            solve_epaisseur: None,
            solve_conique: None,
        }
    }

    /// Crée un plan image (détecteur / écran d'observation)
    pub fn plan_image(semi_diametre: f64) -> Self {
        let mut s = Self::nouvelle(SurfaceType::Plane, f64::INFINITY, 0.0, "AIR", semi_diametre);
        s.commentaire = tr!("Plan Image", "Image Plane", "Bildebene").to_string();
        s
    }

    /// Rotation du repère local de la surface (autour de X puis de Y)
    pub fn rotation(&self) -> nalgebra::Rotation3<f64> {
        let (a, b) = (self.inclinaison_deg[0].to_radians(), self.inclinaison_deg[1].to_radians());
        nalgebra::Rotation3::from_axis_angle(&NaVector3::y_axis(), b)
            * nalgebra::Rotation3::from_axis_angle(&NaVector3::x_axis(), a)
    }

    /// Vrai pour une rupture de repère (Coordinate Break)
    pub fn est_coupure(&self) -> bool {
        self.type_surface == SurfaceType::CoupureRepere
    }

    /// Rotation d'une rupture de repère, comme dans Zemax : autour de X, puis du nouvel Y, puis du
    /// nouveau Z ; dans l'ordre inverse (Z, Y, X) quand l'inclinaison précède le décentrement, de
    /// sorte qu'une rupture aux valeurs opposées dans cet ordre défait exactement la précédente
    pub fn rotation_coupure(&self) -> nalgebra::Rotation3<f64> {
        let rx = nalgebra::Rotation3::from_axis_angle(&NaVector3::x_axis(), self.inclinaison_deg[0].to_radians());
        let ry = nalgebra::Rotation3::from_axis_angle(&NaVector3::y_axis(), self.inclinaison_deg[1].to_radians());
        let rz = nalgebra::Rotation3::from_axis_angle(&NaVector3::z_axis(), self.inclinaison_z_deg.to_radians());
        if self.inclinaison_avant_decentrement { rz * ry * rx } else { rx * ry * rz }
    }

    /// Sommet de la surface dans le repère global (position axiale `z_sommet` et décentrement)
    pub fn sommet_global(&self, z_sommet: f64) -> Point3 {
        Point3::new(self.decentrement[0], self.decentrement[1], z_sommet)
    }

    /// Point du repère local de la surface exprimé dans le repère global
    pub fn vers_global(&self, z_sommet: f64, x: f64, y: f64, z: f64) -> Point3 {
        self.sommet_global(z_sommet) + self.rotation() * Vec3::new(x, y, z)
    }

    /// Vrai si la surface est décentrée ou inclinée
    pub fn est_hors_axe(&self) -> bool {
        !est_nul2(&self.decentrement) || !est_nul2(&self.inclinaison_deg) || self.inclinaison_z_deg != 0.0
    }

    /// Retourne la courbure axiale c = 1 / R (en mm⁻¹), 0.0 si la surface est plane
    pub fn courbure(&self) -> f64 {
        if matches!(self.type_surface, SurfaceType::Plane | SurfaceType::Ideale | SurfaceType::CoupureRepere)
            || !self.rayon.is_finite()
            || self.rayon.abs() < 1e-9
        {
            0.0
        } else {
            1.0 / self.rayon
        }
    }

    /// Calcule la flèche locale (sagitta) z(r) de la surface à une distance radiale `r = sqrt(x² + y²)`
    /// par rapport au sommet de la surface :
    /// z(r) = (c * r²) / (1 + sqrt(1 - (1 + k) * c² * r²)) + A4*r⁴ + A6*r⁶ + A8*r⁸ + A10*r¹⁰
    pub fn fleche(&self, r: f64) -> Option<f64> {
        let c = self.courbure();
        if c == 0.0 && self.type_surface != SurfaceType::Aspherique {
            return Some(0.0);
        }

        let r2 = r * r;
        let k = match self.type_surface {
            SurfaceType::Conique | SurfaceType::Aspherique => self.conique,
            _ => 0.0,
        };

        let radicande = 1.0 - (1.0 + k) * c * c * r2;
        if radicande <= 0.0 {
            return None; // Hors de la calotte géométrique définie
        }

        let mut z = (c * r2) / (1.0 + radicande.sqrt());

        if self.type_surface == SurfaceType::Aspherique {
            let r4 = r2 * r2;
            let r6 = r4 * r2;
            let r8 = r4 * r4;
            let r10 = r8 * r2;
            z += self.aspheriques[0] * r4
                + self.aspheriques[1] * r6
                + self.aspheriques[2] * r8
                + self.aspheriques[3] * r10;
        }

        Some(z)
    }

    /// Calcule la dérivée radiale dz/dr de la flèche locale à la hauteur `r`
    pub fn derivee_fleche_dr(&self, r: f64) -> Option<f64> {
        let c = self.courbure();
        if r.abs() < 1e-14 {
            return Some(0.0);
        }

        let r2 = r * r;
        let k = match self.type_surface {
            SurfaceType::Conique | SurfaceType::Aspherique => self.conique,
            _ => 0.0,
        };

        let radicande = 1.0 - (1.0 + k) * c * c * r2;
        if radicande <= 0.0 {
            return None;
        }

        let mut dz_dr = (c * r) / radicande.sqrt();

        if self.type_surface == SurfaceType::Aspherique {
            let r3 = r2 * r;
            let r5 = r3 * r2;
            let r7 = r5 * r2;
            let r9 = r7 * r2;
            dz_dr += 4.0 * self.aspheriques[0] * r3
                + 6.0 * self.aspheriques[1] * r5
                + 8.0 * self.aspheriques[2] * r7
                + 10.0 * self.aspheriques[3] * r9;
        }

        Some(dz_dr)
    }

    /// Calcule le vecteur normal unitaire sortant (orienté vers +Z) au point local (x, y, z(x, y))
    pub fn normale_locale(&self, x: f64, y: f64) -> Option<Vec3> {
        let r = (x * x + y * y).sqrt();
        if r < 1e-14 {
            return Some(Vec3::new(0.0, 0.0, 1.0));
        }
        let dz_dr = self.derivee_fleche_dr(r)?;
        let dz_dx = dz_dr * (x / r);
        let dz_dy = dz_dr * (y / r);
        let n = Vec3::new(-dz_dx, -dz_dy, 1.0);
        Some(n.normalize())
    }
}

/// Calcul automatique d'un paramètre de surface (solve de Zemax), refait avant chaque analyse
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub enum Solve {
    /// Épaisseur : le rayon marginal arrive à la hauteur `hauteur_mm` sur la surface suivante
    /// (0 = mise au point). `zone` = 0 : rayon paraxial ; sinon rayon réel passant à cette
    /// fraction du rayon de la pupille (0,7071 : zone qui équilibre l'aberration sphérique)
    HauteurMarginale {
        #[serde(default)]
        hauteur_mm: f64,
        #[serde(default)]
        zone: f64,
    },
    /// Épaisseur : le rayon principal paraxial arrive à la hauteur donnée sur la surface suivante
    /// (0 = surface suivante dans une pupille)
    HauteurPrincipale {
        #[serde(default)]
        hauteur_mm: f64,
    },
    /// Rayon : pente paraxiale du rayon marginal après la surface
    AngleMarginal { angle: f64 },
    /// Rayon : pente paraxiale du rayon principal après la surface
    AnglePrincipal { angle: f64 },
    /// Rayon : nombre d'ouverture image imposé (pente marginale −1/2N après la surface)
    NombreOuverture { f_nombre: f64 },
    /// Rayon : surface aplanétique pour le rayon marginal (ni sphérique ni coma)
    Aplanetique,
    /// Reprise (pickup) : valeur de la surface `surface` (numéro affiché, à partir de 1)
    /// multipliée par `facteur`, plus `decalage`
    Reprise {
        surface: usize,
        #[serde(default = "un")]
        facteur: f64,
        #[serde(default)]
        decalage: f64,
    },
}

fn un() -> f64 {
    1.0
}

impl Solve {
    pub fn libelle(&self) -> String {
        match self {
            Solve::HauteurMarginale { hauteur_mm, zone } if *zone != 0.0 => {
                trf!("M h={:.4} z={:.3}", "M h={:.4} z={:.3}", "M h={:.4} z={:.3}", hauteur_mm, zone)
            }
            Solve::HauteurMarginale { hauteur_mm, .. } => format!("M h={hauteur_mm:.4}"),
            Solve::HauteurPrincipale { hauteur_mm } => format!("C h={hauteur_mm:.4}"),
            Solve::AngleMarginal { angle } => format!("M u'={angle:.5}"),
            Solve::AnglePrincipal { angle } => format!("C u'={angle:.5}"),
            Solve::NombreOuverture { f_nombre } => format!("F/{f_nombre:.3}"),
            Solve::Aplanetique => tr!("Aplanét.", "Aplanatic", "Aplanat.").to_string(),
            Solve::Reprise { surface, facteur, decalage } => {
                trf!("P #{} ×{} {:+}", "P #{} ×{} {:+}", "P #{} ×{} {:+}", surface, facteur, decalage)
            }
        }
    }
}

/// Repère local d'une surface dans l'espace global : sommet et orientation des axes
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Repere {
    pub origine: Point3,
    pub rotation: nalgebra::Rotation3<f64>,
}

impl Repere {
    /// Point du repère local exprimé dans le repère global
    pub fn vers_global(&self, x: f64, y: f64, z: f64) -> Point3 {
        self.origine + self.rotation * Vec3::new(x, y, z)
    }

    /// Point global exprimé dans le repère local
    pub fn vers_local(&self, p: &Point3) -> Point3 {
        Point3::from(self.rotation.inverse() * (p - self.origine))
    }

    /// Axe Z local (axe optique de la surface) dans le repère global
    pub fn axe(&self) -> Vec3 {
        self.rotation * Vec3::new(0.0, 0.0, 1.0)
    }
}

/// Rayon lumineux géométrique en coordonnées 3D
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ray {
    /// Origine actuelle du rayon en coordonnées globales (mm)
    pub origine: Point3,
    /// Vecteur directeur unitaire (cosinus directeurs L, M, N)
    pub direction: Vec3,
    /// Longueur d'onde associée en nanomètres
    pub longueur_onde_nm: f64,
    /// Intensité relative transmise (0.0 à 1.0)
    pub intensite: f64,
    /// Chemin optique cumulé (Optical Path Length - OPL) en mm
    pub chemin_optique: f64,
    /// Indique si le rayon a été vignetté (bloqué par un bord de lentille ou réflexion totale)
    pub vignette: bool,
}

impl Ray {
    pub fn nouveau(origine: Point3, direction: Vec3, longueur_onde_nm: f64) -> Self {
        Self {
            origine,
            direction: direction.normalize(),
            longueur_onde_nm,
            intensite: 1.0,
            chemin_optique: 0.0,
            vignette: false,
        }
    }
}

fn longueur_onde_d() -> f64 {
    LAMBDA_D_NM
}

/// Système optique complet (LDE + Pupille + Longueurs d'onde + Catalogue de verres)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpticalSystem {
    /// Nom du design optique
    pub nom: String,
    /// Longueur d'onde principale de référence (en nm, ex: 587.56 nm)
    #[serde(default = "longueur_onde_d")]
    pub longueur_onde_ref_nm: f64,
    /// Liste des longueurs d'onde actives
    #[serde(default)]
    pub longueurs_onde: Vec<Wavelength>,
    /// Spécification de l'ouverture (pupille d'entrée / NA / F#)
    #[serde(default)]
    pub ouverture: ApertureSpec,
    /// Angles de champ (hx_deg, hy_deg) en degrés
    #[serde(default)]
    pub champs_deg: Vec<(f64, f64)>,
    /// Séquence ordonnée des surfaces optiques (de l'entrée jusqu'au plan image)
    pub surfaces: Vec<Surface>,
    /// Distance de l'objet à la première surface (mm) ; absente : objet à l'infini. Quand elle
    /// est donnée, les champs sont des hauteurs d'objet en mm (et non des angles)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_objet_mm: Option<f64>,
    /// Espace image afocal (oculaire, lunette de visée, jumelles, élargisseur de faisceau) :
    /// analyses en angles (µrad) et front d'onde rapporté à une onde plane
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub afocal: bool,
    /// Catalogue de matériaux optiques (les verres standards restent toujours disponibles)
    #[serde(default)]
    pub catalogue: GlassCatalog,
}

impl Default for OpticalSystem {
    fn default() -> Self {
        Self::doublet_achromatique_defaut()
    }
}

impl OpticalSystem {
    /// Crée un nouveau système optique vierge (avec une surface pupille et un plan image)
    pub fn nouveau(nom: impl Into<String>, longueur_onde_ref_nm: f64) -> Self {
        let mut stop = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 50.0, "AIR", 12.5);
        stop.commentaire = tr!("Pupille / Entrée", "Pupil / Entrance", "Pupille / Eintritt").to_string();
        stop.est_diaphragme = true;

        let image = Surface::plan_image(15.0);

        Self {
            nom: nom.into(),
            longueur_onde_ref_nm,
            longueurs_onde: Wavelength::visible_fdc(),
            ouverture: ApertureSpec::PupilleEntreeMm(20.0),
            champs_deg: vec![(0.0, 0.0), (0.0, 3.0)],
            surfaces: vec![stop, image],
            catalogue: GlassCatalog::standard(),
            distance_objet_mm: None,
            afocal: false,
        }
    }

    /// Doublet achromatique collé F=100mm F/4.2 (BK7 / F2) préconfiguré pour démonstration immédiate
    /// (achromatisé F-C et corrigé de l'aberration sphérique : spot RMS ≈ disque d'Airy)
    pub fn doublet_achromatique_defaut() -> Self {
        let mut s1 = Surface::nouvelle(SurfaceType::Spherique, 45.4439, 7.0, "BK7", 15.0);
        s1.commentaire = tr!("Entrée Crown (BK7)", "Crown Entrance (BK7)", "Krone Eintritt (BK7)").to_string();
        s1.est_diaphragme = true;

        let mut s2 = Surface::nouvelle(SurfaceType::Spherique, -43.7259, 2.5, "F2", 15.0);
        s2.commentaire = tr!("Interface Collage (F2)", "Cemented Interface (F2)", "Kittfläche (F2)").to_string();

        let mut s3 = Surface::nouvelle(SurfaceType::Spherique, -708.9172, 93.6852, "AIR", 15.0);
        s3.commentaire = tr!("Sortie Flint", "Flint Exit", "Flint Austritt").to_string();

        let s4 = Surface::plan_image(15.0);

        Self {
            nom: tr!("Doublet Achromatique Fraunhofer 100mm", "Fraunhofer Achromatic Doublet 100mm", "Fraunhofer-Achromat 100mm").to_string(),
            longueur_onde_ref_nm: LAMBDA_D_NM,
            longueurs_onde: Wavelength::visible_fdc(),
            ouverture: ApertureSpec::PupilleEntreeMm(24.0),
            champs_deg: vec![(0.0, 0.0), (0.0, 2.5)],
            surfaces: vec![s1, s2, s3, s4],
            catalogue: GlassCatalog::standard(),
            distance_objet_mm: None,
            afocal: false,
        }
    }

    /// Distance d'objet finie (mm), `None` pour un objet à l'infini
    pub fn objet_fini(&self) -> Option<f64> {
        self.distance_objet_mm.filter(|d| d.is_finite() && *d > 0.0)
    }

    /// Unité des champs : « mm » (hauteur d'objet) ou « ° » (angle)
    pub fn unite_champ(&self) -> &'static str {
        if self.objet_fini().is_some() { "mm" } else { "°" }
    }

    /// Sens de propagation dans l'espace image : +1 vers +Z, -1 après un nombre impair de miroirs
    pub fn sens_propagation_image(&self) -> f64 {
        let nb_miroirs = self
            .surfaces
            .iter()
            .filter(|s| !s.est_coupure() && self.catalogue.resoudre(&s.verre).est_miroir())
            .count();
        if nb_miroirs % 2 == 0 { 1.0 } else { -1.0 }
    }

    /// Positions axiales extrêmes (z_min, z_max) des sommets et des bords de toutes les surfaces (en mm)
    pub fn etendue_axiale(&self) -> (f64, f64) {
        let b = self.boite_englobante();
        (b[0], b[1])
    }

    /// Boîte englobante globale [z_min, z_max, y_min, y_max] des sommets et des bords des
    /// surfaces dans le plan méridien (ruptures de repère exclues), origine incluse
    pub fn boite_englobante(&self) -> [f64; 4] {
        let mut b = [0.0_f64; 4];
        for (surf, rep) in self.surfaces.iter().zip(self.reperes()) {
            if surf.est_coupure() {
                continue;
            }
            let sd = surf.semi_diametre;
            let fleche_bord = surf.fleche(sd).unwrap_or(0.0);
            for p in [rep.vers_global(0.0, 0.0, 0.0), rep.vers_global(0.0, sd, fleche_bord), rep.vers_global(0.0, -sd, fleche_bord)] {
                b[0] = b[0].min(p.z);
                b[1] = b[1].max(p.z);
                b[2] = b[2].min(p.y);
                b[3] = b[3].max(p.y);
            }
        }
        b
    }

    /// Vrai si le système contient une rupture de repère (axe optique replié)
    pub fn a_des_coupures(&self) -> bool {
        self.surfaces.iter().any(|s| s.est_coupure())
    }

    /// Vrai si une surface est décentrée, inclinée, ou si l'axe est replié
    pub fn est_hors_axe(&self) -> bool {
        self.surfaces.iter().any(|s| s.est_hors_axe())
    }

    /// Point de l'axe optique (repère courant, avant le décentrement propre) de chaque surface,
    /// pour dessiner l'axe replié par les ruptures de repère
    pub fn points_axe(&self) -> Vec<Point3> {
        let mut o = Point3::origin();
        let mut r = nalgebra::Rotation3::identity();
        let mut points = Vec::with_capacity(self.surfaces.len());
        for s in &self.surfaces {
            if s.est_coupure() {
                let d = Vec3::new(s.decentrement[0], s.decentrement[1], 0.0);
                if s.inclinaison_avant_decentrement {
                    r *= s.rotation_coupure();
                    o += r * d;
                } else {
                    o += r * d;
                    r *= s.rotation_coupure();
                }
            }
            points.push(o);
            o += r * Vec3::new(0.0, 0.0, s.epaisseur);
        }
        points
    }

    /// Repère de chaque surface : sommet (décentrement compris) et orientation (inclinaison
    /// propre comprise). Une rupture de repère modifie le repère courant de toutes les surfaces
    /// suivantes ; le décentrement et l'inclinaison d'une surface ordinaire ne valent que pour
    /// elle. L'épaisseur fait avancer le long de l'axe Z du repère courant.
    pub fn reperes(&self) -> Vec<Repere> {
        let mut o = Point3::origin();
        let mut r = nalgebra::Rotation3::identity();
        let mut reperes = Vec::with_capacity(self.surfaces.len());
        for s in &self.surfaces {
            let d = Vec3::new(s.decentrement[0], s.decentrement[1], 0.0);
            if s.est_coupure() {
                if s.inclinaison_avant_decentrement {
                    r *= s.rotation_coupure();
                    o += r * d;
                } else {
                    o += r * d;
                    r *= s.rotation_coupure();
                }
                reperes.push(Repere { origine: o, rotation: r });
            } else {
                reperes.push(Repere { origine: o + r * d, rotation: r * s.rotation() });
            }
            o += r * Vec3::new(0.0, 0.0, s.epaisseur);
        }
        reperes
    }

    /// Calcule les positions axiales Z cumulées des sommets de chaque surface (en mm)
    pub fn positions_z_sommets(&self) -> Vec<f64> {
        let mut z = 0.0;
        let mut positions = Vec::with_capacity(self.surfaces.len());
        for surf in &self.surfaces {
            positions.push(z);
            z += surf.epaisseur;
        }
        positions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::glass::{GlassCatalog, LAMBDA_C_NM, LAMBDA_D_NM, LAMBDA_F_NM};

    #[test]
    fn test_dispersion_catalogue_verres() {
        let cat = GlassCatalog::standard();
        let bk7 = cat.resoudre("BK7");
        let n_d = bk7.indice_refraction(LAMBDA_D_NM);
        let n_f = bk7.indice_refraction(LAMBDA_F_NM);
        let n_c = bk7.indice_refraction(LAMBDA_C_NM);

        assert!((n_d - 1.51680).abs() < 2e-5, "nd = {n_d}");
        assert!(n_f > n_d && n_d > n_c, "Dispersion normale : nF > nd > nC");
        let vd_calcule = (n_d - 1.0) / (n_f - n_c);
        assert!((vd_calcule - 64.17).abs() < 0.02, "Vd = {vd_calcule}");
    }

    #[test]
    fn test_fleche_et_normale_surface() {
        let surf = Surface::nouvelle(SurfaceType::Spherique, 50.0, 5.0, "BK7", 15.0);
        let z0 = surf.fleche(0.0).unwrap();
        assert!(z0.abs() < 1e-12);

        let r = 10.0;
        let z_r = surf.fleche(r).unwrap();
        // Pour R = 50, z(10) = 50 - sqrt(50^2 - 10^2) = 50 - sqrt(2400) ≈ 1.01020514
        let attendu = 50.0 - (2500.0_f64 - 100.0).sqrt();
        assert!((z_r - attendu).abs() < 1e-9);

        let n = surf.normale_locale(10.0, 0.0).unwrap();
        assert!((n.norm() - 1.0).abs() < 1e-12);
        assert!(n.x < 0.0 && n.z > 0.0);
    }
}

