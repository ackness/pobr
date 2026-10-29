import { useAnimatedNumber } from './useAnimatedNumber';

/** A number that eases to its new value. `format` receives null for an unbounded stat. */
export function AnimatedNumber({ value, format }: { value: number | null | undefined; format: (value: number | null) => string }) {
  const shown = useAnimatedNumber(value);
  return <>{format(shown ?? null)}</>;
}
