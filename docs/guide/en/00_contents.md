# Coding Studio guide {#sommaire}

## What this guide covers {#sommaire-portee}

Coding Studio is a local tool for the qualitative analysis of semi-structured interviews. It helps you move from transcripts to findings you can defend, where every claim can be traced back to the exact passages. It covers the core of a package such as NVivo (passage coding, code tree, memos, matrices, exports) and adds two options: a second coding pass by a language model, done blind, and a lexical workshop that uses no generative AI.

The guide describes the tool for any study. Questions, attributes, memos and the interpretive framework are defined in the "Settings" (Réglages) of your project. The examples use a fictional study: the adoption of a new loan system by small libraries (interviews BIB-01, BIB-02, ...).

## The workflow in one list {#sommaire-parcours}

The work follows six stages. The workflow is not a waterfall: going back to the interviews, reopening a code or rewriting a theme is normal.

1. Prepare (once per study). You create the project, write your research questions, define the interview attributes, the memo templates and the checklists, then import the transcripts.
2. Code (interview by interview). You write a familiarisation memo, then read the respondent's turns and attach codes to passages. The codebook is built as you code.
3. Review (optional, interview by interview). You harmonise the codebook after each major change, and you can have a language model read an interview blind, to compare the two readings.
4. Analyse (across all interviews). You group codes into themes, test each theme, compare interviews in a matrix, and explore words and nearby passages.
5. Write (across all interviews). You write the findings theme by theme, link each quoted passage to the part it supports, then answer each research question.
6. Export and audit. You produce the export packages, check the audit log, verify three claims chosen at random, and assemble your appendices.

In the app, stages 2 to 5 form the numbered bar of the single-interview screen. Stages 1 and 6 are done from the "Library" (Bibliothèque) and the "Settings".

To add an interview after analysis, follow the [incremental corpus workflow](01_prepare.md#preparer-ajouter). It includes targeted review of earlier passages and updating the analysis.

## The screens of the app {#sommaire-ecrans}

The top navigation offers the Library, "Analyse" (Analyser), "Write" (Rédiger), the Codebook, the Settings and the Guide, plus the interview you have open. Each screen has a "?" button in the top right corner that opens the matching section of this guide in a side drawer, without leaving the screen.

- Library: the list of interviews, the import and the interview record. See [Prepare](01_prepare.md#preparer-bibliotheque).
- "Coding" (Coder): the text of an interview, the list of codes, the memo. See [Code](02_code.md#coder-parcourir).
- "Highlights and lexicon" (Surlignages et lexique): highlighting without a code, and the keyword workshop. See [Code](02_code.md#coder-surlignages).
- "Detail" (Détail): the drawer for a passage you are about to quote. See [Code](02_code.md#coder-detail).
- "Review", AI pane (Revoir, volet IA): preparing the prompt and importing the response. See [Review](03_review.md#revoir-prompt).
- "Review", decisions pane (Revoir, volet décisions): disagreements and proposed concepts. See [Review](03_review.md#revoir-arbitrer).
- "Analyse": themes, matrix, exploration. See [Analyse](04_analyse.md#analyser-themes).
- "Write": five tabs (by theme, synthesis by question, reading through the framework, final audit, documented decisions). See [Write](05_write.md#rediger-theme).
- Codebook: the code tree. See [Code](02_code.md#coder-codebook).
- Settings: everything that is defined once per study. See [Prepare](01_prepare.md#preparer-reglages).
- Guide: this text, with its contents and the "?" button of each screen. See [Contents](00_contents.md#sommaire-lire).

## How to read this guide {#sommaire-lire}

Each stage follows the same plan.

1. Why this stage: three to six sentences on the method.
2. What you do: numbered actions. Under each one, an "In Coding Studio" block says where to click, what is displayed, and what is saved.
3. Common pitfalls: the usual mistakes.
4. What this stage delivers: what you have in hand at the end.

Three principles run through the whole guide.

A code describes, a theme interprets. The codes you attach while coding stay close to what the respondent says. Themes, built later, are propositions that you formulate and defend.

The interpretive framework is optional and comes late. If you choose one, it appears only in the "Analyse" and "Write" stages. It never appears during coding and is never a field on a code.

You remain the author. The tool records, compares, counts and suggests, but it never decides in your place. It also refuses to produce a figure that you could not trace back to the data.

## What can be customised {#sommaire-personnalisable}

These elements are editable lists with default values: memo templates, interview attributes, the end-of-interview checklist, the interpretive framework, theme checks (seven by default), evidence types, link types and matrix dimensions. A JSON project template exports these choices for a new study (see [Prepare](01_prepare.md#preparer-modele)). Only the matrix cell values (present, not mentioned, contradicted, uncertain) are fixed.
