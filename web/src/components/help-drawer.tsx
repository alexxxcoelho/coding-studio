// The "?" button of the toolbar opens this right-hand drawer with the part of
// the guide that docs/guide/guide.json maps to the current screen.

import { GuideArticle } from "@/components/guide-article"
import { Button } from "@/components/ui/button"
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from "@/components/ui/sheet"
import { t } from "@/i18n"
import { GUIDE_SECTIONS, helpTarget, sectionSource } from "@/lib/guide"

export function HelpDrawer({ open, onOpenChange, screen }: { open: boolean; onOpenChange: (open: boolean) => void; screen: string }) {
  const target = helpTarget(screen)
  const section = GUIDE_SECTIONS.find((s) => s.id === target.section)
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[480px] gap-0 p-0 data-[side=right]:sm:max-w-[480px]" data-help-anchor={target.anchor}>
        <SheetHeader className="border-sep border-b-[.5px] pr-12">
          <SheetTitle>{t("help_drawer.aide")}</SheetTitle>
          <SheetDescription>{section ? t("help_drawer.title_tire_du_guide_de", { title: section.title }) : t("help_drawer.guide_de_coding_studio")}</SheetDescription>
        </SheetHeader>
        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <GuideArticle source={sectionSource(target.section)} scrollTo={target.anchor} onNavigate={() => onOpenChange(false)} className="drawer" />
        </div>
        <div className="border-sep flex justify-end border-t-[.5px] p-3">
          <Button asChild variant="outline">
            <a href={`#/guide/${target.anchor}`} onClick={() => onOpenChange(false)}>
              {t("help_drawer.ouvrir_dans_le_guide")}
            </a>
          </Button>
        </div>
      </SheetContent>
    </Sheet>
  )
}
