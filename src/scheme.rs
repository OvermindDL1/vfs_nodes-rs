use crate::{as_any_cast, Node, SchemeError, Vfs};
use futures_lite::Stream;
use std::pin::Pin;
use url::Url;

#[derive(Debug, Clone)]
pub struct NodeMetadata {
	/// If this is true then `get_node` should usually return a Node for this URL, else not, like if
	/// it is a directory for example.
	pub is_node: bool,
	/// The length of the data if it is knowable, shortest possible to longest possible if knowable.
	pub len: Option<(usize, Option<usize>)>,
}

pub struct NodeEntry {
	pub url: Url,
}

// copied from futures-core because futures-lite doesn't re-export it and there's no point not to
// just add it here anyway.  Plus making this one static anyway as it's just going to be used for
// return a read_dir
pub type ReadDirStream = Pin<Box<dyn Stream<Item = NodeEntry> + Send + 'static>>;

/// This is modeled after `std::fs::OpenOptions`, same definitions for the options.
#[derive(Clone, Debug, Default)]
pub struct NodeGetOptions {
	read: bool,
	write: bool,
	append: bool,
	truncate: bool,
	create: bool,
	create_new: bool,
}

impl NodeGetOptions {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn get_read(&self) -> bool {
		self.read
	}

	pub fn get_write(&self) -> bool {
		self.write
	}

	pub fn get_append(&self) -> bool {
		self.append
	}

	pub fn get_truncate(&self) -> bool {
		self.truncate
	}

	pub fn get_create(&self) -> bool {
		self.create
	}

	pub fn get_create_new(&self) -> bool {
		self.create_new
	}

	pub fn read(self, read: bool) -> Self {
		Self { read, ..self }
	}

	pub fn write(self, write: bool) -> Self {
		Self { write, ..self }
	}

	pub fn append(self, append: bool) -> Self {
		Self { append, ..self }
	}

	pub fn truncate(self, truncate: bool) -> Self {
		let write = if truncate { true } else { self.write };
		Self {
			write,
			truncate,
			..self
		}
	}

	pub fn create(self, create: bool) -> Self {
		let write = if create { true } else { self.write };
		Self {
			write,
			create,
			..self
		}
	}

	pub fn create_new(self, create_new: bool) -> Self {
		let (write, create) = if create_new {
			(true, true)
		} else {
			(self.write, self.create)
		};
		Self {
			write,
			create,
			create_new,
			..self
		}
	}
}

impl From<NodeGetOptions> for std::fs::OpenOptions {
	fn from(opts: NodeGetOptions) -> Self {
		let mut opener = std::fs::OpenOptions::new();
		opener
			.read(opts.read)
			.write(opts.write)
			.append(opts.append)
			.truncate(opts.truncate)
			.create(opts.create)
			.create_new(opts.create_new);
		opener
	}
}

#[cfg(feature = "backend_async_std")]
impl From<&NodeGetOptions> for async_std::fs::OpenOptions {
	fn from(opts: &NodeGetOptions) -> Self {
		let mut opener = async_std::fs::OpenOptions::new();
		opener
			.read(opts.read)
			.write(opts.write)
			.append(opts.append)
			.truncate(opts.truncate)
			.create(opts.create)
			.create_new(opts.create_new);
		opener
	}
}

#[cfg(feature = "backend_tokio")]
impl From<&NodeGetOptions> for tokio::fs::OpenOptions {
	fn from(opts: &NodeGetOptions) -> Self {
		let mut opener = tokio::fs::OpenOptions::new();
		opener
			.read(opts.read)
			.write(opts.write)
			.append(opts.append)
			.truncate(opts.truncate)
			.create(opts.create)
			.create_new(opts.create_new);
		opener
	}
}

#[async_trait::async_trait]
pub trait Scheme: as_any_cast::AsAnyCast + Sync + 'static {
	/// Get a node with the requested permission options
	async fn get_node<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
		options: &NodeGetOptions,
	) -> Result<Box<dyn Node>, SchemeError<'a>>;
	/// Request to remove a node, the force option is scheme dependently defined, or ignored.
	async fn remove_node<'a>(
		&self,
		vfs: &Vfs,
		url: &'a Url,
		force: bool,
	) -> Result<(), SchemeError<'a>>;
	async fn metadata<'a>(&self, vfs: &Vfs, url: &'a Url) -> Result<NodeMetadata, SchemeError<'a>>;
	/// List a set of nodes related to a given `url`.  Note, depending on the backend this can and
	/// will include duplicates, recursive paths, directories that aren't actually nodes,, etc...
	/// It's your job to figure out what you want.
	async fn read_dir<'a>(&self, vfs: &Vfs, url: &'a Url)
		-> Result<ReadDirStream, SchemeError<'a>>;
}

impl dyn Scheme {
	pub fn downcast_ref<T: Scheme>(&self) -> Option<&T> {
		self.as_any().downcast_ref()
	}

	pub fn downcast_mut<T: Scheme>(&mut self) -> Option<&mut T> {
		self.as_any_mut().downcast_mut()
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use crate::tests::*;

	#[test]
	fn node_access() {
		let mut vfs = Vfs::empty_with_capacity(10);
		vfs.add_default_schemes().unwrap();
	}
}
