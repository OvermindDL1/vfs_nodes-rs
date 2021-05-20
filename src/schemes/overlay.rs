use crate::scheme::{NodeEntry, NodeGetOptions, NodeMetadata, ReadDirStream};
use crate::{Node, Scheme, SchemeError, Vfs};
use futures_lite::Stream;
use std::borrow::Cow;
use std::option::Option::None;
use std::pin::Pin;
use std::task::{Context, Poll};
use url::Url;

#[derive(Debug)]
pub enum OverlayError {}

impl std::fmt::Display for OverlayError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("IMPOSSIBLE-ERROR")
	}
}

impl std::error::Error for OverlayError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		None
	}
}

enum OverlayAccess {
	Read(Box<dyn Scheme>),
	Write(Box<dyn Scheme>),
	ReadWrite(Box<dyn Scheme>),
}

pub struct OverlayScheme {
	overlays: Vec<OverlayAccess>,
}

pub struct OverlaySchemeBuilder {
	overlays: Vec<OverlayAccess>,
}

impl OverlayScheme {
	pub fn builder_boxed_read(first_overlay: Box<dyn Scheme>) -> OverlaySchemeBuilder {
		OverlaySchemeBuilder {
			overlays: vec![OverlayAccess::Read(first_overlay)],
		}
	}

	pub fn builder_boxed_write(first_overlay: Box<dyn Scheme>) -> OverlaySchemeBuilder {
		OverlaySchemeBuilder {
			overlays: vec![OverlayAccess::Write(first_overlay)],
		}
	}

	pub fn builder_boxed_read_write(first_overlay: Box<dyn Scheme>) -> OverlaySchemeBuilder {
		OverlaySchemeBuilder {
			overlays: vec![OverlayAccess::ReadWrite(first_overlay)],
		}
	}

	pub fn builder_read(first_overlay: impl Scheme) -> OverlaySchemeBuilder {
		Self::builder_boxed_read(Box::new(first_overlay))
	}

	pub fn builder_write(first_overlay: impl Scheme) -> OverlaySchemeBuilder {
		Self::builder_boxed_write(Box::new(first_overlay))
	}

	pub fn builder_read_write(first_overlay: impl Scheme) -> OverlaySchemeBuilder {
		Self::builder_boxed_read_write(Box::new(first_overlay))
	}

	pub fn append_boxed_read(&mut self, overlay: Box<dyn Scheme>) -> &mut Self {
		self.overlays.push(OverlayAccess::Read(overlay));
		self
	}

	pub fn append_boxed_write(&mut self, overlay: Box<dyn Scheme>) -> &mut Self {
		self.overlays.push(OverlayAccess::Write(overlay));
		self
	}

	pub fn append_boxed_read_write(&mut self, overlay: Box<dyn Scheme>) -> &mut Self {
		self.overlays.push(OverlayAccess::ReadWrite(overlay));
		self
	}

	pub fn prepend_boxed_read(&mut self, overlay: Box<dyn Scheme>) -> &mut Self {
		self.overlays.insert(0, OverlayAccess::Read(overlay));
		self
	}

	pub fn prepend_boxed_write(&mut self, overlay: Box<dyn Scheme>) -> &mut Self {
		self.overlays.insert(0, OverlayAccess::Write(overlay));
		self
	}

	pub fn prepend_boxed_read_write(&mut self, overlay: Box<dyn Scheme>) -> &mut Self {
		self.overlays.insert(0, OverlayAccess::ReadWrite(overlay));
		self
	}

	pub fn append_read(&mut self, overlay: impl Scheme) -> &mut Self {
		self.append_boxed_read(Box::new(overlay))
	}

	pub fn append_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.append_boxed_write(Box::new(overlay))
	}

	pub fn append_read_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.append_boxed_read_write(Box::new(overlay))
	}

	pub fn prepend_read(&mut self, overlay: impl Scheme) -> &mut Self {
		self.prepend_boxed_read(Box::new(overlay))
	}

	pub fn prepend_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.prepend_boxed_write(Box::new(overlay))
	}

	pub fn prepend_read_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.prepend_boxed_read_write(Box::new(overlay))
	}
}

impl OverlaySchemeBuilder {
	pub fn build(self) -> OverlayScheme {
		OverlayScheme {
			overlays: self.overlays,
		}
	}

	pub fn boxed_read(mut self, overlay: Box<dyn Scheme>) -> Self {
		self.overlays.push(OverlayAccess::Read(overlay));
		self
	}

	pub fn boxed_write(mut self, overlay: Box<dyn Scheme>) -> Self {
		self.overlays.push(OverlayAccess::Write(overlay));
		self
	}

	pub fn boxed_read_write(mut self, overlay: Box<dyn Scheme>) -> Self {
		self.overlays.push(OverlayAccess::ReadWrite(overlay));
		self
	}

	pub fn read(mut self, overlay: impl Scheme) -> Self {
		self.overlays.push(OverlayAccess::Read(Box::new(overlay)));
		self
	}

	pub fn write(mut self, overlay: impl Scheme) -> Self {
		self.overlays.push(OverlayAccess::Write(Box::new(overlay)));
		self
	}

	pub fn read_write(mut self, overlay: impl Scheme) -> Self {
		self.overlays
			.push(OverlayAccess::ReadWrite(Box::new(overlay)));
		self
	}
}

#[async_trait::async_trait]
impl Scheme for OverlayScheme {
	async fn get_node<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<Box<dyn Node>, SchemeError<'a>> {
		for overlay in self.overlays.iter() {
			let node = match overlay {
				OverlayAccess::Read(scheme) if options.get_read() => {
					Some(scheme.get_node(vfs, url, options))
				}
				OverlayAccess::Write(scheme) if options.get_write() => {
					Some(scheme.get_node(vfs, url, options))
				}
				OverlayAccess::ReadWrite(scheme) if options.get_read() || options.get_write() => {
					Some(scheme.get_node(vfs, url, options))
				}
				_ => None,
			};
			if let Some(node) = node {
				if let Ok(node) = node.await {
					return Ok(node);
				}
			}
		}
		Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
	}

	async fn remove_node<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
		force: bool,
	) -> Result<(), SchemeError<'a>> {
		for overlay in self.overlays.iter() {
			let node = match overlay {
				OverlayAccess::Read(_scheme) => None,
				OverlayAccess::Write(scheme) => Some(scheme.remove_node(vfs, url, force)),
				OverlayAccess::ReadWrite(scheme) => Some(scheme.remove_node(vfs, url, force)),
			};
			if let Some(node) = node {
				if let Ok(node) = node.await {
					return Ok(node);
				}
			}
		}
		Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
	}

	async fn metadata<'a>(&self, vfs: &Vfs, url: &'a Url) -> Result<NodeMetadata, SchemeError<'a>> {
		for overlay in self.overlays.iter() {
			let scheme = match overlay {
				OverlayAccess::Read(scheme) => scheme,
				OverlayAccess::Write(scheme) => scheme,
				OverlayAccess::ReadWrite(scheme) => scheme,
			};
			match scheme.metadata(vfs, url).await {
				Ok(metadata) => return Ok(metadata),
				Err(_error) => continue,
			}
		}
		Err(SchemeError::NodeDoesNotExist(Cow::Borrowed(url.path())))
	}

	async fn read_dir<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
	) -> Result<ReadDirStream, SchemeError<'a>> {
		let mut streams = Vec::with_capacity(self.overlays.len());
		for scheme in self.overlays.iter().rev().map(|overlay| match overlay {
			OverlayAccess::Read(scheme) => scheme,
			OverlayAccess::Write(scheme) => scheme,
			OverlayAccess::ReadWrite(scheme) => scheme,
		}) {
			if let Ok(stream) = scheme.read_dir(vfs, url).await {
				streams.push(stream);
			}
		}
		Ok(Box::pin(OverlayReadDir(streams)))
	}
}

struct OverlayReadDir(Vec<ReadDirStream>);

impl Stream for OverlayReadDir {
	type Item = NodeEntry;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		loop {
			if self.0.is_empty() {
				return Poll::Ready(None);
			}
			match self.0.last_mut().unwrap().as_mut().poll_next(cx) {
				Poll::Pending => return Poll::Pending,
				Poll::Ready(None) => {
					self.0.pop();
				}
				Poll::Ready(Some(entry)) => return Poll::Ready(Some(entry)),
			}
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.0
			.iter()
			.map(|s| s.size_hint())
			.reduce(|(ls, le), (rs, re)| (ls + rs, le.and_then(|le| re.map(|re| re + le))))
			.unwrap_or((0, Some(0)))
	}
}

#[cfg(test)]
#[cfg(feature = "backend_tokio")]
mod async_tokio_tests {
	use crate::scheme::NodeGetOptions;
	use crate::{DataLoaderScheme, OverlayScheme, TokioFileSystemScheme, Vfs};
	use futures_lite::StreamExt;
	use url::Url;

	fn u(s: &str) -> Url {
		Url::parse(s).unwrap()
	}

	#[tokio::test]
	async fn read_only_depth() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"overlay",
			OverlayScheme::builder_read(DataLoaderScheme::default())
				.read(TokioFileSystemScheme::new(std::env::current_dir().unwrap()))
				.build(),
		)
		.unwrap();
		assert!(vfs
			.get_node(&u("overlay:blah"), &NodeGetOptions::new().read(true))
			.await
			.is_ok(),);
		assert!(vfs
			.get_node(&u("overlay:/Cargo.toml"), &NodeGetOptions::new().read(true))
			.await
			.is_ok(),);
		assert!(vfs
			.get_node(&u("fs:/does/not/exist"), &NodeGetOptions::new().read(true))
			.await
			.is_err(),);
	}

	#[tokio::test]
	async fn read_dir() {
		let mut vfs = Vfs::default();
		vfs.add_scheme(
			"overlay",
			OverlayScheme::builder_read(DataLoaderScheme::default())
				.read(TokioFileSystemScheme::new(
					std::env::current_dir().unwrap().join("src/errors"),
				))
				.read(TokioFileSystemScheme::new(
					std::env::current_dir()
						.unwrap()
						.join("src/schemes/filesystem"),
				))
				.build(),
		)
		.unwrap();

		let data = 0;
		let errors = 3;
		let filesystem = 3;

		assert_eq!(
			vfs.read_dir_at("overlay:/").await.unwrap().count().await,
			data + errors + filesystem
		);
	}
}
