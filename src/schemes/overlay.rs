use crate::scheme::NodeGetOptions;
use crate::{Node, Scheme, SchemeError, Vfs};
use std::borrow::Cow;
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
	pub fn builder_read(first_overlay: impl Scheme) -> OverlaySchemeBuilder {
		OverlaySchemeBuilder {
			overlays: vec![OverlayAccess::Read(Box::new(first_overlay))],
		}
	}

	pub fn builder_write(first_overlay: impl Scheme) -> OverlaySchemeBuilder {
		OverlaySchemeBuilder {
			overlays: vec![OverlayAccess::Write(Box::new(first_overlay))],
		}
	}

	pub fn builder_read_write(first_overlay: impl Scheme) -> OverlaySchemeBuilder {
		OverlaySchemeBuilder {
			overlays: vec![OverlayAccess::ReadWrite(Box::new(first_overlay))],
		}
	}

	pub fn append_read(&mut self, overlay: impl Scheme) -> &mut Self {
		self.overlays.push(OverlayAccess::Read(Box::new(overlay)));
		self
	}

	pub fn append_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.overlays.push(OverlayAccess::Write(Box::new(overlay)));
		self
	}

	pub fn append_read_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.overlays
			.push(OverlayAccess::ReadWrite(Box::new(overlay)));
		self
	}

	pub fn prepend_read(&mut self, overlay: impl Scheme) -> &mut Self {
		self.overlays
			.insert(0, OverlayAccess::Read(Box::new(overlay)));
		self
	}

	pub fn prepend_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.overlays
			.insert(0, OverlayAccess::Write(Box::new(overlay)));
		self
	}

	pub fn prepend_read_write(&mut self, overlay: impl Scheme) -> &mut Self {
		self.overlays
			.insert(0, OverlayAccess::ReadWrite(Box::new(overlay)));
		self
	}
}

impl OverlaySchemeBuilder {
	pub fn build(self) -> OverlayScheme {
		OverlayScheme {
			overlays: self.overlays,
		}
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
}

#[cfg(test)]
#[cfg(feature = "backend_tokio")]
mod async_tokio_tests {
	use crate::scheme::NodeGetOptions;
	use crate::{DataLoaderScheme, OverlayScheme, TokioFileSystemScheme, Vfs};
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
}
