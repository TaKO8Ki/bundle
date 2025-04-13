#[derive(Debug, Clone)]
pub struct Gemfile {
    pub sources: Vec<Source>,
    pub gems: Vec<GemStatement>,
    pub ruby_version: Option<String>,
    pub groups: Vec<GroupBlock>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub name: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct GemStatement {
    pub name: String,
    pub version: Option<String>,
    pub options: Vec<GemOption>,
}

#[derive(Debug, Clone)]
pub struct GemOption {
    pub key: String,
    pub value: OptionValue,
}

#[derive(Debug, Clone)]
pub enum OptionValue {
    String(String),
    Boolean(bool),
    Symbol(String),
    Array(Vec<OptionValue>),
    Hash(Vec<(String, OptionValue)>),
}

#[derive(Debug, Clone)]
pub struct GroupBlock {
    pub names: Vec<String>,
    pub gems: Vec<GemStatement>,
}
