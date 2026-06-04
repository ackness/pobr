use super::{
    BreakdownTable, CalcError, Env, MinimalInput, OutputTable, calc_defence, calculate_minimal,
};

pub fn perform(env: &mut Env) -> Result<(), CalcError> {
    if env.player.level == 0 {
        return Err(CalcError::InvalidActorState(
            "player level must be greater than 0",
        ));
    }

    let mut input = MinimalInput::from(env.player.base);
    input.enemy_evasion = env.enemy.base.evasion;
    let output = calculate_minimal(&env.player.mod_db, &env.cfg, &input);
    env.player.output = OutputTable::from(&output);
    env.player.breakdown = BreakdownTable::from_steps(output.breakdown);
    calc_defence(&mut env.player, &env.cfg, env.enemy.base.accuracy);

    Ok(())
}
