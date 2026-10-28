//! Systèmes optiques d'exemple partagés par l'interface graphique, le TUI et le serveur MCP.

use crate::core::glass::LAMBDA_D_NM;
use crate::core::types::{ApertureSpec, OpticalSystem, Surface, SurfaceType, Wavelength};

/// Montage de Bath : laser He-Ne collimaté, cube séparateur, lentille de Bath focalisant le faisceau
/// au centre de courbure du miroir sous test (parabolique F/3), retour au foyer après réflexion.
/// Le spot diagram au plan image montre l'aberration du miroir testé à son centre de courbure.
pub fn creer_interferometre_bath() -> OpticalSystem {
    let mut s0 = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 12.7, "BK7", 6.35);
    s0.commentaire = tr!("Entrée Cube Séparateur 50/50 (BK7)", "Beam Splitter Cube 50/50 Entrance (BK7)", "Strahlteiler-Cube 50/50 Eintritt (BK7)").to_string();
    s0.est_diaphragme = true;

    let mut s1 = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 8.0, "AIR", 6.35);
    s1.commentaire = tr!("Sortie Cube Séparateur", "Beam Splitter Cube Exit", "Strahlteiler-Cube Austritt").to_string();

    let mut s2 = Surface::nouvelle(SurfaceType::Spherique, 12.0, 3.0, "BK7", 4.0);
    s2.commentaire = tr!("Entrée Lentille de Bath (BK7)", "Bath Lens Entrance (BK7)", "Bath-Linse Eintritt (BK7)").to_string();

    let mut s3 = Surface::nouvelle(SurfaceType::Spherique, -12.0, 311.1309, "AIR", 4.0);
    s3.commentaire = tr!("Sortie Lentille de Bath", "Bath Lens Exit", "Bath-Linse Austritt").to_string();

    let mut s4 = Surface::nouvelle(SurfaceType::Conique, -300.0, -300.0, "MIROIR", 26.0);
    s4.commentaire = tr!("Miroir Sous Test Ø52 R300 (parabolique)", "Mirror Under Test Ø52 R300 (parabolic)", "Prüfspiegel Ø52 R300 (parabolisch)").to_string();
    s4.conique = -1.0;

    let mut s5 = Surface::plan_image(3.0);
    s5.commentaire = tr!("Foyer de Retour (Centre de Courbure)", "Return Focus (Centre of Curvature)", "Rückkehrfokus (Krümmungsmittelpunkt)").to_string();

    OpticalSystem {
        nom: tr!("Interféromètre de Bath (Laser He-Ne 632.8 nm)", "Bath Interferometer (He-Ne Laser 632.8 nm)", "Bath-Interferometer (He-Ne-Laser 632.8 nm)").to_string(),
        longueur_onde_ref_nm: 632.8,
        longueurs_onde: vec![Wavelength::new(632.8, 1.0, "He-Ne (632.8 nm)")],
        ouverture: ApertureSpec::PupilleEntreeMm(1.9),
        champs_deg: vec![(0.0, 0.0)],
        surfaces: vec![s0, s1, s2, s3, s4, s5],
        catalogue: Default::default(),
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

        // Bath : aucun rayon perdu ; un miroir sphérique revient en un point au centre de courbure
        let mut bath = creer_interferometre_bath();
        let spot = calculer_spot_diagram(&bath, (0.0, 0.0), 4);
        assert_eq!(spot.nb_rayons_valides, 61);
        bath.surfaces[4].conique = 0.0;
        let spot_sphere = calculer_spot_diagram(&bath, (0.0, 0.0), 4);
        assert!(spot_sphere.rayon_rms_um < spot.rayon_rms_um / 10.0);
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
