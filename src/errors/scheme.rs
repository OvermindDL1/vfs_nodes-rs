use crate::NodeError;
use std::borrow::Cow;
use url::Url;

#[derive(Debug)]
pub enum SchemeError<'name> {
	URLAccessError(Cow<'name, Url>),
	NodeDoesNotExist(Cow<'name, str>),
	NodeAlreadyExists(Cow<'name, str>),
	NodeError(NodeError),
	IOError(std::io::Error),
}

impl<'name> SchemeError<'name> {
	pub fn into_owned(self) -> SchemeError<'static> {
		match self {
			SchemeError::NodeDoesNotExist(name) => {
				SchemeError::NodeDoesNotExist(Cow::Owned(name.into_owned()))
			}
			SchemeError::NodeError(source) => SchemeError::NodeError(source),
			SchemeError::IOError(source) => SchemeError::IOError(source),
			SchemeError::NodeAlreadyExists(name) => {
				SchemeError::NodeAlreadyExists(Cow::Owned(name.into_owned()))
			}
			SchemeError::URLAccessError(url) => {
				SchemeError::URLAccessError(Cow::Owned(url.into_owned()))
			}
		}
	}
}

impl<'name> std::fmt::Display for SchemeError<'name> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			SchemeError::NodeDoesNotExist(name) => {
				f.write_fmt(format_args!("node not found: {}", name))
			}
			SchemeError::NodeError(_source) => f.write_str("node error"),
			SchemeError::IOError(_source) => f.write_str("generic IO error"),
			SchemeError::NodeAlreadyExists(name) => {
				f.write_fmt(format_args!("node already exists: {}", name))
			}
			SchemeError::URLAccessError(url) => {
				f.write_fmt(format_args!("access error with path: {}", url))
			}
		}
	}
}

impl<'name> std::error::Error for SchemeError<'name> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			SchemeError::NodeDoesNotExist(_name) => None,
			SchemeError::NodeError(source) => Some(source),
			SchemeError::IOError(source) => Some(source),
			SchemeError::NodeAlreadyExists(_name) => None,
			SchemeError::URLAccessError(_url) => None,
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
