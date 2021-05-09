use crate::node_implementation_helpers::next_useful_component;
use crate::{ArcNode, CowArcNode, CowWeakNode, Node, WeakNode};
use std::ffi::OsStr;
use std::path::{Components, PathBuf};
use std::sync::{Arc, RwLock, Weak};

/// Symbolic link node, it fully delegates everything it can to another node and a path within that.
///
/// It has some overhead due to it being a weak reference to the other root node, so if it is
/// removed then this one will start to do nothing.
#[derive(Debug)]
pub struct SymlinkNode {
	this: RwLock<WeakNode>,
	parent: WeakNode,
	delegate_root: WeakNode,
	delegate_path: Option<PathBuf>,
}

impl SymlinkNode {
	#[allow(clippy::new_ret_no_self)]
	pub fn new(parent: WeakNode, delegate_root: &ArcNode, delegate_path: PathBuf) -> ArcNode {
		#[allow(clippy::manual_map)]
		let delegate_path = match next_useful_component(&mut delegate_path.components()) {
			None => None,
			Some(_) => Some(delegate_path),
		};
		let delegate_root = Arc::downgrade(delegate_root);
		let self_arc = Arc::new(Self {
			this: RwLock::new(Weak::<Self>::new()),
			parent,
			delegate_root,
			delegate_path,
		});
		let weak = Arc::downgrade(&self_arc);
		*self_arc.this.write().expect("impossible?") = weak;
		self_arc
	}

	pub fn get_delegate_node(&self) -> Option<ArcNode> {
		match self.delegate_root.upgrade() {
			None => None,
			Some(root) => match &self.delegate_path {
				None => Some(root),
				Some(path) => root.get_child(path).map(CowArcNode::into_owned),
			},
		}
	}
}

impl Node for SymlinkNode {
	fn get_child_node_at(&self, name: &OsStr, components: &mut Components) -> Option<CowArcNode> {
		self.get_delegate_node()?
			.get_child_node_at(name, components)
			.map(|n| CowArcNode::Owned(n.into_owned()))
	}

	fn get_parent_node(&self) -> CowWeakNode {
		CowWeakNode::Borrowed(&self.parent)
	}

	fn set_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
		constructor: &mut dyn FnMut(WeakNode) -> ArcNode,
	) -> Result<(), &'static str> {
		self.get_delegate_node()
			.ok_or("symlink root is missing")?
			.set_child_node_at(name, components, constructor)
	}

	fn remove_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
	) -> Result<(), &'static str> {
		self.get_delegate_node()
			.ok_or("symlink root is missing")?
			.remove_child_node_at(name, components)
	}
}

#[cfg(test)]
mod tests {
	use crate::nodes::file_system::FileSystemDirectoryNode;
	use crate::nodes::symlink::SymlinkNode;
	use crate::System;

	#[test]
	fn symlink_works() {
		let system = System::new();
		system
			.set_node_at_path("/fs", |w| {
				FileSystemDirectoryNode::new(w, std::env::current_dir().unwrap(), true)
			})
			.unwrap();
		{
			let fs = system.get_node_at_path("/fs").unwrap();
			system
				.set_node_at_path("/sl", |w| SymlinkNode::new(w, &fs, "/".into()))
				.unwrap();
			system
				.set_node_at_path("/ssl", |w| SymlinkNode::new(w, &fs, "/target".into()))
				.unwrap();
		}
		system.get_node_at_path("/fs/target/debug").unwrap();
		system.get_node_at_path("/sl/target/debug").unwrap();
		system.get_node_at_path("/ssl/debug").unwrap();
	}
}
