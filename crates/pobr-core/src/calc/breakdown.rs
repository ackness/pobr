#[derive(Debug, Clone, PartialEq)]
pub struct BreakdownStep {
    pub name: &'static str,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BreakdownTable {
    steps: Vec<BreakdownStep>,
}

impl BreakdownTable {
    pub fn from_steps(steps: Vec<BreakdownStep>) -> Self {
        Self { steps }
    }

    pub fn push(&mut self, name: &'static str, value: f64) {
        self.steps.push(BreakdownStep { name, value });
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.steps.iter().any(|step| step.name == name)
    }

    pub fn steps(&self) -> &[BreakdownStep] {
        &self.steps
    }
}
