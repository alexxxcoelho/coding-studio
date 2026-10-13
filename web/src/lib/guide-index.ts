// guide.json of docs/guide, typed. Kept apart from lib/guide.ts so that the
// markdown renderer and the guide map do not import each other.

import guide from "@guide/guide.json"

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

export const GUIDE_TITLE: string = guide.title
export const GUIDE_SECTIONS: GuideSection[] = guide.sections as GuideSection[]
export const GUIDE_SCREENS: Record<string, GuideScreen> = guide.screens as Record<string, GuideScreen>
