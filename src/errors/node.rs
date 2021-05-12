#[derive(Debug)]
pub enum NodeError {
	UnknownError(Box<dyn std::error::Error + 'static>),
}

impl NodeError {
	pub fn into_owned(self) -> NodeError {
		match self {
			NodeError::UnknownError(source) => NodeError::UnknownError(source),
		}
	}
}

impl std::fmt::Display for NodeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			NodeError::UnknownError(_source) => f.write_str("unknown error"),
		}
	}
}

impl std::error::Error for NodeError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			NodeError::UnknownError(source) => Some(source.as_ref()),
		}
	}
}
