// The guide: docs/guide/fr rendered in the app. Sommaire at the left (sections,
// then the headings of the open section), the text, and "Sur cette page" at the
// right. #/guide/<anchor> opens the section that owns the anchor and scrolls to it.

import { GuideArticle } from "@/components/guide-article"
import { ToolbarTitle } from "@/components/toolbar"
import { t } from "@/i18n"
import { GUIDE_SECTIONS, sectionForAnchor, sectionSource } from "@/lib/guide"
import { cn } from "@/lib/utils"

export function GuidePage({ anchor }: { anchor: string | null }) {
  const section = (anchor && sectionForAnchor(anchor)) || GUIDE_SECTIONS[0]
  const target = anchor ?? section.anchor
  const headings = section.headings.filter((h) => h.level === 2)
  return (
    <div className="-mx-5 flex flex-wrap">
      <ToolbarTitle title={t("common.guide")} sub={section.title} />
      <nav className="border-sep sticky top-[60px] flex max-h-[calc(100svh-70px)] min-w-0 flex-[0_1_250px] flex-col gap-px self-start overflow-y-auto border-r-[.5px] px-2.5 pb-3.5" aria-label={t("guide.sommaire")}>
        {GUIDE_SECTIONS.map((s, i) => (
          <div key={s.id} className="flex flex-col gap-px">
            <a
              href={`#/guide/${s.anchor}`}
              aria-current={s === section ? "page" : undefined}
              className={cn(
                "flex min-h-[26px] items-center gap-1.5 rounded-md px-2 text-[13px] no-underline",
                s === section && target === s.anchor ? "bg-accent-fill text-primary-foreground" : "text-foreground hover:bg-fill",
              )}
            >
              <span className={cn("w-3.5 tabular-nums", s === section && target === s.anchor ? "text-primary-foreground/80" : "text-label2")}>{i === 0 ? "" : i}</span>
              {s.title}
            </a>
            {s === section &&
              headings.map((h) => (
                <a
                  key={h.anchor}
                  href={`#/guide/${h.anchor}`}
                  className={cn(
                    "flex min-h-[26px] items-center rounded-md pr-2 pl-7 text-[12.5px] no-underline",
                    target === h.anchor ? "bg-accent-fill text-primary-foreground" : "text-label2 hover:bg-fill",
                  )}
                >
                  {h.title}
                </a>
              ))}
          </div>
        ))}
      </nav>
      <article className="flex min-w-0 flex-[999_1_520px] justify-center px-10 pt-2 pb-9">
        <GuideArticle key={section.id} source={sectionSource(section.id)} scrollTo={anchor} className="w-full max-w-[700px]" />
      </article>
      <nav className="sticky top-[60px] flex min-w-0 flex-[0_1_200px] flex-col gap-1.5 self-start px-4 py-3 text-xs" aria-label={t("guide.sur_cette_page")}>
        <span className="hs">{t("guide.sur_cette_page")}</span>
        {headings.map((h) => (
          <a key={h.anchor} href={`#/guide/${h.anchor}`} className={cn("no-underline", target === h.anchor ? "text-foreground font-semibold" : "text-label2")}>
            {h.title}
          </a>
        ))}
      </nav>
    </div>
  )
}
