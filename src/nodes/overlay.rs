use crate::{ArcNode, CowArcNode, CowWeakNode, Node, WeakNode};
use std::ffi::OsStr;
use std::path::Components;
use std::sync::{Arc, RwLock, Weak};

#[derive(Debug)]
pub struct OverlayNode {
	this: RwLock<WeakNode>,
	pub overlays: Vec<ArcNode>,
}

impl OverlayNode {
	#[allow(clippy::new_ret_no_self)]
	pub fn new(overlays: impl IntoIterator<Item = ArcNode>) -> ArcNode {
		let self_arc = Arc::new(Self {
			this: RwLock::new(Weak::<Self>::new()),
			overlays: overlays.into_iter().collect(),
		});
		assert!(
			!self_arc.overlays.is_empty(),
			"there must always be at least one overlay"
		);
		let weak = Arc::downgrade(&self_arc);
		*self_arc.this.write().expect("impossible?") = weak;
		self_arc
	}
}

impl Node for OverlayNode {
	fn get_child_node_at(&self, name: &OsStr, components: &mut Components) -> Option<CowArcNode> {
		self.overlays.iter().fold(None, |acc, n| {
			acc.or_else(|| n.get_child_node_at(name, &mut components.clone()))
		})
	}

	fn get_parent_node(&self) -> CowWeakNode {
		self.overlays
			.first()
			.expect("missing overlays")
			.get_parent_node()
	}

	fn set_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
		constructor: &mut dyn FnMut(WeakNode) -> ArcNode,
	) -> Result<(), &'static str> {
		self.overlays.iter().fold(Err("no overlays"), |acc, n| {
			acc.or_else(|_err| n.set_child_node_at(name, &mut components.clone(), constructor))
		})
	}

	fn remove_child_node_at(
		&self,
		name: &OsStr,
		components: &mut Components,
	) -> Result<(), &'static str> {
		self.overlays.iter().fold(Err("no overlays"), |acc, n| {
			acc.or_else(|_err| n.remove_child_node_at(name, &mut components.clone()))
		})
	}
}

#[cfg(test)]
mod tests {
	use crate::nodes::directory::DirectoryNode;
	use crate::nodes::file_system::FileSystemDirectoryNode;
	use crate::nodes::noop::NoopNode;
	use crate::nodes::overlay::OverlayNode;
	use crate::System;
	use std::sync::Arc;

	#[test]
	fn shadowing_children() {
		let system = System::new();
		system
			.set_node_at_path("/testing", |w| {
				OverlayNode::new(vec![
					DirectoryNode::new(w.clone()),
					FileSystemDirectoryNode::new(w.clone(), std::env::current_dir().unwrap(), true),
					DirectoryNode::new(w.clone()),
				])
			})
			.unwrap();
		let overlay: Arc<OverlayNode> = system
			.get_node_at_path("/testing")
			.unwrap()
			.downcast_arc()
			.unwrap();
		overlay.overlays[2]
			.set_child("blah", |w| NoopNode::new(w))
			.unwrap();
		let deep_child = overlay.overlays[2].get_child("blah").unwrap();
		assert_eq!(
			system
				.get_node_at_path("/testing/blah")
				.unwrap()
				.downcast_ref::<NoopNode>()
				.unwrap() as *const _,
			deep_child.downcast_ref::<NoopNode>().unwrap() as *const _,
			"not the same `blah` deep node"
		);
		overlay.overlays[0]
			.set_child("blah", |w| NoopNode::new(w))
			.unwrap();
		assert_ne!(
			system
				.get_node_at_path("/testing/blah")
				.unwrap()
				.downcast_ref::<NoopNode>()
				.unwrap() as *const _,
			deep_child.downcast_ref::<NoopNode>().unwrap() as *const _,
			"should not be the same `blah` node to the deep node but it is"
		);
	}

	#[test]
	fn can_shadow_children_by_set() {
		let system = System::new();
		system
			.set_node_at_path("/testing", |w| {
				OverlayNode::new(vec![
					DirectoryNode::new(w.clone()),
					FileSystemDirectoryNode::new(w.clone(), std::env::current_dir().unwrap(), true),
					DirectoryNode::new(w.clone()),
				])
			})
			.unwrap();
		let overlay: Arc<OverlayNode> = system
			.get_node_at_path("/testing")
			.unwrap()
			.downcast_arc()
			.unwrap();
		overlay.overlays[2]
			.set_child("bloop", |w| NoopNode::new(w))
			.unwrap();
		system
			.get_node_at_path("/testing/bloop")
			.unwrap()
			.downcast_ref::<NoopNode>()
			.unwrap();
		system
			.set_node_at_path("/testing/bloop", |w| DirectoryNode::new(w))
			.unwrap();
		system
			.get_node_at_path("/testing/bloop")
			.unwrap()
			.downcast_ref::<DirectoryNode>()
			.unwrap();
		// And can still access something in the middle node
		system
			.get_node_at_path("/testing/target/debug")
			.unwrap()
			.downcast_ref::<FileSystemDirectoryNode>()
			.unwrap();
	}
}
