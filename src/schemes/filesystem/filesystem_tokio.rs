use crate::node::IsAllowed;
use crate::scheme::{NodeEntry, NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{Node, PinnedNode, Scheme, SchemeError, Vfs};
use futures_lite::{ready, AsyncRead, AsyncSeek, AsyncWrite, Stream};
use std::borrow::Cow;
use std::io::{IoSlice, SeekFrom};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::OpenOptions;
use url::Url;

#[derive(Debug)]
pub enum TokioFileSystemError {
	// Base64Failure(base64::DecodeError),
}

impl std::fmt::Display for TokioFileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// match self {
		// 	TokioFileSystemError::Base64Failure(_source) => f.write_str("base64 error"),
		// }
		f.write_str("IMPOSSIBLE-ERROR")
	}
}

impl std::error::Error for TokioFileSystemError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		// match self {
		// 	TokioFileSystemError::Base64Failure(source) => Some(source),
		// }
		None
	}
}

// TODO:  Maybe put all path lookups in a hashmap or btree or so with values as the weak node
// TODO:  then lock it on reading/writing?
pub struct TokioFileSystemScheme {
	root_path: PathBuf,
}

impl TokioFileSystemScheme {
	pub fn new(root_path: impl Into<PathBuf>) -> Self {
		Self {
			root_path: root_path.into(),
		}
	}

	pub fn fs_path_from_url<'a>(&self, url: &'a Url) -> Result<PathBuf, SchemeError<'a>> {
		Ok(url
			.path_segments()
			.ok_or(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))?
			.fold(self.root_path.clone(), |mut path, part| {
				path.push(part);
				path
			}))
	}
}

#[async_trait::async_trait]
impl Scheme for TokioFileSystemScheme {
	async fn get_node<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<PinnedNode, SchemeError<'a>> {
		let path = self.fs_path_from_url(url)?;
		if options.get_create() {
			let parent_path = path
				.parent()
				.ok_or(SchemeError::UrlAccessError(Cow::Borrowed(&url)))?;
			tokio::fs::create_dir_all(parent_path).await?;
		}
		let file = OpenOptions::from(options).open(path).await?;
		let node = TokioFileSystemNode {
			file,
			seek: None,
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
		let path = self.fs_path_from_url(url)?;
		if path.exists() {
			if path.is_file() {
				tokio::fs::remove_file(&path).await?;
			} else if path.is_dir() {
				if force {
					tokio::fs::remove_dir_all(&path).await?;
				} else {
					tokio::fs::remove_dir(&path).await?;
				}
			}
		}
		Ok(())
	}

	async fn metadata<'a>(
		&self,
		_vfs: &Vfs,
		url: &'a Url,
	) -> Result<NodeMetadata, SchemeError<'a>> {
		let path = self.fs_path_from_url(url)?;
		if let Ok(metadata) = tokio::fs::metadata(path).await {
			let size = metadata.len() as usize;
			Ok(NodeMetadata {
				is_node: metadata.is_file(),
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
		let path = self.fs_path_from_url(url)?;
		if path.exists() {
			Ok(Box::pin(TokioReadDirWrapper(
				tokio::fs::read_dir(&path).await?,
				url.clone(),
			)))
		} else {
			Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
		}
	}
}

// Yeah, tokio's ReadDir really doesn't implement `Stream`, instead you have to call it manually...
struct TokioReadDirWrapper(tokio::fs::ReadDir, Url);

impl Stream for TokioReadDirWrapper {
	type Item = NodeEntry;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		loop {
			match ready!(self.0.poll_next_entry(cx)) {
				Err(_io_error) => continue,          // skip nodes with IO errors
				Ok(None) => break Poll::Ready(None), // done
				Ok(Some(entry)) => {
					if let Some(entry_sub_path) = entry.file_name().to_str() {
						if let Ok(entry_url) = self.1.join(&entry_sub_path) {
							break Poll::Ready(Some(NodeEntry { url: entry_url }));
						} else {
							continue; // failed parsing new URL entry, invalid name format
						}
					} else {
						continue; // failed to convert the OsStr to a normal string, so it will be an invalid URL as well
					}
				}
			}
		}
	}
}

pub struct TokioFileSystemNode {
	file: tokio::fs::File,
	seek: Option<std::io::SeekFrom>,
	read: bool,
	write: bool,
}

#[async_trait::async_trait]
impl Node for TokioFileSystemNode {
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

impl AsyncRead for TokioFileSystemNode {
	fn poll_read(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		self.read.into_poll_io_then(|| {
			let mut buf = tokio::io::ReadBuf::new(buf);
			{
				let file = Pin::new(&mut self.file);
				ready!(tokio::io::AsyncRead::poll_read(file, cx, &mut buf))?;
			}
			Poll::Ready(Ok(buf.filled().len()))
		})
	}
}

impl AsyncWrite for TokioFileSystemNode {
	fn poll_write(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		self.write.into_poll_io_then(|| {
			let file = Pin::new(&mut self.file);
			tokio::io::AsyncWrite::poll_write(file, cx, buf)
		})
	}

	fn poll_write_vectored(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>],
	) -> Poll<std::io::Result<usize>> {
		self.write.into_poll_io_then(|| {
			let file = Pin::new(&mut self.file);
			tokio::io::AsyncWrite::poll_write_vectored(file, cx, bufs)
		})
	}

	fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		self.write.into_poll_io_then(|| {
			let file = Pin::new(&mut self.file);
			tokio::io::AsyncWrite::poll_flush(file, cx)
		})
	}

	fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		let file = Pin::new(&mut self.file);
		tokio::io::AsyncWrite::poll_shutdown(file, cx)
	}
}

impl AsyncSeek for TokioFileSystemNode {
	fn poll_seek(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		pos: SeekFrom,
	) -> Poll<std::io::Result<u64>> {
		(self.read || self.write).into_poll_io_then(|| {
			if self.seek != Some(pos) {
				{
					let file = Pin::new(&mut self.file);
					tokio::io::AsyncSeek::start_seek(file, pos)?;
				}
				self.as_mut().seek = Some(pos);
			}
			let res = {
				let file = Pin::new(&mut self.file);
				ready!(tokio::io::AsyncSeek::poll_complete(file, cx))
			};
			self.as_mut().seek = None;
			Poll::Ready(res.map(|p| p as u64))
		})
	}
}

#[cfg(test)]
mod tests_general {
	// Unique per test
	use crate::TokioFileSystemScheme as FileSystemScheme;
	use tokio::test as async_test;

	const FILE_CONTENT_TEST_LOC: &str = "fs:/test_node_writing_tokio.txt";
	const FILE_CONTENT_SEEK_TEST_LOC: &str = "fs:/test_node_seeking_tokio.txt";

	// Generic per test
	use crate::scheme::NodeGetOptions;
	use crate::Vfs;
	use futures_lite::io::SeekFrom;
	use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, StreamExt};
	use url::Url;

	const FILE_TEST_CONTENT: &str = "Test content";

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[async_test]
	async fn scheme_access() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		assert!(
			vfs.get_node(&u("fs:/Cargo.toml"), &NodeGetOptions::new().read(true))
				.await
				.is_ok(),
			"file exists"
		);
		assert!(
			vfs.get_node(&u("fs:/target"), &NodeGetOptions::new().read(true))
				.await
				.is_ok(),
			"folder exists"
		);
	}

	#[async_test]
	async fn node_reading_vfs() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		let mut node = vfs
			.get_node_at("fs:/Cargo.toml", &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
		assert!(buffer.starts_with("[package]"));
	}

	#[async_test]
	async fn node_writing() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap().join("target")),
		)
		.unwrap();
		let mut node = vfs
			.get_node(
				&u(FILE_CONTENT_TEST_LOC),
				&NodeGetOptions::new()
					.read(true)
					.write(true)
					.truncate(true)
					.create(true)
					.create_new(false),
			)
			.await
			.unwrap();
		node.write_all(FILE_TEST_CONTENT.as_bytes()).await.unwrap();
		node.flush().await.unwrap();
		let mut node = vfs
			.get_node(&u(FILE_CONTENT_TEST_LOC), &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
		vfs.remove_node(&u(FILE_CONTENT_TEST_LOC), false)
			.await
			.unwrap();
		assert_eq!(&buffer, FILE_TEST_CONTENT);
	}

	#[async_test]
	async fn node_seeking() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap().join("target")),
		)
		.unwrap();
		let mut node = vfs
			.get_node(
				&u(FILE_CONTENT_SEEK_TEST_LOC),
				&NodeGetOptions::new()
					.read(true)
					.write(true)
					.truncate(true)
					.create(true)
					.create_new(false),
			)
			.await
			.unwrap();
		node.write_all(FILE_TEST_CONTENT.as_bytes()).await.unwrap();
		// Always be sure to flush before seeking if any writes were performed, not necessary for
		// async_std, but it is for tokio, and it's good form anyway when seeking.
		node.flush().await.unwrap();
		node.seek(SeekFrom::Start(0)).await.unwrap();
		let mut buffer = String::new();
		node.read_to_string(&mut buffer).await.unwrap();
		vfs.remove_node(&u(FILE_CONTENT_SEEK_TEST_LOC), false)
			.await
			.unwrap();
		assert_eq!(&buffer, FILE_TEST_CONTENT);
	}

	#[async_test]
	async fn metadata() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		let metadata = vfs.metadata_at("fs:/Cargo.toml").await.unwrap();
		assert!(metadata.is_node);
		assert!(metadata.len.unwrap().0 > 0);
		let metadata = vfs.metadata_at("fs:/src").await.unwrap();
		assert!(!metadata.is_node);
		assert!(vfs.metadata_at("fs:/blah").await.is_err());
		assert!(vfs.metadata_at("nothing:").await.is_err());
	}

	#[async_test]
	async fn list_nodes() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"fs",
			FileSystemScheme::new(std::env::current_dir().unwrap()),
		)
		.unwrap();
		assert_eq!(
			vfs.read_dir_at("fs:/src/schemes/filesystem/")
				.await
				.unwrap()
				.filter(|u| u.url.path().ends_with("mod.rs"))
				.count()
				.await,
			1
		);
		assert_eq!(
			vfs.read_dir_at("fs:/src/schemes/filesystem/")
				.await
				.unwrap()
				.filter(|u| u.url.path().ends_with("mod.rs"))
				.next()
				.await
				.unwrap()
				.url
				.path(),
			"/src/schemes/filesystem/mod.rs"
		);
		assert_eq!(
			vfs.read_dir_at("fs:/src/schemes/filesystem")
				.await
				.unwrap()
				.filter(|u| u.url.path().ends_with("mod.rs"))
				.next()
				.await
				.unwrap()
				.url
				.path(),
			"/src/schemes/mod.rs",
			"like std::fs::read_dir trim any non-dir elements in the path"
		);
	}
}
