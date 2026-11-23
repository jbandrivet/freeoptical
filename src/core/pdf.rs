//! Petit générateur de PDF vectoriel (texte, traits, surfaces pleines), sans dépendance.
//!
//! Coordonnées en millimètres depuis le coin HAUT gauche de la page A4 (210 × 297 mm), comme
//! sur un plan. Polices standard du PDF (Helvetica, Courier) en codage WinAnsi : les lettres
//! accentuées du français et de l'allemand passent ; les rares signes absents (λ, ∞, →…) sont
//! remplacés par un équivalent écrit.

/// Largeur et hauteur d'une page A4 portrait, en mm
pub const A4: (f64, f64) = (210.0, 297.0);

const PT_PAR_MM: f64 = 72.0 / 25.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Police {
    Helvetica,
    HelveticaGras,
    Courier,
    CourierGras,
}

impl Police {
    fn nom_ressource(self) -> &'static str {
        match self {
            Police::Helvetica => "F1",
            Police::HelveticaGras => "F2",
            Police::Courier => "F3",
            Police::CourierGras => "F4",
        }
    }
}

/// Alignement horizontal d'un texte par rapport au point donné
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignement {
    Gauche,
    Centre,
    Droite,
}

/// Couleur RVB (0 à 1)
pub type Couleur = [f64; 3];
pub const NOIR: Couleur = [0.0, 0.0, 0.0];
pub const GRIS: Couleur = [0.45, 0.45, 0.45];

/// Largeurs des caractères d'Helvetica (millièmes de la taille), de l'espace (32) au tilde (126)
const LARGEURS_HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // espace à /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // 0 à 9
    278, 278, 584, 584, 584, 556, 1015, // : à @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667,
    611, 722, 667, 944, 667, 667, 611, // A à Z
    278, 278, 278, 469, 556, 333, // [ à `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500,
    278, 556, 500, 722, 500, 500, 500, // a à z
    334, 260, 334, 584, // { à ~
];

/// Lettre de base d'une lettre accentuée (pour sa largeur)
fn lettre_de_base(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'î' | 'ï' | 'í' | 'ì' => 'i',
        'Î' | 'Ï' | 'Í' | 'Ì' => 'I',
        'ô' | 'ö' | 'ó' | 'ò' | 'õ' | 'ø' => 'o',
        'Ô' | 'Ö' | 'Ó' | 'Ò' | 'Õ' | 'Ø' => 'O',
        'ù' | 'û' | 'ü' | 'ú' => 'u',
        'Ù' | 'Û' | 'Ü' | 'Ú' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        'ñ' => 'n',
        'ÿ' | 'ý' => 'y',
        'ß' => 'B',
        _ => c,
    }
}

/// Remplace les signes que le codage WinAnsi n'a pas
fn remplacer_absents(texte: &str) -> String {
    let mut s = String::with_capacity(texte.len());
    for c in texte.chars() {
        match c {
            'λ' => s.push_str("lambda"),
            'Δ' => s.push_str("delta "),
            'σ' => s.push_str("sigma"),
            'π' => s.push_str("pi"),
            '∞' => s.push_str("inf."),
            '→' => s.push_str("->"),
            '←' => s.push_str("<-"),
            '≈' => s.push('~'),
            '≤' => s.push_str("<="),
            '≥' => s.push_str(">="),
            '−' => s.push('-'),
            '\u{202f}' | '\u{2009}' => s.push(' '),
            _ => s.push(c),
        }
    }
    s
}

/// Octet WinAnsi (cp1252) d'un caractère, `?` s'il n'existe pas
fn octet_winansi(c: char) -> u8 {
    let u = c as u32;
    if (0x20..0x7f).contains(&u) || (0xa0..=0xff).contains(&u) {
        return u as u8;
    }
    match c {
        '€' => 0x80,
        '‚' => 0x82,
        '„' => 0x84,
        '…' => 0x85,
        '‰' => 0x89,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '›' => 0x9b,
        'œ' => 0x9c,
        'Ÿ' => 0x9f,
        _ => b'?',
    }
}

/// Largeur d'un texte en mm dans une police et une taille (points) données
pub fn largeur_texte(texte: &str, police: Police, taille_pt: f64) -> f64 {
    let texte = remplacer_absents(texte);
    let milliemes: f64 = match police {
        Police::Courier | Police::CourierGras => 600.0 * texte.chars().count() as f64,
        Police::Helvetica | Police::HelveticaGras => texte
            .chars()
            .map(|c| {
                let b = lettre_de_base(c) as u32;
                let l = if (32..127).contains(&b) { LARGEURS_HELVETICA[(b - 32) as usize] as f64 } else { 556.0 };
                if police == Police::HelveticaGras { l * 1.06 } else { l }
            })
            .sum(),
    };
    milliemes / 1000.0 * taille_pt / PT_PAR_MM
}

fn chaine_pdf(texte: &str) -> Vec<u8> {
    let mut o = vec![b'('];
    for c in remplacer_absents(texte).chars() {
        let b = octet_winansi(c);
        if matches!(b, b'(' | b')' | b'\\') {
            o.push(b'\\');
        }
        o.push(b);
    }
    o.push(b')');
    o
}

fn n(v: f64) -> String {
    // Trois décimales en points suffisent largement (moins d'un millième de mm)
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Une page en cours de dessin
#[derive(Debug, Default, Clone)]
pub struct Page {
    contenu: Vec<u8>,
    largeur_mm: f64,
    hauteur_mm: f64,
}

impl Page {
    fn x(&self, x_mm: f64) -> f64 {
        x_mm * PT_PAR_MM
    }
    fn y(&self, y_mm: f64) -> f64 {
        (self.hauteur_mm - y_mm) * PT_PAR_MM
    }
    fn ecrire(&mut self, s: &str) {
        self.contenu.extend_from_slice(s.as_bytes());
    }

    pub fn largeur(&self) -> f64 {
        self.largeur_mm
    }
    pub fn hauteur(&self) -> f64 {
        self.hauteur_mm
    }

    /// Texte dont la ligne de base passe par (x, y)
    pub fn texte(&mut self, x: f64, y: f64, texte: &str, police: Police, taille_pt: f64, alignement: Alignement, couleur: Couleur) {
        if texte.is_empty() {
            return;
        }
        let l = largeur_texte(texte, police, taille_pt);
        let x0 = match alignement {
            Alignement::Gauche => x,
            Alignement::Centre => x - 0.5 * l,
            Alignement::Droite => x - l,
        };
        let cmd = format!(
            "BT {} {} {} rg /{} {} Tf {} {} Td ",
            n(couleur[0]),
            n(couleur[1]),
            n(couleur[2]),
            police.nom_ressource(),
            n(taille_pt),
            n(self.x(x0)),
            n(self.y(y))
        );
        self.ecrire(&cmd);
        let c = chaine_pdf(texte);
        self.contenu.extend_from_slice(&c);
        self.ecrire(" Tj ET\n");
    }

    /// Texte tourné (angle en degrés, sens trigonométrique), aligné autour de (x, y)
    pub fn texte_tourne(&mut self, x: f64, y: f64, angle_deg: f64, texte: &str, police: Police, taille_pt: f64, alignement: Alignement, couleur: Couleur) {
        let l = largeur_texte(texte, police, taille_pt);
        let decalage = match alignement {
            Alignement::Gauche => 0.0,
            Alignement::Centre => -0.5 * l,
            Alignement::Droite => -l,
        } * PT_PAR_MM;
        let (s, c) = angle_deg.to_radians().sin_cos();
        let cmd = format!(
            "BT {} {} {} rg /{} {} Tf {} {} {} {} {} {} Tm {} 0 Td ",
            n(couleur[0]),
            n(couleur[1]),
            n(couleur[2]),
            police.nom_ressource(),
            n(taille_pt),
            n(c),
            n(s),
            n(-s),
            n(c),
            n(self.x(x)),
            n(self.y(y)),
            n(decalage)
        );
        self.ecrire(&cmd);
        let t = chaine_pdf(texte);
        self.contenu.extend_from_slice(&t);
        self.ecrire(" Tj ET\n");
    }

    fn style_trait(&mut self, epaisseur_mm: f64, couleur: Couleur, tirets: Option<(f64, f64)>) {
        let t = match tirets {
            Some((a, b)) => format!("[{} {}] 0 d", n(a * PT_PAR_MM), n(b * PT_PAR_MM)),
            None => "[] 0 d".to_string(),
        };
        let cmd = format!("{} w {} {} {} RG {} 1 J 1 j\n", n(epaisseur_mm * PT_PAR_MM), n(couleur[0]), n(couleur[1]), n(couleur[2]), t);
        self.ecrire(&cmd);
    }

    fn chemin(&mut self, points: &[(f64, f64)]) {
        let mut s = String::new();
        for (k, &(x, y)) in points.iter().enumerate() {
            s.push_str(&format!("{} {} {} ", n(self.x(x)), n(self.y(y)), if k == 0 { "m" } else { "l" }));
        }
        self.ecrire(&s);
    }

    /// Ligne brisée ouverte
    pub fn polyligne(&mut self, points: &[(f64, f64)], epaisseur_mm: f64, couleur: Couleur) {
        self.polyligne_tirets(points, epaisseur_mm, couleur, None);
    }

    /// Ligne brisée, en tirets (longueur du trait, de l'espace en mm) si demandé
    pub fn polyligne_tirets(&mut self, points: &[(f64, f64)], epaisseur_mm: f64, couleur: Couleur, tirets: Option<(f64, f64)>) {
        if points.len() < 2 {
            return;
        }
        self.style_trait(epaisseur_mm, couleur, tirets);
        self.chemin(points);
        self.ecrire("S\n");
    }

    pub fn ligne(&mut self, a: (f64, f64), b: (f64, f64), epaisseur_mm: f64, couleur: Couleur) {
        self.polyligne(&[a, b], epaisseur_mm, couleur);
    }

    /// Polygone plein, avec un contour si `contour` est donné
    pub fn polygone(&mut self, points: &[(f64, f64)], remplissage: Couleur, contour: Option<(f64, Couleur)>) {
        if points.len() < 3 {
            return;
        }
        if let Some((e, c)) = contour {
            self.style_trait(e, c, None);
        }
        let cmd = format!("{} {} {} rg\n", n(remplissage[0]), n(remplissage[1]), n(remplissage[2]));
        self.ecrire(&cmd);
        self.chemin(points);
        self.ecrire(if contour.is_some() { "h B\n" } else { "h f\n" });
    }

    /// Rectangle (coin haut gauche, largeur, hauteur)
    pub fn rectangle(&mut self, x: f64, y: f64, l: f64, h: f64, epaisseur_mm: f64, couleur: Couleur) {
        self.polyligne(&[(x, y), (x + l, y), (x + l, y + h), (x, y + h), (x, y)], epaisseur_mm, couleur);
    }

    pub fn rectangle_plein(&mut self, x: f64, y: f64, l: f64, h: f64, remplissage: Couleur) {
        self.polygone(&[(x, y), (x + l, y), (x + l, y + h), (x, y + h)], remplissage, None);
    }

    /// Limite le dessin qui suit au rectangle donné (jusqu'à `fin_decoupe`)
    pub fn decoupe(&mut self, x: f64, y: f64, l: f64, h: f64) {
        self.ecrire("q\n");
        let cmd = format!("{} {} {} {} re W n\n", n(self.x(x)), n(self.y(y + h)), n(l * PT_PAR_MM), n(h * PT_PAR_MM));
        self.ecrire(&cmd);
    }

    pub fn fin_decoupe(&mut self) {
        self.ecrire("Q\n");
    }

    /// Flèche de cote : trait de a à b terminé par une pointe à chaque bout
    pub fn cote(&mut self, a: (f64, f64), b: (f64, f64), epaisseur_mm: f64, couleur: Couleur) {
        self.ligne(a, b, epaisseur_mm, couleur);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-9 {
            return;
        }
        let (ux, uy) = (dx / l, dy / l);
        let (lp, lg) = (2.2_f64.min(0.4 * l), 0.7);
        for (p, s) in [(a, 1.0), (b, -1.0)] {
            let base = (p.0 + s * ux * lp, p.1 + s * uy * lp);
            self.polygone(&[p, (base.0 - uy * lg, base.1 + ux * lg), (base.0 + uy * lg, base.1 - ux * lg)], couleur, None);
        }
    }
}

/// Document PDF : suite de pages
#[derive(Debug, Default)]
pub struct Document {
    pages: Vec<Page>,
    titre: String,
}

impl Document {
    pub fn nouveau(titre: impl Into<String>) -> Self {
        Self { pages: Vec::new(), titre: titre.into() }
    }

    /// Ajoute une page (largeur, hauteur en mm) et la retourne pour y dessiner
    pub fn nouvelle_page(&mut self, (largeur_mm, hauteur_mm): (f64, f64)) -> &mut Page {
        self.pages.push(Page { contenu: Vec::new(), largeur_mm, hauteur_mm });
        self.pages.last_mut().unwrap()
    }

    pub fn pages_mut(&mut self) -> &mut [Page] {
        &mut self.pages
    }

    pub fn nb_pages(&self) -> usize {
        self.pages.len()
    }

    /// Fichier PDF complet
    pub fn octets(&self) -> Vec<u8> {
        let mut sortie: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut positions: Vec<usize> = Vec::new();
        let objet = |sortie: &mut Vec<u8>, positions: &mut Vec<usize>, corps: &[u8]| {
            positions.push(sortie.len());
            sortie.extend_from_slice(format!("{} 0 obj\n", positions.len()).as_bytes());
            sortie.extend_from_slice(corps);
            sortie.extend_from_slice(b"\nendobj\n");
        };
        // 1 catalogue, 2 pages, 3 à 6 polices, 7 infos, puis (page, contenu) par page
        let nb = self.pages.len();
        let kids: Vec<String> = (0..nb).map(|k| format!("{} 0 R", 8 + 2 * k)).collect();
        objet(&mut sortie, &mut positions, b"<< /Type /Catalog /Pages 2 0 R >>");
        objet(&mut sortie, &mut positions, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), nb).as_bytes());
        for base in ["Helvetica", "Helvetica-Bold", "Courier", "Courier-Bold"] {
            objet(&mut sortie, &mut positions, format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>").as_bytes());
        }
        let mut infos = b"<< /Producer (FreeOptical) /Title ".to_vec();
        infos.extend_from_slice(&chaine_pdf(&self.titre));
        infos.extend_from_slice(b" >>");
        objet(&mut sortie, &mut positions, &infos);
        for (k, p) in self.pages.iter().enumerate() {
            let page = format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R /F4 6 0 R >> >> /Contents {} 0 R >>",
                n(p.largeur_mm * PT_PAR_MM),
                n(p.hauteur_mm * PT_PAR_MM),
                9 + 2 * k
            );
            objet(&mut sortie, &mut positions, page.as_bytes());
            let mut flux = format!("<< /Length {} >>\nstream\n", p.contenu.len()).into_bytes();
            flux.extend_from_slice(&p.contenu);
            flux.extend_from_slice(b"\nendstream");
            objet(&mut sortie, &mut positions, &flux);
        }
        let debut_xref = sortie.len();
        sortie.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", positions.len() + 1).as_bytes());
        for p in &positions {
            sortie.extend_from_slice(format!("{:010} 00000 n \n", p).as_bytes());
        }
        sortie.extend_from_slice(
            format!("trailer\n<< /Size {} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{}\n%%EOF\n", positions.len() + 1, debut_xref).as_bytes(),
        );
        sortie
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_du_fichier() {
        let mut d = Document::nouveau("Essai (titre)");
        let p = d.nouvelle_page(A4);
        p.texte(20.0, 20.0, "Lentille à ménisque — Ø 50 mm, λ = 587 nm, R = ∞", Police::Helvetica, 11.0, Alignement::Gauche, NOIR);
        p.cote((20.0, 40.0), (80.0, 40.0), 0.2, NOIR);
        p.polygone(&[(10.0, 10.0), (20.0, 10.0), (15.0, 20.0)], [0.8, 0.9, 1.0], Some((0.3, NOIR)));
        d.nouvelle_page(A4);
        let o = d.octets();
        let texte = String::from_utf8_lossy(&o);
        assert!(texte.starts_with("%PDF-1.4"));
        assert!(texte.contains("/Count 2"));
        // Table des renvois : chaque position pointe bien sur « n 0 obj »
        let debut: usize = texte.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        let xref = &texte[debut..];
        for (k, ligne) in xref.lines().skip(3).take_while(|l| l.ends_with(" n ")).enumerate() {
            let pos: usize = ligne[..10].parse().unwrap();
            assert!(texte[pos..].starts_with(&format!("{} 0 obj", k + 1)), "objet {} mal placé", k + 1);
        }
        // Accents en WinAnsi, signes absents remplacés, parenthèses protégées
        assert!(o.windows(4).any(|w| w == b"\xe0 m\xe9"), "à ménisque en cp1252");
        assert!(texte.contains("lambda = 587 nm") && texte.contains("R = inf."));
        assert!(texte.contains("(Essai \\(titre\\))"));
    }

    #[test]
    fn largeurs() {
        // Courier : 0,6 × taille par caractère ; Helvetica « i » plus étroit que « m »
        assert!((largeur_texte("abcd", Police::Courier, 10.0) - 4.0 * 6.0 / PT_PAR_MM).abs() < 1e-9);
        assert!(largeur_texte("iiii", Police::Helvetica, 10.0) < largeur_texte("mmmm", Police::Helvetica, 10.0));
        assert_eq!(largeur_texte("é", Police::Helvetica, 10.0), largeur_texte("e", Police::Helvetica, 10.0));
    }
}
