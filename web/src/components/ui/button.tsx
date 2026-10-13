import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"
import { Slot } from "radix-ui"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 cursor-pointer items-center justify-center rounded-md border-0 text-sm font-normal whitespace-nowrap no-underline transition-colors select-none active:not-aria-[haspopup]:opacity-80 disabled:pointer-events-none disabled:opacity-50 aria-invalid:shadow-[0_0_0_1px_var(--destructive)] [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        // .btn.def: the default action, accent fill and white text.
        default: "bg-accent-fill text-primary-foreground shadow-[0_.5px_1.5px_rgba(0,0,0,.22)] hover:brightness-110",
        // .btn: neutral control.
        outline: "ctl text-foreground hover:brightness-[.97] aria-expanded:brightness-[.95] dark:hover:brightness-110",
        secondary: "ctl text-foreground hover:brightness-[.97] aria-expanded:brightness-[.95] dark:hover:brightness-110",
        // .btn.plain: text in the accent colour, no surface.
        ghost: "bg-transparent text-foreground hover:bg-fill aria-expanded:bg-fill",
        destructive: "bg-destructive/10 text-destructive hover:bg-destructive/20",
        link: "text-accent-text underline-offset-4 hover:underline",
      },
      size: {
        default: "h-[24px] gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        xs: "h-[20px] gap-1 rounded-[5px] px-2 text-xs [&_svg:not([class*='size-'])]:size-3",
        sm: "h-[22px] gap-1 rounded-[5px] px-2 text-[12.5px] [&_svg:not([class*='size-'])]:size-3.5",
        // .btn.lg: the 30px capsule of toolbars and dialogs.
        lg: "h-[30px] gap-1.5 rounded-full px-3.5",
        icon: "size-[24px]",
        "icon-xs": "size-[20px] rounded-[5px] [&_svg:not([class*='size-'])]:size-3",
        "icon-sm": "size-[22px] rounded-[5px]",
        "icon-lg": "size-[30px] rounded-full",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  asChild = false,
  ...props
}: React.ComponentProps<"button"> &
  VariantProps<typeof buttonVariants> & {
    asChild?: boolean
  }) {
  const Comp = asChild ? Slot.Root : "button"

  return (
    <Comp
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
