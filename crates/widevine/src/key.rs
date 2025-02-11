use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct WidevineKey {
    pub r#type: &'static str,
    pub id: String,
    pub key: String,
}

impl Display for WidevineKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}:{}", self.r#type, self.id, self.key)
    }
}
