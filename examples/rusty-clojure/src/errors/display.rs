use super::*;

impl Error for ClojureError {}

impl Debug for ClojureError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.kind, f)
    }
}

impl Display for ClojureError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.kind, f)
    }
}

impl Display for ClojureErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ClojureErrorKind::UnknownError => {
                write!(f, "UnknownError")
            }
        }
    }
}
