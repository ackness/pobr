import { itemValueParts } from '../../lib/itemDisplay';

/** Emphasize displayed numbers without changing the localized sentence or its units. */
export function ValueText({ text }: { text: string }) {
  return <>{itemValueParts(text).map((part, index) => part.numeric
    ? <strong className="display-value" key={index}>{part.text}</strong> : part.text)}</>;
}
