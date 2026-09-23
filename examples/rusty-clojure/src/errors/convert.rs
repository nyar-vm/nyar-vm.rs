use super::*;

impl From<ClojureErrorKind> for ClojureError {
    fn from(value: ClojureErrorKind) -> Self {
        Self {
            kind: Box::new(value),
        }
    }
}
