//! Project configuration of V1 lot 2: interview attributes, familiarisation
//! memo templates, and the editable lists (end-of-interview checklist, theme
//! checks, evidence natures, link types, matrix dimensions, interpretive
//! framework). Pure data and validation; storage lives in `cs-store`.
//!
//! Everything here is versioned and audited as part of the study
//! configuration. None of it enters the coding screen or the codebook: the
//! interpretive framework in particular is for Analyse and Write only.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::CsError;

/// Settings key of the project configuration (next to `study_config`).
pub const PROJECT_CONFIG_KEY: &str = "project_config";

/// Version of the project template JSON format.
pub const TEMPLATE_FORMAT_VERSION: u32 = 1;

/// Id of the default memo template.
pub const DEFAULT_MEMO_TEMPLATE: &str = "default";

const MAX_LABEL: usize = 200;

fn invalid<T>(msg: String) -> Result<T, CsError> {
    Err(CsError::Validation(msg))
}

/// One entry of an editable list: a stable id and a label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub id: String,
    pub label: String,
}

fn item(id: &str, label: &str) -> ListItem {
    ListItem {
        id: id.into(),
        label: label.into(),
    }
}

/// A dimension of the interpretive framework.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dimension {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub definition: String,
}

/// An interpretive framework (Analyse and Write only). `preset` is `toe`,
/// `tam_utaut` or `custom`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Framework {
    pub preset: String,
    pub dimensions: Vec<Dimension>,
}

/// Names of the framework presets.
pub const FRAMEWORK_PRESETS: [&str; 2] = ["toe", "tam_utaut"];

fn dim(id: &str, label: &str, definition: &str) -> Dimension {
    Dimension {
        id: id.into(),
        label: label.into(),
        definition: definition.into(),
    }
}

/// The dimensions of a preset, `None` for an unknown name.
pub fn framework_preset(name: &str) -> Option<Framework> {
    let dimensions = match name {
        "toe" => vec![
            dim(
                "technologie",
                "Technologie",
                "Caractéristiques de la technologie et de son adéquation à l'organisation.",
            ),
            dim(
                "organisation",
                "Organisation",
                "Taille, ressources, compétences, structure et culture de l'organisation.",
            ),
            dim(
                "environnement",
                "Environnement",
                "Partenaires, concurrence, réglementation et soutien extérieur.",
            ),
        ],
        "tam_utaut" => vec![
            dim(
                "utilite",
                "Utilité perçue",
                "Croyance que l'usage améliore la performance (TAM).",
            ),
            dim(
                "facilite",
                "Facilité d'utilisation perçue",
                "Croyance que l'usage demande peu d'effort (TAM).",
            ),
            dim(
                "performance",
                "Attentes de performance",
                "Gain attendu dans le travail (UTAUT).",
            ),
            dim(
                "effort",
                "Attentes d'effort",
                "Facilité attendue de l'usage (UTAUT).",
            ),
            dim(
                "influence",
                "Influence sociale",
                "Poids des attentes de l'entourage (UTAUT).",
            ),
            dim(
                "conditions",
                "Conditions facilitatrices",
                "Moyens et soutien disponibles pour utiliser (UTAUT).",
            ),
        ],
        _ => return None,
    };
    Some(Framework {
        preset: name.into(),
        dimensions,
    })
}

/// Type of an interview attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeKind {
    Text,
    Choice,
    Number,
    Date,
}

/// Definition of an interview attribute (sector, role, size...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributeDef {
    pub id: String,
    pub label: String,
    pub kind: AttributeKind,
    /// Values of a `choice` attribute, in display order.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Whether the value may go to the model (prompt placeholder).
    #[serde(default)]
    pub shared_with_model: bool,
}

impl AttributeDef {
    /// Validates and normalises a value; `Ok(None)` for an empty value.
    pub fn check_value(&self, raw: &str) -> Result<Option<String>, CsError> {
        let v = raw.trim();
        if v.is_empty() {
            return Ok(None);
        }
        let bad = |why: &str| {
            Err(CsError::Validation(format!(
                "attribute {}: {why} (got {v:?})",
                self.id
            )))
        };
        match self.kind {
            AttributeKind::Text => {
                if v.chars().count() > 2000 {
                    return bad("text longer than 2000 characters");
                }
                Ok(Some(v.to_string()))
            }
            AttributeKind::Choice => {
                if self.choices.iter().any(|c| c == v) {
                    Ok(Some(v.to_string()))
                } else {
                    bad(&format!("value not in the list {:?}", self.choices))
                }
            }
            AttributeKind::Number => match v.replace(',', ".").parse::<f64>() {
                Ok(n) if n.is_finite() => Ok(Some(v.replace(',', "."))),
                _ => bad("not a number"),
            },
            AttributeKind::Date => {
                if valid_date(v) {
                    Ok(Some(v.to_string()))
                } else {
                    bad("not a date YYYY-MM-DD")
                }
            }
        }
    }
}

/// Definition of a case (decision) field: a project-defined characteristic of
/// a decision, stored by id in `Case::characteristics` (F-87).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseFieldDef {
    pub id: String,
    pub label: String,
    pub kind: AttributeKind,
    /// Values of a `choice` field, in display order.
    #[serde(default)]
    pub choices: Vec<String>,
}

impl CaseFieldDef {
    /// Validates and normalises a value; `Ok(None)` for an empty value.
    pub fn check_value(&self, raw: &str) -> Result<Option<String>, CsError> {
        AttributeDef {
            id: self.id.clone(),
            label: self.label.clone(),
            kind: self.kind,
            choices: self.choices.clone(),
            shared_with_model: false,
        }
        .check_value(raw)
        .map_err(|e| match e {
            CsError::Validation(m) => CsError::Validation(m.replacen("attribute", "case field", 1)),
            other => other,
        })
    }
}

/// Fields of the first study, kept as legacy case fields: (id, label, kind).
/// A database that holds values in them gets these definitions on migration.
pub const LEGACY_CASE_FIELDS: [(&str, &str, AttributeKind); 8] = [
    ("decision", "Decision", AttributeKind::Text),
    ("granularity", "Granularity", AttributeKind::Text),
    ("carrier_chosen", "Carrier chosen", AttributeKind::Text),
    (
        "carriers_rejected",
        "Carriers rejected",
        AttributeKind::Text,
    ),
    ("account_type", "Account type", AttributeKind::Text),
    ("swiss_link", "Swiss link", AttributeKind::Text),
    ("stage", "Stage", AttributeKind::Text),
    ("outcome_status", "Outcome status", AttributeKind::Text),
];

/// `YYYY-MM-DD` with a real calendar day.
pub fn valid_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().ok();
    if !s.is_ascii() {
        return false;
    }
    let (Some(y), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    y >= 1 && (1..=max).contains(&d)
}

/// A question of a memo template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoQuestion {
    pub id: String,
    pub text: String,
}

/// A familiarisation memo template. `version` goes up by one each time the
/// name or the questions change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoTemplate {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default = "one")]
    pub version: u32,
    pub questions: Vec<MemoQuestion>,
}

fn one() -> u32 {
    1
}

/// A past version of a memo template, kept so that an interview keeps the
/// questions it was answered with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoSnapshot {
    pub template_id: String,
    pub version: u32,
    pub name: String,
    pub questions: Vec<MemoQuestion>,
}

/// The whole project configuration of lot 2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub attributes: Vec<AttributeDef>,
    #[serde(default = "default_templates")]
    pub memo_templates: Vec<MemoTemplate>,
    #[serde(default = "default_checklist")]
    pub checklist: Vec<ListItem>,
    #[serde(default = "default_theme_checks")]
    pub theme_checks: Vec<ListItem>,
    #[serde(default = "default_evidence_natures")]
    pub evidence_natures: Vec<ListItem>,
    #[serde(default = "default_link_types")]
    pub link_types: Vec<ListItem>,
    /// Fields of a case (decision), defined by the project (F-87). Empty: the
    /// characteristics of a case are free key/value pairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub case_fields: Vec<CaseFieldDef>,
    /// Custom matrix dimensions (for example "who imposes", "who pays").
    #[serde(default)]
    pub matrix_dimensions: Vec<ListItem>,
    /// How a passage came to be said (F-09). The earlier fixed list is the default.
    #[serde(default = "default_passage_origins")]
    pub passage_origins: Vec<ListItem>,
    /// Role of a passage in a decision (F-09). The earlier fixed list is the default.
    #[serde(default = "default_passage_roles")]
    pub passage_roles: Vec<ListItem>,
    /// None by default. Used only in Analyse and Write.
    #[serde(default)]
    pub interpretive_framework: Option<Framework>,
    /// Earlier versions of memo templates (not part of a template export).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memo_history: Vec<MemoSnapshot>,
}

pub fn default_templates() -> Vec<MemoTemplate> {
    let q = |id: &str, text: &str| MemoQuestion {
        id: id.into(),
        text: text.into(),
    };
    vec![MemoTemplate {
        id: DEFAULT_MEMO_TEMPLATE.into(),
        name: "Par défaut".into(),
        default: true,
        version: 1,
        questions: vec![
            q("q1", "Que raconte le répondant ?"),
            q("q2", "Qu'est-ce qui m'a surpris ?"),
            q("q3", "Quelles contradictions apparaissent ?"),
            q("q4", "Que demander aux entretiens suivants ?"),
        ],
    }]
}

pub fn default_checklist() -> Vec<ListItem> {
    vec![
        item("turns_seen", "Tous les tours du répondant vus"),
        item("later_handled", "Codes Plus tard traités ou assumés"),
        item("code_memos", "Mémos des nouveaux codes écrits"),
        item("end_memo", "Mémo de fin d'entretien rédigé"),
    ]
}

pub fn default_theme_checks() -> Vec<ListItem> {
    vec![
        item("coherence", "Cohérence des passages"),
        item("distinct", "Distinction des autres thèmes"),
        item("title_asserts", "Un titre qui affirme"),
        item(
            "centrality",
            "Centralité pour au moins une question de recherche",
        ),
        item(
            "observations",
            "Plusieurs observations, ou un cas unique justifié",
        ),
        item("nuance", "Des données qui nuancent"),
        item(
            "extent",
            "Des passages qui soutiennent exactement l'étendue de l'affirmation",
        ),
    ]
}

pub fn default_evidence_natures() -> Vec<ListItem> {
    vec![
        item("first_hand", "Récit de première main"),
        item("reported", "Propos rapporté"),
        item("figure_estimate", "Chiffre estimé"),
        item("quoted_document", "Document cité"),
        item("opinion", "Opinion"),
    ]
}

pub fn default_passage_origins() -> Vec<ListItem> {
    crate::analysis::PASSAGE_ORIGINS
        .iter()
        .map(|id| item(id, crate::analysis::passage_origin_label(id)))
        .collect()
}

pub fn default_passage_roles() -> Vec<ListItem> {
    crate::analysis::PASSAGE_ROLES
        .iter()
        .map(|id| item(id, crate::analysis::passage_role_label(id)))
        .collect()
}

pub fn default_link_types() -> Vec<ListItem> {
    vec![
        item("supports", "soutient"),
        item("nuances", "nuance"),
        item("conditions", "conditionne"),
        item("mechanism", "mécanisme"),
        item("exception", "exception"),
        item("contradicts", "contredit"),
    ]
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            attributes: Vec::new(),
            case_fields: Vec::new(),
            memo_templates: default_templates(),
            checklist: default_checklist(),
            theme_checks: default_theme_checks(),
            evidence_natures: default_evidence_natures(),
            link_types: default_link_types(),
            matrix_dimensions: Vec::new(),
            passage_origins: default_passage_origins(),
            passage_roles: default_passage_roles(),
            interpretive_framework: None,
            memo_history: Vec::new(),
        }
    }
}

/// `^[a-z0-9][a-z0-9_-]*$`, at most 60 characters.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    id.len() <= 60
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn check_label(what: &str, label: &str) -> Result<(), CsError> {
    let t = label.trim();
    if t.is_empty() {
        return invalid(format!("{what}: the label is empty"));
    }
    if t.chars().count() > MAX_LABEL {
        return invalid(format!(
            "{what}: the label is longer than {MAX_LABEL} characters"
        ));
    }
    if t.contains('\n') {
        return invalid(format!("{what}: the label must be on one line"));
    }
    Ok(())
}

fn check_ids<'a>(
    what: &str,
    ids: impl Iterator<Item = &'a str>,
    seen: &mut BTreeSet<String>,
) -> Result<(), CsError> {
    for id in ids {
        if !valid_id(id) {
            return invalid(format!(
                "{what}: id {id:?} must match ^[a-z0-9][a-z0-9_-]*$ (60 characters at most)"
            ));
        }
        if !seen.insert(id.to_string()) {
            return invalid(format!("{what}: duplicate id {id}"));
        }
    }
    Ok(())
}

fn check_list(what: &str, list: &[ListItem], allow_empty: bool) -> Result<(), CsError> {
    if list.is_empty() && !allow_empty {
        return invalid(format!("{what}: the list is empty"));
    }
    check_ids(
        what,
        list.iter().map(|i| i.id.as_str()),
        &mut BTreeSet::new(),
    )?;
    let mut labels = BTreeSet::new();
    for i in list {
        check_label(&format!("{what} {}", i.id), &i.label)?;
        if !labels.insert(i.label.trim().to_lowercase()) {
            return invalid(format!("{what}: duplicate label {:?}", i.label.trim()));
        }
    }
    Ok(())
}

impl ProjectConfig {
    /// Checks every part and reports the first problem, naming the element.
    /// See [`Self::problems`] for all of them at once.
    pub fn validate(&self) -> Result<(), CsError> {
        match self.problems().into_iter().next() {
            Some(p) => invalid(p),
            None => Ok(()),
        }
    }

    /// Every problem of the configuration (used by the template import, which
    /// reports them all at once).
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |r: Result<(), CsError>| {
            if let Err(CsError::Validation(m)) = r {
                out.push(m);
            }
        };
        // attributes
        let mut seen = BTreeSet::new();
        for a in &self.attributes {
            push(check_ids(
                "attribute",
                std::iter::once(a.id.as_str()),
                &mut seen,
            ));
            push(check_label(&format!("attribute {}", a.id), &a.label));
            match a.kind {
                AttributeKind::Choice => {
                    if a.choices.is_empty() {
                        push(invalid(format!(
                            "attribute {}: a choice needs values",
                            a.id
                        )));
                    }
                    let mut vals = BTreeSet::new();
                    for c in &a.choices {
                        if c.trim().is_empty() || c.trim() != c || c.contains('\n') {
                            push(invalid(format!(
                                "attribute {}: choice {c:?} is empty or has stray spaces",
                                a.id
                            )));
                        } else if !vals.insert(c.clone()) {
                            push(invalid(format!("attribute {}: duplicate choice {c}", a.id)));
                        }
                    }
                }
                _ => {
                    if !a.choices.is_empty() {
                        push(invalid(format!(
                            "attribute {}: only a choice attribute has values",
                            a.id
                        )));
                    }
                }
            }
        }
        // case fields
        let mut seen = BTreeSet::new();
        for f in &self.case_fields {
            push(check_ids(
                "case field",
                std::iter::once(f.id.as_str()),
                &mut seen,
            ));
            push(check_label(&format!("case field {}", f.id), &f.label));
            if f.kind == AttributeKind::Choice {
                if f.choices.is_empty() {
                    push(invalid(format!(
                        "case field {}: a choice needs values",
                        f.id
                    )));
                }
                let mut vals = BTreeSet::new();
                for c in &f.choices {
                    if c.trim().is_empty() || c.trim() != c || c.contains('\n') {
                        push(invalid(format!(
                            "case field {}: choice {c:?} is empty or has stray spaces",
                            f.id
                        )));
                    } else if !vals.insert(c.clone()) {
                        push(invalid(format!(
                            "case field {}: duplicate choice {c}",
                            f.id
                        )));
                    }
                }
            } else if !f.choices.is_empty() {
                push(invalid(format!(
                    "case field {}: only a choice field has values",
                    f.id
                )));
            }
        }
        // memo templates
        if self.memo_templates.is_empty() {
            push(invalid("memo templates: the list is empty".into()));
        }
        let mut seen = BTreeSet::new();
        for t in &self.memo_templates {
            push(check_ids(
                "memo template",
                std::iter::once(t.id.as_str()),
                &mut seen,
            ));
            push(check_label(&format!("memo template {}", t.id), &t.name));
            if t.version == 0 {
                push(invalid(format!(
                    "memo template {}: version starts at 1",
                    t.id
                )));
            }
            if t.questions.is_empty() {
                push(invalid(format!("memo template {}: no question", t.id)));
            }
            let mut qs = BTreeSet::new();
            for q in &t.questions {
                push(check_ids(
                    &format!("memo template {} question", t.id),
                    std::iter::once(q.id.as_str()),
                    &mut qs,
                ));
                push(check_label(
                    &format!("memo template {} question {}", t.id, q.id),
                    &q.text,
                ));
            }
        }
        let defaults = self.memo_templates.iter().filter(|t| t.default).count();
        if !self.memo_templates.is_empty() && defaults != 1 {
            push(invalid(format!(
                "memo templates: exactly one must be the default, found {defaults}"
            )));
        }
        push(check_list("checklist", &self.checklist, true));
        push(check_list("theme checks", &self.theme_checks, true));
        push(check_list("evidence natures", &self.evidence_natures, true));
        push(check_list("link types", &self.link_types, false));
        push(check_list(
            "matrix dimensions",
            &self.matrix_dimensions,
            true,
        ));
        push(check_list("passage origins", &self.passage_origins, true));
        push(check_list("passage roles", &self.passage_roles, true));
        if let Some(f) = &self.interpretive_framework {
            if f.preset != "custom" && !FRAMEWORK_PRESETS.contains(&f.preset.as_str()) {
                push(invalid(format!(
                    "framework: preset must be toe, tam_utaut or custom, not {}",
                    f.preset
                )));
            }
            if f.dimensions.is_empty() {
                push(invalid("framework: no dimension".into()));
            }
            let mut ids = BTreeSet::new();
            for d in &f.dimensions {
                push(check_ids(
                    "framework dimension",
                    std::iter::once(d.id.as_str()),
                    &mut ids,
                ));
                push(check_label(
                    &format!("framework dimension {}", d.id),
                    &d.label,
                ));
            }
        }
        out
    }

    /// The default memo template.
    pub fn default_template(&self) -> Option<&MemoTemplate> {
        self.memo_templates.iter().find(|t| t.default)
    }

    pub fn case_field(&self, id: &str) -> Option<&CaseFieldDef> {
        self.case_fields.iter().find(|f| f.id == id)
    }

    pub fn attribute(&self, id: &str) -> Option<&AttributeDef> {
        self.attributes.iter().find(|a| a.id == id)
    }

    pub fn template(&self, id: &str) -> Option<&MemoTemplate> {
        self.memo_templates.iter().find(|t| t.id == id)
    }

    /// Questions of a template at a version: the current one, or the kept
    /// snapshot.
    pub fn template_at(&self, id: &str, version: u32) -> Option<MemoSnapshot> {
        if let Some(t) = self.template(id).filter(|t| t.version == version) {
            return Some(MemoSnapshot {
                template_id: t.id.clone(),
                version: t.version,
                name: t.name.clone(),
                questions: t.questions.clone(),
            });
        }
        self.memo_history
            .iter()
            .find(|s| s.template_id == id && s.version == version)
            .cloned()
    }

    /// The next configuration: memo template versions go up for every
    /// template whose name or questions changed, and the previous version is
    /// kept in the history.
    pub fn versioned_after(&self, before: &ProjectConfig) -> ProjectConfig {
        let mut next = self.clone();
        next.memo_history = before.memo_history.clone();
        for t in &mut next.memo_templates {
            match before.template(&t.id) {
                None => t.version = t.version.max(1),
                Some(old) => {
                    if old.name != t.name || old.questions != t.questions {
                        t.version = old.version + 1;
                        if !next
                            .memo_history
                            .iter()
                            .any(|s| s.template_id == old.id && s.version == old.version)
                        {
                            next.memo_history.push(MemoSnapshot {
                                template_id: old.id.clone(),
                                version: old.version,
                                name: old.name.clone(),
                                questions: old.questions.clone(),
                            });
                        }
                    } else {
                        t.version = old.version;
                    }
                }
            }
        }
        next
    }
}

/// A research question as carried by a project template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateQuestion {
    pub id: String,
    #[serde(default)]
    pub text: String,
}

/// Value of `ProjectTemplate::format`.
pub const TEMPLATE_FORMAT: &str = "coding-studio-project-template";

/// The project template: the whole study configuration, no interview data,
/// no coding, no provider secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTemplate {
    pub format: String,
    pub version: u32,
    /// Study name, segmentation settings, phase 1 categories, prompt template.
    pub study_config: crate::StudyConfig,
    pub research_questions: Vec<TemplateQuestion>,
    pub project_config: ProjectConfig,
}
