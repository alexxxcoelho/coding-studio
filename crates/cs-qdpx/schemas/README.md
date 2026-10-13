# REFI-QDA schemas

The official schemas, `Codebook.xsd` and `Project-mrt2019.xsd`, are published by the REFI-QDA working group at <https://www.qdasoftware.org> (pages "Codebook implementation files" and "Project implementation files"). They are not committed here: the site serves them through a content manager whose file links could not be retrieved from the build host when `cs-qdpx` was written (13.10.2026), and no copy was found elsewhere on it.

`src/refi.rs` therefore implements the subset below from the published structure of the standard, and the tests check element names, namespaces, required attributes and the nesting of what the exporter writes. Full validation against the XSD is still to do: place the two files in this folder and validate the output of `export_qdc` and `export_qdpx` (for example with `xmllint --noout --schema`).

Subset written:

- Codebook: `CodeBook@origin` (namespace `urn:QDA-XML:codebook:1.0`) / `Codes` / `Code@guid,name,isCodable` / `Description`, nested `Code`.
- Project: `Project@name,origin` (namespace `urn:QDA-XML:project:1.0`) / `Users/User@guid,name`, `CodeBook` (as above), `Sources/TextSource@guid,name,plainTextPath` / `PlainTextSelection@guid,startPosition,endPosition,creatingUser,creationDateTime` / `Coding@guid` / `CodeRef@targetGUID`.

Read, in addition: `PlainTextContent` (inline text), any element order, any namespace (with a warning), and other source kinds (listed and ignored).
