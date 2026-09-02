#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileSystemEntryType {
    Directory,
    File,
}

/// Marker shown before an entry's name in a listing.
///
/// Geometric Shapes, not the folder and document pictographs this used to use. Those
/// (U+1F5C0, U+1F5CB) are absent from every common monospace font, so each was borrowed
/// from a different fallback -- Adwaita Mono and Font Awesome here -- which is why they
/// did not match. Worse, both declare East Asian Width Neutral, so a terminal reserves
/// one cell while the substitute paints about two, and the column drifts.
///
/// U+25A0 and U+25A1 are in JetBrains Mono, Consolas, Cascadia Mono and DejaVu Sans Mono
/// directly. One cell, no fallback, and legible in a bare Windows console.
impl std::fmt::Debug for FileSystemEntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemEntryType::Directory => write!(f, "■"),
            FileSystemEntryType::File => write!(f, "□"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_markers_are_distinguishable() {
        let directory = format!("{:?}", FileSystemEntryType::Directory);
        let file = format!("{:?}", FileSystemEntryType::File);
        assert_ne!(directory, file, "a listing has to tell them apart");
    }

    /// The bug being fixed: mismatched markers pushed every following column out of line.
    #[test]
    fn the_two_markers_occupy_the_same_width() {
        let directory = format!("{:?}", FileSystemEntryType::Directory);
        let file = format!("{:?}", FileSystemEntryType::File);

        assert_eq!(directory.chars().count(), 1, "one cell, so columns stay put");
        assert_eq!(file.chars().count(), 1);

        for marker in [&directory, &file] {
            let c = marker.chars().next().unwrap() as u32;
            // Geometric Shapes. Outside it lies the pictograph range that forced a
            // font fallback and, with it, the mismatch.
            assert!(
                (0x25A0..=0x25FF).contains(&c),
                "U+{:04X} is outside Geometric Shapes and may not be in a monospace font",
                c
            );
        }
    }
}
