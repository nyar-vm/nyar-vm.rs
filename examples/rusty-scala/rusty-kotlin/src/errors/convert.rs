use super::*;

impl From<KotlinErrorKind> for KotlinError {
    fn from(value: KotlinErrorKind) -> Self {
        Self {
            kind: Box::new(value),
        }
    }
}
