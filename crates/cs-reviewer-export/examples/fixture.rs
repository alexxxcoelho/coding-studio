use coding_studio_final_export::{catalogue, encrypt, reader_html, select, Record};
use serde_json::json;

fn main() {
    let c = catalogue(&[
        Record { path:vec!["Entretiens".into(),"TEST-01".into()], value:json!({
            "U001":{"Texte":"Le suivi du stock aide à préparer les commandes.","Codes":["Stock visibility", "Stated business value"]},
            "U002":{"Texte":"<img src=x onerror=alert(1)> Ceci est du texte, sans exécution."},
            "Nom":"NOM EXCLU DU TEST"}), selected_by_default:true },
        Record { path:vec!["Analyse".into(),"Thème 1".into()], value:json!({"Notes":"• Visibilité utile pour les commandes.\n• Exemple synthétique uniquement."}), selected_by_default:true }
    ]).unwrap();
    let ids: Vec<_> = c
        .fields
        .iter()
        .filter(|f| f.path.last().unwrap() != "Nom")
        .map(|f| f.id.clone())
        .collect();
    let p = select(&c, &c.snapshot, &ids).unwrap();
    let e = encrypt(&p, "synthetic test password 12345").unwrap();
    let dest = std::env::args().nth(1).expect("output HTML path");
    std::fs::write(dest, reader_html(&e).unwrap()).unwrap();
    println!(
        "Synthetic encrypted reader generated; {} fields; excluded name omitted.",
        p.fields.len()
    );
}
