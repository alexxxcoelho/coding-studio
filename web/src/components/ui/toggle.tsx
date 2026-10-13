"use client"

import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "cn"
import { Toggle as TogglePrimitive } from "radix-ui"

const toggleVariants = cva(
  "group/toggle inline-flex cursor-pointer items-center justify-center gap-1 rounded-[5px] border-0 text-[12.5px] font-normal whitespace-nowrap text-foreground transition-colors outline-none hover:bg-fill disabled:pointer-events-none disabled:opacity-50 aria-invalid:shadow-[0_0_0_1px_var(--destructive)] data-[state=on]:ctl data-[state=on]:font-medium data-[state=on]:hover:bg-ctl [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-3.5",
  {
    variants: {
      variant: {
        default: "bg-transparent",
        outline: "bg-transparent",
      },
      size: {
        default: "h-[22px] min-w-[22px] px-2.5",
        sm: "h-[22px] min-w-[22px] px-2.5",
        lg: "h-[26px] min-w-[26px] px-3",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Toggle({
  className,
  variant = "default",
  size = "default",
  ...props
}: React.ComponentProps<typeof TogglePrimitive.Root> &
  VariantProps<typeof toggleVariants>) {
  return (
    <TogglePrimitive.Root
      data-slot="toggle"
      className={cn(toggleVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Toggle, toggleVariants }
