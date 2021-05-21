use crate::scheme::{NodeEntry, NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{Node, PinnedNode, Scheme, SchemeError, Vfs};
use dashmap::DashMap;
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite, Stream};
use std::borrow::Cow;
use std::io::SeekFrom;
use std::option::Option::None;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use url::Url;

#[derive(Default)]
pub struct MemoryScheme {
	storage: DashMap<PathBuf, Arc<RwLock<Vec<u8>>>>,
}

impl MemoryScheme {
	pub fn new() -> Self {
		Self::default()
	}
}

#[async_trait::async_trait]
impl Scheme for MemoryScheme {
	async fn get_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<PinnedNode, SchemeError<'a>> {
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
		let node = MemoryNode {
			data,
			cursor,
			read: options.get_read(),
			write: options.get_write(),
		};
		Ok(Box::pin(node))
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

	async fn metadata<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
	) -> Result<NodeMetadata, SchemeError<'a>> {
		let path = Path::new(url.path());
		if let Some(data) = self.storage.get(path) {
			let size = data.read().expect("poisoned lock").len();
			Ok(NodeMetadata {
				is_node: true,
				len: Some((size, Some(size))),
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
		if !path.ends_with('/') {
			if let Some(pos) = path.rfind('/') {
				path = &path[..pos];
			} else {
				path = "/";
			}
		}
		// Yes, a clone, maybe make this more efficient in future, but it's probably fine anyway
		// since the data itself is stored out-of-band in an Arc anyway, although the PathBuf's are
		// probably the more expensive clone anyway, hrmm...  This for now anyway...
		Ok(Box::pin(MemoryReadDir(
			self.storage.clone().into_iter(),
			Url::parse(&format!("{}:{}", url.scheme(), path))?,
		)))
	}
}

struct MemoryReadDir(
	dashmap::iter::OwningIter<PathBuf, Arc<RwLock<Vec<u8>>>>,
	Url,
);

impl Stream for MemoryReadDir {
	type Item = NodeEntry;

	fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = self.get_mut();
		let root_path = this.1.path();
		loop {
			if let Some((path, _data)) = this.0.next() {
				let path = path
					.to_str()
					.expect("somehow a non-url-safe path was added to a Memory scheme");
				// TODO:  Just return things in the current 'directory', probably want something better than a single dashmap
				if path.starts_with(root_path) {
					let mut url = this.1.clone();
					url.set_path(path);
					break Poll::Ready(Some(NodeEntry { url }));
				} else {
					continue;
				}
			} else {
				break Poll::Ready(None);
			}
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		todo!()
	}
}

pub struct MemoryNode {
	data: Arc<RwLock<Vec<u8>>>,
	cursor: usize,
	read: bool,
	write: bool,
}

#[async_trait::async_trait]
impl Node for MemoryNode {
	fn is_reader(&self) -> bool {
		self.read
	}

	fn is_writer(&self) -> bool {
		self.write
	}

	fn is_seeker(&self) -> bool {
		self.read || self.write
	}
	// async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
	// 	if self.read {
	// 		Some(self)
	// 	} else {
	// 		None
	// 	}
	// }
	//
	// async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
	// 	if self.write {
	// 		Some(self)
	// 	} else {
	// 		None
	// 	}
	// }
	//
	// async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
	// 	if self.read || self.write {
	// 		Some(self)
	// 	} else {
	// 		None
	// 	}
	// }
}

impl AsyncRead for MemoryNode {
	fn poll_read(
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		if !self.read {
			return Poll::Ready(Err(std::io::Error::from_raw_os_error(13)));
		}
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
		mut self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		if !self.write {
			return Poll::Ready(Err(std::io::Error::from_raw_os_error(13)));
		}
		let mut data = self.data.write().expect("poisoned lock");
		if self.cursor >= data.len() {
			data.extend_from_slice(buf);
			let len = data.len();
			drop(data);
			self.cursor = len;
		} else if self.cursor + buf.len() < data.len() {
			data.as_mut_slice()[self.cursor..self.cursor + buf.len()].copy_from_slice(buf);
			drop(data);
			self.cursor += buf.len()
		} else {
			let at = buf.len() - ((self.cursor + buf.len()) - data.len());
			let (inside, outside) = buf.split_at(at);
			data.as_mut_slice()[self.cursor..].copy_from_slice(inside);
			data.extend_from_slice(outside);
			let len = data.len();
			drop(data);
			self.cursor = len;
		}
		Poll::Ready(Ok(buf.len()))
	}

	// fn poll_write_vectored(
	// 	self: Pin<&mut Self>,
	// 	_cx: &mut Context<'_>,
	// 	bufs: &[IoSlice<'_>],
	// ) -> Poll<std::io::Result<usize>> {
	// 	let mut amt = 0;
	// 	let mut data = self.data.write().expect("poisoned lock");
	// 	for buf in bufs {
	// 		amt += buf.len();
	// 		data.extend_from_slice(&*buf);
	// 	}
	// 	Poll::Ready(Ok(amt))
	// }

	fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		if !self.write {
			return Poll::Ready(Err(std::io::Error::from_raw_os_error(13)));
		}
		Poll::Ready(Ok(()))
	}

	fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		if !self.write {
			return Poll::Ready(Err(std::io::Error::from_raw_os_error(13)));
		}
		Poll::Ready(Ok(()))
	}
}

impl AsyncSeek for MemoryNode {
	fn poll_seek(
		self: Pin<&mut Self>,
		_cx: &mut Context<'_>,
		pos: SeekFrom,
	) -> Poll<std::io::Result<u64>> {
		if !self.read && !self.write {
			return Poll::Ready(Err(std::io::Error::from_raw_os_error(13)));
		}
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
	use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, StreamExt};
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
		node.read_to_string(&mut buffer).await.unwrap();
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
		node.write_all("test string".as_bytes()).await.unwrap();
		node.seek(SeekFrom::Start(0)).await.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
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
			node.write_all("test string".as_bytes()).await.unwrap();
			node.seek(SeekFrom::Start(0)).await.unwrap();
			let mut buffer = String::new();
			node.read_to_string(&mut buffer).await.unwrap();
			assert_eq!(&buffer, "test string");
		}
		{
			let mut node = vfs
				.get_node_at("mem:test", &NodeGetOptions::new().read(true))
				.await
				.unwrap();
			let mut buffer = String::new();
			node.read_to_string(&mut buffer).await.unwrap();
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
		node.write_all("test".as_bytes()).await.unwrap();
		node.seek(SeekFrom::Start(0)).await.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
		assert_eq!(&buffer, "test");
		buffer.clear();
		node.seek(SeekFrom::Start(2)).await.unwrap();
		node.read_to_string(&mut buffer).await.unwrap();
		assert_eq!(&buffer, "st");
	}
	#[tokio::test]
	async fn node_read_dir() {
		let mut vfs = Vfs::empty();
		vfs.add_scheme("mem", MemoryScheme::default()).unwrap();

		async fn add_empty_entry(vfs: &Vfs, name: &str) {
			vfs.get_node_at(
				&format!("mem:{}", name),
				&NodeGetOptions::new().create_new(true),
			)
			.await
			.unwrap();
		}
		add_empty_entry(&vfs, "/test0").await;
		add_empty_entry(&vfs, "/test1").await;
		add_empty_entry(&vfs, "/test2").await;
		add_empty_entry(&vfs, "/test/blah0").await;
		add_empty_entry(&vfs, "/test/blah1").await;

		assert_eq!(vfs.read_dir_at("mem:/").await.unwrap().count().await, 5);
		assert_eq!(vfs.read_dir_at("mem:/test").await.unwrap().count().await, 5);
		assert_eq!(
			vfs.read_dir_at("mem:/test/").await.unwrap().count().await,
			2
		);
	}
}
