/// Errors that can be returned by the API
#[derive(Debug)]
pub enum Error {
    /// Networking problems, source will be a std::io::Error
    NetworkError { source: std::io::Error },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NetworkError { source } => write!(f, "Network error: {}", source),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NetworkError { source } => Some(source),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Self::NetworkError { source: e }
    }
}
