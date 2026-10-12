# Coding procedure and its instrumentation

This document describes the coding procedure applied to the interviews of the thesis. The procedure is set out first in terms that any researcher could reproduce with a spreadsheet and a text editor. The second part states how Coding Studio executes each step and what it records. The text is written so that it can be adapted for the methodology chapter and the appendix on the use of artificial intelligence.

## Part one: the procedure

### 1. Corpus and sources

The corpus consists of one folder per interview, identified by a stable code (INT, ECO, RET or ADJ followed by a number) that links the interview to the fieldwork tracker, the preparation file and the analysis. Each folder holds the transcript in one or more forms: the transcript produced by the conferencing platform, the raw output of the speech recognition engine, a cleaned and diarised version, or, for written contributions, the message received. Before any analysis, every file is inventoried and its SHA-256 digest is recorded, so that any later change to a source can be detected.

### 2. Units of analysis

One transcript per interview is designated as the primary source. It is divided into units by a fixed rule that requires no interpretation: one unit for each continuous turn of a speaker, a turn longer than 150 words being divided at sentence boundaries without reordering. Each unit receives an identifier composed of the interview code and a sequential number, together with the speaker, the time mark at which the turn begins and the text. The text is not altered beyond the removal of redundant spaces, so that every quotation can later be found word for word. Lines of the source that are not speaker turns, such as editorial notes on inaudible passages, are listed rather than silently discarded. The list of units is fixed once coding begins.

### 3. Two passes over the units

The Technology, Organisation and Environment framework names three contexts that shape the adoption of an innovation, the technological, the organisational and the environmental, without prescribing a fixed list of factors within them: each study selects the factors that fit its technology and setting (Tornatzky and Fleischer 1990; Baker 2012). The framework is therefore kept as the theoretical frame of the thesis, and the coding works in two passes that use it differently.

The first pass is a provisional pre-classification. The candidate assigns each unit to the technological context (TECH), the organisational context (ORG), the environmental and interorganisational context (ENV), or to CONTEXT when the unit only describes the respondent, the organisation or the scope of the account; a unit that touches several contexts carries each of them, and a unit that is not relevant is marked NA. These assignments orient the reading and decide which units are coded in detail; they remain open to revision throughout the analysis and are not submitted to a second coder.

The second pass codes every unit whose first-pass category is not NA with concrete codes, each naming a mechanism in the respondent's terms, such as a legal obligation, a customer requirement, a lack of skills, or a cost borne by a party that draws no benefit from it. Each code is attached to a TOE context where the attachment is meaningful, and records whether it comes from the literature or from the field. A unit may carry several codes. When no existing code fits, the candidate records a provisional label with a short note rather than forcing a code; the label is later either turned into a new code, defined and marked as coming from the field, or mapped to an existing code, and the decision is applied to every unit that carries the label. Coming from the field is thus a property of a code, not a category set beside the three contexts.

### 4. The candidate's coding

For each unit coded in detail, the candidate may also record a sub-code, a modifier, an evidence level, the provenance of the statement and an analytical memo, and may mark the exact words that will serve as a quotation. Interview summaries and record sheets may be consulted during this step; they are never given to the secondary coder. The codebook evolves as interviews are coded: definitions are refined, codes that prove to name the same mechanism are merged, and the passages already coded with a code are revisited when its definition changes. Each such change is dated and kept.

### 5. A language model as a challenger

The concrete codes, and only they, are submitted to a language model acting as a blind second reader. The model receives the codes offered (code, label and definition, grouped by TOE context), the rules of the task and the units, and nothing else: neither the candidate's codes, nor the first-pass categories, nor the provisional labels, nor memos, summaries or material from other interviews. The rules are those of protocol P-04: use only the codes listed, answer NA when no code applies, code what is said in the unit rather than what may be inferred from the interview, treat the interviewer's turns as NA unless they report a fact from the field, write no commentary, and process every unit in order. The answer must be a single JSON object in a prescribed structure, which names the interview and the version of the units it refers to. Long interviews are divided into parts, each answered separately; the parts generated together form a batch, and each run of the model answers one batch.

An answer is accepted only when the candidate has already coded every unit of the part it answers, so that the candidate's reading is formed before the model's is seen. It is kept exactly as returned and is accepted only if it refers to the right interview and version of the units, covers every unit of its part exactly once, and uses only the codes offered in that batch or NA. An answer that fails any of these checks is rejected as a whole, and every problem found is reported. For each run the model name and version, temperature, seed, context length, batch size, prompt version and time of execution are recorded.

### 6. Review

The comparison serves the interpretation, not a target figure. Every unit on which the candidate's codes differ from those of the model is reviewed, together with a sample of the units on which they agree: at least five agreeing units or a tenth of them, whichever is larger, drawn by a deterministic procedure whose seed is recorded, so that agreement is examined as well as disagreement. For each reviewed unit the candidate keeps the initial coding, adopts the model's codes or settles on a different set of codes, and gives a written rationale for every disagreement. The candidate keeps the final decision; a decision may be revised, and earlier decisions remain on record. The candidate's coding remains editable after the model's answer, and any change made after it is flagged, so that a revision prompted by the comparison can be distinguished from the initial reading.

The share of units with identical codes and Cohen's kappa per code, computed on the presence or absence of each code, are reported for transparency, without any model in the calculation; kappa is reported as undefined when only one level is observed for a code. The final coding of a unit is the review decision where one exists, and the candidate's coding otherwise.

### 7. Synthesis by case

Each interview is then summarised by case, a case being one identifiable project. For each case the candidate records the trigger of the project, the alternatives considered, the constraints met, the choice made and its result, each statement tied to the units that support it, together with the stage reached and the outcome status in the terms of the codebook. The cases are then compared question by question: each code is mapped to the research questions it bears on, and for each question the codes found in each case are set side by side with the units that carry them. The comparison is used to answer the research questions, not to count how often each context appears.

### 8. Audit trail

Every step above is recorded as it happens: inventory of sources, choice of the primary source, segmentation, each coding decision with its pass and its previous and new value, each label and its resolution, each change to the codebook, each prompt, each run and each answer, each agreement sample, each review decision, each case and each export. The record can be extended but never edited. Each entry contains the digest of the preceding one, so that any later alteration of the record becomes detectable. For each interview, an archive gathers the units, both passes of the candidate's coding, every prompt and answer, the agreement report, the review log, the final coding, the labels and their resolution, the case syntheses, the codebook, the relevant part of the record and a manifest of the digests of all these files.

## Part two: execution by Coding Studio

Coding Studio is a program written for this thesis that executes the procedure above on the candidate's computer. It runs locally, makes no network call and never calls a language model itself: the candidate copies each prompt into the chosen model's interface and pastes the answer back. The source code is kept with the thesis tools in `00_Tools/CodingStudio`.

### Sources and units

The library is built by scanning the interview folder. For each file the program records its type, size, modification time and SHA-256 digest; archive folders, scripts, media and attachments are excluded from the inventory and listed as such. On every subsequent scan, only additions, removals and changes of digest are recorded. Readers are provided for the formats found in the corpus: WebVTT from the conferencing platform, timestamped text from the speech recognition engine, cleaned transcripts in Markdown with interviewer and respondent labels, exports of the Budgie Echo transcription tool and written responses. The segmentation rule is the one of `00_Tools/Scripts/segment_transcript.py`; the list of units is stored with the digest of the source and its own digest, and a new segmentation is refused once any coding exists.

### Coding

The candidate codes in a two-pane interface: the units on the left, the selected unit and the code picker on the right. In the first pass a single key assigns a category and moves to the next unit. In the second pass the picker offers every active code, those attached to the unit's first-pass contexts first, and a provisional label can be put on the unit at any time. The program refuses codes that are unknown, inactive or merged, NA combined with another code, and quotations that are not an exact substring of the unit. The codebook is first read from the thesis coding database, which the program opens in read-only mode, and then maintained in the program: codes can be created, edited, merged into another code or deactivated, each change incrementing the version of the code, and the codebook can be exported as files in the format of the thesis database. For any code, the program lists every unit that carries it, so that passages can be revisited after a change of definition.

### Prompts and answers

The prompt is generated from the units in scope and the codes offered to the model, in English, and is identical for identical inputs. It carries the identifiers of the interview, of the version of the units and of the codes offered, so that each answer can be matched to the material it was produced from. Every generated prompt is stored with its digest, its batch and the list of codes it offered. Before an answer is accepted, the program checks that the candidate has coded every unit of its part; it then keeps the pasted text and its digest, extracts the JSON object, and applies the checks described in step 5 against the batch the run answers, so that a later change to the codebook does not invalidate an answer to an earlier prompt.

### Review, cases and export

When a run becomes complete, the program draws its sample of agreeing units, stores it with its seed and records it. The review presents each disagreement, then each sampled unit, with the candidate's codes and each run's codes side by side; a disagreement cannot be decided without a rationale. The agreement measures are computed with the formulas of `00_Tools/Scripts/compute_agreement.py`. Cases are recorded per interview with their five fields and the units that support each one, and a cross-case view arranges, for each research question, the codes found in each case. The export writes the archive described in step 8 to a new dated folder, never overwriting an earlier one, with CSV files in the formats of the thesis scripts so that the results can be recomputed independently of the program.

### Integrity of the record

The record is a table of the program's database in which each entry stores the SHA-256 digest of its own content and of the preceding entry. Database triggers refuse any modification or deletion of the record, of the stored prompts, of the stored answers, of the agreement samples, of the review decisions and of the resolutions of labels. The program recomputes the whole chain on request and reports the first entry that does not match. Each change to the coding and its entry in the record are written in a single transaction, so that neither can exist without the other.

### Limits

The program enforces the order of operations it controls: the candidate codes a unit before the model's answer on it can be imported, and later revisions are flagged rather than hidden. It cannot prevent a researcher from consulting a model outside the program. The candidate's coding remains a human judgement that the program records but does not assess. The digest chain detects alterations of the record; it does not prevent someone with access to the files from replacing the whole database, which is why the exported archives, with their manifests, are kept with the thesis materials.
