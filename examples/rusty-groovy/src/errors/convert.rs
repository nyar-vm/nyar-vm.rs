use super::*;

impl From<GroovyErrorKind> for GroovyError {
    fn from(value: GroovyErrorKind) -> Self {
        Self {
            kind: Box::new(value),
        }
    }
}
