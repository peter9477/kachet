//! Minimal embedded-typst compiler: one in-memory source, embedded
//! fonts, no filesystem or package access. Used for report PDF export.

use std::sync::OnceLock;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn fonts() -> &'static Fonts {
    static FONTS: OnceLock<Fonts> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut book = FontBook::new();
        let mut fonts = Vec::new();
        for data in typst_assets::fonts() {
            let bytes = Bytes::new(data);
            for font in Font::iter(bytes) {
                book.push(font.info().clone());
                fonts.push(font);
            }
        }
        Fonts { book: LazyHash::new(book), fonts }
    })
}

struct PdfWorld {
    library: LazyHash<Library>,
    source: Source,
}

impl World for PdfWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &fonts().book
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().get_without_slash().into()))
    }
    fn font(&self, index: usize) -> Option<Font> {
        fonts().fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Compile typst markup to a PDF. Returns a human-readable error string
/// with compiler diagnostics on failure.
pub(crate) fn compile(source: String) -> Result<Vec<u8>, String> {
    let vpath = VirtualPath::new("report.typ").map_err(|e| e.to_string())?;
    let id = FileId::new(RootedPath::new(VirtualRoot::Project, vpath));
    let world = PdfWorld {
        library: LazyHash::new(Library::default()),
        source: Source::new(id, source),
    };
    let doc = typst::compile(&world)
        .output
        .map_err(|errs| {
            errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; ")
        })?;
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default())
        .map_err(|errs| errs.iter().map(|e| e.message.to_string()).collect::<Vec<_>>().join("; "))
}
