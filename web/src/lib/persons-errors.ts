// The refusals of the register (400 and 409) in plain words. The server's
// messages are exact but technical; each known one is explained in terms of
// the screen (a name, an alias, a pseudonym, a group). An unknown message is
// shown as the server wrote it.

import { t } from "@/i18n"
import { ApiError } from "./api"

type Rule = [RegExp, (m: RegExpMatchArray) => string]

const RULES: Rule[] = [
  [
    /^the name "(.*)" is used by (.+) and by (.+)$/s,
    (m) => t("persons.err_name_twice", { name: m[1], a: owner(m[2]), b: owner(m[3]) }),
  ],
  [
    /^the pseudonym "(.*)" is used by (.+) and by (.+)$/s,
    (m) => t("persons.err_pseudonym_twice", { pseudonym: m[1], a: owner(m[2]), b: owner(m[3]) }),
  ],
  [
    /^the pseudonym "(.*)" of (.+) contains a listed name or alias$/s,
    (m) => t("persons.err_pseudonym_contains_name", { pseudonym: m[1], who: owner(m[2]) }),
  ],
  [
    /^(P-\d+) is not anonymised but its name "(.*)" contains a name or alias that another record replaces$/s,
    (m) => t("persons.err_unflagged_name_replaced", { id: m[1], name: m[2] }),
  ],
  [/^a group named "(.*)" already exists at this level$/s, (m) => t("persons.err_group_twice", { name: m[1] })],
  [/^(G-\d+) cannot be moved under (G-\d+): it would create a cycle$/, () => t("persons.err_group_cycle")],
  [/^(G-\d+) is (G-\d+) or below it: choose another group to reassign to$/, () => t("persons.err_reassign_below")],
  [/^group (G-\d+) is not empty \((\d+) sub-group\(s\), (\d+) person\(s\)\)/, (m) => t("persons.err_group_not_empty", { groups: m[2], persons: m[3] })],
  [/^(P-\d+) is linked to (\d+) interview\(s\)/, (m) => t("persons.err_person_linked", { n: m[2] })],
  [/^the name of a group is empty$/, () => t("persons.err_group_empty")],
  [/^the name of a group has at most 200 characters/, () => t("persons.err_group_name_chars")],
  [/^a record needs a name$/, () => t("persons.err_needs_name")],
  [/^the name has a line break$/, () => t("persons.err_name_break")],
  [/^the kind of a record cannot change$/, () => t("persons.err_kind_fixed")],
  [/^email "(.*)" has no @$/s, (m) => t("persons.err_email", { email: m[1] })],
  [/^alias "(.*)" is shorter than (\d+) characters/s, (m) => t("persons.err_alias_short", { alias: m[1], n: m[2] })],
  [/not a date|valid date|YYYY-MM-DD/i, () => t("persons.err_date")],
]

/** "P-0003" stays an id, "the manual list" reads as such. */
function owner(raw: string): string {
  return raw === "the manual list" ? t("persons.owner_manual_list") : raw
}

/** The rules for the problems and warnings of a CSV import (one message per row). */
const IMPORT_RULES: Rule[] = [
  [/^the file is empty$/, () => t("persons.imp_empty")],
  [/^the header has no name column/, () => t("persons.imp_no_name_column")],
  [/^the name is empty$/, () => t("persons.imp_name_empty")],
  [/^"(.*)" already appears at row (\d+)$/s, (m) => t("persons.imp_twice", { name: m[1], row: m[2] })],
  [/^unknown person_id (.+)$/, (m) => t("persons.imp_unknown_id", { id: m[1] })],
  [/^anonymise must be yes or no/, () => t("persons.imp_anonymise")],
  [/^unknown interview (.+)$/, (m) => t("persons.imp_unknown_interview", { id: m[1] })],
  [/^kind must be person or organisation/, () => t("persons.imp_kind")],
  [/^column "(.*)" is ignored$/s, (m) => t("persons.imp_column_ignored", { column: m[1] })],
]

function explain(message: string, rules: Rule[]): string | null {
  for (const [re, fn] of rules) {
    const m = message.match(re)
    if (m) {
      const text = fn(m)
      if (text) return text
    }
  }
  return null
}

/** The explanation of a refusal of the register, or the server's own text. */
export function plainError(e: unknown): string {
  if (!(e instanceof ApiError)) return String(e)
  return explain(e.message, RULES) ?? e.message
}

/** A problem or a warning of a CSV import, in plain words (the rules of the register apply to a row as well). */
export function plainImportMessage(message: string): string {
  return explain(message, IMPORT_RULES) ?? explain(message, RULES) ?? message
}
