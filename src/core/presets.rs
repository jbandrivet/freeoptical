//! Systèmes optiques d'exemple partagés par l'interface graphique, le TUI et le serveur MCP.

use crate::core::glass::LAMBDA_D_NM;
use crate::core::types::{ApertureSpec, OpticalSystem, Surface, SurfaceType, Wavelength};

/// Miroir parabolique Ø52 R300 au centre de courbure, comme dans un interféromètre de Bath :
/// laser He-Ne collimaté, cube séparateur, lentille de Bath (idéale) focalisant le faisceau au
/// centre de courbure, retour au même point après réflexion. La lentille est idéale parce que,
/// dans le vrai montage, ondes test et référence la traversent toutes deux sur le même cône : ses
/// aberrations se compensent. Le front d'onde (F6) montre donc l'écart du miroir à la sphère, deux
/// fois l'écart de surface ; l'interférogramme lui-même (écart source/référence, franges,
/// réduction, analyse d'une photo) est dans Outils > Interféromètre de Bath.
pub fn creer_interferometre_bath() -> OpticalSystem {
    const FOCALE_LENTILLE: f64 = 10.0;
    const RAYON_MIROIR: f64 = 300.0;
    const DEMI_DIAMETRE: f64 = 26.0;
    let mut s0 = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 12.7, "BK7", 6.35);
    s0.commentaire = tr!("Entrée Cube Séparateur 50/50 (BK7)", "Beam Splitter Cube 50/50 Entrance (BK7)", "Strahlteiler-Cube 50/50 Eintritt (BK7)").to_string();

    let mut s1 = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 8.0, "AIR", 6.35);
    s1.commentaire = tr!("Sortie Cube Séparateur", "Beam Splitter Cube Exit", "Strahlteiler-Cube Austritt").to_string();

    // Foyer de la lentille au centre de courbure du miroir
    let mut s2 = Surface::nouvelle(SurfaceType::Ideale, f64::INFINITY, FOCALE_LENTILLE + RAYON_MIROIR, "AIR", 2.0);
    s2.focale_ideale_mm = FOCALE_LENTILLE;
    s2.commentaire = tr!("Lentille de Bath f = 10 mm (idéale)", "Bath Lens f = 10 mm (ideal)", "Bath-Linse f = 10 mm (ideal)").to_string();

    let mut s3 = Surface::nouvelle(SurfaceType::Conique, -RAYON_MIROIR, -RAYON_MIROIR, "MIROIR", DEMI_DIAMETRE);
    s3.commentaire = tr!("Miroir Sous Test Ø52 R300 (parabolique)", "Mirror Under Test Ø52 R300 (parabolic)", "Prüfspiegel Ø52 R300 (parabolisch)").to_string();
    s3.conique = -1.0;
    // Le miroir est la pupille : c'est lui qu'on observe, et la sphère de référence du front
    // d'onde passe par lui (rayon R) au lieu d'une pupille de sortie proche du foyer
    s3.est_diaphragme = true;

    let mut s4 = Surface::plan_image(3.0);
    s4.commentaire = tr!("Foyer de Retour (Centre de Courbure)", "Return Focus (Centre of Curvature)", "Rückkehrfokus (Krümmungsmittelpunkt)").to_string();

    // Faisceau élargi jusqu'au bord du miroir (un peu en deçà, pour ne pas vignetter)
    let fleche_bord = DEMI_DIAMETRE * DEMI_DIAMETRE / (2.0 * RAYON_MIROIR);
    let pupille = 2.0 * FOCALE_LENTILLE * DEMI_DIAMETRE / (RAYON_MIROIR - fleche_bord) * 0.999;

    OpticalSystem {
        nom: tr!("Interféromètre de Bath (Laser He-Ne 632.8 nm)", "Bath Interferometer (He-Ne Laser 632.8 nm)", "Bath-Interferometer (He-Ne-Laser 632.8 nm)").to_string(),
        longueur_onde_ref_nm: 632.8,
        longueurs_onde: vec![Wavelength::new(632.8, 1.0, "He-Ne (632.8 nm)")],
        ouverture: ApertureSpec::PupilleEntreeMm(pupille),
        champs_deg: vec![(0.0, 0.0)],
        surfaces: vec![s0, s1, s2, s3, s4],
        catalogue: Default::default(),
        distance_objet_mm: None,
        afocal: false,
        tolerances: Vec::new(),
    }
}

/// Singlet F/2 dont la face d'entrée est un ellipsoïde/hyperboloïde corrigeant l'aberration sphérique
pub fn creer_singlet_aspherique() -> OpticalSystem {
    let mut s1 = Surface::nouvelle(SurfaceType::Aspherique, 42.1287, 8.0, "LAK9", 16.0);
    s1.commentaire = tr!("Entrée Asphérique (LAK9)", "Aspheric Entrance (LAK9)", "Asphärischer Eintritt (LAK9)").to_string();
    s1.conique = -1.3244;
    s1.est_diaphragme = true;

    let mut s2 = Surface::nouvelle(SurfaceType::Spherique, -180.0, 46.2868, "AIR", 16.0);
    s2.commentaire = tr!("Sortie Sphérique", "Spherical Exit", "Sphärischer Austritt").to_string();

    let s3 = Surface::plan_image(15.0);

    OpticalSystem {
        nom: tr!("Singlet Asphérique F/2 (LAK9)", "Aspheric Singlet F/2 (LAK9)", "Asphärische Einzellinse F/2 (LAK9)").to_string(),
        longueur_onde_ref_nm: LAMBDA_D_NM,
        longueurs_onde: vec![Wavelength::new(LAMBDA_D_NM, 1.0, tr!("Raie d (587.6 nm)", "d line (587.6 nm)", "d-Linie (587.6 nm)"))],
        ouverture: ApertureSpec::PupilleEntreeMm(25.0),
        champs_deg: vec![(0.0, 0.0)],
        surfaces: vec![s1, s2, s3],
        catalogue: Default::default(),
        distance_objet_mm: None,
        afocal: false,
        tolerances: Vec::new(),
    }
}

/// Télescope Cassegrain classique : primaire parabolique F/2, secondaire hyperbolique (k = -((m+1)/(m-1))²),
/// foyer 33 mm derrière le primaire
pub fn creer_telescope_cassegrain() -> OpticalSystem {
    let mut s1 = Surface::nouvelle(SurfaceType::Conique, -160.0, -55.0, "MIROIR", 22.0);
    s1.commentaire = tr!("Miroir Primaire Concave", "Concave Primary Mirror", "Konkaver Hauptspiegel").to_string();
    s1.conique = -1.0; // Paraboloïde
    s1.est_diaphragme = true;

    let mut s2 = Surface::nouvelle(SurfaceType::Conique, -65.0, 108.3333, "MIROIR", 10.0);
    s2.commentaire = tr!("Miroir Secondaire Convexe", "Convex Secondary Mirror", "Konvexer Fangspiegel").to_string();
    s2.conique = -2.56; // Hyperboloïde, grandissement m = 4.33

    let s3 = Surface::plan_image(12.0);

    OpticalSystem {
        nom: tr!("Télescope Réflecteur Cassegrain", "Cassegrain Reflecting Telescope", "Cassegrain-Spiegelteleskop").to_string(),
        longueur_onde_ref_nm: LAMBDA_D_NM,
        longueurs_onde: Wavelength::visible_fdc(),
        ouverture: ApertureSpec::PupilleEntreeMm(36.0),
        champs_deg: vec![(0.0, 0.0)],
        surfaces: vec![s1, s2, s3],
        catalogue: Default::default(),
        distance_objet_mm: None,
        afocal: false,
        tolerances: Vec::new(),
    }
}

/// Spectrographe à réseau : fente (objet à 200 mm), collimateur idéal f = 200 mm, réseau en
/// transmission 600 traits/mm, caméra idéale f = 200 mm ; domaine 400 à 700 nm
pub fn creer_spectrographe_reseau() -> OpticalSystem {
    let mut collimateur = Surface::nouvelle(SurfaceType::Ideale, f64::INFINITY, 20.0, "AIR", 15.0);
    collimateur.focale_ideale_mm = 200.0;
    collimateur.commentaire = tr!("Collimateur (idéal)", "Collimator (ideal)", "Kollimator (ideal)").to_string();
    let mut reseau = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 30.0, "AIR", 15.0);
    reseau.reseau_traits_mm = 600.0;
    reseau.reseau_ordre = 1;
    reseau.est_diaphragme = true;
    reseau.commentaire = tr!("Réseau 600 tr/mm, ordre 1", "Grating 600 l/mm, order 1", "Gitter 600 L/mm, Ordnung 1").to_string();
    let mut camera = Surface::nouvelle(SurfaceType::Ideale, f64::INFINITY, 200.0, "AIR", 40.0);
    camera.focale_ideale_mm = 200.0;
    camera.commentaire = tr!("Caméra (idéale)", "Camera (ideal)", "Kamera (ideal)").to_string();
    let mut detecteur = Surface::plan_image(80.0);
    detecteur.commentaire = tr!("Détecteur", "Detector", "Detektor").to_string();
    OpticalSystem {
        nom: tr!("Spectrographe à réseau 600 tr/mm", "Grating spectrograph 600 l/mm", "Gitterspektrograph 600 L/mm").to_string(),
        longueur_onde_ref_nm: 550.0,
        longueurs_onde: [400.0, 475.0, 550.0, 625.0, 700.0]
            .iter()
            .map(|&l| Wavelength::new(l, 1.0, format!("{l:.0} nm")))
            .collect(),
        // Faisceau F/10 issu de la fente
        ouverture: ApertureSpec::OuvertureNumeriqueObjet(0.05),
        champs_deg: vec![(0.0, 0.0), (0.0, 2.0)],
        surfaces: vec![collimateur, reseau, camera, detecteur],
        catalogue: Default::default(),
        distance_objet_mm: Some(200.0),
        afocal: false,
        tolerances: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{
        analysis::calculer_spot_diagram,
        glass::GlassCatalog,
        optimizer::{OptimizationTarget, OptimizationVariable, optimiser_systeme_dls},
        paraxial::calculer_paraxial,
        raytracer::tracer_eventail_meridional,
    };

    fn miroir_seul() -> OpticalSystem {
        let mut sys = OpticalSystem::nouveau("Miroir sphérique", LAMBDA_D_NM);
        let mut m = Surface::nouvelle(SurfaceType::Spherique, -160.0, -80.0, "MIROIR", 20.0);
        m.est_diaphragme = true;
        sys.surfaces = vec![m, Surface::plan_image(10.0)];
        sys
    }

    #[test]
    fn test_focale_miroir_concave_positive() {
        let p = calculer_paraxial(&miroir_seul());
        assert!((p.focale_effl_mm - 80.0).abs() < 1e-9, "EFFL = {}", p.focale_effl_mm);
        assert!((p.tirage_bfl_mm + 80.0).abs() < 1e-9, "BFL = {}", p.tirage_bfl_mm);
    }

    #[test]
    fn spectrographe_d_exemple() {
        let s = creer_spectrographe_reseau();
        let r = crate::core::spectre::calculer_spectre(&s, (0.0, 0.0), 400.0, 700.0, 7, 20.0).unwrap();
        // Fente de 20 µm imagée 1:1 (puis élargie par l'anamorphose du réseau)
        assert!(r.image_fente_um.unwrap() > 19.9);
        assert!(r.resolution[3].unwrap() > 500.0);
        assert!(r.tache_rms_um.iter().flatten().all(|t| *t < 1e-3));
    }

    #[test]
    fn test_presets_au_point() {
        let cass = creer_telescope_cassegrain();
        let p = calculer_paraxial(&cass);
        assert!((p.focale_effl_mm - 346.6667).abs() < 1e-3);
        assert!(calculer_spot_diagram(&cass, (0.0, 0.0), 4).rayon_rms_um < 1.0);

        let doublet = OpticalSystem::doublet_achromatique_defaut();
        assert!((calculer_paraxial(&doublet).focale_effl_mm - 100.0).abs() < 1.0);
        assert!(calculer_spot_diagram(&doublet, (0.0, 0.0), 4).rayon_rms_um < 6.0);

        let singlet = creer_singlet_aspherique();
        assert!(calculer_spot_diagram(&singlet, (0.0, 0.0), 4).rayon_rms_um < 3.0);

        // Bath : aucun rayon perdu, le miroir est couvert jusqu'au bord ; un miroir sphérique
        // donne une teinte plate, la parabole la sphérique attendue au centre de courbure
        let mut bath = creer_interferometre_bath();
        let spot = calculer_spot_diagram(&bath, (0.0, 0.0), 4);
        assert_eq!(spot.nb_rayons_valides, 61);
        let bord = &crate::core::raytracer::tracer_eventail_meridional(&bath, 0.0, 3, 632.8)[2];
        let h = bord.impacts[3].point_global.y.abs();
        assert!(h > 25.9 && h < 26.0, "bord du miroir à {h} mm");
        let front = crate::core::onde::calculer_front_onde(&bath, (0.0, 0.0), 632.8, 64).unwrap();
        let parametres = crate::core::bath::ParametresBath { diametre_mm: 2.0 * h, rayon_courbure_mm: 300.0, ..Default::default() };
        let attendu = parametres.spherique_theorique_ondes(-1.0);
        assert!((front.zernike_ondes[10].abs() / attendu - 1.0).abs() < 0.02, "Z11 = {} / {attendu}", front.zernike_ondes[10]);
        bath.surfaces[3].conique = 0.0;
        let front = crate::core::onde::calculer_front_onde(&bath, (0.0, 0.0), 632.8, 64).unwrap();
        assert!(front.pv_ondes < 1e-3, "sphère : PV = {}", front.pv_ondes);
    }

    #[test]
    fn test_diaphragme_et_nombre_ouverture() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[0].est_diaphragme = false;
        sys.surfaces[2].est_diaphragme = true;
        let p = calculer_paraxial(&sys);
        assert!(p.position_pupille_entree_mm > 1.0);

        // Le rayon principal d'un champ hors axe passe par le centre du diaphragme
        let chef = &tracer_eventail_meridional(&sys, 1.0, 3, LAMBDA_D_NM)[1];
        assert!(chef.impacts[2].point_global.y.abs() < 0.01);

        sys.ouverture = ApertureSpec::NombreOuverture(8.0);
        let p = calculer_paraxial(&sys);
        assert!((p.nombre_ouverture_fnum - 8.0).abs() < 1e-9);
    }

    #[test]
    fn test_plan_image_ne_vignette_pas() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[3].semi_diametre = 0.001;
        let spot = calculer_spot_diagram(&sys, (0.0, 0.0), 4);
        assert_eq!(spot.nb_rayons_valides, 3 * 61);
    }

    #[test]
    fn test_json_minimal_et_verre_inconnu() {
        let json = r#"{ "nom": "Minimal", "surfaces": [
            { "rayon": 50.0, "epaisseur": 5.0, "verre": "BK7" },
            { "rayon": null, "epaisseur": 95.0 },
            { "epaisseur": 0.0 } ] }"#;
        let sys: OpticalSystem = serde_json::from_str(json).unwrap();
        assert!(calculer_paraxial(&sys).focale_effl_mm.is_finite());

        let cat = GlassCatalog::standard();
        assert!(cat.est_connu("bk7") && cat.est_connu("1.6:40") && cat.est_connu("MIROIR"));
        assert!(!cat.est_connu("BK-7"));
    }

    #[test]
    fn test_optimisation_garde_epaisseur_negative() {
        let mut sys = miroir_seul();
        let vars = vec![
            OptimizationVariable { index_surface: 0, parametre: "conique".to_string() },
            OptimizationVariable { index_surface: 0, parametre: "epaisseur".to_string() },
        ];
        let cibles = vec![OptimizationTarget {
            type_cible: "spot_rms".to_string(),
            valeur_cible: 0.0,
            poids: 1.0,
        }];
        optimiser_systeme_dls(&mut sys, &vars, &cibles, 20);
        assert!(sys.surfaces[0].epaisseur < -70.0, "épaisseur = {}", sys.surfaces[0].epaisseur);
        assert!(calculer_spot_diagram(&sys, (0.0, 0.0), 4).rayon_rms_um < 13.0);
    }
}
