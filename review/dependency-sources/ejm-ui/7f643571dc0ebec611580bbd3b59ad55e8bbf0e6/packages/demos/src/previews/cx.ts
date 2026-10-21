/** Joins class names, skipping false values. The views use plain strings so they do not need a class merger. */
export const cx = (...names: (string | false | null | undefined)[]): string => names.filter(Boolean).join(' ');
