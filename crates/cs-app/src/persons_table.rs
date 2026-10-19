//! Local, read-only XLSX preview. No macros, formulas, URLs or relationships are executed.
use crate::ApiResult;
use anyhow::{ensure, Context, Result};
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};

#[derive(Deserialize)]
pub(crate) struct TableBody {
    data: Vec<u8>,
}
pub(crate) async fn preview(Json(body): Json<TableBody>) -> ApiResult {
    if body.data.len() > 5 * 1024 * 1024 {
        return Err(crate::invalid("Le fichier Excel dépasse 5 Mio"));
    }
    let v = tokio::task::spawn_blocking(move || read_xlsx(&body.data))
        .await
        .map_err(|_| crate::invalid("Lecture Excel interrompue"))??;
    Ok(Json(v))
}
fn xml(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String> {
    let mut file = zip
        .by_name(name)
        .with_context(|| format!("Classeur incomplet : {name}"))?;
    ensure!(
        file.size() <= 12 * 1024 * 1024,
        cs_model::CsError::Validation("Une feuille Excel est trop volumineuse".into())
    );
    let mut text = String::new();
    file.by_ref()
        .take(12 * 1024 * 1024 + 1)
        .read_to_string(&mut text)?;
    ensure!(
        text.len() <= 12 * 1024 * 1024,
        cs_model::CsError::Validation("Une feuille Excel est trop volumineuse".into())
    );
    Ok(text)
}
fn strings(node: roxmltree::Node<'_, '_>) -> String {
    node.descendants()
        .filter(|n| n.has_tag_name("t"))
        .filter_map(|n| n.text())
        .collect()
}
fn column(reference: &str) -> Result<usize> {
    let mut n = 0usize;
    for c in reference.bytes().take_while(u8::is_ascii_alphabetic) {
        n = n
            .checked_mul(26)
            .and_then(|v| v.checked_add((c.to_ascii_uppercase() - b'A' + 1) as usize))
            .context("Colonne Excel invalide")?;
    }
    ensure!(
        (1..=1024).contains(&n),
        cs_model::CsError::Validation("Limite de 1024 colonnes Excel".into())
    );
    Ok(n - 1)
}
fn read_xlsx(data: &[u8]) -> Result<serde_json::Value> {
    let mut zip = zip::ZipArchive::new(Cursor::new(data))
        .context("Choisissez un fichier Excel .xlsx (ou .xlsm), ou un CSV")?;
    ensure!(
        zip.len() <= 1000,
        cs_model::CsError::Validation("Classeur Excel trop complexe".into())
    );
    let expanded: u64 = zip
        .file_names()
        .filter_map(|n| n.ok())
        .map(|s| s.into_owned())
        .collect::<Vec<_>>()
        .iter()
        .map(|n| zip.by_name(n).map(|f| f.size()).unwrap_or(0))
        .fold(0u64, u64::saturating_add);
    ensure!(
        expanded <= 50 * 1024 * 1024,
        cs_model::CsError::Validation("Classeur Excel trop volumineux après décompression".into())
    );
    let shared = if zip.by_name("xl/sharedStrings.xml").is_ok() {
        let s = xml(&mut zip, "xl/sharedStrings.xml")?;
        let doc = roxmltree::Document::parse(&s)?;
        doc.descendants()
            .filter(|n| n.has_tag_name("si"))
            .map(strings)
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    let styles = if zip.by_name("xl/styles.xml").is_ok() {
        let s = xml(&mut zip, "xl/styles.xml")?;
        let doc = roxmltree::Document::parse(&s)?;
        doc.descendants()
            .find(|n| n.has_tag_name("cellXfs"))
            .map(|n| {
                n.children()
                    .filter(|n| n.has_tag_name("xf"))
                    .map(|n| {
                        n.attribute("numFmtId")
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(0)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };
    let wb = xml(&mut zip, "xl/workbook.xml")?;
    let workbook = roxmltree::Document::parse(&wb)?;
    let date1904 = workbook
        .descendants()
        .find(|n| n.has_tag_name("workbookPr"))
        .and_then(|n| n.attribute("date1904"))
        .is_some_and(|v| v == "1" || v == "true");
    let rel = xml(&mut zip, "xl/_rels/workbook.xml.rels")?;
    let relationships = roxmltree::Document::parse(&rel)?;
    let paths: BTreeMap<_, _> = relationships
        .descendants()
        .filter(|n| n.has_tag_name("Relationship") && n.attribute("TargetMode") != Some("External"))
        .filter_map(|n| Some((n.attribute("Id")?, n.attribute("Target")?)))
        .collect();
    let mut sheets = vec![];
    let mut warnings = vec![];
    for sheet in workbook.descendants().filter(|n| n.has_tag_name("sheet")) {
        let name = sheet.attribute("name").unwrap_or("Feuille");
        let id = sheet
            .attributes()
            .find(|a| a.name() == "id")
            .map(|a| a.value())
            .context("Feuille sans relation")?;
        let target = paths
            .get(id)
            .context("Relation de feuille absente ou externe")?;
        let path = if target.starts_with("/xl/") {
            target.trim_start_matches('/').to_string()
        } else {
            format!("xl/{target}")
        };
        ensure!(
            std::path::Path::new(&path)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            cs_model::CsError::Validation("Relation Excel hors du classeur refusée".into())
        );
        let s = xml(&mut zip, &path)?;
        let doc = roxmltree::Document::parse(&s)?;
        let mut rows = vec![];
        for row in doc.descendants().filter(|n| n.has_tag_name("row")) {
            ensure!(
                rows.len() < 5000,
                cs_model::CsError::Validation(
                    "Limite de 5000 lignes par feuille pour l’import des intervenants".into()
                )
            );
            let mut cells = vec![];
            for cell in row.children().filter(|n| n.has_tag_name("c")) {
                let col = cell
                    .attribute("r")
                    .map(column)
                    .transpose()?
                    .unwrap_or(cells.len());
                ensure!(
                    col < 1024,
                    cs_model::CsError::Validation("Trop de colonnes Excel".into())
                );
                let raw = cell
                    .children()
                    .find(|n| n.has_tag_name("v"))
                    .and_then(|n| n.text())
                    .unwrap_or("");
                if cell.children().any(|n| n.has_tag_name("f")) && raw.is_empty() {
                    warnings.push(format!(
                        "{name} : formule sans résultat enregistré, cellule laissée vide"
                    ));
                }
                let mut value = match cell.attribute("t").unwrap_or("") {
                    "s" => shared
                        .get(
                            raw.parse::<usize>()
                                .context("Index de texte Excel invalide")?,
                        )
                        .context("Texte partagé Excel absent")?
                        .clone(),
                    "inlineStr" => strings(cell),
                    "b" => {
                        if raw == "1" {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                    "e" => {
                        warnings.push(format!("{name} : cellule contenant une erreur Excel"));
                        raw.into()
                    }
                    _ => raw.into(),
                };
                let format = cell
                    .attribute("s")
                    .and_then(|v| v.parse::<usize>().ok())
                    .and_then(|i| styles.get(i))
                    .copied()
                    .unwrap_or(0);
                if matches!(format, 14..=22) && !value.is_empty() {
                    if let Ok(serial) = value.parse::<f64>() {
                        if serial.is_finite() && (0.0..=2_958_465.0).contains(&serial) {
                            let origin = if date1904 {
                                chrono::NaiveDate::from_ymd_opt(1904, 1, 1)
                            } else if serial < 60.0 {
                                chrono::NaiveDate::from_ymd_opt(1899, 12, 31)
                            } else {
                                chrono::NaiveDate::from_ymd_opt(1899, 12, 30)
                            }
                            .unwrap();
                            if let Some(date) = origin
                                .checked_add_signed(chrono::Duration::days(serial.floor() as i64))
                            {
                                value = date.format("%Y-%m-%d").to_string();
                            }
                        }
                    }
                }
                if cells.len() <= col {
                    cells.resize(col + 1, String::new());
                }
                cells[col] = value;
            }
            if cells.iter().any(|v| !v.trim().is_empty()) {
                rows.push(cells);
            }
        }
        if !rows.is_empty() {
            sheets.push(json!({"name":name,"rows":rows}));
        }
    }
    ensure!(
        !sheets.is_empty(),
        cs_model::CsError::Validation("Aucune feuille contenant des données".into())
    );
    warnings.sort();
    warnings.dedup();
    Ok(json!({"sheets":sheets,"warnings":warnings}))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn xlsx_preview_reads_strings_gaps_sheets_and_cached_formulas_without_writes() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in [
            (
                "xl/workbook.xml",
                r#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="People" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Nom</t></si><si><r><t>Sophie</t></r><r><t> Martin</t></r></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row><c r="A1" t="s"><v>0</v></c><c r="C1" t="inlineStr"><is><t>Entretien</t></is></c></row><row><c r="A2" t="s"><v>1</v></c><c r="C2" t="inlineStr"><is><t>INT-01</t></is></c><c r="D2"><f>2+1</f><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let value = read_xlsx(&bytes).unwrap();
        assert_eq!(
            value["sheets"][0]["rows"][0],
            json!(["Nom", "", "Entretien"])
        );
        assert_eq!(
            value["sheets"][0]["rows"][1],
            json!(["Sophie Martin", "", "INT-01", "3"])
        );
    }
}
