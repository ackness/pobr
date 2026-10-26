import { Slider } from '../ui/slider';
import { useUi } from './UiContext';

interface AppRangeProps {
  min: number;
  max: number;
  step: number;
  value: number;
  onValueChange: (value: number) => void;
  disabled?: boolean;
  'aria-label': string;
}

/** A single numeric value in both presentations; callers own the roll conversion. */
export function AppRange({ value, onValueChange, ...props }: AppRangeProps) {
  const { v2 } = useUi();
  return v2
    ? <Slider {...props} className="min-h-6" value={[value]} onValueChange={([next]) => {
      if (next !== undefined) onValueChange(next);
    }} />
    : <input {...props} type="range" value={value} onChange={event => onValueChange(Number(event.target.value))} />;
}
