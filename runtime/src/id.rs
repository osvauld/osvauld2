use std::{borrow::Borrow, ops::Deref, sync::Arc};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Id(Arc<str>);
impl From<&str> for Id {
    fn from(s: &str) -> Self {
        Id(Arc::from(s))
    }
}
impl From<String> for Id {
    fn from(s: String) -> Self {
        Id(Arc::from(s))
    }
}
impl Borrow<str> for Id {
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl Deref for Id {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
