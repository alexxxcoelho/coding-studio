# Prepare the study {#preparer}

## Why this stage {#preparer-pourquoi}

A qualitative analysis is defended through its traceability, and traceability is decided before the first code is attached. You state what you are looking for (the research questions), whom you interviewed (the interview attributes) and how you will read (the memo templates and the checklist). Braun and Clarke (2006, 2021) remind us that thematic analysis is not a neutral technique: the researcher's choices shape it, and those choices must be explicit. Noting from the start the status of each statement (a lived deployment, a project not yet carried out, expert opinion, indirect observation) prevents giving later the same weight to very different situations. This stage is done once per study.

## What you do {#preparer-actions}

### 1. Create the project {#preparer-projet}

A project corresponds to one database and one folder of transcripts. Everything you do afterwards (codes, memos, themes, log) is written to this database.

**In Coding Studio.** On the first launch of the desktop app, a window asks where to keep the database: choose a folder that you back up. From a terminal, you point to the database and the interview folder with launch options. To change study, open or create another database from the "Database" (Base de données) menu. Nothing leaves your machine unless you configure an AI provider.

### 2. Open the "Settings" (Réglages) {#preparer-reglages}

Settings is the screen where the study is defined. You visit it at the start, then rarely.

**In Coding Studio.** Click "Settings" in the top bar. The "Study" (Étude) block holds the study name, a one-paragraph description (read by the model if you run an AI review), the research questions, the coding rules, the maximum length of a unit (150 words by default), the interviewer names and the identifier prefixes. "Save" (Enregistrer) validates the block, and every change is dated in the log. The other blocks are "AI providers" (Fournisseurs IA, optional), "Exports and checks" (Exports et vérification), and "Study models and lists" (Modèles et listes de l'étude), which hold the attributes, the memo templates, the interpretive framework, the custom matrix dimensions and the editable lists. Example coding rule: "Code what the respondent asserts; several codes are possible; mark as not relevant if nothing useful."

### 3. Write the research questions {#preparer-questions}

Your questions guide the reading. They link codes to themes, and themes to the final answers.

**In Coding Studio.** In "Settings", "Study" block, field "Research questions" (Questions de recherche), enter one question per line, each with a short identifier (Q1, Q2, ...). The identifiers are reused in the theme record, in "Write" and in the exports. Fictional example: Q1, why does a small library change its loan system? Q2, what makes the change difficult? Q3, what role does the library network play? A reworded text stays on record in the log.

### 4. Define the interview attributes {#preparer-attributs}

An attribute is a characteristic of the interview that you will want to use later as a filter or as a matrix row. It is defined once for the project and filled in for each interview.

**In Coding Studio.** In "Settings", "Study models and lists" block, section "Interview attributes" (Attributs d'entretien), click "+ Attribute" (+ Attribut), give it a name and a type: text, value list, number or date. Examples: library type (list: municipal, school, association), respondent's role (text), number of volunteers (number), interview date (date), interview guide version (list: A, B), status of the statements (list: lived experience, project, expert opinion, indirect observation). Attributes become columns of the Library, filters, and rows or columns of the matrix. The status of the statements is strongly recommended. Values are filled in on each interview's record (action 10). The same section offers custom matrix dimensions ("+ Dimension"), for example "who decides, who pays, who uses".

### 5. Prepare the memo templates {#preparer-memos}

The familiarisation memo is written after a full reading of an interview and before any coding. Its questions depend on your study, so they are set in a template.

**In Coding Studio.** In "Settings", "Study models and lists" block, section "Familiarisation memo templates" (Modèles de mémo de familiarisation), click "+ Template" (+ Modèle), give it a name, then enter the questions. You can have several templates (for example "Default", with 4 questions, and "Expert interview", with 6 questions) and mark one as the default. On an interview's record you choose the template; on the "Coding" (Coder) screen you can add, remove or reword a question for that interview only, without changing the template.

A generic default template:

1. What does this respondent tell us, in a few lines?
2. What surprised me?
3. What contradictions or particularities can be noted?
4. What should we ask the following interviews?

A second example, for a study of another kind (patients' experience after leaving hospital):

1. What is the path described, from the first symptom to today?
2. Who helped, and who failed to?
3. What words does the respondent use to talk about their situation?
4. What point should be checked by ear before it is quoted?

### 6. Set the end-of-interview checklist {#preparer-checklist}

The checklist is a set of tick boxes that you go through at the end of coding an interview. It stops you forgetting a verification step.

**In Coding Studio.** In "Settings", "Study models and lists" block, section "Editable lists" (Listes éditables), the "End of interview" (Fin d'entretien) list has four points by default: all the respondent's turns seen; "Later" (Plus tard) codes handled or accepted; memos written for the new codes; end-of-interview memo written (the respondent's position, the central mechanism, exceptions, questions for the following interviews). Add, remove or reword items. It is shown in the "End of interview" panel of the "Coding" screen (see [Code](02_code.md#coder-checklist)). The other editable lists are the theme checks (7), the evidence types (5) and the link types (6).

### 7. Choose an interpretive framework, or none {#preparer-cadre}

An interpretive framework is a theoretical grid used to read your themes once they are built. It is optional. In the hybrid approach of Fereday and Muir-Cochrane (2006), coding stays open to the data and the framework helps with interpretation afterwards. The tool follows this logic: the framework appears only in the "Analyse" and "Write" stages, never in coding or in the codebook.

**In Coding Studio.** In "Settings", "Study models and lists" block, section "Interpretive framework (optional)" (Cadre d'interprétation (facultatif)), choose "None" (Aucun, the default), a preset (TOE: technology, organisation, environment; TAM/UTAUT) or "Custom" (Personnalisé), where you name your own dimensions. The tool always adds an "outside the framework" line for what the framework does not explain. With "None", the framework reading fields do not appear.

### 8. Declare the interviewers {#preparer-intervieweurs}

The interviewer's turns give context, but they are not coded. The tool therefore needs to know which turns they are.

**In Coding Studio.** In "Settings", "Study" block, field "Interviewer names" (Noms des intervieweurs), enter the names or labels as they appear in your transcripts (for example INT). The matching turns are shown in grey on the "Coding" screen and are never coding targets. A question may be supplied to the model as context for the respondent’s turn; it is not respondent evidence. Review the prompt and its pseudonymisation before any transmission.

### 9. Import the transcripts and read the "Library" (Bibliothèque) {#preparer-bibliotheque}

Import transcripts that are already cleaned up: speakers identified, time markers, names pseudonymised if your commitments require it.

**In Coding Studio.** Place each interview in a subfolder of the interview folder, named with the prefix set in the study (BIB-01, BIB-02), containing a cleaned transcript with one turn per paragraph, in the form `**INT [00:25]:** question` followed by `**RESP [00:29]:** answer` (a fictional example is in `examples/library`). In the Library, click "Import transcripts" (Importer des transcriptions). Each interview becomes a row, with the columns ID, the study attributes, "Guide", "Memo" (Mémo: done or to do), "Coding" (Coder: for example 40 / 97 turns) and "Review". The filter above the list narrows the rows by an attribute (for example guide version: B). A new interview is split into units, one speaking turn each, cut at the maximum length set in the study; the splitting is deterministic. A fingerprint of each source file is computed to detect any change.

### 10. Fill in the "Interview record" (Fiche d'entretien) {#preparer-fiche}

The record gathers what you know about the interview before coding it.

**In Coding Studio.** In the Library, one click on a row opens the "Interview record" on the right; a double-click, or the "Code" (Coder) button on the record, opens the Coding screen. The record has three blocks. "Attributes": the fields you have defined (lists are shown as menus), to be filled in before coding because they are then used as filters in "Analyse". "Familiarisation memo": the choice of memo template, with a count of the questions added or removed for this interview. "Source": the transcript file, the number of turns and the note on interviewer turns, which are greyed out and never coded (for example 84 turns, of which 61 are the respondent's and 23 are greyed out).

### 11. Set up pseudonymisation {#preparer-pseudo}

Pseudonymisation protects respondents and their organisations. It applies to your transcripts before import, and to what is sent to a model.

**In Coding Studio.** In "Settings", "AI providers" block, tick "Pseudonymise names of people and organisations before any sending" (Pseudonymiser les noms de personnes et d'organisations avant tout envoi), then complete the replacement list (real name to pseudonym). It applies to every generated prompt. It complements the pseudonymisation of your transcripts without replacing it: a form of name that you have not listed will not be replaced. Read a prompt before sending it to a hosted provider. No provider key is stored: it is referred to by its environment variable name.

### 12. Export or import a project template {#preparer-modele}

A project template reuses your choices for a new study, without the data.

**In Coding Studio.** In the "Study models and lists" block, "Export" (Exporter) writes a JSON file with the description, questions, rules, framework, attributes, memo templates, editable lists and matrix dimensions. It contains no transcript, no coding and no written memo. In a new database, "Import a project template (JSON)" (Importer un modèle de projet (JSON)) fills in these fields: check them before you start.

### 13. Add an interview to an already analysed corpus {#preparer-ajouter}

A new interview extends the study. Use the current codebook as a starting point without forcing the new account into existing conclusions. The whole corpus does not need to be recoded.

1. Back up the project before importing. In Library, use Import transcripts, then check the source, speakers, timestamps, segmentation and attributes. Do not replace the sources of previously coded interviews.
2. Read the whole interview and write the familiarisation memo before coding. Distinguish lived experience, reported observations, expectations and hypotheses. Interviewer questions provide context, never respondent evidence on their own.
3. Code exact passages with the current codebook. For an English-language thesis, keep code names and definitions in English, and verbatim quotations in their original language. Reuse the same code for a common phenomenon across sectors and speakers. Add each independently expressed dimension separately: “Stock visibility” describes the phenomenon; add “Stated business value” only when the respondent frames it as business value. A passage can carry both. Do not combine them into a composite label.
4. Examine every respondent turn. Explicitly mark out-of-scope passages NA; retain uncertainty for review instead of inventing a code. Refine the initial selections at the end of the session: passage boundaries, independent codes, definitions, inclusion and exclusion criteria, memo and example quotation.
5. Create or split a code only when a useful distinction is missing. Document the reason and codebook version. Use Lexicon, keyword in context and similar passages to locate possible occurrences in earlier interviews, then reread their context. Revise only affected passages and close their harmonisation items; a search hit is not enough to apply a code.
6. AI review remains optional. If used, preserve the frozen codebook and reference reading, prompt, actual response and decisions. Arbitrate disagreements, check sampled agreements and decide candidate concepts. An earlier run does not automatically cover new passages or a revised codebook. AI-assisted construction is not independent human validation and must be disclosed accordingly.
7. Check that the final decided codes are actually attached to the exact source passages used in analysis. Recording an arbitration does not replace checking the highlights. Complete the checklist, memo and pending review items. Check thesis quotations and uncertain passages against available audio; audio availability does not mean that listening has taken place.
8. Refresh themes, counter-examples, annotated matrix cells and evidence links, cases and research-question syntheses. Recompute lexical exploration and attribute frequencies; frequency does not establish analytical importance. Revise the Write findings text before regenerating dated exports and the analysis report. The report reproduces stored text and evidence; generating it does not itself validate the findings.

**Read the progress indicators.** Coding counts examined turns, coded or explicitly out of scope, against respondent turns. Later, to-review marks, harmonisation and open decisions remain unfinished work. AI review displays its own coverage and can remain absent or partial without cancelling completed coding. The arbitration counter includes disagreements and sampled agreements in both numerator and denominator. Ready describes a working state in the tool, not audio certification or independent scientific validation.

## Common pitfalls {#preparer-pieges}

- Writing questions too broad to guide the reading. A question that is answered with "everything" guides nothing.
- Forgetting the status of the statements, then comparing a lived deployment and an expert opinion as if they were equivalent.
- Believing the pseudonymisation list is enough. It cannot make up for a transcript that was poorly anonymised.

## What this stage delivers {#preparer-sortie}

- A project with its database, its questions and its coding rules.
- Defined attributes, memo templates and a checklist that suit the study.
- An interpretive framework chosen or, most often, none.
- The interviews listed in the Library, split into units, each with a completed record.
- A pseudonymisation list ready to use.
- If you wish, a reusable JSON project template.
