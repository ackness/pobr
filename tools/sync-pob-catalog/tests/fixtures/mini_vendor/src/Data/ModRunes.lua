return {
    ["Legacy Rune"] = {
        weapon = {
            type = "Rune",
            "Adds 7 to 11 Fire Damage",
            "Bonded: 30% increased Ignite Magnitude",
            statOrder = { 832, 1077 },
            rank = { 15 },
        },
    },
    ["Nested Rune"] = {
        weapon = {
            type = "Rune",
            "Adds 7 to 11 Fire Damage",
            statOrder = { 832 },
            bonded = {
                "30% increased Ignite Magnitude",
                statOrder = { 1077 },
            },
            levelReq = 15,
        },
    },
}
