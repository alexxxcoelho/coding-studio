// Réglages: the settings of the application (appearance, reading, AI
// providers, advanced), a window-like dialog as in design/macos/Settings and
// SettingsIA. The settings of the study are a page of their own.

import type { ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { Dialog as DialogPrimitive, Tabs as TabsPrimitive } from "radix-ui"

import { GearIcon, ProviderIcon, SlidersIcon } from "@/components/icons"
import { ProvidersSection } from "@/components/settings/model-providers"
import { Button } from "@/components/ui/button"
import { FIELD } from "@/components/ui/field-styles"
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { appearance, useAppearance } from "@/lib/appearance"
import { api } from "@/lib/api"
import { ACCENTS, ACCENT_IDS, TEXT_SIZES, type AccentId, type TextSize, type ThemeChoice } from "@/lib/theme"
import { cn } from "@/lib/utils"

import type { SettingsTab } from "@/components/use-settings-dialog"

const TABS: { id: SettingsTab; label: string; Icon: typeof GearIcon }[] = [
  { id: "general", label: "Général", Icon: GearIcon },
  { id: "providers", label: "Fournisseurs IA", Icon: ProviderIcon },
  { id: "advanced", label: "Avancé", Icon: SlidersIcon },
]

export function SettingsDialog({
  open,
  onOpenChange,
  tab,
  onTabChange,
  pollMs,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  tab: SettingsTab
  onTabChange: (tab: SettingsTab) => void
  pollMs?: number
}) {
  const current = TABS.find((t) => t.id === tab) ?? TABS[0]
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showCloseButton={false}
        className="flex max-h-[min(720px,calc(100svh-3rem))] w-[680px] max-w-[calc(100%-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-[680px]"
      >
        <TabsPrimitive.Root value={tab} onValueChange={(v) => onTabChange(v as SettingsTab)} className="flex min-h-0 flex-1 flex-col">
          <div className="border-sep bg-under relative flex flex-col items-center gap-1.5 border-b-[.5px] px-3.5 pt-3 pb-2">
            <DialogTitle className="text-sm font-semibold leading-none">{`Réglages, ${current.label}`}</DialogTitle>
            <DialogDescription className="sr-only">Réglages de l'application : apparence, fournisseurs IA et options avancées.</DialogDescription>
            <TabsPrimitive.List className="flex gap-1" aria-label="Sections des réglages">
              {TABS.map(({ id, label, Icon }) => (
                <TabsPrimitive.Trigger
                  key={id}
                  value={id}
                  className="group text-label2 data-[state=active]:bg-fill2 data-[state=active]:text-foreground flex min-w-[76px] cursor-pointer flex-col items-center gap-[3px] rounded-lg border-0 bg-transparent px-2 pt-1.5 pb-[5px] text-[11.5px]"
                >
                  <span className="group-data-[state=active]:text-accent-text grid place-items-center">
                    <Icon size={22} />
                  </span>
                  {label}
                </TabsPrimitive.Trigger>
              ))}
            </TabsPrimitive.List>
            <Button variant="ghost" size="sm" className="absolute top-2.5 right-3 text-accent-text" onClick={() => onOpenChange(false)}>
              OK
            </Button>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
            <TabsPrimitive.Content value="general" className="outline-none">
              <GeneralTab />
            </TabsPrimitive.Content>
            <TabsPrimitive.Content value="providers" className="mt-5 outline-none">
              <ProvidersSection pollMs={pollMs} />
            </TabsPrimitive.Content>
            <TabsPrimitive.Content value="advanced" className="outline-none">
              <AdvancedTab onNavigate={() => onOpenChange(false)} />
            </TabsPrimitive.Content>
          </div>
        </TabsPrimitive.Root>
        <DialogPrimitive.Close className="sr-only">Fermer</DialogPrimitive.Close>
      </DialogContent>
    </Dialog>
  )
}

// --------------------------------------------------------------- Général

const THEMES: { id: ThemeChoice; label: string }[] = [
  { id: "system", label: "Automatique" },
  { id: "light", label: "Clair" },
  { id: "dark", label: "Sombre" },
]

function ThemeThumb({ id }: { id: ThemeChoice }) {
  if (id === "system")
    return (
      <span className="pv">
        <span className="half lt">
          <span className="s" />
          <span className="w" />
        </span>
        <span className="half dk">
          <span className="s" style={{ left: -41 }} />
          <span className="w" style={{ left: -24 }} />
        </span>
      </span>
    )
  return (
    <span className="pv">
      <span className={cn("half", id === "light" ? "lt" : "dk")}>
        <span className="s" />
        <span className="w" style={{ right: -6 }} />
      </span>
    </span>
  )
}

const SWATCH_BG: Record<AccentId, string> = {
  system: "conic-gradient(#FF3B30,#FF9500,#FFCC00,#28CD41,#007AFF,#AF52DE,#FF2D55,#FF3B30)",
  graphite: ACCENTS.graphite.light,
  bleu: ACCENTS.bleu.light,
  violet: ACCENTS.violet.light,
  rose: ACCENTS.rose.light,
  rouge: ACCENTS.rouge.light,
  orange: ACCENTS.orange.light,
  jaune: ACCENTS.jaune.light,
  vert: ACCENTS.vert.light,
}

function accentLabel(id: AccentId): string {
  return id === "system" ? "Système" : ACCENTS[id].label
}

function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return <button type="button" role="switch" className="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)} />
}

function Group({ title, footer, children }: { title: string; footer?: ReactNode; children: ReactNode }) {
  return (
    <>
      <div className="gh">{title}</div>
      <div className="grp">{children}</div>
      {footer && <p className="gf">{footer}</p>}
    </>
  )
}

function GeneralTab() {
  const a = useAppearance()
  return (
    <>
      <div className="gh">Apparence</div>
      <div className="grp">
        <div className="row" style={{ flexDirection: "column", alignItems: "flex-start", gap: 8 }}>
          <div className="themes" role="radiogroup" aria-label="Apparence">
            {THEMES.map((t) => (
              <button key={t.id} type="button" role="radio" aria-checked={a.theme === t.id} className="th" onClick={() => appearance.setTheme(t.id)}>
                <ThemeThumb id={t.id} />
                {t.label}
              </button>
            ))}
          </div>
        </div>
        <div className="row">
          <span className="k">
            Couleur d'accent<small>Système suit la couleur choisie dans macOS.</small>
          </span>
          <div className="sws" role="radiogroup" aria-label="Couleur d'accent">
            {ACCENT_IDS.map((id) => (
              <button
                key={id}
                type="button"
                role="radio"
                aria-checked={a.accent === id}
                aria-label={accentLabel(id)}
                title={accentLabel(id)}
                className="cs"
                style={{ background: SWATCH_BG[id] }}
                onClick={() => appearance.setAccent(id)}
              />
            ))}
          </div>
        </div>
      </div>
      <p className="gf">Automatique suit le mode clair ou sombre du système, y compris le passage automatique en soirée.</p>

      <Group title="Lecture">
        <div className="row">
          <span className="k">Taille du texte des transcriptions</span>
          <ToggleGroup
            type="single"
            value={a.textSize}
            onValueChange={(v) => v && appearance.setTextSize(v as TextSize)}
            aria-label="Taille du texte des transcriptions"
          >
            {(Object.keys(TEXT_SIZES) as TextSize[]).map((s) => (
              <ToggleGroupItem key={s} value={s}>
                {TEXT_SIZES[s].label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
        <div className="row">
          <span className="k">Afficher les raccourcis sous la transcription</span>
          <Switch checked={a.showShortcuts} onChange={(v) => appearance.setShowShortcuts(v)} label="Afficher les raccourcis" />
        </div>
      </Group>

      <Group title="Langue" footer="Le français est la seule langue de l'interface pour l'instant.">
        <label className="row">
          <span className="k">Langue de l'interface</span>
          <select className={FIELD} style={{ width: 140 }} defaultValue="fr" aria-label="Langue de l'interface">
            <option value="fr">Français</option>
          </select>
        </label>
      </Group>
    </>
  )
}

// ---------------------------------------------------------------- Avancé

function AdvancedTab({ onNavigate }: { onNavigate: () => void }) {
  const state = useQuery({ queryKey: ["state"], queryFn: api.state })
  const verify = useQuery({ queryKey: ["chain"], queryFn: api.verify })
  const s = state.data
  return (
    <>
      <Group title="Base de données" footer="Chemins en lecture seule : ils se règlent au lancement de l'application.">
        <div className="row">
          <span className="k">Base de l'étude</span>
          <span className="v mono break-all">{s?.db_path ?? "..."}</span>
        </div>
        <div className="row">
          <span className="k">Base de codage d'origine</span>
          <span className="v mono break-all">{s?.coding_db ?? "aucune"}</span>
        </div>
        <div className="row">
          <span className="k">Dossier des entretiens</span>
          <span className="v mono break-all">{s?.library_root ?? "non choisi"}</span>
        </div>
        <div className="row">
          <span className="k">Version</span>
          <span className="v">{s?.version ?? "..."}</span>
        </div>
      </Group>

      <Group title="Vérification">
        <div className="row">
          <span className="k">
            Chaîne du journal d'audit
            <small>
              {verify.data
                ? verify.data.ok
                  ? `Intacte, ${verify.data.n_events} événements.`
                  : `Rompue à l'événement ${verify.data.first_bad_seq}.`
                : verify.isError
                  ? "Serveur injoignable."
                  : "Vérification en cours."}
            </small>
          </span>
          <Button variant="outline" size="sm" onClick={() => verify.refetch()} disabled={verify.isFetching}>
            Vérifier
          </Button>
        </div>
        <div className="row">
          <span className="k">Journal d'audit</span>
          <Button variant="ghost" size="sm" className="text-accent-text" asChild>
            <a href="#/audit" onClick={onNavigate}>
              Ouvrir
            </a>
          </Button>
        </div>
      </Group>
    </>
  )
}
