//! Prepress sheet imposition, booklet saddle-stitching, and mechanical creep mathematics.

/// A physical sheet signature in a saddle-stitch booklet.
#[derive(Debug, Clone, PartialEq)]
pub struct BookletSignature {
    /// 0-indexed sheet layer (0 is outermost cover, S-1 is innermost centerfold)
    pub sheet_index: usize,
    /// Total sheets in the signature booklet
    pub total_sheets: usize,
    /// 1-based page index for Front Face, Left side (0 = blank padding)
    pub front_left: usize,
    /// 1-based page index for Front Face, Right side (0 = blank padding)
    pub front_right: usize,
    /// 1-based page index for Back Face, Left side (0 = blank padding)
    pub back_left: usize,
    /// 1-based page index for Back Face, Right side (0 = blank padding)
    pub back_right: usize,
    /// Inward mechanical spine shift in PDF points to compensate for paper creep
    pub creep_pt: f32,
}

/// Calculate mechanical creep displacement for uniform paper stock:
/// $$\Delta_{\text{creep}}(k) = k \cdot t$$
#[inline]
pub fn calculate_booklet_creep(sheet_index: usize, paper_thickness_pt: f32) -> f32 {
    sheet_index as f32 * paper_thickness_pt
}

/// Calculate mechanical creep displacement for hybrid stock (heavy cover + light body):
/// $$\Delta_{\text{creep}}(k) = \begin{cases} 0 & k = 0 \\ t_{\text{cover}} + (k - 1) \cdot t_{\text{body}} & k \ge 1 \end{cases}$$
#[inline]
pub fn calculate_hybrid_booklet_creep(
    sheet_index: usize,
    cover_thickness_pt: f32,
    body_thickness_pt: f32,
) -> f32 {
    if sheet_index == 0 {
        0.0
    } else {
        cover_thickness_pt + (sheet_index - 1) as f32 * body_thickness_pt
    }
}

/// Computes saddle-stitch booklet imposition layout for a given page count and paper thickness.
///
/// Automatically pads page count to the nearest multiple of 4 ($M = 4 \lceil N / 4 \rceil$).
pub fn calculate_booklet_signatures(
    page_count: usize,
    paper_thickness_pt: f32,
) -> Vec<BookletSignature> {
    if page_count == 0 {
        return Vec::new();
    }

    // Pad to nearest multiple of 4
    let padded_pages = page_count.div_ceil(4) * 4;
    let total_sheets = padded_pages / 4;
    let mut signatures = Vec::with_capacity(total_sheets);

    for s in 0..total_sheets {
        let fl = padded_pages - 2 * s;
        let fr = 2 * s + 1;
        let bl = 2 * s + 2;
        let br = padded_pages - 2 * s - 1;

        signatures.push(BookletSignature {
            sheet_index: s,
            total_sheets,
            front_left: if fl <= page_count { fl } else { 0 },
            front_right: if fr <= page_count { fr } else { 0 },
            back_left: if bl <= page_count { bl } else { 0 },
            back_right: if br <= page_count { br } else { 0 },
            creep_pt: calculate_booklet_creep(s, paper_thickness_pt),
        });
    }

    signatures
}

/// N-up grid configuration describing rows and columns per parent sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NupGrid {
    pub cols: u8,
    pub rows: u8,
}

impl NupGrid {
    /// Determines optimal rows and columns for standard N-up configurations (1, 2, 4, 8, 16).
    pub fn for_nup(nup: u8) -> Self {
        match nup {
            1 => Self { cols: 1, rows: 1 },
            2 => Self { cols: 2, rows: 1 },
            4 => Self { cols: 2, rows: 2 },
            8 => Self { cols: 4, rows: 2 },
            16 => Self { cols: 4, rows: 4 },
            _ => {
                // Approximate square layout for non-standard n-up counts
                let cols = (nup as f32).sqrt().ceil() as u8;
                let rows = nup.div_ceil(cols);
                Self { cols, rows }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_booklet_imposition_16_pages() {
        let sigs = calculate_booklet_signatures(16, 0.2835);
        assert_eq!(sigs.len(), 4);

        // Sheet 0 (Cover sheet): Front: 16 (Back cover), 1 (Front cover); Back: 2, 15
        assert_eq!(sigs[0].front_left, 16);
        assert_eq!(sigs[0].front_right, 1);
        assert_eq!(sigs[0].back_left, 2);
        assert_eq!(sigs[0].back_right, 15);
        assert_eq!(sigs[0].creep_pt, 0.0);

        // Sheet 3 (Centerfold): Front: 10, 7; Back: 8, 9
        assert_eq!(sigs[3].front_left, 10);
        assert_eq!(sigs[3].front_right, 7);
        assert_eq!(sigs[3].back_left, 8);
        assert_eq!(sigs[3].back_right, 9);
        assert!((sigs[3].creep_pt - 3.0 * 0.2835).abs() < 1e-5);
    }

    #[test]
    fn test_booklet_imposition_odd_pages_padding() {
        // 5 pages must pad to 8 (2 sheets)
        let sigs = calculate_booklet_signatures(5, 0.2835);
        assert_eq!(sigs.len(), 2);

        // Sheet 0: Front: 0 (padded page 8), 1; Back: 2, 0 (padded page 7)
        assert_eq!(sigs[0].front_left, 0); // 8 is padded
        assert_eq!(sigs[0].front_right, 1);
        assert_eq!(sigs[0].back_left, 2);
        assert_eq!(sigs[0].back_right, 0); // 7 is padded

        // Sheet 1: Front: 0 (padded page 6), 3; Back: 4, 5
        assert_eq!(sigs[1].front_left, 0); // 6 is padded
        assert_eq!(sigs[1].front_right, 3);
        assert_eq!(sigs[1].back_left, 4);
        assert_eq!(sigs[1].back_right, 5);
    }

    #[test]
    fn test_mechanical_creep_uniform_and_hybrid() {
        let t = 0.2835; // ~0.1mm
        assert_eq!(calculate_booklet_creep(0, t), 0.0);
        assert!((calculate_booklet_creep(2, t) - 2.0 * t).abs() < 1e-5);

        // Hybrid: 300gsm cover (0.85pt) + 80gsm body (0.2835pt)
        let t_cover = 0.85;
        assert_eq!(calculate_hybrid_booklet_creep(0, t_cover, t), 0.0);
        assert_eq!(calculate_hybrid_booklet_creep(1, t_cover, t), t_cover);
        assert!((calculate_hybrid_booklet_creep(2, t_cover, t) - (t_cover + t)).abs() < 1e-5);
    }

    #[test]
    fn test_nup_grid_determination() {
        assert_eq!(NupGrid::for_nup(1), NupGrid { cols: 1, rows: 1 });
        assert_eq!(NupGrid::for_nup(2), NupGrid { cols: 2, rows: 1 });
        assert_eq!(NupGrid::for_nup(4), NupGrid { cols: 2, rows: 2 });
        assert_eq!(NupGrid::for_nup(8), NupGrid { cols: 4, rows: 2 });
        assert_eq!(NupGrid::for_nup(16), NupGrid { cols: 4, rows: 4 });
    }
}
