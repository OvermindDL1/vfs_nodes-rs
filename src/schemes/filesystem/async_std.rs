use crate::scheme::NodeGetOptions;
use crate::{Node, Scheme, SchemeError, Vfs};
use async_std::fs::OpenOptions;
use futures_lite::{AsyncRead, AsyncSeek, AsyncWrite};
use std::borrow::Cow;
use std::path::PathBuf;
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
	) -> Result<Box<dyn Node>, SchemeError<'a>> {
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
		let node = file;
		Ok(Box::new(node))
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
				async_std::fs::remove_file(&path).await?;
			} else if path.is_dir() {
				if force {
					async_std::fs::remove_dir_all(&path).await?;
				} else {
					async_std::fs::remove_dir(&path).await?;
				}
			}
		}
		Ok(())
	}
}

#[async_trait::async_trait]
impl Node for async_std::fs::File {
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

// pub struct AsyncStdFileSystemNode {
// 	file: async_std::fs::File,
// }
//
// #[async_trait::async_trait]
// impl Node for AsyncStdFileSystemNode {
// 	async fn read<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncRead + Unpin)> {
// 		Some(&mut self.file)
// 	}
//
// 	async fn write<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncWrite + Unpin)> {
// 		Some(&mut self.file)
// 	}
//
// 	async fn seek<'s>(&'s mut self) -> Option<&'s mut (dyn AsyncSeek + Unpin)> {
// 		Some(&mut self.file)
// 	}
// }

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
	use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
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
		node.read()
			.await
			.unwrap()
			.read_to_string(&mut buffer)
			.await
			.unwrap();
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
		let writer = node.write().await.unwrap();
		writer
			.write_all(FILE_TEST_CONTENT.as_bytes())
			.await
			.unwrap();
		writer.flush().await.unwrap();
		let mut node = vfs
			.get_node(&u(FILE_CONTENT_TEST_LOC), &NodeGetOptions::new().read(true))
			.await
			.unwrap();
		let reader = node.read().await.unwrap();
		let mut buffer = String::new();
		reader.read_to_string(&mut buffer).await.unwrap();
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
		let writer = node.write().await.unwrap();
		writer
			.write_all(FILE_TEST_CONTENT.as_bytes())
			.await
			.unwrap();
		// Always be sure to flush before seeking if any writes were performed, not necessary for
		// async_std, but it is for tokio, and it's good form anyway when seeking.
		writer.flush().await.unwrap();
		node.seek()
			.await
			.unwrap()
			.seek(SeekFrom::Start(0))
			.await
			.unwrap();
		let reader = node.read().await.unwrap();
		let mut buffer = String::new();
		reader.read_to_string(&mut buffer).await.unwrap();
		vfs.remove_node(&u(FILE_CONTENT_SEEK_TEST_LOC), false)
			.await
			.unwrap();
		assert_eq!(&buffer, FILE_TEST_CONTENT);
	}
}
