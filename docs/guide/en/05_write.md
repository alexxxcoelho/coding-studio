# Write the findings {#rediger}

## Why this stage {#rediger-pourquoi}

A thematic analysis is judged on the text that presents it. Braun and Clarke (2006, 2021) ask for a report that tells a coherent story about the data, not a list of categories. The "Write" (Rédiger) screen therefore organises the work around the retained themes. For each theme, you write five short parts supported by precise passages; the answers to the research questions come afterwards, as a synthesis of the themes. Each passage is linked to the part it supports by a typed link and a one-line note: the old "for or against" disappears, because a passage can qualify, condition or contradict. If you have chosen an interpretive framework (Gale et al., 2013, for the logic of the matrix), it serves here to read the findings. No figure is typed in by hand.

## What you do {#rediger-actions}

### 1. Open the "By theme" (Par thème) tab {#rediger-theme}

Write starts from the themes you built and retained in the Analyse stage. It appears in the main navigation, like "Analyse" (Analyser).

**In Coding Studio.** Click "Write" in the top bar. The screen has five tabs: "By theme", "Synthesis by question" (Synthèse par question), "Reading through the framework" (Lecture par le cadre; only if the study has a framework), "Final audit" (Audit final) and "Documented decisions" (Décisions documentées). In "By theme", the left column lists the retained themes (only themes with the status "Retained" appear), then the passages of the selected theme: interview, unit number, extract and cell value (present, contradicted...). The order of themes in the list becomes the order of the chapter of findings. Clicking a passage opens it in its interview, Detail drawer included (see [Code](02_code.md#coder-detail)).

### 2. Write the five parts of the theme {#rediger-parties}

The five parts give the reader a fixed path, from the statement to the limits.

**In Coding Studio.** In the centre, the chosen theme is shown with its questions (for example Q1, Q3) and five text areas:

1. Analytical statement: one sentence that says what the data suggest, taken from the theme record.
2. Mechanism and evidence: who does what, and why, with brief and faithful extracts.
3. Variations and divergent cases.
4. Competing explanations and limits: taken from the alternative interpretation in the record.
5. Mini-synthesis: the partial answer the theme gives to its questions, reused in the "Synthesis by question" tab.

Below part 2, a "Nature of evidence" (Nature de la preuve) menu for the part indicates whether it rests on lived experiences across several interviews, a project or intention, an opinion, a document, or a single interview. No frequency is calculated. The text can be exported in Markdown, with the quotations as notes. Fictional example: "Several respondents describe the absence of need without regret [BIB-01 U018 supports]; paper loans are enough for them [BIB-04 U061 supports]."

### 3. Link a passage to a part {#rediger-liens}

The link says what the passage contributes, not merely that it exists.

**In Coding Studio.** Drag a passage from the left column into the target part: a link is created and appears in the text as a pill ("BIB-01 U018 supports"). The link type is chosen from the study list: "supports" (soutient), "qualifies" (nuance), "conditions" (conditionne), "mechanism" (mécanisme), "exception" (exception), "contradicts" (contredit). The default is "supports". Each link also carries a one-line note on what the passage contributes. Example: "conditions: 'if the network requires it'". A passage can be linked to several parts or themes. There is no longer a binary for or against.

### 4. Write the synthesis by research question {#rediger-question}

The answers come from the themes, not from frequencies.

**In Coding Studio.** Open the "Synthesis by question" tab. One page per research question (the questions come from "Settings" (Réglages)) brings together the themes that answer it, with their mini-syntheses, then the answer in a few paragraphs and the limits. Click "Save" (Enregistrer). The text is dated and its earlier versions are kept.

### 5. Read the themes with the framework {#rediger-tableau}

This tab exists only if the study has an interpretive framework.

**In Coding Studio.** Open the "Reading through the framework" tab. The table crosses the retained themes with the framework dimensions: what the framework explains, what it does not explain, and what remains outside it. To complete the analysis, add a complementary dimension (one the framework does not contain, for example who pays) and a limit of evidence for each theme. The table is exported with the rest.

### 6. Run the final audit of three statements {#rediger-audit}

Before submission, check that the statements are not unsupported.

**In Coding Studio.** Open the "Final audit" tab. The tool draws three statements from your text at random. For each one, trace back to the linked passages, the exact quotation and the audio verification status, then tick: "supported" (étayée), "to qualify" (à nuancer) or "to remove" (à retirer). You can draw again. The result goes into the export (see [Export](06_export_and_audit.md#exporter-audit-final)). If a link is missing (no link, unverified passage), correct it before concluding.

### 7. Attach documented decisions {#rediger-decisions}

**In Coding Studio.** The "Documented decisions" tab shows the decisions created in Analyse (see [Analyse](04_analyse.md#analyser-decisions)). Opening one views it; "New decision" (Nouvelle décision) creates one. They serve as illustrations, not as the backbone.

## Common pitfalls {#rediger-pieges}

- Replacing analysis with quotations. An extract explains nothing until you have said what it contributes.
- Saying "all", "never" or "the majority" without counts that justify it.
- Linking every passage as "supports". Look also for "qualifies", "exception" and "contradicts": these are what give a theme its value.
- Citing a passage not verified against the audio, or not saying what share of the quotations is verified.

## What this stage delivers {#rediger-sortie}

- For each retained theme, five dated parts of text, with their linked passages (link type and one-line note).
- Quotations verified against the audio, qualified by nature, origin and role, with their flags where relevant.
- An answer to each research question, with competing explanations and limits.
- If you have a framework, a reading table by theme.
- A final audit of three statements drawn at random, with its full trail.

### Filter passages and edit the write-up in JSON

Under **Theme passages**, combine text search with interview, code, citation use,
verification and matrix-value filters. Citation use distinguishes unused passages,
used passages and a specific writing part. The counter shows visible results
against the full list. Reset restores the complete list; filtering does not edit
passages or links, and drag-and-drop and keyboard linking remain available.

**Export writing (JSON)** includes theme parts and order, citation links, question
syntheses, framework readings, documented decisions and verdicts for existing
audits. Retain identifiers and the read-only `references` section, including source
quotations. Edit transcripts and theme definitions in their dedicated tools.

Choose **Import writing (JSON)**, paste or open the file, then validate and preview.
Review the changes and confirm replacement before saving. Omitted entries stay
unchanged. Empty link arrays remove a part's links; empty text clears its text.
Earlier versions remain available. Invalid references or a changed database reject
the entire batch. Changed verdicts must match current evidence; imports do not
recreate historical audit draws.
