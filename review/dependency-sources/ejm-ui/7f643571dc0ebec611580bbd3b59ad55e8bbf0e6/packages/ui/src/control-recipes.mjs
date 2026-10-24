import { cva } from 'class-variance-authority';

/** Fixed native buttons can use these parts without loading the variant resolver or React adapters. */
export const buttonBaseClass = 'group inline-flex items-center justify-center gap-2 font-semibold select-none touch-target focus-ring transition-transform duration-fast ease-standard motion-safe:active:scale-[0.97] disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50';
export const buttonGhostClass = 'rounded-pill whitespace-nowrap text-fg hover:bg-tone';
export const buttonMediumClass = 'min-h-11 px-6 text-base';

/** Canonical native-control styles shared by React and the static HTML profile. */
export const buttonVariants = /* @__PURE__ */ cva(
  buttonBaseClass,
  {
    variants: {
      variant: {
        primary: 'rounded-pill whitespace-nowrap bg-primary text-on-primary hover:bg-primary-hover hover:text-on-primary-hover',
        secondary: 'rounded-pill whitespace-nowrap bg-tone text-fg hover:bg-line',
        ghost: buttonGhostClass,
        link: 'rounded-control text-start text-link underline decoration-1 underline-offset-[0.2em] hover:decoration-2',
        danger: 'rounded-pill whitespace-nowrap bg-danger text-on-danger hover:bg-danger/85',
      },
      size: {
        sm: 'min-h-9 px-4 text-sm',
        md: buttonMediumClass,
        lg: 'min-h-14 px-8 text-lg',
        icon: 'size-11 p-0',
      },
      density: { comfortable: '', compact: 'rounded-compact text-[0.8125rem]' },
    },
    compoundVariants: [
      { variant: 'link', className: 'px-0' },
      // Compact: 32px tall on a fine pointer (compact-target), 44px on a coarse one (touch-target, already on every button). The icon is 32px square.
      { density: 'compact', className: 'min-h-0 px-3 compact-target' },
      { density: 'compact', size: 'icon', className: 'size-8 px-0' },
    ],
    defaultVariants: { variant: 'primary', size: 'md', density: 'comfortable' },
  },
);

/** The field look shared by Input, Textarea, Select and native HTML. */
export const fieldClass =
  'block w-full rounded-control border border-control bg-field px-3 text-base text-fg placeholder:text-muted touch-target disabled:cursor-not-allowed disabled:opacity-50 aria-[invalid=true]:border-danger';
