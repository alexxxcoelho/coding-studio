# Review (optional) {#revoir}

## Why this stage {#revoir-pourquoi}

Review is for checking your own reading, not for replacing it. The first part uses no AI: after a merge or a redefinition of a code, you re-read the units of earlier interviews that it concerns. The second part is a review by a language model, which you run if it seems useful for an interview. The model goes after you and blind: if it went first, its coding would steer yours; if it saw your codes, it would tend to confirm them. It therefore receives the turns and the codebook, never your coding. The result is a documented reading check, not a measure of reliability between coders, which reflexive thematic analysis as set out by Braun and Clarke (2021) does not require. An interview can be coded without this review; when it is done, state in your report which interviews it covers.

## What you do {#revoir-actions}

### 1. Harmonise without AI {#revoir-harmoniser}

When two codes are merged or a definition changes, units already coded may no longer match the new meaning.

**In Coding Studio.** After a merge, a split or a redefinition in the "Codebook", the list of units to re-verify appears on the Codebook screen and in the "Decisions" pane (volet décisions) of "Review". Each row shows the text of the unit and its current codes. Re-read it, then confirm the coding or correct it. When the whole list is handled, the "Library" (Bibliothèque) marks the interview as reviewed.

### 2. Decide whether an AI review is useful {#revoir-decider}

An AI review costs attention, and it exposes a text to a third party if the model is hosted. It is justified mainly on the first interviews, or on an interview you have doubts about.

**In Coding Studio.** The step bar shows the state of the review for the interview: "no review" (pas de revue), "AI review pending" (revue IA en attente) or "AI review done, 9 of 12 decided" (revue IA faite, 9 sur 12 décidées). A review is never a lock: you can move on to "Analyse" (Analyser) without having done it.

### 3. Configure a provider, or do without one {#revoir-fournisseurs}

Copying and pasting into the model of your choice is the reference route and is always available. A configured provider only makes the round trip on your behalf.

**In Coding Studio.** In "Settings" (Réglages), "AI providers" (Fournisseurs IA) block, the list shows the declared providers (local command, OpenAI-compatible API, account connection). Click "Add a provider" (Ajouter un fournisseur) and fill in its type. A key is never stored: it is referred to by its environment variable name. Each provider declares what it sends and where it runs (locally or hosted). For a local command, "Test isolation" (Tester l'isolation) checks its isolation and dates the check. Sending is always an explicit, logged action. See also [pseudonymisation](01_prepare.md#preparer-pseudo).

### 4. Generate the prompt {#revoir-prompt}

Generating the prompt freezes your coding of the interview and prepares the text to send.

**In Coding Studio.** Open the interview, then the Review stage. The "AI" pane (volet IA) starts with "Generate prompt" (Générer le prompt), disabled until the relevant turns are coded. On click, your coding is saved as it stands: it is the reference for the comparison, even if you change your mind later. The prompt contains the study description, the questions, the coding rules, the codebook (tree, definitions, examples), the respondent attribute useful for context and, for each turn, the interviewer question that precedes it. Names of people and organisations are pseudonymised before sending. Your codes never appear in it. If necessary, the prompt is split into parts; a line shows the date, the codebook version, the number of units and the number of parts. Click "Copy part 1 of 2" (Copier la partie 1 sur 2), then part 2 of 2. The model may flag a "concept missing from the codebook": this suggestion will arrive as a candidate in the decisions pane.

### 5. Import the model's response {#revoir-reponse}

Once the response is obtained, it enters the tool, which checks it before accepting it.

**In Coding Studio.** Reference route (2a): paste the prompt into the model of your choice, then, in the "Model used" (Modèle utilisé) field, name it. Paste its response into the field "Response to part 1 of 2" (Réponse à la partie 1 sur 2), then click "Import the response" (Importer la réponse). Repeat for each part. A malformed response is rejected as a whole, with the list of problems; nothing is kept half-way. With a configured provider (2b), choose it and click "Launch" (Lancer): the tool makes the round trip, then applies the same validation. The log records the provider, the place of execution, the text sent and the raw response. Runs appear in the "Saved runs" (Runs enregistrés) list, for example "R1, 13.10 09:40, model X, 2 of 2 parts, complete. Arbitrate". One complete run is enough. A second run is only useful to compare two models.

### 6. Arbitrate the disagreements {#revoir-arbitrer}

The tool lists the units where the model and you did not attach the same codes. These are the ones that deserve your decision.

**In Coding Studio.** In the decisions pane, the "Disagreements" (Désaccords) section shows one card per unit, with a counter (for example 7 of 9 decided). A card contains the text of the turn with the question that precedes it, your frozen codes, the model's codes, and three choices: "Keep mine" (Garder le mien), "Take the model's" (Prendre celui du modèle), "Other: choose the codes" (Autre : choisir les codes). A justification is mandatory. It is entered in one sentence, for example "the model read a constraint where the respondent describes a choice". Click "Save" (Enregistrer). Your decision is final. The tool keeps three objects separately, never overwriting any of them: your frozen reading, the model's reading and your decision.

### 7. Check a sample of agreements {#revoir-accords}

An agreement is not proof: you and the model may be wrong in the same way.

**In Coding Studio.** The "Sample of agreements" (Échantillon d'accords) section presents a few units drawn at random from those where you and the model agree (the draw is recorded with its seed). For each one, click "Confirm" (Confirmer) or "Correct" (Corriger). A justification is optional.

### 8. Handle concepts missing from the codebook {#revoir-concepts}

The model may flag a concept your codebook does not cover: it is a candidate, never a code applied on its own.

**In Coding Studio.** The "Concepts missing from the codebook" (Concepts absents du codebook) section lists each proposal with the units that motivate it. For each one, choose "Create a code" (Créer un code), "Attach to an existing code" (Rattacher à un code existant) or "Dismiss" (Écarter). The decision is justified and dated, as for a disagreement.

### 9. Revise the codebook after review {#revoir-codebook}

Disagreements often reveal a poorly defined code.

**In Coding Studio.** Go back to the Codebook to refine a definition, merge or split; the list of units to re-verify appears again (action 1). Running a review again after a major change only makes sense if you judge it useful.

## Common pitfalls {#revoir-pieges}

- Treating the AI review as a second coder whose agreement proves reliability. It checks your reading, nothing more.
- Looking only at disagreements. An agreement can hide a shared error.
- Sending a prompt to a hosted provider without having re-read the pseudonymisation list.

## What this stage delivers {#revoir-sortie}

- For each reviewed interview, three distinct objects: your frozen reading, the model's reading, and your justified decisions.
- Disagreements settled, and a verified sample of agreements.
- Codes created, attached or dismissed from the concepts proposed.
- A harmonised codebook, with the list of re-verified units.
- A log that proves the order of operations: your coding comes before the model's.

### Paste a batch of arbitrations

Under **Review → Decisions**, choose **Paste arbitrations (JSON)**. Provide
`interview_id` and a `decisions` array containing `unit_id`, `decision`,
`final_codes` and `comment`. Supported choices are `keep_mine`, `accept_reviewer`,
`merge` and `custom`. Codes may use stable ids or exact tokens; use `NA` alone.

Select **Validate and preview**, review each excerpt and decision, then **Save
batch**. Replacing existing decisions requires an explicit checkbox. The entire
batch is saved together, or none of it is saved if validation or storage fails.
Previous decisions remain in history; frozen human references and AI answers
remain unchanged. Final codes are authoritative: choices differing from the
frozen human coding or an AI run are recorded as custom decisions. `merge` combines
codes for a passage; it does not merge codes in the codebook.

In **Harmonise**, **Confirm all** confirms all pending items of this interview
in one batch, retaining their current codes and any notes you entered. Completed
items stay unchanged. The button count states the batch scope; other interviews
are unaffected.
