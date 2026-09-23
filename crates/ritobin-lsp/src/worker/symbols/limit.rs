#[derive(Default)]
pub struct Limit {
    pub max: Option<usize>,
    pub value: usize,
}

impl Limit {
    pub fn update(&mut self, value: usize) -> bool {
        self.value = value;
        self.exceeded()
    }
    pub fn exceeded(&self) -> bool {
        self.max.is_some_and(|max| self.value >= max)
    }
    pub fn report(&self) -> Option<(usize, bool)> {
        self.max.map(|max| (max, self.exceeded()))
    }
}
