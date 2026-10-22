import { useId, type ComponentProps, type ReactNode } from 'react';
import type { Density } from '../../lib/density';
import { cn } from '../../lib/utils';

export interface RadioOption {
  value: string;
  label: ReactNode;
  description?: ReactNode;
  disabled?: boolean;
}

export interface RadioGroupProps extends Omit<ComponentProps<'fieldset'>, 'children' | 'name' | 'onChange'> {
  label: ReactNode;
  name: string;
  value: string;
  onValueChange: (value: string) => void;
  options: readonly RadioOption[];
  required?: boolean;
  density?: Density;
}

/** Native radios share a form name, browser validation and arrow-key behavior. The label is the whole touch target. */
export function RadioGroup({ label, name, value, onValueChange, options, required, disabled, density = 'comfortable', className, ...props }: RadioGroupProps) {
  const uid = useId();
  return (
    <fieldset role="radiogroup" disabled={disabled} className={cn('m-0 min-w-0 border-0 p-0', className)} {...props}>
      <legend className="mb-3 text-meta text-muted">{label}</legend>
      <div className="flex flex-wrap gap-x-6 gap-y-2">
        {options.map((option, index) => {
          const id = `${uid}-${index}`;
          const unavailable = disabled || option.disabled;
          return (
            <label key={option.value} htmlFor={id} className={cn('inline-flex cursor-pointer items-center gap-3 rounded-control touch-target', density === 'compact' ? 'min-h-0 compact-target text-sm' : 'min-h-11 text-base', unavailable && 'cursor-not-allowed opacity-50')}>
              <input
                id={id}
                type="radio"
                name={name}
                value={option.value}
                checked={value === option.value}
                onChange={event => { if (event.currentTarget.checked) onValueChange(event.currentTarget.value); }}
                required={required}
                disabled={unavailable}
                aria-labelledby={`${id}-label`}
                aria-describedby={option.description ? `${id}-description` : undefined}
                className="size-4 shrink-0 accent-primary focus-ring"
              />
              <span className="min-w-0">
                <span id={`${id}-label`} className="block font-semibold">{option.label}</span>
                {option.description && <span id={`${id}-description`} className="mt-0.5 block text-sm text-muted">{option.description}</span>}
              </span>
            </label>
          );
        })}
      </div>
    </fieldset>
  );
}
