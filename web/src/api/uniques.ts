export interface UniqueEntry {
  id: number;
  name: string;
  item_type: string;
  raw: string;
  bases: { name: string; item_class: string }[];
}
export interface UniqueSelection {
  id: number;
  variant?: number;
  alts: number[];
  version?: number;
  groups: Record<number, number>;
  rolls: Record<number, number>;
}
export interface UniqueDraft { text: string; base: string; required_level: number | null }
