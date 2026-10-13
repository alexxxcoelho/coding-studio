//! Study configuration: everything that was thesis-specific and is now data.
//!
//! Stored by `cs-store` as JSON in the `settings` table under the key
//! `study_config`. `StudyConfig::default()` reproduces the behaviour of the
//! tool before it was generalised (TOE contexts, interview id prefixes INT,
//! ECO, RET and ADJ, 150 words per unit, prompt `CS-P04-EN v3.1`).

use serde::{Deserialize, Serialize};

use crate::phase::phase1_codebook;
use crate::{CsError, MAX_WORDS};

/// Key of the `settings` table holding the JSON form of the configuration.
pub const STUDY_CONFIG_KEY: &str = "study_config";

/// Longest accepted `description` or `coding_rules`, in characters.
pub const MAX_TEXT: usize = 20_000;

/// Smallest accepted `max_words`.
pub const MIN_MAX_WORDS: usize = 20;

/// Identifier of the prompt template shipped with the tool.
pub const DEFAULT_PROMPT_VERSION: &str = "CS-P04-EN v3.1";

/// Generic speaker labels the interviewer appears under in platform
/// transcripts. A study lists its own labels in its configuration.
pub const DEFAULT_INTERVIEWERS: &[&str] = &["Interviewer", "Intervieweur", "Intervieweuse"];

/// Interview id prefix of a new project. A study lists its own prefixes in
/// its configuration.
pub const DEFAULT_ID_PREFIXES: &[&str] = &["INT"];

/// Placeholder replaced by the number of codes offered in `codebook_intro`.
pub const CODE_COUNT_PLACEHOLDER: &str = "{count}";

/// Ready-made phase 1 category sets. Serialised in snake_case: `toe`,
/// `tam_utaut`, `none`, `custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameworkPreset {
    /// Technology, organisation, environment, plus CONTEXT.
    Toe,
    /// TAM and UTAUT constructs, plus CONTEXT.
    TamUtaut,
    /// No framework: CONTEXT only.
    None,
    /// The categories listed in `StudyConfig::phase1_categories`.
    Custom,
}

impl FrameworkPreset {
    /// Phase 1 categories of the preset, in display order (`NA` is implicit
    /// and never listed). `custom` is the user's list, returned unchanged for
    /// `Custom` and ignored by the other presets.
    pub fn categories(&self, custom: &[Category]) -> Vec<Category> {
        let contexts = || {
            phase1_codebook()
                .codes
                .into_iter()
                .map(|c| Category {
                    name: c.code,
                    label: c.label,
                    definition: c.definition,
                    attachment: c.toe,
                })
                .collect::<Vec<_>>()
        };
        match self {
            FrameworkPreset::Toe => contexts(),
            FrameworkPreset::TamUtaut => {
                let mut out: Vec<Category> = TAM_UTAUT
                    .iter()
                    .map(|(name, label, definition)| Category {
                        name: (*name).into(),
                        label: (*label).into(),
                        definition: (*definition).into(),
                        attachment: None,
                    })
                    .collect();
                out.extend(contexts().into_iter().filter(|c| c.name == "CONTEXT"));
                out
            }
            FrameworkPreset::None => contexts()
                .into_iter()
                .filter(|c| c.name == "CONTEXT")
                .collect(),
            FrameworkPreset::Custom => custom.to_vec(),
        }
    }
}

/// Constructs of TAM and UTAUT: name, label, definition.
const TAM_UTAUT: [(&str, &str, &str); 4] = [
    (
        "PERFORMANCE_EXPECTANCY",
        "Performance expectancy",
        "Beliefs that using the technology will help the unit attain gains in performance: the relative advantage of TAM and the performance expectancy of UTAUT.",
    ),
    (
        "EFFORT_EXPECTANCY",
        "Effort expectancy",
        "Degree of ease associated with using the technology: the perceived ease of use of TAM and the effort expectancy of UTAUT.",
    ),
    (
        "SOCIAL_INFLUENCE",
        "Social influence",
        "Perceived pressure from people who matter, such as colleagues, managers, partners or customers, to use or not to use the technology.",
    ),
    (
        "FACILITATING_CONDITIONS",
        "Facilitating conditions",
        "Perceived availability of the resources, support and infrastructure that make the use of the technology possible.",
    ),
];

/// One phase 1 category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    /// Code written by the candidate in phase 1, e.g. `TECH`. Serialised as
    /// `code`, like the codes of the codebook (`name` is read too).
    #[serde(rename = "code", alias = "name")]
    pub name: String,
    pub label: String,
    pub definition: String,
    /// TOE letter (`T`, `O`, `E`) that phase 2 codes attached to this
    /// category carry, or none.
    #[serde(default)]
    pub attachment: Option<String>,
}

/// Static parts of the AI prompt. The dynamic parts (header lines, codebook
/// lines, unit lines, output instructions and JSON schema) are produced by
/// `cs-ai` and cannot be removed by a template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Stored with every prompt part as `prompt_version`.
    pub version: String,
    /// Opening text: title, role and task.
    pub header: String,
    /// Sentence introducing the codebook lines.
    pub codebook_intro: String,
    /// Sentence introducing the unit lines.
    pub units_intro: String,
    /// Closing text: the numbered rules.
    pub footer: String,
}

impl Default for PromptTemplate {
    /// The text of `CS-P04-EN v3.1`, split into the four parts. The schema,
    /// meta lines, codebook lines and unit lines are added by `cs-ai`.
    fn default() -> Self {
        PromptTemplate {
            version: DEFAULT_PROMPT_VERSION.into(),
            header: [
                "# Coding Studio: blind secondary coding",
                "",
                "Detailed coding with concrete codes",
                "",
                "## Role",
                "",
                "You are a secondary coder on a qualitative study. You apply a closed codebook. You do not produce analysis, you do not build themes, and you do not interpret beyond the assignment of codes.",
                "",
                "## Task",
                "",
                "For each unit listed under UNITS, assign the code or codes of the codebook that apply. The codes name concrete mechanisms, such as a legal obligation, a customer requirement or a lack of skills. They are grouped by the context of the technology-organisation-environment (TOE) framework they belong to; the grouping is there for orientation only, and any code may apply to any unit.",
            ]
            .join("\n"),
            codebook_intro: "One code per line, as JSON with the fields code, label and definition. These {count} codes and NA are the only valid values.".into(),
            units_intro: "One unit per line, as JSON with the fields unit_id, speaker (INT = interviewer, RESP = respondent, any other label is the speaker label of the source), timestamp and text. Characters may appear as JSON escapes; read them as the characters they encode.".into(),
            footer: [
                "1. Use ONLY the codes of the codebook, written exactly as listed. You may not create, merge or rename codes.",
                "2. If no code applies, answer NA. NA is a valid and frequent answer; never force a code for the sake of completeness. NA is never combined with another code.",
                "3. A unit may carry several codes.",
                "4. Code what is SAID in the unit, not what you infer from the context of the interview. A unit that could not be coded on its own is coded NA.",
                "5. Interviewer turns (speaker INT) are coded NA by default, unless the interviewer reports a fact from the field in that turn.",
                "6. Write no comment, no justification and no memo.",
                "7. Process EVERY unit listed under UNITS, in order, without omitting any: one entry per unit, no unit twice, no unit that is not listed.",
            ]
            .join("\n"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyConfig {
    pub framework: FrameworkPreset,
    pub phase1_categories: Vec<Category>,
    pub id_prefixes: Vec<String>,
    /// Speaker labels treated as the interviewer when detecting roles.
    pub interviewers: Vec<String>,
    pub max_words: usize,
    pub prompt_template: PromptTemplate,
    pub study_name: String,
    /// What the study is about, in the researcher's words. Feeds the AI prompt
    /// and the Write step. Independent of `prompt_template`.
    #[serde(default)]
    pub description: String,
    /// The coding rules of the study (what counts as a unit of meaning, when to
    /// use several codes...). Independent of `prompt_template`.
    #[serde(default)]
    pub coding_rules: String,
}

impl Default for StudyConfig {
    /// Equal to the behaviour of the tool before it was generalised.
    fn default() -> Self {
        StudyConfig {
            framework: FrameworkPreset::Toe,
            phase1_categories: FrameworkPreset::Toe.categories(&[]),
            id_prefixes: DEFAULT_ID_PREFIXES.iter().map(|s| s.to_string()).collect(),
            interviewers: DEFAULT_INTERVIEWERS.iter().map(|s| s.to_string()).collect(),
            max_words: MAX_WORDS,
            prompt_template: PromptTemplate::default(),
            study_name: String::new(),
            description: String::new(),
            coding_rules: String::new(),
        }
    }
}

impl StudyConfig {
    /// Refuses empty or duplicate category names, a category named `NA`,
    /// empty or non-alphanumeric id prefixes, an empty prefix list and
    /// `max_words` below `MIN_MAX_WORDS`. Messages name the offending field.
    pub fn validate(&self) -> Result<(), CsError> {
        let invalid = |m: String| Err(CsError::Validation(m));

        let categories = self.effective_categories();
        if categories.is_empty() {
            return invalid("the phase 1 category list is empty".into());
        }
        let mut seen: Vec<&str> = Vec::new();
        for c in &categories {
            let name = c.name.trim();
            if name.is_empty() {
                return invalid("a phase 1 category name is empty".into());
            }
            if name == crate::NA {
                return invalid("NA is reserved and cannot be a phase 1 category".into());
            }
            if seen.contains(&name) {
                return invalid(format!("duplicate phase 1 category {name}"));
            }
            seen.push(name);
        }

        if self.max_words < MIN_MAX_WORDS {
            return invalid(format!(
                "max_words must be at least {MIN_MAX_WORDS}, not {}",
                self.max_words
            ));
        }

        if self.id_prefixes.is_empty() {
            return invalid("id_prefixes must list at least one prefix".into());
        }
        let mut prefixes: Vec<&str> = Vec::new();
        for p in &self.id_prefixes {
            if p.is_empty() {
                return invalid("id_prefixes contains an empty prefix".into());
            }
            if !p.chars().all(|c| c.is_ascii_alphanumeric()) {
                return invalid(format!(
                    "id_prefixes: {p:?} must contain only letters and digits"
                ));
            }
            if prefixes.contains(&p.as_str()) {
                return invalid(format!("id_prefixes: duplicate prefix {p}"));
            }
            prefixes.push(p);
        }

        for (what, text) in [
            ("description", &self.description),
            ("coding_rules", &self.coding_rules),
        ] {
            if text.chars().count() > MAX_TEXT {
                return invalid(format!("{what} is longer than {MAX_TEXT} characters"));
            }
        }

        if self.prompt_template.version.trim().is_empty() {
            return invalid("the prompt template version is empty".into());
        }
        Ok(())
    }

    /// Phase 1 categories in force: the preset's for `Toe`, `TamUtaut` and
    /// `None`, the stored list for `Custom`.
    pub fn effective_categories(&self) -> Vec<Category> {
        self.framework.categories(&self.phase1_categories)
    }

    /// Moves the text a configuration saved before `description` and
    /// `coding_rules` existed kept in the prompt template: a customised
    /// `header` becomes the description and a customised `footer` the coding
    /// rules, each only when the new field is empty. Text equal to the shipped
    /// prompt is not copied. The prompt template itself is not touched.
    pub fn migrate_legacy_text(&mut self) {
        let shipped = PromptTemplate::default();
        if self.description.trim().is_empty()
            && !self.prompt_template.header.trim().is_empty()
            && self.prompt_template.header != shipped.header
        {
            self.description = self.prompt_template.header.clone();
        }
        if self.coding_rules.trim().is_empty()
            && !self.prompt_template.footer.trim().is_empty()
            && self.prompt_template.footer != shipped.footer
        {
            self.coding_rules = self.prompt_template.footer.clone();
        }
    }

    /// Copy whose `phase1_categories` equals `effective_categories()`.
    pub fn normalised(&self) -> StudyConfig {
        StudyConfig {
            phase1_categories: self.effective_categories(),
            ..self.clone()
        }
    }

    /// Pattern `^(P1|P2|...)-?(\d+)` built from `id_prefixes`: group 1 is the
    /// prefix, group 2 the number. Prefixes are escaped.
    pub fn id_regex(&self) -> regex::Regex {
        let alternatives: Vec<String> = self.id_prefixes.iter().map(|p| regex::escape(p)).collect();
        regex::Regex::new(&format!(r"^({})-?(\d+)", alternatives.join("|")))
            .expect("escaped prefixes always form a valid pattern")
    }
}
