//! Exports : vue affichée en image PNG, fiche du système en PDF (avec les plans des lentilles
//! et des miroirs), coupe en DXF pour la CAO. Le nom et le dossier se choisissent dans la
//! fenêtre de fichiers (mode Exporter).

use super::*;

/// Ce que l'on exporte
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeExport {
    /// La vue affichée (coupe, spot, FTM…) telle qu'elle est à l'écran
    Image,
    /// Fiche du système et plans de fabrication
    Fiche,
    /// Coupe méridienne pour la CAO
    Dxf,
}

impl TypeExport {
    pub fn extension(self) -> &'static str {
        match self {
            TypeExport::Image => "png",
            TypeExport::Fiche => "pdf",
            TypeExport::Dxf => "dxf",
        }
    }

    pub fn titre(self) -> &'static str {
        match self {
            TypeExport::Image => tr!(" Exporter la vue en image PNG ", " Export the view as a PNG image ", " Ansicht als PNG-Bild exportieren "),
            TypeExport::Fiche => tr!(" Exporter la fiche et les plans en PDF ", " Export the sheet and drawings as PDF ", " Datenblatt und Zeichnungen als PDF exportieren "),
            TypeExport::Dxf => tr!(" Exporter la coupe en DXF (CAO) ", " Export the section as DXF (CAD) ", " Schnitt als DXF exportieren (CAD) "),
        }
    }
}

#[derive(Default)]
pub struct EtatExport {
    /// Rectangle de la vue affichée (à découper dans la capture d'écran)
    pub rect_vue: Option<egui::Rect>,
    /// Image demandée : fichier et nombre d'images à attendre (la fenêtre de fichiers doit
    /// avoir disparu de l'écran avant la capture)
    image_en_attente: Option<(PathBuf, u8)>,
    recepteur: Option<mpsc::Receiver<Result<PathBuf, String>>>,
}

/// Nom de fichier sans caractères gênants
fn nom_fichier(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let n = n.trim_matches('_').to_string();
    if n.is_empty() { "systeme".into() } else { n }
}

/// Découpe `rect` (en points) dans la capture et l'écrit en PNG
fn enregistrer_capture(image: &egui::ColorImage, rect: Option<egui::Rect>, ppp: f32, chemin: &Path) -> Result<(), String> {
    let [l, h] = image.size;
    let (x0, y0, x1, y1) = match rect {
        Some(r) => (
            ((r.min.x * ppp).floor().max(0.0) as usize).min(l),
            ((r.min.y * ppp).floor().max(0.0) as usize).min(h),
            ((r.max.x * ppp).ceil().max(0.0) as usize).min(l),
            ((r.max.y * ppp).ceil().max(0.0) as usize).min(h),
        ),
        None => (0, 0, l, h),
    };
    if x1 <= x0 || y1 <= y0 {
        return Err(tr!("zone de la vue vide", "empty view area", "leerer Ansichtsbereich").into());
    }
    let decoupe = image.region_by_pixels([x0, y0], [x1 - x0, y1 - y0]);
    let mut octets = Vec::with_capacity(decoupe.pixels.len() * 4);
    for c in &decoupe.pixels {
        let [r, g, b, _] = c.to_array();
        octets.extend_from_slice(&[r, g, b, 255]);
    }
    let img = image::RgbaImage::from_raw((x1 - x0) as u32, (y1 - y0) as u32, octets).ok_or("image")?;
    img.save_with_format(chemin, image::ImageFormat::Png).map_err(|e| e.to_string())
}

impl EtatExport {
    /// Vrai entre la demande d'une image et sa capture
    pub fn capture_en_cours(&self) -> bool {
        self.image_en_attente.is_some() || self.recepteur.is_some()
    }
}

impl FreeOpticalGuiApp {
    /// Nom proposé pour un export
    pub fn nom_export(&self, t: TypeExport) -> String {
        let base = nom_fichier(&self.systeme.nom);
        match t {
            TypeExport::Image => format!("{base}_{}.png", nom_fichier(self.vue_active.libelle_court())),
            TypeExport::Fiche => format!("{base}_{}.pdf", tr!("fiche", "sheet", "Datenblatt")),
            TypeExport::Dxf => format!("{base}.dxf"),
        }
    }

    /// Lance l'export ; faux en cas d'échec. Les remarques (pièce impossible à dessiner) sont
    /// laissées dans le bilan de la fenêtre de fichiers.
    pub fn exporter(&mut self, t: TypeExport, chemin: &Path) -> bool {
        if let Some(d) = chemin.parent() {
            fichiers::memoriser_dossier(d);
        }
        let resultat: Result<Vec<String>, String> = match t {
            TypeExport::Image => {
                // La capture se fait dans quelques images, quand la fenêtre de fichiers est fermée
                self.export.image_en_attente = Some((chemin.to_path_buf(), 2));
                self.statut_message = tr!("Capture de la vue…", "Capturing the view…", "Ansicht wird aufgenommen…").into();
                return true;
            }
            TypeExport::Fiche => {
                let (octets, remarques) = crate::core::fiche::fiche_pdf(&self.systeme);
                std::fs::write(chemin, octets).map(|_| remarques).map_err(|e| e.to_string())
            }
            TypeExport::Dxf => {
                let d = crate::core::dxf::exporter_dxf(&self.systeme);
                std::fs::write(chemin, d.texte).map(|_| d.remarques).map_err(|e| e.to_string())
            }
        };
        match resultat {
            Ok(remarques) => {
                self.statut_message = trf!("Exporté dans {}.", "Exported to {}.", "Exportiert nach {}.", chemin.display());
                if !remarques.is_empty() {
                    self.bilan_import = Some((true, self.statut_message.clone(), remarques));
                }
                true
            }
            Err(e) => {
                self.statut_message = trf!("ÉCHEC de l'export dans {} : {}", "FAILED to export to {}: {}", "FEHLER beim Export nach {}: {}", chemin.display(), e);
                self.bilan_import = Some((false, self.statut_message.clone(), Vec::new()));
                false
            }
        }
    }

    /// Capture d'écran en attente et résultat de la capture
    pub(super) fn suivre_export(&mut self, ctx: &Context) {
        if let Some((chemin, attente)) = self.export.image_en_attente.take() {
            if attente > 0 {
                self.export.image_en_attente = Some((chemin, attente - 1));
                ctx.request_repaint();
            } else {
                let (emetteur, recepteur) = mpsc::channel();
                self.export.recepteur = Some(recepteur);
                let rect = self.export.rect_vue;
                let ppp = ctx.pixels_per_point();
                let c = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
                    let r = enregistrer_capture(&image, rect, ppp, &chemin).map(|_| chemin);
                    let _ = emetteur.send(r);
                    c.request_repaint();
                })));
                ctx.request_repaint();
            }
        }
        if let Some(r) = self.export.recepteur.as_ref().and_then(|r| r.try_recv().ok()) {
            self.export.recepteur = None;
            self.statut_message = match r {
                Ok(c) => trf!("Vue exportée dans {}.", "View exported to {}.", "Ansicht exportiert nach {}.", c.display()),
                Err(e) => trf!("ÉCHEC de l'export de la vue : {}", "FAILED to export the view: {}", "FEHLER beim Export der Ansicht: {}", e),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_fiche_et_dxf() {
        let dossier = std::env::temp_dir().join(format!("freeoptical-export-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let mut app = FreeOpticalGuiApp::nouveau(OpticalSystem::doublet_achromatique_defaut());
        assert!(app.nom_export(TypeExport::Fiche).ends_with(".pdf"));
        let pdf = dossier.join("fiche.pdf");
        assert!(app.exporter(TypeExport::Fiche, &pdf));
        assert!(std::fs::read(&pdf).unwrap().starts_with(b"%PDF"));
        let dxf = dossier.join("coupe.dxf");
        assert!(app.exporter(TypeExport::Dxf, &dxf));
        assert!(std::fs::read_to_string(&dxf).unwrap().ends_with("EOF\n"));
        // Dossier inexistant : échec signalé
        assert!(!app.exporter(TypeExport::Dxf, &dossier.join("absent").join("x.dxf")));
        assert!(app.bilan_import.as_ref().is_some_and(|b| !b.0));
        std::fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn decoupe_de_la_capture() {
        let dossier = std::env::temp_dir().join(format!("freeoptical-capture-{}", std::process::id()));
        std::fs::create_dir_all(&dossier).unwrap();
        let mut img = egui::ColorImage::filled([40, 30], Color32::BLACK);
        img.pixels[10 * 40 + 20] = Color32::RED;
        let chemin = dossier.join("vue.png");
        // Rectangle en points avec 2 pixels par point, débordant de l'image
        let rect = egui::Rect::from_min_max(egui::pos2(5.0, 2.5), egui::pos2(100.0, 100.0));
        enregistrer_capture(&img, Some(rect), 2.0, &chemin).unwrap();
        let lu = image::open(&chemin).unwrap().to_rgba8();
        assert_eq!(lu.dimensions(), (30, 25));
        assert_eq!(lu.get_pixel(10, 5).0, [255, 0, 0, 255]);
        std::fs::remove_dir_all(&dossier).unwrap();
    }
}
