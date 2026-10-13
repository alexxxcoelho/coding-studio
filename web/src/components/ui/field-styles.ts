// Shared class strings for text fields, as .field in design/macos/macos.css:
// 24px high, white (dark: #3A3A3C) control with a hairline edge, no border.
export const FIELD =
  "h-[24px] w-full min-w-0 rounded-md border-0 ctl-field px-2 text-sm text-foreground transition-colors placeholder:text-label3 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:shadow-[inset_0_0_0_1px_var(--destructive)]"

export const FIELD_AREA =
  "w-full min-w-0 rounded-md border-0 ctl-field px-2 py-1.5 text-sm leading-[1.45] text-foreground placeholder:text-label3 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:shadow-[inset_0_0_0_1px_var(--destructive)]"

/** A row of a menu (.mi): 24px, accent fill and white text when highlighted. */
export const MENU_ITEM =
  "relative flex h-[24px] cursor-default items-center gap-2 rounded-[5px] px-2 text-sm outline-hidden select-none focus:bg-accent-fill focus:text-primary-foreground data-highlighted:bg-accent-fill data-highlighted:text-primary-foreground data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4"

/** The glass surface of menus, popovers and dialogs (.menu). */
export const MENU_SURFACE = "glass-menu text-popover-foreground rounded-[10px]"
