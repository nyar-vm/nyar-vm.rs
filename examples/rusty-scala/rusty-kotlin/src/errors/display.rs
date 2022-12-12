use super::*;

impl Error for KotlinError {}

impl Debug for KotlinError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.kind, f)
    }
}

impl Display for KotlinError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.kind, f)
    }
}

impl Display for KotlinErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            KotlinErrorKind::UnknownError => {
                write!(f, "UnknownError")
            }
        }
    }
}
