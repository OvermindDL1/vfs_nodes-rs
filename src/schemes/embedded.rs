use crate::node::poll_io_err;
use crate::scheme::{NodeEntry, NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{Node, PinnedNode, Scheme, SchemeError, Vfs};
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite, Stream};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::io::SeekFrom;
use std::marker::PhantomData;
use std::option::Option::None;
use std::pin::Pin;
use std::task::{Context, Poll};
use url::Url;

pub struct EmbeddedScheme<Embed: RustEmbed + Send + Sync + 'static> {
	_phantom: PhantomData<Embed>,
}

impl<Embed: RustEmbed + Send + Sync + 'static> Default for EmbeddedScheme<Embed> {
	fn default() -> Self {
		EmbeddedScheme {
			_phantom: PhantomData::default(),
		}
	}
}

impl<Embed: RustEmbed + Send + Sync + 'static> EmbeddedScheme<Embed> {
	pub fn new() -> Self {
		Self::default()
	}
}

#[async_trait::async_trait]
impl<Embed: RustEmbed + Send + Sync + 'static> Scheme for EmbeddedScheme<Embed> {
	async fn get_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<PinnedNode, SchemeError<'a>> {
		if url.path().is_empty() {
			return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
		}
		if options.get_read() {
			if let Some(data) = Embed::get(&url.path()[1..]) {
				Ok(Box::pin(EmbeddedNode { data, cursor: 0 }))
			} else {
				Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
			}
		} else {
			Err(SchemeError::UrlAccessError(Cow::Borrowed(url)))
		}
	}

	async fn remove_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		_force: bool,
	) -> Result<(), SchemeError<'a>> {
		Err(SchemeError::UrlAccessError(Cow::Borrowed(url)))
	}

	async fn metadata<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
	) -> Result<NodeMetadata, SchemeError<'a>> {
		if url.path().is_empty() {
			return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
		}
		if let Some(data) = Embed::get(&url.path()[1..]) {
			Ok(NodeMetadata {
				is_node: true,
				len: Some((data.len(), Some(data.len()))),
			})
		} else {
			Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
		}
	}

	async fn read_dir<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
	) -> Result<ReadDirStream, SchemeError<'a>> {
		let mut path = url.path();
		if !path.starts_with('/') {
			return Err(SchemeError::UrlAccessError(Cow::Borrowed(url)));
		}
		if !path.ends_with('/') {
			if let Some(pos) = path.rfind('/') {
				path = &path[..pos];
			} else {
				return Err(SchemeError::UrlAccessError(Cow::Borrowed(url)));
			}
		}
		// RustEmbed doesn't have `Send` on it's internal debug iterator, so no compile, even though
		// there's no reason it couldn't have it, plus why don't we just get a slice of names of the
		// filenames anyway?  Meh, packing it all together here...
		let data: Vec<_> = Embed::iter().collect();
		let mut url = url.clone();
		url.set_path(path);
		Ok(Box::pin(EmbeddedReadDir(data.into_iter(), url)))
	}
}

struct EmbeddedReadDir(std::vec::IntoIter<Cow<'static, str>>, Url);

impl Stream for EmbeddedReadDir {
	type Item = NodeEntry;

	fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = self.get_mut();
		let base_path = &this.1.path()[1..]; // `read_dir` already checked for the prefix '/'
		loop {
			if let Some(path) = this.0.next() {
				if path.starts_with(base_path) {
					// TODO:  Just return things in the current 'directory'
					if let Ok(url) = Url::parse(&format!("{}:/{}", this.1.scheme(), path)) {
						let entry = NodeEntry { url };
						return Poll::Ready(Some(entry));
					} else {
						return Poll::Ready(None);
					}
				} else {
					continue;
				}
			} else {
				return Poll::Ready(None);
			}
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.0.size_hint()
	}
}

pub struct EmbeddedNode {
	data: Cow<'static, [u8]>,
	cursor: usize,
}

#[async_trait::async_trait]
impl Node for EmbeddedNode {
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

impl AsyncRead for EmbeddedNode {
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

impl AsyncWrite for EmbeddedNode {
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

impl AsyncSeek for EmbeddedNode {
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
	use crate::{EmbeddedScheme, Vfs};
	use futures_lite::io::SeekFrom;
	use futures_lite::{AsyncReadExt, AsyncSeekExt, StreamExt};
	use url::Url;

	#[derive(rust_embed::RustEmbed)]
	#[folder = "examples"]
	struct EmbedTest;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn embed_read() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("embed", EmbeddedScheme::<EmbedTest>::new())
			.unwrap();
		let read = &NodeGetOptions::new().read(true);
		let buffer = &mut String::new();
		assert!(vfs.get_node_at("embed:/nothing/here", read).await.is_err());
		vfs.get_node(&u("embed:/full_tokio.rs"), read)
			.await
			.unwrap()
			.read_to_string(buffer)
			.await
			.unwrap();
		assert!(buffer.contains("main"));
		buffer.clear();
	}

	#[tokio::test]
	async fn embed_seeking() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("embed", EmbeddedScheme::<EmbedTest>::new())
			.unwrap();
		let read = &NodeGetOptions::new().read(true);
		let mut node = vfs
			.get_node(&u("embed:/full_tokio.rs"), read)
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
		assert!(buffer.contains("main"));
		node.seek(SeekFrom::End(2)).await.unwrap();
		buffer.clear();
		node.read_to_string(&mut buffer).await.unwrap();
		assert!(!buffer.contains("main"));
	}

	#[tokio::test]
	async fn embed_read_dir() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("embed", EmbeddedScheme::<EmbedTest>::new())
			.unwrap();
		assert!(vfs.read_dir_at("embed:/").await.unwrap().count().await > 0);
		assert_eq!(
			vfs.read_dir_at("embed:/full/").await.unwrap().count().await,
			1
		);
	}
}
