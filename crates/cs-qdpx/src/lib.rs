//! REFI-QDA interoperability for Coding Studio (SPEC F-103).
//!
//! Export of a project as a REFI-QDA codebook (`.qdc`) or project (`.qdpx`),
//! import of a codebook and of the codes and text codings of a project, so
//! that work can move between Coding Studio and NVivo, MAXQDA or ATLAS.ti.
//! The mapping rules are in [`export`] and [`import`], the GUID scheme in
//! [`guid`], the plain text layout in [`layout`], the XML subset in [`refi`].

pub mod export;
pub mod guid;
pub mod import;
pub mod issue;
pub mod layout;
pub mod refi;

pub use export::{export_qdc, export_qdpx, Export, ExportOptions, ExportSummary};
pub use import::{import_qdc, import_qdpx, ImportReport, Mode};
pub use issue::{Error, Issue, Result, Severity};
