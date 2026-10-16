/// Every PoE2 utility slot, including magic base-name lookup and local property scope.
pub fn code() -> String {
    let mut items = String::new();
    let mut slots = String::new();
    for (index, (slot, base)) in [
        ("Flask 1", "Ultimate Life Flask"),
        ("Flask 2", "Ultimate Mana Flask"),
        ("Charm 1", "Ruby Charm"),
        ("Charm 2", "Sapphire Charm"),
        ("Charm 3", "Silver Charm"),
    ]
    .iter()
    .enumerate()
    {
        let id = index + 1;
        items.push_str(&format!("<Item id=\"{id}\">Rarity: MAGIC\nAmple {base} of the Walrus\nQuality: +20%\nImplicits: 0\n40% increased Charges\n20% reduced Charges per use\n</Item>"));
        slots.push_str(&format!(
            "<Slot name=\"{slot}\" itemId=\"{id}\" active=\"true\"/>"
        ));
    }
    pobr_build::encode_pob_code(&format!("<PathOfBuilding2><Build level=\"80\" className=\"Witch\" ascendClassName=\"None\"/><Items activeItemSet=\"1\">{items}<ItemSet id=\"1\">{slots}</ItemSet></Items></PathOfBuilding2>")).unwrap()
}
