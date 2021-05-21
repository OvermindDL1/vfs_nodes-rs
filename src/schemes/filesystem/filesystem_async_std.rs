use crate::node::IsAllowed;
use crate::scheme::{NodeEntry, NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{Node, PinnedNode, Scheme, SchemeError, Vfs};
use async_std::fs::OpenOptions;
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite, StreamExt};
use std::borrow::Cow;
use std::io::{IoSlice, IoSliceMut, SeekFrom};
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use url::Url;

#[derive(Debug)]
pub enum AsyncStdFileSystemError {
	Base64Failure(base64::DecodeError),
}

impl std::fmt::Display for AsyncStdFileSystemError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			AsyncStdFileSystemError::Base64Failure(_source) => f.write_str("base64 error"),
		}
	}
}

impl std::error::Error for AsyncStdFileSystemError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			AsyncStdFileSystemError::Base64Failure(source) => Some(source),
		}
	}
}

// TODO:  Maybe put all path lookups in a hashmap or btree or so with values as the weak node
// TODO:  then lock it on reading/writing?
pub struct AsyncStdFileSystemScheme {
	root_path: PathBuf,
}

impl AsyncStdFileSystemScheme {
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
impl Scheme for AsyncStdFileSystemScheme {
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
			async_std::fs::create_dir_all(parent_path).await?;
		}
		let file = OpenOptions::from(options).open(path).await?;
		// let node = AsyncStdFileSystemNode {
		// 	file,
		// };
		let node = AsyncStdFileSystemNode {
			file,
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
		if path.is_file() {
			async_std::fs::remove_file(&path).await?;
		} else if path.is_dir() {
			if force {
				async_std::fs::remove_dir_all(&path).await?;
			} else {
				async_std::fs::remove_dir(&path).await?;
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
		if let Ok(metadata) = async_std::fs::metadata(path).await {
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
			let url = url.clone();
			let stream = async_std::fs::read_dir(&path)
				.await?
				.filter_map(move |found| {
					if let Ok(entry) = found {
						if let Some(entry_subpath) = entry.file_name().to_str() {
							if let Ok(entry_url) = url.join(entry_subpath) {
								Some(NodeEntry { url: entry_url })
							} else {
								None
							}
						} else {
							None
						}
					} else {
						None
					}
				});
			Ok(Box::pin(stream))
		} else {
			Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
		}
	}
}

pub struct AsyncStdFileSystemNode {
	file: async_std::fs::File,
	read: bool,
	write: bool,
}

#[async_trait::async_trait]
impl Node for AsyncStdFileSystemNode {
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
	// 		Some(&mut self.file)
	// 	} else {
	// 		None
	// 	}
	// }
	//
	// async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
	// 	if self.write {
	// 		Some(&mut self.file)
	// 	} else {
	// 		None
	// 	}
	// }
	//
	// async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
	// 	if self.read || self.write {
	// 		Some(&mut self.file)
	// 	} else {
	// 		None
	// 	}
	// }
}

impl AsyncRead for AsyncStdFileSystemNode {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<std::io::Result<usize>> {
		self.read
			.into_poll_io_then(|| Pin::new(&mut self.get_mut().file).poll_read(cx, buf))
	}

	fn poll_read_vectored(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &mut [IoSliceMut<'_>],
	) -> Poll<std::io::Result<usize>> {
		self.read
			.into_poll_io_then(|| Pin::new(&mut self.get_mut().file).poll_read_vectored(cx, bufs))
	}
}

impl AsyncWrite for AsyncStdFileSystemNode {
	fn poll_write(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<std::io::Result<usize>> {
		self.write
			.into_poll_io_then(|| Pin::new(&mut self.get_mut().file).poll_write(cx, buf))
	}

	fn poll_write_vectored(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		bufs: &[IoSlice<'_>],
	) -> Poll<std::io::Result<usize>> {
		self.write
			.into_poll_io_then(|| Pin::new(&mut self.get_mut().file).poll_write_vectored(cx, bufs))
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		self.write
			.into_poll_io_then(|| Pin::new(&mut self.get_mut().file).poll_flush(cx))
	}

	fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
		Pin::new(&mut self.get_mut().file).poll_close(cx)
	}
}

impl AsyncSeek for AsyncStdFileSystemNode {
	fn poll_seek(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		pos: SeekFrom,
	) -> Poll<std::io::Result<u64>> {
		(self.read || self.write)
			.into_poll_io_then(|| Pin::new(&mut self.get_mut().file).poll_seek(cx, pos))
	}
}

#[cfg(test)]
mod tests_general {
	// Unique per test
	use crate::AsyncStdFileSystemScheme as FileSystemScheme;
	use async_std::test as async_test;

	const FILE_CONTENT_TEST_LOC: &str = "fs:/test_node_writing_async_std.txt";
	const FILE_CONTENT_SEEK_TEST_LOC: &str = "fs:/test_node_seeking_async_std.txt";

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
	async fn list_nodes() {
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
	async fn metadata() {
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
