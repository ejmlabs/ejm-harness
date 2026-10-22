import type { ComponentProps } from 'react';
import { cn } from '../../lib/utils';
import { fieldClass } from '../../control-recipes.mjs';
export { fieldClass } from '../../control-recipes.mjs';

/** A native text input. Pair it with a `<label>`; set `aria-invalid` and `aria-describedby` for errors. */
export function Input({ className, type = 'text', ...props }: ComponentProps<'input'>) {
  return <input type={type} className={cn(fieldClass, 'min-h-11', className)} {...props} />;
}
