import * as React from "react"
import { cn } from "cn"

import { FIELD } from "@/components/ui/field-styles"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        FIELD, "file:inline-flex file:h-5 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground",
        className
      )}
      {...props}
    />
  )
}

export { Input }
