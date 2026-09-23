use super::*;

impl Error for ScalaError {}

impl Debug for ScalaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&self.kind, f)
    }
}

impl Display for ScalaError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.kind, f)
    }
}

impl Display for ScalaErrorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ScalaErrorKind::UnknownError => {
                write!(f, "UnknownError")
            }
        }
    }
}
