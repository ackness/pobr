#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageType {
    Physical,
    Fire,
    Cold,
    Lightning,
    Chaos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassId {
    Marauder,
    Duelist,
    Ranger,
    Shadow,
    Witch,
    Templar,
    Scion,
}
