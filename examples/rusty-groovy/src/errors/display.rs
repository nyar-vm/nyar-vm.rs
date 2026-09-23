use super::*;

impl Error for GroovyError {}

impl Debug for GroovyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.kind, f)
    }
}

impl Display for GroovyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.kind, f)
    }
}

impl Display for GroovyErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GroovyErrorKind::UnknownError => {
                write!(f, "UnknownError")
            }
        }
    }
}
