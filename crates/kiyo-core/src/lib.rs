//! `kiyo-core`: Zero-copy PDF document model, cross-reference streaming, and manipulation.

use anyhow::Result;
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

/// PDF document backed by zero-copy virtual memory mapping.
pub struct Document {
    pub file_size: u64,
    pub mmap: Arc<Mmap>,
}

impl Document {
    /// Opens and memory-maps a PDF document from disk with kernel madvise optimization.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let file_size = file.metadata()?.len();
        let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };

        #[cfg(target_os = "linux")]
        unsafe {
            libc::madvise(
                mmap.as_ptr() as *mut libc::c_void,
                mmap.len(),
                libc::MADV_WILLNEED | libc::MADV_RANDOM,
            );
        }

        Ok(Self {
            file_size,
            mmap: Arc::new(mmap),
        })
    }

    /// Accesses the underlying memory-mapped bytes as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.mmap
    }

    /// Inspects and parses document object catalog and xref table using lopdf.
    pub fn load_lopdf(&self) -> Result<lopdf::Document> {
        let doc = lopdf::Document::load_mem(self.as_slice())?;
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;
    use std::io::Write;

    #[test]
    fn test_document_open_mmap_and_lopdf_parsing() {
        let temp_dir = std::env::temp_dir();
        let test_path = temp_dir.join(format!("kiyo_test_{}.pdf", std::process::id()));

        // Create a minimal valid PDF 1.4 document
        let mut pdf_doc = lopdf::Document::with_version("1.4");
        let pages_id = pdf_doc.new_object_id();
        let font_id = pdf_doc.add_object(lopdf::dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = pdf_doc.add_object(lopdf::dictionary! {
            "Font" => lopdf::dictionary! {
                "F1" => font_id,
            },
        });
        let content_id = pdf_doc.add_object(lopdf::Stream::new(
            lopdf::dictionary! {},
            b"BT /F1 12 Tf 72 712 Td (Hello Kiyo) Tj ET".to_vec(),
        ));
        let page_id = pdf_doc.add_object(lopdf::dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });
        let pages = lopdf::dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        };
        pdf_doc.objects.insert(pages_id, lopdf::Object::Dictionary(pages));
        let catalog_id = pdf_doc.add_object(lopdf::dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        pdf_doc.trailer.set("Root", catalog_id);

        let mut file = File::create(&test_path).expect("Create test PDF file");
        pdf_doc.save_to(&mut file).expect("Write test PDF file");
        file.flush().expect("Flush file");
        drop(file);

        // Open with Document::open (mmap2 + madvise)
        let doc = Document::open(&test_path).expect("Open mmap document");
        assert!(doc.file_size > 0);
        assert_eq!(doc.as_slice().len() as u64, doc.file_size);
        assert!(doc.as_slice().starts_with(b"%PDF-"), "Header: {:?}", String::from_utf8_lossy(&doc.as_slice()[..15]));

        // Parse with load_lopdf
        let parsed = doc.load_lopdf().expect("Parse via lopdf");
        assert!(parsed.version.starts_with("1."));
        let page_count = parsed.get_pages().len();
        assert_eq!(page_count, 1);

        // Cleanup
        let _ = std::fs::remove_file(test_path);
    }
}
