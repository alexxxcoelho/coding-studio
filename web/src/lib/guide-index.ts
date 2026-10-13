// guide.json of docs/guide, typed, in the two languages of the interface: the
// French guide is the top level of the file, the English one is under
// languages.en (same section ids, anchors and screens, translated titles and
// files). Kept apart from lib/guide.ts so that the markdown renderer and the
// guide map do not import each other.

import guide from "@guide/guide.json"

import { getLang, type Lang } from "@/i18n"

export interface GuideHeadingRef {
  level: number
  title: string
  anchor: string
}

export interface GuideSection {
  id: string
  title: string
  file: string
  anchor: string
  headings: GuideHeadingRef[]
}

export interface GuideScreen {
  section: string
  file: string
  anchor: string
}

interface GuideData {
  title: string
  help_button: string
  sections: GuideSection[]
  screens: Record<string, GuideScreen>
}

const DATA: Record<Lang, GuideData> = {
  fr: guide as unknown as GuideData,
  en: guide.languages.en as unknown as GuideData,
}

/** Title of the guide, its sections and its screen map, in `lang` (the current language by default). */
export function guideTitle(lang: Lang = getLang()): string {
  return DATA[lang].title
}
export function guideSections(lang: Lang = getLang()): GuideSection[] {
  return DATA[lang].sections
}
export function guideScreens(lang: Lang = getLang()): Record<string, GuideScreen> {
  return DATA[lang].screens
}
