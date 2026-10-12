# Draft decision for the Decision Log (to review, number and copy by the candidate)

Not an entry of `03_Method/Decision_Log_2026-07-09.md`. The log ends at D-010 while P-04 already cites D-011: check the numbering before assigning one.

## D-0xx, Coding in two phases, TOE contexts first

Date: 2026-10-10

Decision. The coding of each interview runs in two phases. Phase 1 codes every unit against the three TOE contexts only (TECH, ORG, ENV), plus CONTEXT for descriptive material about the respondent, the organisation or the scope of the account (no explanatory role: a unit that says why adoption advanced, stalled or failed is coded TECH, ORG or ENV), INDUCTIVE-PENDING for relevant material outside the three contexts, and NA. The candidate codes first, a language model codes blind (P-04), agreement is computed and every divergence is arbitrated. Phase 2 codes only the units reconciled as relevant, with the study-specific codes of codebook v0.2 restricted to the families reconciled in phase 1, plus the cross-cutting families (outcome, turning point, transfer, stage, context), followed by a second blind model pass and a second arbitration. Inductive codes are created by the candidate alone in phase 2 and recorded in an inductive log (definition, example, reason, date).

Reason. Tornatzky and Fleischer (1990), as summarised by Baker (2012), define the three contexts but no fixed list of factors within them; the factors are study-specific. Coding the contexts first makes inter-coder agreement measurable on a small, stable category set, and confines the finer judgements to units already reconciled as relevant.

Change from codebook v0.2. Section 5 applied the study-specific codes in the first cycle. They move to phase 2. Section 12 ("TOE-informed first cycle, inductive log and second-cycle cross-case synthesis") was submitted to the supervisor: to be reported to him.

Open point. Threshold of recurrence for retaining an inductive code as a theme (for example two cases): to be decided and recorded.

Instrument. Coding Studio (`00_Tools/CodingStudio`) enforces the order of phases and the locks; see `docs/METHOD.md`.
