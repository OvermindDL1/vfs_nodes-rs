use crate::node::AsyncReadWriteUnpin;
use crate::scheme::NodeGetOptions;
use crate::{Node, NodeError, Scheme, SchemeError};
use std::borrow::Cow;
use std::io::Cursor;
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadHalf, WriteHalf};
use url::Url;

#[derive(Debug)]
pub enum DataLoaderError {
	Base64Failure(base64::DecodeError),
}

impl std::fmt::Display for DataLoaderError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			DataLoaderError::Base64Failure(_source) => f.write_str("base64 error"),
		}
	}
}

impl std::error::Error for DataLoaderError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			DataLoaderError::Base64Failure(source) => Some(source),
		}
	}
}

pub struct DataLoaderScheme {}

impl Default for DataLoaderScheme {
	fn default() -> Self {
		DataLoaderScheme {}
	}
}

#[async_trait::async_trait]
impl Scheme for DataLoaderScheme {
	async fn get_node<'a>(
		&self,
		url: &'a Url,
		_options: NodeGetOptions,
	) -> Result<Box<dyn Node>, SchemeError<'a>> {
		if url.path_segments().is_some() {
			return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
		}
		let (data_type, data) = url
			.path()
			.split_once(',')
			.unwrap_or(("text/plain;charset=US-ASCII", url.path()));
		let (_mimetype, data) = if data_type == "base64" || data_type.ends_with(";base64") {
			let mimetype = data_type.trim_end_matches("base64").trim_end_matches(';');
			let data = base64::decode(data).map_err(|source| {
				NodeError::UnknownError(Box::new(DataLoaderError::Base64Failure(source)))
			})?;
			(mimetype, data)
		} else {
			let mimetype = data_type;
			let data = percent_encoding::percent_decode_str(&data).collect();
			(mimetype, data)
		};

		let node = DataLoaderNode {
			data: Cursor::new(data.into_boxed_slice()),
			//mimetype: mimetype.to_owned(),
		};
		Ok(Box::new(node))
	}

	async fn remove_node<'a>(&self, _url: &'a Url, _force: bool) -> Result<(), SchemeError<'a>> {
		Ok(())
	}
}

pub struct DataLoaderNode {
	//mimetype: String,
	data: Cursor<Box<[u8]>>,
}

#[async_trait::async_trait]
impl Node for DataLoaderNode {
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
		Some(&mut self.data)
	}

	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
		None
	}

	async fn read_write<'s>(
		&'s mut self,
	) -> Option<(
		ReadHalf<&'s mut dyn AsyncReadWriteUnpin>,
		WriteHalf<&'s mut dyn AsyncReadWriteUnpin>,
	)> {
		None
	}

	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
		Some(&mut self.data)
	}
}

#[cfg(test)]
mod tests {
	use crate::scheme::NodeGetOptions;
	use crate::{DataLoaderScheme, Scheme, Vfs};
	use tokio::io::AsyncReadExt;
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn scheme_access() {
		let scheme: &mut dyn Scheme = &mut DataLoaderScheme::default();
		let read = NodeGetOptions::new().read(true);
		assert!(
			scheme.get_node(&u("test:blah"), read.clone()).await.is_ok(),
			"text_basic"
		);
		assert!(
			scheme
				.get_node(&u("data:Some test text"), read.clone())
				.await
				.is_ok(),
			"text_unencoded_technically_invalid_but_okay"
		);
		assert!(
			scheme
				.get_node(&u("data:Some%20test%20text"), read.clone())
				.await
				.is_ok(),
			"text_percent_encoded"
		);
		assert!(
			scheme
				.get_node(&u("data:base64,U29tZSB0ZXN0IHRleHQ="), read.clone())
				.await
				.is_ok(),
			"text_base64"
		);
	}

	#[tokio::test]
	async fn node_reading() {
		{
			let scheme: &mut dyn Scheme = &mut DataLoaderScheme::default();
			let mut node = scheme
				.get_node(&u("data:test"), NodeGetOptions::new().read(true))
				.await
				.unwrap();
			let reader = node.read().await.unwrap();
			let mut buffer = String::new();
			reader.read_to_string(&mut buffer).await.unwrap();
			assert_eq!(&buffer, "test");
		}
		{
			let vfs = Vfs::default();
			let mut test_node = vfs
				.get_node_at("data:test", NodeGetOptions::new().read(true))
				.await
				.unwrap();
			let mut buffer = String::new();
			test_node
				.read()
				.await
				.unwrap()
				.read_to_string(&mut buffer)
				.await
				.unwrap();
			assert_eq!(&buffer, "test")
		}
	}
}
