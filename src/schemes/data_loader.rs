use crate::scheme::NodeGetOptions;
use crate::{Node, NodeError, Scheme, SchemeError};
use std::borrow::Cow;
use std::io::SeekFrom;
// use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadHalf, WriteHalf};
use futures_io::{AsyncRead, AsyncSeek, AsyncWrite};
use std::pin::Pin;
use std::task::{Context, Poll};
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
			data: data.into_boxed_slice(),
			cursor: 0,
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
	data: Box<[u8]>,
	cursor: usize,
}

#[async_trait::async_trait]
impl Node for DataLoaderNode {
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
		Some(self)
	}

	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
		None
	}

	// async fn read_write<'s>(
	// 	&'s mut self,
	// ) -> Option<(
	// 	ReadHalf<&'s mut dyn AsyncReadWriteUnpin>,
	// 	WriteHalf<&'s mut dyn AsyncReadWriteUnpin>,
	// )> {
	// 	None
	// }

	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
		Some(self)
	}
}

impl AsyncRead for DataLoaderNode {
	fn poll_read(
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		if self.cursor > self.data.len() {
			return Poll::Ready(Ok(0));
		}

		let amt = std::cmp::min(self.data.len() - self.cursor, buf.len());
		buf[..amt].copy_from_slice(&self.data[self.cursor..(self.cursor + amt)]);
		self.cursor += amt;

		Poll::Ready(Ok(amt))
	}
}

impl AsyncSeek for DataLoaderNode {
	fn poll_seek(
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		pos: SeekFrom,
	) -> Poll<std::io::Result<u64>> {
		match pos {
			SeekFrom::Start(pos) => {
				if pos > self.data.len() as u64 {
					self.cursor = self.data.len();
				} else {
					self.cursor = pos as usize;
				}
			}
			SeekFrom::End(end_pos) => {
				if end_pos > 0 {
					self.cursor = self.data.len();
				} else if (-end_pos) as usize > self.data.len() {
					self.cursor = 0;
				} else {
					self.cursor = self.data.len() - ((-end_pos) as usize);
				}
			}
			SeekFrom::Current(offset) => {
				let new_cur = self.cursor as i64 + offset;
				if new_cur < 0 {
					self.cursor = 0;
				} else if new_cur as usize > self.data.len() {
					self.cursor = self.data.len();
				} else {
					self.cursor = new_cur as usize;
				}
			}
		};
		Poll::Ready(Ok(self.cursor as u64))
	}
}

#[cfg(test)]
#[cfg(feature = "async-tokio")]
mod async_tokio_tests {
	use crate::scheme::NodeGetOptions;
	use crate::{DataLoaderScheme, Scheme, Vfs};
	use tokio::io::AsyncReadExt;
	use tokio_util::compat::FuturesAsyncReadCompatExt;
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
			reader.compat().read_to_string(&mut buffer).await.unwrap();
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
				.compat()
				.read_to_string(&mut buffer)
				.await
				.unwrap();
			assert_eq!(&buffer, "test")
		}
	}
}
