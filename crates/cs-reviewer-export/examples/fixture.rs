use coding_studio_final_export::{catalogue, encrypt, reader_html, select, Record};
use serde_json::{json, Value};
fn record(category: &str, ids: &[&str], value: Value) -> Record {
    let mut path = vec![category.to_string()];
    path.extend(ids.iter().map(|s| s.to_string()));
    Record {
        path,
        value,
        selected_by_default: true,
    }
}
fn main() {
    let text = "😀 Le suivi du stock aide à préparer les commandes.";
    let span = |quote: &str, id: &str| {
        let byte = text.find(quote).unwrap();
        let start = text[..byte].chars().count();
        json!({"passage_id":id,"interview_id":"TEST-01","unit_id":"TEST-01-U001","text":quote,"start_offset":start,"end_offset":start+quote.chars().count(),"origin":"opinion","role":"undetermined"})
    };
    let c = catalogue(&[
        record("Interviews", &["TEST-01"], json!({"interview_id":"TEST-01","folder_name":"Entretien synthétique A"})),
        record("Transcript", &["TEST-01","TEST-01-U001"], json!({"interview_id":"TEST-01","unit_id":"TEST-01-U001","seq":1,"role":"respondent","speaker_raw":"Participant A","text":text,"timestamp":"00:00:10","private_note":"NOM EXCLU DU TEST"})),
        record("Transcript", &["TEST-01","TEST-01-U002"], json!({"interview_id":"TEST-01","unit_id":"TEST-01-U002","seq":2,"role":"interviewer","speaker_raw":"Chercheur","text":"Que faites-vous de cette information ?","timestamp":"00:00:20"})),
        record("Transcript", &["TEST-01","TEST-01-U003"], json!({"interview_id":"TEST-01","unit_id":"TEST-01-U003","seq":3,"role":"respondent","speaker_raw":"Participant A","text":"<img src=x onerror=alert(1)> Ceci est du texte, sans exécution."})),
        record("Transcript", &["TEST-02","TEST-02-U001"], json!({"interview_id":"TEST-02","unit_id":"TEST-02-U001","seq":1,"role":"respondent","text":"Le stock est contrôlé avant de préparer les commandes."})),
        record("Original transcript", &["TEST-01","TEST-01-U001"], json!({"interview_id":"TEST-01","unit_id":"TEST-01-U001","seq":1,"speaker_raw":"Sophie Fictional","role":"respondent","text":"Original retained participant wording."})),
        record("Prepared transcript", &["TEST-01","TEST-01-U001"], json!({"interview_id":"TEST-01","unit_id":"TEST-01-U001","seq":1,"speaker_raw":"Participant A","role":"respondent","text":"Approved translated reading copy."})),
        record("Codebook", &["C-GROUP"], json!({"code_id":"C-GROUP","name":"Operational purposes","definition":"A group of operational purposes."})),
        record("Codebook", &["C-0001"], json!({"code_id":"C-0001","name":"Stock visibility","definition":"Knowing stock quantities and availability.","parent_id":"C-GROUP"})),
        record("Codebook", &["C-0002"], json!({"code_id":"C-0002","name":"Stated business value","definition":"A benefit or a reason for the solution.","parent_id":"C-GROUP"})),
        record("Passages", &["TEST-01","P-0001"], span("suivi du stock","P-0001")),
        record("Passages", &["TEST-01","P-0002"], span("stock aide","P-0002")),
        record("Passage coding", &["TEST-01","1"], json!({"interview_id":"TEST-01","passage_id":"P-0001","code_id":"C-0001"})),
        record("Passage coding", &["TEST-01","2"], json!({"interview_id":"TEST-01","passage_id":"P-0002","code_id":"C-0002"})),
        record("Arbitration", &["TEST-01","2","TEST-01-U001"], json!({"interview_id":"TEST-01","unit_id":"TEST-01-U001","phase":2,"final_codes":["C-0001","C-0002"],"rationale":"Separate the inventory phenomenon from its business value."})),
        record("Interview notes", &["TEST-01","attributes"], json!({"interview_id":"TEST-01","kind":"attributes","body":{"sector":"Retail","actor_group":"Retailer"}})),
        record("Interview notes", &["TEST-01","memo"], json!({"interview_id":"TEST-01","kind":"memo","body":{"template_id":"default","answers":{"q1":{"text":"- Stock information supports order preparation.\n- Synthetic research example."}}}})),
        record("People", &["P-01"], json!({"person_id":"P-01","name":"Participant A","organisation":"Synthetic retailer","person_function":"Stock manager"})),
        record("People interviews", &["P-01","TEST-01"], json!({"person_id":"P-01","interview_id":"TEST-01","role":"respondent"})),
        record("Themes", &["T-0001"], json!({"theme_id":"T-0001","title":"Stock information supports operational action","status":"retained","proposition":"- Information is useful when it supports a task.","limits":"- Synthetic data, no empirical conclusion.","main_passages":["P-0001","P-0002"],"ord":1})),
        record("Theme codes", &["T-0001","C-0001"], json!({"theme_id":"T-0001","code_id":"C-0001"})),
        record("Matrix", &["interview:TEST-01","theme:T-0001"], json!({"row_key":"interview:TEST-01","col_key":"theme:T-0001","value":"present","summary":"Stock supports preparation."})),
        record("Matrix", &["interview:TEST-02","theme:T-0001"], json!({"row_key":"interview:TEST-02","col_key":"theme:T-0001","value":"uncertain","summary":"Benefit not quantified."})),
        record("Matrix passages", &["interview:TEST-01","theme:T-0001","P-0001"], json!({"row_key":"interview:TEST-01","col_key":"theme:T-0001","passage_id":"P-0001"})),
        record("Analytical notes", &["theme_part","T-0001","analytical_statement"], json!({"scope":"theme_part","target":"T-0001","sub":"analytical_statement","body":{"text":"- The stock record is useful when it changes an operational action.","nature":"opinion"}})),
        record("Analytical notes", &["theme_part","T-0001","mechanism_evidence"], json!({"scope":"theme_part","target":"T-0001","sub":"mechanism_evidence","body":{"text":"- Participant A links stock information with order preparation.","nature":"opinion"}})),
        record("Evidence links", &["L-1"], json!({"link_id":"L-1","target_kind":"theme_part","target_id":"T-0001","part":"mechanism_evidence","passage_id":"P-0001","link_type":"supports"})),
        record("Research questions", &["SQ1"], json!({"id":"SQ1","text":"When does stock information become useful?"})),
        record("Analytical notes", &["rq_synthesis","SQ1",""], json!({"scope":"rq_synthesis","target":"SQ1","sub":"","body":{"answer":"- When it supports a concrete task.","limits":"- Synthetic example only."}})),
        record("Cases", &["CASE-1"], json!({"case_id":"CASE-1","label":"Order preparation","fields":{"trigger":{"text":"- Need to prepare orders."},"choice":{"text":"- Use stock information."}}})),
        record("Case units", &["CASE-1","TEST-01","TEST-01-U001"], json!({"case_id":"CASE-1","interview_id":"TEST-01","unit_id":"TEST-01-U001"})),
    ]).unwrap();
    let ids: Vec<_> = c
        .fields
        .iter()
        .filter(|f| f.path.last().unwrap() != "private_note")
        .map(|f| f.id.clone())
        .collect();
    let p = select(&c, &c.snapshot, &ids).unwrap();
    let e = encrypt(&p, "synthetic test password 12345").unwrap();
    let dest = std::env::args().nth(1).expect("output HTML path");
    std::fs::write(dest, reader_html(&e).unwrap()).unwrap();
    println!(
        "Synthetic reader: {} selected fields; 2 interviews and linked analysis.",
        p.fields.len()
    );
}
