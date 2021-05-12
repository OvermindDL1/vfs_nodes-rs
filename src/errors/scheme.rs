use crate::NodeError;
use std::borrow::Cow;

#[derive(Debug)]
pub enum SchemeError<'name> {
	NodeDoesNotExist(Cow<'name, str>),
	NodeError(NodeError),
	IOError(std::io::Error),
}

impl<'name> SchemeError<'name> {
	pub fn into_owned(self) -> SchemeError<'static> {
		match self {
			SchemeError::NodeDoesNotExist(scheme_name) => {
				SchemeError::NodeDoesNotExist(Cow::Owned(scheme_name.into_owned()))
			}
			SchemeError::NodeError(source) => SchemeError::NodeError(source),
			SchemeError::IOError(source) => SchemeError::IOError(source),
		}
	}
}

impl<'name> std::fmt::Display for SchemeError<'name> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			SchemeError::NodeDoesNotExist(scheme_name) => {
				f.write_fmt(format_args!("scheme not found: {}", scheme_name))
			}
			SchemeError::NodeError(_source) => f.write_str("node error"),
			SchemeError::IOError(_source) => f.write_str("generic IO error"),
		}
	}
}

impl<'name> std::error::Error for SchemeError<'name> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			SchemeError::NodeDoesNotExist(_scheme_name) => None,
			SchemeError::NodeError(source) => Some(source),
			SchemeError::IOError(source) => Some(source),
		}
	}
}

impl<'name> From<NodeError> for SchemeError<'name> {
	fn from(source: NodeError) -> Self {
		SchemeError::NodeError(source)
	}
}

impl From<std::io::Error> for SchemeError<'static> {
	fn from(source: std::io::Error) -> Self {
		SchemeError::IOError(source)
	}
}
