import { Slider } from '../ui/slider';

interface AppRangeProps {
  min: number;
  max: number;
  step: number;
  value: number;
  onValueChange: (value: number) => void;
  disabled?: boolean;
  'aria-label': string;
}

/** A single numeric value; callers own the roll conversion. */
export function AppRange({ value, onValueChange, ...props }: AppRangeProps) {
  return <Slider {...props} className="min-h-6" value={[value]} onValueChange={([next]) => {
    if (next !== undefined) onValueChange(next);
  }} />;
}
