import * as React from "react"
import { cn } from "cn"

import { FIELD_AREA } from "@/components/ui/field-styles"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        FIELD_AREA, "flex field-sizing-content min-h-[60px]",
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
