use crate::scheme::NodeGetOptions;
use crate::{Node, Scheme, SchemeError, Vfs};
use dashmap::DashMap;
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite};
use std::borrow::Cow;
use std::io::{IoSlice, SeekFrom};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use url::Url;

#[derive(Debug)]
pub enum MemoryError {}

impl std::fmt::Display for MemoryError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("IMPOSSIBLE-ERROR")
	}
}

impl std::error::Error for MemoryError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		None
	}
}

#[derive(Default)]
pub struct MemoryScheme {
	storage: DashMap<PathBuf, Arc<RwLock<Vec<u8>>>>,
}

#[async_trait::async_trait]
impl Scheme for MemoryScheme {
	async fn get_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<Box<dyn Node>, SchemeError<'a>> {
		let path = Path::new(url.path());
		let data = if let Some(data) = self.storage.get(path) {
			if options.get_create_new() {
				// Only create a new one, and it exists, so return
				return Err(SchemeError::NodeAlreadyExists(Cow::Borrowed(url.path())));
			}
			if options.get_truncate() {
				data.write().expect("poisoned lock").clear();
			}
			data.clone()
		} else {
			if !options.get_create() {
				// Don't create if missing
				return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
			}
			let data = Arc::new(RwLock::new(Vec::new()));
			self.storage.insert(path.to_owned(), data.clone());
			data
		};

		let cursor = if options.get_append() {
			data.read().expect("poisoned lock").len()
		} else {
			0
		};
		let node = MemoryNode { data, cursor };
		Ok(Box::new(node))
	}

	async fn remove_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		force: bool,
	) -> Result<(), SchemeError<'a>> {
		let path = Path::new(url.path());
		if let Some((_path, data)) = self.storage.remove(path) {
			if force {
				let mut data = data.write().expect("poisoned lock");
				data.clear();
				data.shrink_to_fit();
			}
			Ok(())
		} else {
			return Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())));
		}
	}
}

pub struct MemoryNode {
	data: Arc<RwLock<Vec<u8>>>,
	cursor: usize,
}

#[async_trait::async_trait]
impl Node for MemoryNode {
	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
		Some(self)
	}

	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
		Some(self)
	}

	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
		Some(self)
	}
}

impl AsyncRead for MemoryNode {
	fn poll_read(
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		let data = self.data.read().expect("poisoned lock");
		if self.cursor >= data.len() {
			return Poll::Ready(Ok(0));
		}

		let amt = std::cmp::min(data.len() - self.cursor, buf.len());
		buf[..amt].copy_from_slice(&data[self.cursor..(self.cursor + amt)]);
		drop(data); // Minimize the life of the lock
		self.cursor += amt;

		Poll::Ready(Ok(amt))
	}
}

impl AsyncWrite for MemoryNode {
	fn poll_write(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		let mut data = self.data.write().expect("poisoned lock");
		data.extend_from_slice(buf);
		Poll::Ready(Ok(buf.len()))
	}

	fn poll_write_vectored(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>],
	) -> Poll<std::io::Result<usize>> {
		let mut amt = 0;
		let mut data = self.data.write().expect("poisoned lock");
		for buf in bufs {
			amt += buf.len();
			data.extend_from_slice(&*buf);
		}
		Poll::Ready(Ok(amt))
	}

	fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		Poll::Ready(Ok(()))
	}

	fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		Poll::Ready(Ok(()))
	}
}

impl AsyncSeek for MemoryNode {
	fn poll_seek(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		pos: SeekFrom,
	) -> Poll<std::io::Result<u64>> {
		let this = self.get_mut();
		match pos {
			SeekFrom::Start(pos) => {
				let data = this.data.read().expect("poisoned lock");
				if pos > data.len() as u64 {
					this.cursor = data.len();
				} else {
					drop(data); // Minimize the life of the lock
					this.cursor = pos as usize;
				}
			}
			SeekFrom::End(end_pos) => {
				if end_pos > 0 {
					this.cursor = this.data.read().expect("poisoned lock").len();
				} else {
					let data = this.data.read().expect("poisoned lock");
					if (-end_pos) as usize > data.len() {
						drop(data); // Minimize the life of the lock
						this.cursor = 0;
					} else {
						this.cursor = data.len() - ((-end_pos) as usize);
					}
				}
			}
			SeekFrom::Current(offset) => {
				let new_cur = this.cursor as i64 + offset;
				if new_cur < 0 {
					this.cursor = 0;
				} else {
					let data = this.data.read().expect("poisoned lock");
					if new_cur as usize > data.len() {
						this.cursor = data.len();
					} else {
						drop(data); // Minimize the life of the lock
						this.cursor = new_cur as usize;
					}
				}
			}
		};
		Poll::Ready(Ok(this.cursor as u64))
	}
}

#[cfg(test)]
#[cfg(feature = "backend_tokio")]
mod async_tokio_tests {
	use crate::scheme::NodeGetOptions;
	use crate::{MemoryScheme, Vfs};
	use futures_lite::io::SeekFrom;
	use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn node_reading() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("mem", MemoryScheme::default()).unwrap();
		let mut node = vfs
			.get_node_at(
				"mem:test",
				&NodeGetOptions::new().read(true).create_new(true),
			)
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read()
			.await
			.unwrap()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
		assert_eq!(&buffer, "");
	}

	#[tokio::test]
	async fn node_writing() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("mem", MemoryScheme::default()).unwrap();
		let mut node = vfs
			.get_node_at(
				"mem:test",
				&NodeGetOptions::new()
					.write(true)
					.read(true)
					.create_new(true),
			)
			.await
			.unwrap();
		node.write()
			.await
			.unwrap()
			.write_all("test string".as_bytes())
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read()
			.await
			.unwrap()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
		assert_eq!(&buffer, "test string");
	}

	#[tokio::test]
	async fn node_stored() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("mem", MemoryScheme::default()).unwrap();
		{
			let mut node = vfs
				.get_node_at(
					"mem:test",
					&NodeGetOptions::new()
						.write(true)
						.read(true)
						.create_new(true),
				)
				.await
				.unwrap();
			node.write()
				.await
				.unwrap()
				.write_all("test string".as_bytes())
				.await
				.unwrap();
			let mut buffer = String::new();
			node.read()
				.await
				.unwrap()
				.read_to_string(&mut buffer)
				.await
				.unwrap();
			assert_eq!(&buffer, "test string");
		}
		{
			let mut node = vfs
				.get_node_at("mem:test", &NodeGetOptions::new().read(true))
				.await
				.unwrap();
			let mut buffer = String::new();
			node.read()
				.await
				.unwrap()
				.read_to_string(&mut buffer)
				.await
				.unwrap();
			assert_eq!(&buffer, "test string");
		}
	}

	#[tokio::test]
	async fn node_seeking() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("mem", MemoryScheme::default()).unwrap();
		let mut node = vfs
			.get_node(
				&u("mem:test"),
				&NodeGetOptions::new()
					.write(true)
					.read(true)
					.create_new(true),
			)
			.await
			.unwrap();
		node.write()
			.await
			.unwrap()
			.write_all("test".as_bytes())
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read()
			.await
			.unwrap()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
		assert_eq!(&buffer, "test");
		node.seek()
			.await
			.unwrap()
			.seek(SeekFrom::Start(2))
			.await
			.unwrap();
		buffer.clear();
		node.read()
			.await
			.unwrap()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
		assert_eq!(&buffer, "st");
	}
}
