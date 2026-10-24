import type { ClassProp } from 'class-variance-authority/types';

/** The same control choices accepted by the React Button. */
export interface ButtonRecipeProps {
  variant?: 'primary' | 'secondary' | 'ghost' | 'link' | 'danger' | null;
  size?: 'sm' | 'md' | 'lg' | 'icon' | null;
  density?: 'comfortable' | 'compact' | null;
}

export declare const buttonBaseClass: string;
export declare const buttonGhostClass: string;
export declare const buttonMediumClass: string;

export declare const buttonVariants: (props?: ButtonRecipeProps & ClassProp) => string;
export declare const fieldClass: string;
