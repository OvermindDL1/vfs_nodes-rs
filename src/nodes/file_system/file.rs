use crate::{ArcNode, CowArcNode, CowWeakNode, Node, WeakNode};
use std::ffi::OsStr;
use std::path::{Components, PathBuf};
use std::sync::{Arc, RwLock, Weak};

#[derive(Debug)]
pub struct FileSystemFileNode {
	this: RwLock<WeakNode>,
	parent: WeakNode,
	file_path: PathBuf,
	read_only: bool,
}

impl FileSystemFileNode {
	#[allow(clippy::new_ret_no_self)]
	pub fn new(parent: WeakNode, file_path: PathBuf, read_only: bool) -> ArcNode {
		let self_arc = Arc::new(Self {
			this: RwLock::new(Weak::<Self>::new()),
			parent,
			file_path,
			read_only,
		});
		let weak = Arc::downgrade(&self_arc);
		*self_arc.this.write().expect("impossible?") = weak;
		self_arc
	}
}

impl Node for FileSystemFileNode {
	fn get_child_node_at(&self, _name: &OsStr, _components: &mut Components) -> Option<CowArcNode> {
		None
	}

	fn get_parent_node(&self) -> CowWeakNode {
		CowWeakNode::Borrowed(&self.parent)
	}

	fn set_child_node_at(
		&self,
		_name: &OsStr,
		_components: &mut Components,
		_constructor: &mut dyn FnMut(WeakNode) -> ArcNode,
	) -> Result<(), &'static str> {
		Err("cannot set a child node of a filesystem file")
	}

	fn remove_child_node_at(
		&self,
		_name: &OsStr,
		_components: &mut Components,
	) -> Result<(), &'static str> {
		Err("there are no child nodes of a filesystem file to remove")
	}
}

#[cfg(test)]
mod tests {
	use crate::nodes::file_system::FileSystemDirectoryNode;
	use crate::System;
	use std::borrow::Cow;

	#[test]
	fn read_write_file() {
		let system = System::new();
		system
			.set_node_at_path("/fs", |p| {
				FileSystemDirectoryNode::new(
					p,
					std::env::current_dir().unwrap().join("target"),
					false,
				)
			})
			.unwrap();
		// Create it if it doesn't already exist
		if system.get_node_at_path("/fs/testing_file").is_none() {
			system
				.set_node_at_path("/fs/testing_file", |_p| {
					FileSystemDirectoryNode::create_dir()
				})
				.unwrap();
		}
		if system
			.get_node_at_path("/fs/testing_file/test.txt")
			.is_some()
		{
			system
				.remove_node_at_path("/fs/testing_file/test.txt")
				.unwrap();
		}
		system
			.set_node_at_path("/fs/testing_file/test.txt", |_p| {
				FileSystemDirectoryNode::create_file(Cow::Borrowed("This is a test text file\n"))
			})
			.unwrap();
		let _file = system
			.get_node_at_path("/fs/testing_file/test.txt")
			.unwrap();
	}
}
