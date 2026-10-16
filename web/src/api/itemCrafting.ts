/** Mirrors build_api/crafting.rs. Spawn weights are ordered, including zero exclusions. */
export interface CraftingMetadata {
  bases: { id: string; name: string; item_class: string; tags: string[]; mod_domain: number }[];
  mods: { id: string; domain: number; kind: 'prefix' | 'suffix'; level: number; group: string;
    spawn_weights: { tag: string; weight: number }[] }[];
}
