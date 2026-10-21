# Export and audit {#exporter}

## Why this stage {#exporter-pourquoi}

A qualitative analysis that cannot be reviewed has to be taken on trust. Coding Studio keeps everything in a single database and produces exports from which your tables and appendices are drawn, so that no figure in the report is typed in by hand. The audit log records every operation in order, and each entry is linked to the previous one by a fingerprint: an entry cannot be deleted or changed without breaking the chain. This is what makes it possible to prove that your coding came before the model's. Braun and Clarke (2021) ask for a transparent report of the choices made; the export package is the concrete form of that. Export along the way to keep a backup, then one last time before submission.

## What you do {#exporter-actions}

### 1. Export an interview {#exporter-entretien}

The export of an interview gathers everything about it in a dated folder.

**In Coding Studio.** In "Settings" (Réglages), "Exports and checks" (Exports et vérification) block, click "Export an interview" (Exporter un entretien) and choose the interview. The tool writes a time-stamped folder (for example `exports/2026-10-13_BIB-01`) and never overwrites an earlier export. The screen then shows the number of files and confirms that the fingerprints are verified. The folder contains in particular the list of units, the human coding, the coding of each AI review (if one took place) and its comparison with the text sent and the raw response, the decisions, the codebook, the quotations with their verification status, the audit log and the manifest.

### 2. Export the cross-case package {#exporter-transversal}

The cross-case export covers the whole corpus.

**In Coding Studio.** In the same block, click "Export the cross-case package" (Exporter le transversal). The package contains the answers file (the text from the "Write" (Rédiger) stage), the matrix, the interview coverage table, the list of cases, a manifest and a README file that describes the content. You do not have to build a saturation table yourself: it is a file produced by the export from the codes applied.

For a Word document gathering the findings and their evidence, also use [Analysis report (Word) in Analyse](04_analyse.md#analyser-rapport).

### 3. Read the manifest {#exporter-manifeste}

The manifest is the verifiable table of contents of the package.

**In Coding Studio.** Each package includes a manifest that lists every file with its size and its SHA-256 fingerprint, a number computed from the content that changes if the file changes by a single character. To check an archived package, recompute the fingerprint of a file with your system tool (sha256sum, Get-FileHash) and compare it with the manifest. Keep the database and the exports together.

### 4. Check the audit log chain {#exporter-journal}

Verification confirms that the log has not been altered.

**In Coding Studio.** In "Settings", "Exports and checks" block, click "Audit log" (Journal d'audit) to browse the operations: date, author, type of action, old and new value, reason. Click "Check chain" (Vérifier la chaîne): the tool recalculates the fingerprints from end to end and shows the result. A valid chain does not prove that your choices are right, but that the order and content of the recorded operations have not been changed after the fact. The same views, with the saturation table and the cross-case matrix, remain available in this block; they are outputs, not stages.

### 5. Replay the final audit {#exporter-audit-final}

The final audit of three statements (the "Final audit" tab in "Write", see [Write](05_write.md#rediger-audit)) is the submission check.

**In Coding Studio.** After the last export, draw again in the "Final audit" tab of "Write". For each statement, the result ("supported", "to qualify" or "to remove") goes into the export: attach it to your submission folder. If a trail is broken, correct it, export again and repeat the check.

### 6. Choose what goes in the appendices {#exporter-annexes}

Not everything needs to go in the appendices: choose what lets the reader judge rigour without drowning in detail.

**In Coding Studio.** From the packages, as a general rule take:

- the list of codes, with definition, inclusions, exclusions and the number of passages and interviews (codebook file);
- the table of codes by research question;
- the verification coverage: the share of units verified against the audio, next to each count;
- the distributions of evidence types, origins and roles of the quotations;
- the theme records and their decision memos;
- the documented decisions;
- the answers file, which is the final text;
- if you ran AI reviews, the declaration of the interviews concerned, the provider and the place of execution, the prompt and the raw response.

The complete audit log and copies of the database are kept in your archive, together with the original transcript. Do not publish any file that has not been pseudonymised without a manual check of the free-text content (memos, notes), which the pseudonymisation list does not cover.

## Common pitfalls {#exporter-pieges}

- Moving or renaming a file in a package: its fingerprint no longer matches the manifest.
- Declaring automatic saturation. A table is not proof of sufficiency: state which signals justify that the data are sufficient for your questions, and which areas remain less well covered.
- Forgetting that free text (memos, notes) can contain names.

## What this stage delivers {#exporter-sortie}

- One package per interview and one cross-case package, each with its manifest and fingerprints.
- An audit log whose chain has been verified.
- The text of the findings, the tables and the appendices drawn directly from the exports.
- The result of the final audit of three statements.
- A coherent archive of the database, the sources and the exports, ready to submit.


## Final reviewer export

Open **Final reviewer export** from Analyse or the Library export menu. This creates a local, read-only review package. It does not publish anything.

1. **Content.** Expand sections down to each field. A folder checkbox selects its fields; deselect a leaf to create an exception. Click a field to preview it. Complete text is exported even when the preview is limited. Original files, people records, interview metadata and AI passes start unchecked. Select the source file for an unsegmented transcript. References never reintroduce the content of excluded objects.
2. **Access.** Choose and retain a unique passphrase of at least 16 characters. It is not saved in the export and cannot be recovered. Send it separately. The HTTPS address is optional: leave it blank until hosting is defined. No public URL or QR will be created. With a real address, URL.txt and QR.svg represent that address; nothing is uploaded automatically.
3. **Review.** Check the selection, volume and pseudonymisation options, then create the protected export. upload.zip contains only the reader and encrypted data, never the working database or password. Host its complete contents on a trusted HTTPS server, then test the real link.

The reader searches the included fields. It opens with the password in a recent browser supporting WebCrypto. For local attachments, select the data subfolder in the reader. Hosted attachments load from the same origin. Files are encrypted in chunks; files over 1 GiB are rejected explicitly without truncation. Downloads use neutral names; original names are separate selectable fields.

Pseudonymisation replaces only configured forms of names in the copy. Zero replacements does not mean anonymous. Original files are not pseudonymised and may reveal voices or identities. The password protects encrypted content but provides neither account management nor revocation of downloaded copies. Project changes after opening the catalogue require refreshing the selection. Changed or missing sources cause generation to fail without leaving an incomplete final package.

## Notes style

Use brief notes, with bullets when useful: main idea, useful passage, contrast, open question, limitation. Interview notes, theme sheets and writing areas serve as the researcher’s working notes. A code definition needs one simple sentence. A processing status or note does not replace checking a quotation against its recording.

The participant review Word export uses English headings and instructions. Final
codes appear directly after each turn, with definitions in an appendix. Original
source wording is preserved and an approved translation is included separately
when available. Export does not automatically translate quotations or researcher
notes.
