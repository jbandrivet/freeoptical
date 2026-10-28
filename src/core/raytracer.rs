use crate::core::paraxial::{focale_paraxiale, position_pupille_entree};
use crate::core::types::{ApertureSpec, OpticalSystem, Point3, Ray, Surface, SurfaceType, Vec3};
use serde::{Deserialize, Serialize};

/// Enregistrement d'une intersection entre un rayon et une surface optique
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceHit {
    /// Index de la surface dans le système optique (0..N-1)
    pub index_surface: usize,
    /// Point d'intersection en coordonnées globales 3D (mm)
    pub point_global: Point3,
    /// Point d'intersection en coordonnées locales du sommet de la surface (mm)
    pub point_local: Point3,
    /// Normale unitaire locale de la surface au point d'impact
    pub normale: Vec3,
    /// Vecteur directeur unitaire du rayon sortant (après réfraction ou réflexion)
    pub direction_sortante: Vec3,
    /// Indice de réfraction du milieu après la surface
    pub indice_apres: f64,
    /// Vrai si le rayon a été vignetté sur cette surface (hors ouverture ou réflexion totale)
    pub vignette: bool,
}

/// Trajectoire complète d'un rayon à travers toutes les surfaces du système optique
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TracedRay {
    /// Rayon au point de départ (avant la première surface)
    pub rayon_initial: Ray,
    /// Longueur d'onde en nanomètres
    pub longueur_onde_nm: f64,
    /// Angle de champ associé (hx_deg, hy_deg)
    pub champ_deg: (f64, f64),
    /// Coordonnées pupillaires normalisées (px, py) dans [-1, 1]
    pub pupille_norm: (f64, f64),
    /// Liste ordonnée des impacts sur chaque surface
    pub impacts: Vec<SurfaceHit>,
    /// Vrai si le rayon est vignetté (bloqué avant d'atteindre le plan image)
    pub vignette: bool,
    /// Chemin optique total cumulé (OPL en mm)
    pub chemin_optique_total: f64,
}

impl TracedRay {
    /// Retourne le point d'impact sur la dernière surface (plan image) si le rayon n'est pas vignetté
    pub fn impact_plan_image(&self) -> Option<&SurfaceHit> {
        if self.vignette {
            None
        } else {
            self.impacts.last()
        }
    }
}

/// Calcule la distance d'intersection `t` le long du rayon local `P(t) = P0 + t * D`
/// avec une surface optique dont le sommet est en `(0, 0, 0)`.
pub fn intersecter_surface_locale(p0: &Point3, dir: &Vec3, surface: &Surface) -> Option<f64> {
    let x0 = p0.x;
    let y0 = p0.y;
    let z0 = p0.z;
    let l = dir.x;
    let m = dir.y;
    let n = dir.z;

    let c = surface.courbure();

    // 1. Estimation initiale analytique (plane ou quadrique exacte : sphère / conique)
    let mut t = if c == 0.0 {
        if n.abs() < 1e-14 {
            return None;
        }
        -z0 / n
    } else {
        let k = match surface.type_surface {
            SurfaceType::Conique | SurfaceType::Aspherique => surface.conique,
            _ => 0.0,
        };
        // Forme numériquement stable de Spencer & Murty : A*t^2 - 2*B*t + C = 0
        let a = c * (l * l + m * m + (1.0 + k) * n * n);
        let b = n - c * (x0 * l + y0 * m + (1.0 + k) * z0 * n);
        let grand_c = c * (x0 * x0 + y0 * y0 + (1.0 + k) * z0 * z0) - 2.0 * z0;

        if a.abs() < 1e-14 {
            if b.abs() < 1e-14 {
                return None;
            }
            grand_c / (2.0 * b)
        } else {
            let disc = b * b - a * grand_c;
            if disc < 0.0 {
                return None;
            }
            let sqrt_disc = disc.sqrt();
            // La racine continue passant par le sommet (0,0,0) :
            let denom1 = if b >= 0.0 {
                b + sqrt_disc
            } else {
                b - sqrt_disc
            };
            let t1 = if denom1.abs() > 1e-14 {
                grand_c / denom1
            } else {
                (b + sqrt_disc) / a
            };

            // Vérifions que le point appartient bien à la branche de la flèche z(r)
            let p_test = p0 + t1 * dir;
            let r_test = (p_test.x * p_test.x + p_test.y * p_test.y).sqrt();
            if let Some(z_sag) = surface.fleche(r_test) {
                if surface.type_surface != SurfaceType::Aspherique
                    && (p_test.z - z_sag).abs() > 1e-5
                {
                    let t2 = (b + if b >= 0.0 { sqrt_disc } else { -sqrt_disc }) / a;
                    t2
                } else {
                    t1
                }
            } else {
                return None;
            }
        }
    };

    // 2. Raffinement par Newton-Raphson pour les surfaces asphériques paires
    if surface.type_surface == SurfaceType::Aspherique {
        for _ in 0..15 {
            let px = x0 + t * l;
            let py = y0 + t * m;
            let pz = z0 + t * n;
            let r = (px * px + py * py).sqrt();

            let z_sag = surface.fleche(r)?;
            let residu = pz - z_sag;
            if residu.abs() < 1e-12 {
                break;
            }

            let dz_dr = surface.derivee_fleche_dr(r)?;
            let dr_dt = if r > 1e-14 {
                (px * l + py * m) / r
            } else {
                0.0
            };
            let df_dt = n - dz_dr * dr_dt;
            if df_dt.abs() < 1e-14 {
                break;
            }
            t -= residu / df_dt;
        }
    }

    // Validation finale sur la calotte
    let pf = p0 + t * dir;
    let rf = (pf.x * pf.x + pf.y * pf.y).sqrt();
    let z_sag = surface.fleche(rf)?;
    if (pf.z - z_sag).abs() > 1e-4 {
        return None;
    }

    Some(t)
}

/// Applique la loi vectorielle 3D de Snell-Descartes (réfraction ou réflexion miroir).
/// Retourne `Some(direction_sortante)` ou `None` en cas de Réflexion Totale Interne (TIR).
pub fn snell_descartes_3d(
    direction_incidente: &Vec3,
    normale_surface: &Vec3,
    n1: f64,
    n2: f64,
    est_miroir: bool,
) -> Option<Vec3> {
    let i = direction_incidente.normalize();
    let mut n = normale_surface.normalize();

    // Oriente la normale à l'encontre du rayon incident pour avoir cos(θ1) = -I · N >= 0
    let mut cos_theta1 = -i.dot(&n);
    if cos_theta1 < 0.0 {
        n = -n;
        cos_theta1 = -cos_theta1;
    }

    if est_miroir {
        let reflechi = i + 2.0 * cos_theta1 * n;
        return Some(reflechi.normalize());
    }

    let mu = n1 / n2;
    let sin2_theta2 = mu * mu * (1.0 - cos_theta1 * cos_theta1);
    if sin2_theta2 > 1.0 {
        // Réflexion Totale Interne (TIR)
        return None;
    }

    let cos_theta2 = (1.0 - sin2_theta2).sqrt();
    let refracte = mu * i + (mu * cos_theta1 - cos_theta2) * n;
    Some(refracte.normalize())
}

/// Équation vectorielle des réseaux : la composante tangentielle de `n·d` augmente de
/// `k = m λ N` (ordre × longueur d'onde en mm × traits par mm) selon la direction `g` des
/// sillons croisés (perpendiculaire aux traits), la composante normale assure |d'| = 1.
/// Avec `k = 0` on retrouve la réfraction ou la réflexion ordinaire. `None` : ordre évanescent.
pub fn diffraction_reseau_3d(
    d: &Vec3,
    normale: &Vec3,
    g: &Vec3,
    n1: f64,
    n2: f64,
    est_miroir: bool,
    k: f64,
) -> Option<Vec3> {
    let n = normale.normalize();
    let cos_i = d.dot(&n);
    let tangent_g = g - g.dot(&n) * n;
    let g_t = if tangent_g.norm() > 1e-12 { tangent_g.normalize() } else { return None };
    let t_sortie = n1 * (d - cos_i * n) + k * g_t;
    let n_sortie = if est_miroir { n1 } else { n2 };
    let reste = n_sortie * n_sortie - t_sortie.norm_squared();
    if reste < 0.0 {
        return None;
    }
    let signe = if cos_i >= 0.0 { 1.0 } else { -1.0 };
    let composante_normale = if est_miroir { -signe } else { signe } * reste.sqrt();
    Some(((t_sortie + composante_normale * n) / n_sortie).normalize())
}

/// Trace séquentiellement un rayon unique à travers toutes les surfaces du système optique
pub fn tracer_rayon_unique(
    systeme: &OpticalSystem,
    rayon_initial: Ray,
    champ_deg: (f64, f64),
    pupille_norm: (f64, f64),
) -> TracedRay {
    let positions_z = systeme.positions_z_sommets();
    let mut courant = rayon_initial.clone();
    let mut impacts = Vec::with_capacity(systeme.surfaces.len());
    let mut n_courant = 1.0; // Milieu objet initial = Air (n = 1.0)

    let nb_surfaces = systeme.surfaces.len();

    for (idx, (surface, &z_sommet)) in systeme.surfaces.iter().zip(positions_z.iter()).enumerate() {
        // Repère local de la surface : sommet décentré, axes inclinés
        let sommet_global = surface.sommet_global(z_sommet);
        let rotation = surface.rotation();
        let inverse = rotation.inverse();
        let p0_local = Point3::from(inverse * (courant.origine - sommet_global));
        let dir_local = inverse * courant.direction;

        let Some(t) = intersecter_surface_locale(&p0_local, &dir_local, surface) else {
            courant.vignette = true;
            break;
        };

        let p_local = p0_local + t * dir_local;
        let p_global = sommet_global + rotation * p_local.coords;
        let r_radial = (p_local.x * p_local.x + p_local.y * p_local.y).sqrt();

        // Mise à jour du chemin optique parcouru depuis l'origine précédente
        // t est signé : un segment virtuel (surface fictive en arrière) compte négativement
        courant.chemin_optique += t * n_courant;

        let Some(normale_locale) = surface.normale_locale(p_local.x, p_local.y) else {
            courant.vignette = true;
            break;
        };
        let normale = rotation * normale_locale;

        let mat_apres = systeme.catalogue.resoudre(&surface.verre);
        let est_miroir = mat_apres.est_miroir();
        let est_plan_image = idx + 1 == nb_surfaces;

        // Vérification du vignettage mécanique par le demi-diamètre
        // (le plan image n'arrête aucun rayon : son demi-diamètre ne sert qu'à l'affichage)
        let hors_ouverture = !est_plan_image
            && (r_radial > surface.semi_diametre + 1e-6 || r_radial < surface.rayon_obstruction);

        let n_apres = if est_miroir {
            n_courant
        } else {
            mat_apres.indice_refraction(courant.longueur_onde_nm)
        };

        let direction_sortante = if est_plan_image {
            // Sur le plan image, le rayon s'arrête sans nouvelle réfraction
            courant.direction
        } else {
            let sortie = if surface.reseau_traits_mm != 0.0 && surface.reseau_ordre != 0 {
                diffraction_reseau_3d(
                    &courant.direction,
                    &normale,
                    &(rotation * Vec3::new(0.0, 1.0, 0.0)),
                    n_courant,
                    n_apres,
                    est_miroir,
                    surface.reseau_ordre as f64 * courant.longueur_onde_nm * 1e-6 * surface.reseau_traits_mm,
                )
            } else {
                snell_descartes_3d(&courant.direction, &normale, n_courant, n_apres, est_miroir)
            };
            match sortie {
                Some(d) => d,
                None => {
                    // Réflexion totale interne non désirée
                    courant.vignette = true;
                    impacts.push(SurfaceHit {
                        index_surface: idx,
                        point_global: p_global,
                        point_local: p_local,
                        normale,
                        direction_sortante: courant.direction,
                        indice_apres: n_apres,
                        vignette: true,
                    });
                    break;
                }
            }
        };

        let vignette_ici = hors_ouverture || courant.vignette;
        if vignette_ici {
            courant.vignette = true;
        }

        impacts.push(SurfaceHit {
            index_surface: idx,
            point_global: p_global,
            point_local: p_local,
            normale,
            direction_sortante,
            indice_apres: n_apres,
            vignette: vignette_ici,
        });

        courant.origine = p_global;
        courant.direction = direction_sortante;
        n_courant = n_apres;

        if courant.vignette {
            break;
        }
    }

    TracedRay {
        rayon_initial,
        longueur_onde_nm: courant.longueur_onde_nm,
        champ_deg,
        pupille_norm,
        impacts,
        vignette: courant.vignette,
        chemin_optique_total: courant.chemin_optique,
    }
}

/// Retourne le demi-diamètre effectif de la pupille d'entrée (en mm)
pub fn rayon_pupille_entree(systeme: &OpticalSystem) -> f64 {
    let r_spec = match systeme.ouverture {
        ApertureSpec::PupilleEntreeMm(epd) => epd * 0.5,
        // Objet à l'infini : l'ouverture numérique est rapportée à l'espace image (F/# = 1 / 2NA)
        ApertureSpec::OuvertureNumeriqueObjet(na) => focale_paraxiale(systeme).abs() * na,
        ApertureSpec::NombreOuverture(fnum) => {
            focale_paraxiale(systeme).abs() / (2.0 * fnum.max(0.1))
        }
    };

    if r_spec.is_finite() && r_spec > 0.0 {
        r_spec
    } else {
        // Système afocal : à défaut de focale, on remplit 90 % de la première surface
        systeme
            .surfaces
            .first()
            .map(|s| s.semi_diametre * 0.9)
            .unwrap_or(10.0)
            .max(0.5)
    }
}

/// Paramètres de visée communs à tous les rayons d'un faisceau : rayon et position axiale
/// de la pupille d'entrée, plan de départ des rayons en amont de la première surface.
#[derive(Debug, Clone, Copy)]
pub struct VisePupille {
    pub rayon_mm: f64,
    pub z_pupille_mm: f64,
    pub z_depart_mm: f64,
}

impl VisePupille {
    pub fn calculer(systeme: &OpticalSystem) -> Self {
        let fleche_bord = systeme
            .surfaces
            .first()
            .and_then(|s| s.fleche(s.semi_diametre))
            .unwrap_or(0.0);
        Self {
            rayon_mm: rayon_pupille_entree(systeme),
            z_pupille_mm: position_pupille_entree(systeme),
            z_depart_mm: fleche_bord.min(0.0) - 15.0,
        }
    }

    /// Construit un rayon visant le point `(px, py)` (normalisé dans `[-1, 1]`) de la pupille d'entrée
    pub fn rayon(&self, px: f64, py: f64, hx_deg: f64, hy_deg: f64, longueur_onde_nm: f64) -> Ray {
        let x_pup = px * self.rayon_mm;
        let y_pup = py * self.rayon_mm;

        let tx = hx_deg.to_radians().tan();
        let ty = hy_deg.to_radians().tan();
        let dir = Vec3::new(tx, ty, 1.0).normalize();

        // Le rayon passe par (x_pup, y_pup, z_pupille) ; on le fait partir du plan z_depart,
        // en amont de la première surface, quelle que soit la position de la pupille
        let t_recul = (self.z_depart_mm - self.z_pupille_mm) / dir.z;
        let origine = Point3::new(
            x_pup + t_recul * dir.x,
            y_pup + t_recul * dir.y,
            self.z_depart_mm,
        );

        Ray::nouveau(origine, dir, longueur_onde_nm)
    }
}

/// Construit un rayon incident à partir de coordonnées pupillaires normalisées `(px, py)` dans `[-1, 1]`
/// et d'un angle de champ `(hx_deg, hy_deg)` en degrés.
pub fn creer_rayon_pupillaire(
    systeme: &OpticalSystem,
    px: f64,
    py: f64,
    hx_deg: f64,
    hy_deg: f64,
    longueur_onde_nm: f64,
) -> Ray {
    VisePupille::calculer(systeme).rayon(px, py, hx_deg, hy_deg, longueur_onde_nm)
}

/// Génère et trace un éventail méridional (plan YZ, px = 0, py de -1 à +1) pour l'affichage 2D
pub fn tracer_eventail_meridional(
    systeme: &OpticalSystem,
    hy_deg: f64,
    nb_rayons: usize,
    longueur_onde_nm: f64,
) -> Vec<TracedRay> {
    let n = nb_rayons.max(3);
    let mut resultats = Vec::with_capacity(n);
    let vise = VisePupille::calculer(systeme);

    for i in 0..n {
        let py = -1.0 + 2.0 * (i as f64) / ((n - 1) as f64);
        let ray = vise.rayon(0.0, py, 0.0, hy_deg, longueur_onde_nm);
        resultats.push(tracer_rayon_unique(systeme, ray, (0.0, hy_deg), (0.0, py)));
    }

    resultats
}

/// Génère et trace une distribution pupillaire hexapolaire (anneaux concentriques 1 + 6 + 12 + 18...)
/// utilisée par Zemax pour les Spot Diagrams et le rendu 3D.
pub fn tracer_grille_hexapolaire(
    systeme: &OpticalSystem,
    hx_deg: f64,
    hy_deg: f64,
    nb_anneaux: usize,
    longueur_onde_nm: f64,
) -> Vec<TracedRay> {
    let mut rayons = Vec::new();
    let anneaux = nb_anneaux.max(1);
    let vise = VisePupille::calculer(systeme);

    // Rayon central (chef)
    let ray_chef = vise.rayon(0.0, 0.0, hx_deg, hy_deg, longueur_onde_nm);
    rayons.push(tracer_rayon_unique(
        systeme,
        ray_chef,
        (hx_deg, hy_deg),
        (0.0, 0.0),
    ));

    for k in 1..=anneaux {
        let rho = (k as f64) / (anneaux as f64);
        let nb_pts = 6 * k;
        for j in 0..nb_pts {
            let theta = 2.0 * std::f64::consts::PI * (j as f64) / (nb_pts as f64);
            let px = rho * theta.cos();
            let py = rho * theta.sin();
            let ray = vise.rayon(px, py, hx_deg, hy_deg, longueur_onde_nm);
            rayons.push(tracer_rayon_unique(
                systeme,
                ray,
                (hx_deg, hy_deg),
                (px, py),
            ));
        }
    }

    rayons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::analysis::{calculer_aberration_spherique_longitudinale, calculer_spot_diagram};
    use crate::core::optimizer::{
        OptimizationTarget, OptimizationVariable, optimiser_systeme_dls,
    };
    use crate::core::paraxial::calculer_paraxial;

    #[test]
    fn test_raytracing_doublet_achromatique() {
        let sys = OpticalSystem::doublet_achromatique_defaut();
        let paraxial = calculer_paraxial(&sys);
        assert!(
            (paraxial.focale_effl_mm - 100.0).abs() < 3.0,
            "EFFL attendue ~ 100mm, obtenue: {}",
            paraxial.focale_effl_mm
        );

        let spot = calculer_spot_diagram(&sys, (0.0, 0.0), 3);
        assert!(spot.nb_rayons_valides > 30);
        assert!(spot.rayon_rms_um < 150.0);

        let lsa = calculer_aberration_spherique_longitudinale(&sys, 10);
        assert!(!lsa.echantillons.is_empty());
    }

    #[test]
    fn test_intersection_aspherique_et_dls() {
        let mut sys = OpticalSystem::doublet_achromatique_defaut();
        sys.surfaces[0].type_surface = SurfaceType::Aspherique;
        sys.surfaces[0].aspheriques[0] = 1e-7;

        let eventail = tracer_eventail_meridional(&sys, 0.0, 7, sys.longueur_onde_ref_nm);
        assert_eq!(eventail.len(), 7);
        assert!(eventail.iter().all(|r| !r.vignette));

        let vars = vec![
            OptimizationVariable {
                index_surface: 2,
                parametre: "rayon".to_string(),
            },
            OptimizationVariable {
                index_surface: 2,
                parametre: "epaisseur".to_string(),
            },
        ];
        let cibles = vec![
            OptimizationTarget {
                type_cible: "focale".to_string(),
                valeur_cible: 100.0,
                poids: 1.0,
            },
            OptimizationTarget {
                type_cible: "spot_rms".to_string(),
                valeur_cible: 0.0,
                poids: 1.0,
            },
        ];

        let rapport = optimiser_systeme_dls(&mut sys, &vars, &cibles, 8);
        assert!(rapport.merite_final <= rapport.merite_initial);
    }

    #[test]
    fn reseau_de_diffraction() {
        use crate::core::types::{Surface, SurfaceType};
        let n = Vec3::new(0.0, 0.0, 1.0);
        let g = Vec3::new(0.0, 1.0, 0.0);
        // k = 0 : réfraction ordinaire
        let d = Vec3::new(0.1, 0.2, 1.0).normalize();
        let a = diffraction_reseau_3d(&d, &n, &g, 1.0, 1.5, false, 0.0).unwrap();
        let b = snell_descartes_3d(&d, &n, 1.0, 1.5, false).unwrap();
        assert!((a - b).norm() < 1e-12);
        // Réseau en transmission 300 traits/mm, incidence normale, ordre 1 : sin θ = λ N
        let k = 587.5618e-6 * 300.0;
        let t = diffraction_reseau_3d(&Vec3::new(0.0, 0.0, 1.0), &n, &g, 1.0, 1.0, false, k).unwrap();
        assert!((t.y - k).abs() < 1e-12 && t.z > 0.0);
        // Montage de Littrow en réflexion (1200 traits/mm) : le rayon revient sur lui-même
        let k = 587.5618e-6 * 1200.0;
        let s = k / 2.0;
        let d = Vec3::new(0.0, -s, -(1.0 - s * s).sqrt());
        let r = diffraction_reseau_3d(&d, &n, &g, 1.0, 1.0, true, k).unwrap();
        assert!((r + d).norm() < 1e-12, "{r:?}");
        // Ordre évanescent
        assert!(diffraction_reseau_3d(&Vec3::new(0.0, 0.0, 1.0), &n, &g, 1.0, 1.0, false, 1.2).is_none());

        // Dans le lancer de rayons : réseau plan puis plan image à 100 mm
        let mut reseau = Surface::nouvelle(SurfaceType::Plane, f64::INFINITY, 100.0, "AIR", 20.0);
        reseau.reseau_traits_mm = 300.0;
        reseau.reseau_ordre = 1;
        reseau.est_diaphragme = true;
        let sys = OpticalSystem {
            surfaces: vec![reseau, Surface::plan_image(50.0)],
            ..OpticalSystem::nouveau("réseau", 587.5618)
        };
        let tr = tracer_rayon_unique(
            &sys,
            Ray::nouveau(Point3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, 1.0), 587.5618),
            (0.0, 0.0),
            (0.0, 0.0),
        );
        let y = tr.impact_plan_image().unwrap().point_global.y;
        let sin: f64 = 587.5618e-6 * 300.0;
        assert!((y - 100.0 * sin / (1.0 - sin * sin).sqrt()).abs() < 1e-9, "y = {y}");
    }
}

