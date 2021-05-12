use crate::SchemeError;
use std::borrow::Cow;
use url::ParseError;

#[derive(Debug)]
pub enum VfsError<'name> {
	SchemeAlreadyExists(String),
	SchemeNotFound(Cow<'name, str>),
	SchemeWrongType(Cow<'name, str>, &'static str),
	UrlParseFailed(url::ParseError),
	SchemeError(SchemeError<'static>),
}

impl<'scheme_name> VfsError<'scheme_name> {
	pub fn into_owned(self) -> VfsError<'static> {
		match self {
			VfsError::SchemeAlreadyExists(scheme_name) => {
				VfsError::SchemeAlreadyExists(scheme_name)
			}
			VfsError::SchemeNotFound(scheme_name) => {
				VfsError::SchemeNotFound(Cow::Owned(scheme_name.into_owned()))
			}
			VfsError::SchemeWrongType(scheme_name, type_name) => {
				VfsError::SchemeWrongType(Cow::Owned(scheme_name.into_owned()), type_name)
			}
			VfsError::UrlParseFailed(source) => VfsError::UrlParseFailed(source),
			VfsError::SchemeError(source) => VfsError::SchemeError(source.into_owned()),
		}
	}
}

impl<'scheme_name> std::fmt::Display for VfsError<'scheme_name> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			VfsError::SchemeAlreadyExists(scheme_name) => {
				f.write_fmt(format_args!("scheme already exists: {}", scheme_name))
			}
			VfsError::SchemeNotFound(scheme_name) => {
				f.write_fmt(format_args!("scheme not found: {}", scheme_name))
			}
			VfsError::SchemeWrongType(scheme_name, type_name) => f.write_fmt(format_args!(
				"scheme `{}` cannot be cast to type: {}",
				scheme_name, type_name
			)),
			VfsError::UrlParseFailed(_source) => f.write_str("url failed to parse"),
			VfsError::SchemeError(_source) => f.write_str("scheme error"),
		}
	}
}

impl<'scheme_name> std::error::Error for VfsError<'scheme_name> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			VfsError::SchemeAlreadyExists(_scheme_name) => None,
			VfsError::SchemeNotFound(_scheme_name) => None,
			VfsError::SchemeWrongType(_scheme_name, _type_name) => None,
			VfsError::UrlParseFailed(source) => Some(source),
			VfsError::SchemeError(source) => Some(source),
		}
	}
}

impl From<url::ParseError> for VfsError<'static> {
	fn from(source: ParseError) -> Self {
		VfsError::UrlParseFailed(source)
	}
}

impl<'name> From<SchemeError<'name>> for VfsError<'static> {
	fn from(source: SchemeError<'name>) -> Self {
		VfsError::SchemeError(source.into_owned())
	}
}
