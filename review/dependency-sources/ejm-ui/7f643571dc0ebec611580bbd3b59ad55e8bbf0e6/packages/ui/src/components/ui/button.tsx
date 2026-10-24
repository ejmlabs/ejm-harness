import { Slot, Slottable } from '@radix-ui/react-slot';
import type { VariantProps } from 'class-variance-authority';
import { buttonVariants } from '../../control-recipes.mjs';
import { ArrowRight } from 'lucide-react';
import type { ComponentProps } from 'react';
import type { Density } from '../../lib/density';
import { cn } from '../../lib/utils';

export { buttonVariants } from '../../control-recipes.mjs';

/**
 * Pill button. primary is the Sage fill (Ink text), secondary a quiet tonal pill, ghost has no fill until hover, link reads as text.
 * danger is the red fill for a destructive action; size icon is a square for an icon-only button (give it an aria-label); density compact is for
 * applications. Pills stay on one line; a link wraps, so a long label does not overflow a 320px screen.
 * Only transform moves: the arrow slides 4px on hover and the whole button sinks a hair while pressed.
 */
export interface ButtonProps extends ComponentProps<'button'>, Omit<VariantProps<typeof buttonVariants>, 'density'> {
  /** `compact` for applications: 5px radius, 13px type, 32px target on a fine pointer (44px on touch). Default `comfortable`. */
  density?: Density;
  /** Render the child element (a link, say) with the button's look instead of a `<button>`. */
  asChild?: boolean;
  /** Add the trailing arrow. It moves 4px on hover and focus, and stays put under reduced motion. */
  arrow?: boolean;
}

export function Button({ className, variant, size, density, asChild = false, arrow = false, type, children, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : 'button';
  return (
    <Comp className={cn(buttonVariants({ variant, size, density }), className)} type={asChild ? undefined : (type ?? 'button')} {...props}>
      <Slottable>{children}</Slottable>
      {arrow && <ArrowRight aria-hidden className="size-4 shrink-0 transition-transform duration-fast ease-standard motion-safe:group-hover:translate-x-1 motion-safe:group-focus-visible:translate-x-1" />}
    </Comp>
  );
}
