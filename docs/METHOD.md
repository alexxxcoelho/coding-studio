# Coding procedure and its instrumentation

This document describes the coding procedure applied to the interviews of the thesis. The procedure is set out first in terms that any researcher could reproduce with a spreadsheet and a text editor. The second part states how Coding Studio executes each step and what it records. The text is written so that it can be adapted for the methodology chapter and the appendix on the use of artificial intelligence.

## Part one: the procedure

### 1. Corpus and sources

The corpus consists of one folder per interview, identified by a stable code (INT, ECO, RET or ADJ followed by a number) that links the interview to the fieldwork tracker, the preparation file and the analysis. Each folder holds the transcript in one or more forms: the transcript produced by the conferencing platform, the raw output of the speech recognition engine, a cleaned and diarised version, or, for written contributions, the message received. Before any analysis, every file is inventoried and its SHA-256 digest is recorded, so that any later change to a source can be detected.

### 2. Units of analysis

One transcript per interview is designated as the primary source. It is divided into units by a fixed rule that requires no interpretation: one unit for each continuous turn of a speaker, a turn longer than 150 words being divided at sentence boundaries without reordering. Each unit receives an identifier composed of the interview code and a sequential number, together with the speaker, the time mark at which the turn begins and the text. The text is not altered beyond the removal of redundant spaces, so that every quotation can later be found word for word. Lines of the source that are not speaker turns, such as editorial notes on inaudible passages, are listed rather than silently discarded. The list of units is fixed once coding begins.

### 3. Primary coding by the candidate

The candidate codes every unit against the codebook, which follows the Technology, Organisation and Environment framework and adds families for outcomes, turning points and transferability, together with inductive codes created during the analysis. A unit may carry several codes; a unit to which no code applies is coded NA, which is a valid and frequent answer. For each unit the candidate may also record a sub-code, a modifier, an evidence level, the provenance of the statement and an analytical memo, and may mark the exact words that will serve as a quotation. Interview summaries and record sheets may be consulted during this step; they are never given to the secondary coder.

### 4. Blind secondary coding by a language model

The secondary coding is a measurement instrument and nothing more. It may be carried out only after the primary coding of the interview is complete, and the primary coding is frozen from the moment the first answer of the model is accepted. The two codings are therefore independent by construction.

The model receives the codebook (code, family, label and definition of each code offered to it), the rules of the task and the units, and nothing else: neither the candidate's codes, nor memos, nor summaries, nor material from other interviews. The rules are those of protocol P-04: use only the codes listed, answer NA when no code applies, code what is said in the unit rather than what may be inferred from the interview, treat the interviewer's turns as NA unless they report a fact from the field, write no commentary, and process every unit in order. The answer must be a single JSON object in a prescribed structure, which names the interview and the version of the units it refers to. Long interviews may be divided into parts, each answered separately.

Three runs are carried out for each interview: the main model with the main wording, the same model with a reworded prompt, and a second, open-weight model with the main wording. For each run the model name and version, temperature, seed, context length, batch size, prompt version and time of execution are recorded.

Each answer is kept exactly as returned. It is accepted only if it refers to the right interview and version of the units, covers every unit of its part exactly once, and uses only codes offered to the model or NA. An answer that fails any of these checks is rejected as a whole, and every problem found is reported.

### 5. Agreement

Agreement is computed without the intervention of any model. For each pair of codings (the candidate against each run, and the runs against one another) two measures are reported: the share of units on which the full set of codes is identical, and, for each code, the raw agreement and Cohen's kappa computed on the presence or absence of the code in each unit, the form suited to multi-label coding. Kappa is reported as undefined when only one level is observed for a code.

### 6. Arbitration

A unit is divergent when the candidate's codes differ from those of at least one run, or when the candidate agrees with some runs while the runs disagree among themselves; the instability of the model is treated as information on the ambiguity of the unit rather than as noise to be averaged out. Every divergent unit is decided by the candidate, without sampling. The decision retains the candidate's coding, the coding of one run, or a different set of codes, and is always accompanied by a written rationale. A decision may be revised; earlier decisions remain on record.

The final coding of an interview is complete only when every divergence is decided. Units on which all codings agree keep the common codes; divergent units take the decided codes. Each unit of the final coding states which of the two bases applies.

### 7. Audit trail

Every step above is recorded as it happens: inventory of sources, choice of the primary source, segmentation, each coding decision with its previous and new value, each prompt, each run and each answer, each arbitration and each export. The record can be extended but never edited. Each entry contains the digest of the preceding one, so that any later alteration of the record becomes detectable. For each interview, an archive gathers the units, both codings, every prompt and answer, the agreement report, the adjudication log, the final coding, the relevant part of the record and a manifest of the digests of all these files.

## Part two: execution by Coding Studio

Coding Studio is a program written for this thesis that executes the procedure above on the candidate's computer. It runs locally, makes no network call and never calls a language model itself: the candidate copies each prompt into the chosen model's interface and pastes the answer back. The source code is kept with the thesis tools in `00_Tools/CodingStudio`.

### Sources and units

The library is built by scanning the interview folder. For each file the program records its type, size, modification time and SHA-256 digest; archive folders, scripts, media and attachments are excluded from the inventory and listed as such. On every subsequent scan, only additions, removals and changes of digest are recorded. Readers are provided for the formats found in the corpus: WebVTT from the conferencing platform, timestamped text from the speech recognition engine, cleaned transcripts in Markdown with interviewer and respondent labels, exports of the Budgie Echo transcription tool and written responses. The segmentation rule is the one of `00_Tools/Scripts/segment_transcript.py`; the list of units is stored with the digest of the source and its own digest, and a new segmentation is refused once any coding exists.

### Coding

The candidate codes in a two-pane interface: the units on the left, the selected unit and the code picker on the right. The program refuses codes that are absent from the codebook or inactive, NA combined with another code, and quotations that are not an exact substring of the unit. The codebook is read from the thesis coding database, which the program opens in read-only mode; codes created during the analysis are added to the INDUCTIVE family and recorded.

### Prompts and answers

The prompt is generated from the units and the codes offered to the model, in English, and is identical for identical inputs. It carries the identifiers of the interview, of the version of the units and of the codebook, so that each answer can be matched to the material it was produced from. Every generated prompt is stored with its digest. Before an answer is accepted, the program checks that the primary coding is complete; it then keeps the pasted text and its digest, extracts the JSON object, and applies the checks described in step 4. Once an answer is accepted, the primary coding of the interview can no longer be modified.

### Agreement, arbitration and export

The agreement measures are computed by the program with the formulas of `00_Tools/Scripts/compute_agreement.py`. Divergent units are presented one by one, with the candidate's codes and each run's codes side by side and the codes present in only one coding highlighted; a decision cannot be recorded without a rationale. The export writes the archive described in step 7 to a new dated folder, never overwriting an earlier one, with CSV files in the formats of the thesis scripts so that the results can be recomputed independently of the program.

### Integrity of the record

The record is a table of the program's database in which each entry stores the SHA-256 digest of its own content and of the preceding entry. Database triggers refuse any modification or deletion of the record, of the stored prompts, of the stored answers and of the arbitration decisions. The program recomputes the whole chain on request and reports the first entry that does not match. Each change to the coding and its entry in the record are written in a single transaction, so that neither can exist without the other.

### Limits

The program guarantees the independence of the two codings with respect to the order of operations it controls; it cannot prevent a researcher from consulting a model outside the program. The candidate's coding remains a human judgement that the program records but does not assess. The digest chain detects alterations of the record; it does not prevent someone with access to the files from replacing the whole database, which is why the exported archives, with their manifests, are kept with the thesis materials.
