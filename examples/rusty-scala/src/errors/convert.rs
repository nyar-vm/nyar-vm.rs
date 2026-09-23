use super::*;

impl From<ScalaErrorKind> for ScalaError {
    fn from(value: ScalaErrorKind) -> Self {
        Self {
            kind: Box::new(value),
        }
    }
}
