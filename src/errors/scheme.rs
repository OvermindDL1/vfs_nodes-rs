use crate::VfsError;
use std::borrow::Cow;
use url::Url;

#[derive(Debug)]
pub enum SchemeError<'name> {
	GenericError(
		Option<&'static str>,
		Option<Box<dyn std::error::Error + 'static + Send + Sync>>,
	),
	UrlParseError(url::ParseError),
	UrlAccessError(Cow<'name, Url>),
	NodeDoesNotExist(Cow<'name, str>),
	NodeAlreadyExists(Cow<'name, str>),
	IOError(std::io::Error),
}

impl<'name> SchemeError<'name> {
	pub fn into_owned(self) -> SchemeError<'static> {
		match self {
			SchemeError::NodeDoesNotExist(name) => {
				SchemeError::NodeDoesNotExist(Cow::Owned(name.into_owned()))
			}
			SchemeError::NodeAlreadyExists(name) => {
				SchemeError::NodeAlreadyExists(Cow::Owned(name.into_owned()))
			}
			SchemeError::UrlAccessError(url) => {
				SchemeError::UrlAccessError(Cow::Owned(url.into_owned()))
			}
			SchemeError::GenericError(msg, source) => SchemeError::GenericError(msg, source),
			SchemeError::UrlParseError(path) => SchemeError::UrlParseError(path),
			SchemeError::IOError(source) => SchemeError::IOError(source),
		}
	}
}

impl<'name> std::fmt::Display for SchemeError<'name> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			SchemeError::GenericError(msg, _source) => f.write_str(msg.unwrap_or("generic error")),
			SchemeError::NodeDoesNotExist(name) => {
				f.write_fmt(format_args!("node not found: {}", name))
			}
			SchemeError::IOError(_source) => f.write_str("generic IO error"),
			SchemeError::NodeAlreadyExists(name) => {
				f.write_fmt(format_args!("node already exists: {}", name))
			}
			SchemeError::UrlAccessError(url) => {
				f.write_fmt(format_args!("access error with path: {}", url))
			}
			SchemeError::UrlParseError(_source) => f.write_str("failed parsing url string"),
		}
	}
}

impl<'name> std::error::Error for SchemeError<'name> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			SchemeError::GenericError(_msg, source) => source.as_ref().map(|source| {
				let source: &dyn std::error::Error = &**source;
				source
			}),
			SchemeError::NodeDoesNotExist(_name) => None,
			SchemeError::IOError(source) => Some(source),
			SchemeError::NodeAlreadyExists(_name) => None,
			SchemeError::UrlAccessError(_url) => None,
			SchemeError::UrlParseError(source) => Some(source),
		}
	}
}

impl From<&'static str> for SchemeError<'static> {
	fn from(msg: &'static str) -> Self {
		SchemeError::GenericError(Some(msg), None)
	}
}

impl From<Option<&'static str>> for SchemeError<'static> {
	fn from(msg: Option<&'static str>) -> Self {
		SchemeError::GenericError(msg, None)
	}
}

impl
	From<(
		&'static str,
		Option<Box<dyn std::error::Error + Send + Sync>>,
	)> for SchemeError<'static>
{
	fn from(
		(msg, source): (
			&'static str,
			Option<Box<dyn std::error::Error + Send + Sync>>,
		),
	) -> Self {
		SchemeError::GenericError(Some(msg), source)
	}
}

impl
	From<(
		Option<&'static str>,
		Option<Box<dyn std::error::Error + Send + Sync>>,
	)> for SchemeError<'static>
{
	fn from(
		(msg, source): (
			Option<&'static str>,
			Option<Box<dyn std::error::Error + Send + Sync>>,
		),
	) -> Self {
		SchemeError::GenericError(msg, source)
	}
}

impl From<(&'static str, Box<dyn std::error::Error + Send + Sync>)> for SchemeError<'static> {
	fn from((msg, source): (&'static str, Box<dyn std::error::Error + Send + Sync>)) -> Self {
		SchemeError::GenericError(Some(msg), Some(source))
	}
}

impl
	From<(
		Option<&'static str>,
		Box<dyn std::error::Error + Send + Sync>,
	)> for SchemeError<'static>
{
	fn from(
		(msg, source): (
			Option<&'static str>,
			Box<dyn std::error::Error + Send + Sync>,
		),
	) -> Self {
		SchemeError::GenericError(msg, Some(source))
	}
}

impl From<Option<Box<dyn std::error::Error + Send + Sync>>> for SchemeError<'static> {
	fn from(source: Option<Box<dyn std::error::Error + Send + Sync>>) -> Self {
		SchemeError::GenericError(None, source)
	}
}

impl From<Box<dyn std::error::Error + Send + Sync>> for SchemeError<'static> {
	fn from(source: Box<dyn std::error::Error + Send + Sync>) -> Self {
		SchemeError::GenericError(None, Some(source))
	}
}

impl From<std::io::Error> for SchemeError<'static> {
	fn from(source: std::io::Error) -> Self {
		SchemeError::IOError(source)
	}
}

impl From<url::ParseError> for SchemeError<'static> {
	fn from(source: url::ParseError) -> Self {
		SchemeError::UrlParseError(source)
	}
}

impl<'name> From<VfsError<'name>> for SchemeError<'static> {
	fn from(source: VfsError<'name>) -> Self {
		SchemeError::GenericError(Some("vfs error"), Some(Box::new(source.into_owned())))
	}
}
