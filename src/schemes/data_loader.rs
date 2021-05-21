use crate::node::poll_io_err;
use crate::scheme::{NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{Node, PinnedNode, Scheme, SchemeError, Vfs};
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite};
use std::borrow::Cow;
use std::io::SeekFrom;
use std::pin::Pin;
use std::task::{Context, Poll};
use url::Url;

pub struct DataLoaderScheme {}

impl Default for DataLoaderScheme {
	fn default() -> Self {
		DataLoaderScheme {}
	}
}

impl DataLoaderScheme {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn parse_url_into_data(url: &Url) -> Result<(&str, Box<[u8]>), SchemeError> {
		if url.path_segments().is_some() {
			return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
		}
		let (data_type, data) = url
			.path()
			.split_once(',')
			.unwrap_or(("text/plain;charset=US-ASCII", url.path()));
		let (mimetype, data) = if data_type == "base64" || data_type.ends_with(";base64") {
			let mimetype = data_type.trim_end_matches("base64").trim_end_matches(';');
			let data = base64::decode(data).map_err(|source| {
				(
					"data_loader error",
					Box::new(source) as Box<dyn std::error::Error + Send + Sync>,
				)
			})?;
			(mimetype, data)
		} else {
			let mimetype = data_type;
			let data = percent_encoding::percent_decode_str(&data).collect();
			(mimetype, data)
		};
		Ok((mimetype, data.into_boxed_slice()))
	}
}

#[async_trait::async_trait]
impl Scheme for DataLoaderScheme {
	async fn get_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		_options: &NodeGetOptions,
	) -> Result<PinnedNode, SchemeError<'a>> {
		let (_mimetype, data) = Self::parse_url_into_data(url)?;
		let node = DataLoaderNode {
			data,
			cursor: 0,
			//mimetype: mimetype.to_owned(),
		};
		Ok(Box::pin(node))
	}

	async fn remove_node<'a>(
		&self,
		_vfs: &Vfs,
		_url: &'a Url,
		_force: bool,
	) -> Result<(), SchemeError<'a>> {
		Ok(())
	}

	async fn metadata<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
	) -> Result<NodeMetadata, SchemeError<'a>> {
		let (_mimetype, data) = Self::parse_url_into_data(url)?;
		Ok(NodeMetadata {
			is_node: true,
			len: Some((data.len(), Some(data.len()))),
		})
	}

	async fn read_dir<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
	) -> Result<ReadDirStream, SchemeError<'a>> {
		Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.as_str())))
	}
}

pub struct DataLoaderNode {
	//mimetype: String,
	data: Box<[u8]>,
	cursor: usize,
}

#[async_trait::async_trait]
impl Node for DataLoaderNode {
	fn is_reader(&self) -> bool {
		true
	}

	fn is_writer(&self) -> bool {
		false
	}

	fn is_seeker(&self) -> bool {
		true
	}
	// async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
	// 	Some(self)
	// }
	//
	// async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
	// 	None
	// }
	//
	// async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
	// 	Some(self)
	// }
}

impl AsyncRead for DataLoaderNode {
	fn poll_read(
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		if self.cursor >= self.data.len() {
			return Poll::Ready(Ok(0));
		}

		let amt = std::cmp::min(self.data.len() - self.cursor, buf.len());
		buf[..amt].copy_from_slice(&self.data[self.cursor..(self.cursor + amt)]);
		self.cursor += amt;

		Poll::Ready(Ok(amt))
	}
}

impl AsyncWrite for DataLoaderNode {
	fn poll_write(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		_buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		poll_io_err()
	}

	fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		poll_io_err()
	}

	fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		poll_io_err()
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
#[cfg(feature = "backend_tokio")]
mod async_tokio_tests {
	use crate::scheme::NodeGetOptions;
	use crate::Vfs;
	use futures_lite::io::SeekFrom;
	use futures_lite::{AsyncReadExt, AsyncSeekExt};
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn scheme_access() {
		let vfs = Vfs::default();
		let read = NodeGetOptions::new().read(true);
		assert!(
			vfs.get_node(&u("data:blah"), &read).await.is_ok(),
			"text_basic"
		);
		assert!(
			vfs.get_node(&u("data:Some test text"), &read).await.is_ok(),
			"text_unencoded_technically_invalid_but_okay"
		);
		assert!(
			vfs.get_node(&u("data:Some%20test%20text"), &read)
				.await
				.is_ok(),
			"text_percent_encoded"
		);
		assert!(
			vfs.get_node(&u("data:base64,U29tZSB0ZXN0IHRleHQ="), &read)
				.await
				.is_ok(),
			"text_base64"
		);
	}

	#[tokio::test]
	async fn node_reading() {
		let vfs = Vfs::default();
		let mut test_node = vfs
			.get_node_at("data:test", &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		test_node.read_to_string(&mut buffer).await.unwrap();
		assert_eq!(&buffer, "test")
	}

	#[tokio::test]
	async fn node_seeking() {
		let vfs = Vfs::default();
		let mut node = vfs
			.get_node(&u("data:test"), &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
		assert_eq!(&buffer, "test");
		node.seek(SeekFrom::Start(2)).await.unwrap();
		buffer.clear();
		node.read_to_string(&mut buffer).await.unwrap();
		assert_eq!(&buffer, "st");
	}

	#[tokio::test]
	async fn node_writing() {
		let vfs = Vfs::default();
		let node = vfs
			.get_node(&u("data:test"), &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		assert!(!node.is_writer());
	}
}
