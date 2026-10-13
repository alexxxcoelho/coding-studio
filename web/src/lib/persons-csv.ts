// The template of the CSV import of Personnes: the header row the server reads.

const BASE_COLUMNS = [
  "person_id",
  "name",
  "kind",
  "organisation",
  "function",
  "email",
  "phone",
  "notes",
  "aliases",
  "groups",
  "anonymise",
  "pseudonym",
  "interviews",
  "consent_status",
  "consent_date",
  "consent_document",
]

/** The header row of the template, then one example line. */
export function templateCsv(attributeIds: string[]): string {
  const header = [...BASE_COLUMNS, ...attributeIds.map((id) => `attr:${id}`)]
  return `${header.join(",")}\n`
}
