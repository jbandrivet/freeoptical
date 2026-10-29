//! Vues de l'énergie encerclée, des aberrations chromatiques (décalage focal, couleur latérale)
//! et de l'empreinte des faisceaux sur une surface.

use super::aberrations::{Courbe, echelle, graphe};
use super::ondes::libelle_champ;
use super::*;

/// Couleurs des champs dans l'empreinte
const COULEURS_CHAMPS: [Color32; 6] = [
    Color32::from_rgb(0, 220, 255),
    Color32::from_rgb(80, 230, 80),
    Color32::from_rgb(255, 220, 60),
    Color32::from_rgb(255, 120, 255),
    Color32::from_rgb(255, 160, 60),
    Color32::from_rgb(160, 160, 255),
];

impl FreeOpticalGuiApp {
    pub(super) fn dessiner_energie(&mut self, ui: &mut egui::Ui) {
        self.selecteurs_ondes(ui, false);
        let unite = if self.systeme.afocal { "µrad" } else { "µm" };
        let e = match self.ondes.energie(&self.systeme) {
            Ok(e) => e.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        let diff = |v: Option<f64>| v.map(|v| format!("{v:.3}")).unwrap_or_else(|| "—".into());
        ui.label(
            RichText::new(trf!(
                "Champ {} — 50 % : {:.3} {} (géom.), {} {} (diffr.) — 80 % : {:.3} {} (géom.), {} {} (diffr.) — Airy {:.3} {}",
                "Field {} — 50%: {:.3} {} (geom.), {} {} (diffr.) — 80%: {:.3} {} (geom.), {} {} (diffr.) — Airy {:.3} {}",
                "Feld {} — 50 %: {:.3} {} (geom.), {} {} (Beug.) — 80 %: {:.3} {} (geom.), {} {} (Beug.) — Airy {:.3} {}",
                libelle_champ(self.systeme.unite_champ(), e.champ_deg),
                e.r50_geo_um,
                unite,
                diff(e.r50_diffraction_um),
                unite,
                e.r80_geo_um,
                unite,
                diff(e.r80_diffraction_um),
                unite,
                e.rayon_airy_um,
                unite
            ))
            .monospace()
            .color(COL_DOS_YELLOW),
        );
        ui.label(
            RichText::new(tr!(
                "Cyan : géométrique — vert : diffraction (PSF polychromatique) — tirets : tache d'Airy d'une pupille pleine sans aberration",
                "Cyan: geometric — green: diffraction (polychromatic PSF) — dashed: Airy pattern of an unobstructed aberration-free pupil",
                "Cyan: geometrisch — grün: Beugung (polychromatische PSF) — gestrichelt: Airy-Scheibchen einer freien, fehlerfreien Pupille"
            ))
            .monospace()
            .color(COL_DOS_GRAY),
        );
        let points = |v: &[f64]| e.rayons_um.iter().copied().zip(v.iter().copied()).collect::<Vec<_>>();
        let mut courbes = vec![
            Courbe { points: points(&e.airy), couleur: COL_DOS_GRAY, tirets: true },
            Courbe { points: points(&e.geometrique), couleur: COL_DOS_CYAN, tirets: false },
        ];
        if let Some(d) = &e.diffraction {
            courbes.push(Courbe { points: points(d), couleur: COL_DOS_GREEN, tirets: false });
        }
        let r_max = e.rayons_um.last().copied().unwrap_or(1.0);
        graphe(
            ui,
            ui.available_size(),
            tr!("ÉNERGIE ENCERCLÉE", "ENCIRCLED ENERGY", "EINGESCHLOSSENE ENERGIE"),
            &trf!("rayon ({})", "radius ({})", "Radius ({})", unite),
            (0.0, r_max),
            (0.0, 1.0),
            &courbes,
        );
    }

    pub(super) fn dessiner_chromatique(&mut self, ui: &mut egui::Ui) {
        let r = self.ondes.chromatique(&self.systeme).clone();
        let dispo = ui.available_size();
        let taille = Vec2::new(dispo.x / 2.0 - 6.0, (dispo.y - 40.0).max(120.0));
        match &r.decalage {
            Ok(d) => {
                ui.label(
                    RichText::new(trf!(
                        "Décalage focal : étendue {:.2} µm ({:.1} → {:.1} nm), profondeur de foyer ±{:.2} µm (±2 λ N²) — référence {:.1} nm",
                        "Focal shift: range {:.2} µm ({:.1} → {:.1} nm), depth of focus ±{:.2} µm (±2 λ N²) — reference {:.1} nm",
                        "Fokusverschiebung: Bereich {:.2} µm ({:.1} → {:.1} nm), Schärfentiefe ±{:.2} µm (±2 λ N²) — Referenz {:.1} nm",
                        d.etendue_um,
                        d.lambda_foyer_min_nm,
                        d.lambda_foyer_max_nm,
                        d.profondeur_diffraction_um,
                        d.longueur_onde_ref_nm
                    ))
                    .monospace()
                    .color(COL_DOS_YELLOW),
                );
            }
            Err(e) => Self::afficher_erreur_onde(ui, e),
        }
        if let Ok(c) = &r.laterale {
            ui.label(
                RichText::new(trf!(
                    "Couleur latérale : écart maximal {:.3} µm au bord du champ {} — rayon d'Airy {:.3} µm",
                    "Lateral colour: largest spread {:.3} µm at the edge of the field {} — Airy radius {:.3} µm",
                    "Farbquerfehler: größte Abweichung {:.3} µm am Feldrand {} — Airy-Radius {:.3} µm",
                    c.ecart_max_um,
                    libelle_champ(self.systeme.unite_champ(), c.champ_max),
                    c.rayon_airy_um
                ))
                .monospace()
                .color(COL_DOS_YELLOW),
            );
        }
        ui.horizontal(|ui| {
            if let Ok(d) = &r.decalage {
                let (a, b) = (d.longueurs_onde_nm[0], *d.longueurs_onde_nm.last().unwrap_or(&700.0));
                let e = echelle(d.decalage_um.iter().copied().chain([d.profondeur_diffraction_um]), 1.0);
                let p = d.profondeur_diffraction_um;
                graphe(
                    ui,
                    taille,
                    tr!("DÉCALAGE FOCAL CHROMATIQUE (µm)", "CHROMATIC FOCAL SHIFT (µm)", "CHROMATISCHE FOKUSVERSCHIEBUNG (µm)"),
                    tr!("longueur d'onde (nm)", "wavelength (nm)", "Wellenlänge (nm)"),
                    (a, b),
                    (-e, e),
                    &[
                        Courbe { points: vec![(a, p), (b, p)], couleur: COL_DOS_GRAY, tirets: true },
                        Courbe { points: vec![(a, -p), (b, -p)], couleur: COL_DOS_GRAY, tirets: true },
                        Courbe {
                            points: d.longueurs_onde_nm.iter().copied().zip(d.decalage_um.iter().copied()).collect(),
                            couleur: COL_DOS_CYAN,
                            tirets: false,
                        },
                    ],
                );
            }
            match &r.laterale {
                Ok(c) => {
                    let e = echelle(c.ecart_um.iter().flatten().flatten().copied().chain([c.rayon_airy_um]), 0.1);
                    let mut courbes = vec![
                        Courbe { points: vec![(0.0, c.rayon_airy_um), (1.0, c.rayon_airy_um)], couleur: COL_DOS_GRAY, tirets: true },
                        Courbe { points: vec![(0.0, -c.rayon_airy_um), (1.0, -c.rayon_airy_um)], couleur: COL_DOS_GRAY, tirets: true },
                    ];
                    for (v, &l) in c.ecart_um.iter().zip(&c.longueurs_onde_nm) {
                        courbes.push(Courbe {
                            points: c.fractions_champ.iter().zip(v).filter_map(|(&f, y)| y.map(|y| (f, y))).collect(),
                            couleur: couleur_spectre(l),
                            tirets: false,
                        });
                    }
                    graphe(
                        ui,
                        taille,
                        tr!("COULEUR LATÉRALE (µm) — tirets : Airy", "LATERAL COLOUR (µm) — dashed: Airy", "FARBQUERFEHLER (µm) — gestrichelt: Airy"),
                        tr!("fraction du champ", "field fraction", "Feldanteil"),
                        (0.0, 1.0),
                        (-e, e),
                        &courbes,
                    );
                }
                Err(e) => {
                    ui.label(RichText::new(trf!("Couleur latérale : {}", "Lateral colour: {}", "Farbquerfehler: {}", e)).monospace().color(COL_DOS_GRAY));
                }
            }
        });
    }

    pub(super) fn dessiner_empreinte(&mut self, ui: &mut egui::Ui) {
        let nb = self.systeme.surfaces.len();
        let mut choix = self.ondes.surface_empreinte.min(nb.saturating_sub(1));
        let mut ajuster = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr!("Surface :", "Surface:", "Fläche:")).monospace());
            egui::ComboBox::from_id_salt("choix_surface_empreinte")
                .selected_text(format!("#{:02} {}", choix + 1, self.systeme.surfaces.get(choix).map(|s| s.commentaire.as_str()).unwrap_or("")))
                .show_ui(ui, |ui| {
                    for (i, s) in self.systeme.surfaces.iter().enumerate() {
                        if !s.est_coupure() {
                            ui.selectable_value(&mut choix, i, format!("#{:02} {}", i + 1, s.commentaire));
                        }
                    }
                });
            ajuster = ui
                .button(tr!("Ajuster tous les demi-diamètres", "Fit all semi-diameters", "Alle Halbdurchmesser anpassen"))
                .on_hover_text(tr!(
                    "Donne à chaque surface le demi-diamètre juste nécessaire pour transmettre tous les champs et toutes les longueurs d'onde (comme le demi-diamètre automatique de Zemax)",
                    "Gives each surface the semi-diameter just needed to pass all fields and wavelengths (like Zemax automatic semi-diameters)",
                    "Gibt jeder Fläche den Halbdurchmesser, der gerade alle Felder und Wellenlängen durchlässt (wie die automatischen Halbdurchmesser von Zemax)"
                ))
                .clicked();
        });
        if ajuster {
            let requis = crate::core::empreinte::demi_diametres_requis(&self.systeme);
            let n = self.systeme.surfaces.len();
            for (i, (s, r)) in self.systeme.surfaces.iter_mut().zip(requis).enumerate() {
                // Le plan image garde son demi-diamètre s'il est plus grand (il n'arrête rien)
                if !s.est_coupure() && r > 0.0 && (i + 1 < n || r > s.semi_diametre) {
                    s.semi_diametre = (r * 1000.0).ceil() / 1000.0;
                }
            }
            self.recalculer();
            self.statut_message = tr!(
                "Demi-diamètres ajustés aux faisceaux de tous les champs.",
                "Semi-diameters fitted to the beams of all fields.",
                "Halbdurchmesser an die Bündel aller Felder angepasst."
            )
            .to_string();
        }
        self.ondes.choisir_surface_empreinte(choix);
        let e = match self.ondes.empreinte(&self.systeme) {
            Ok(e) => e.clone(),
            Err(e) => {
                let e = e.clone();
                Self::afficher_erreur_onde(ui, &e);
                return;
            }
        };
        ui.label(
            RichText::new(trf!(
                "Demi-diamètre {:.3} mm — rayon utilisé {:.3} mm — nécessaire {:.3} mm — rayons arrêtés ici ou avant : {} / {}",
                "Semi-diameter {:.3} mm — radius used {:.3} mm — needed {:.3} mm — rays stopped here or before: {} / {}",
                "Halbdurchmesser {:.3} mm — genutzter Radius {:.3} mm — nötig {:.3} mm — hier oder davor gestoppte Strahlen: {} / {}",
                e.semi_diametre_mm,
                e.rayon_utile_mm,
                e.rayon_requis_mm,
                e.nb_vignettes,
                e.nb_rayons
            ))
            .monospace()
            .color(if e.rayon_requis_mm > e.semi_diametre_mm + 1e-6 && e.surface + 1 < nb { COL_DOS_ORANGE } else { COL_DOS_YELLOW }),
        );
        // Légende des champs
        ui.horizontal_wrapped(|ui| {
            for (i, &c) in self.systeme.champs_deg.iter().enumerate() {
                ui.label(RichText::new(format!("■ {}", libelle_champ(self.systeme.unite_champ(), c))).monospace().color(COULEURS_CHAMPS[i % COULEURS_CHAMPS.len()]));
            }
            ui.label(RichText::new(tr!("✕ arrêté", "✕ stopped", "✕ gestoppt")).monospace().color(COL_DOS_RED));
        });
        let dispo = ui.available_size();
        let (rect, _) = ui.allocate_exact_size(dispo, Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, COL_DOS_CANVAS);
        let etendue = e
            .points
            .iter()
            .map(|p| p.x_mm.abs().max(p.y_mm.abs()))
            .fold(e.semi_diametre_mm, f64::max)
            .max(1e-3)
            * 1.08;
        let echelle_px = (rect.width().min(rect.height()) as f64 / 2.0 - 12.0).max(10.0) / etendue;
        let centre = rect.center();
        let vers = |x: f64, y: f64| Pos2::new(centre.x + (x * echelle_px) as f32, centre.y - (y * echelle_px) as f32);
        tracer_segment(&painter, vers(-etendue, 0.0), vers(etendue, 0.0), 0.5, Color32::from_rgb(45, 45, 70));
        tracer_segment(&painter, vers(0.0, -etendue), vers(0.0, etendue), 0.5, Color32::from_rgb(45, 45, 70));
        painter.circle_stroke(centre, (e.semi_diametre_mm * echelle_px) as f32, Stroke::new(1.5, COL_DOS_WHITE));
        if e.rayon_obstruction_mm > 0.0 {
            painter.circle_stroke(centre, (e.rayon_obstruction_mm * echelle_px) as f32, Stroke::new(1.2, COL_DOS_GRAY));
        }
        for p in &e.points {
            let q = vers(p.x_mm, p.y_mm);
            if p.vignette {
                let d = 2.0;
                tracer_segment(&painter, q - Vec2::splat(d), q + Vec2::splat(d), 1.0, COL_DOS_RED);
                tracer_segment(&painter, q + Vec2::new(-d, d), q + Vec2::new(d, -d), 1.0, COL_DOS_RED);
            } else {
                painter.circle_filled(q, 1.6, COULEURS_CHAMPS[p.champ % COULEURS_CHAMPS.len()]);
            }
        }
        painter.text(
            rect.left_top() + Vec2::new(6.0, 4.0),
            Align2::LEFT_TOP,
            trf!("±{:.2} mm (repère local)", "±{:.2} mm (local frame)", "±{:.2} mm (lokales System)", etendue),
            FontId::new(10.5, FontFamily::Monospace),
            COL_DOS_GRAY,
        );
    }
}
