use crate::{ArcNode, CowArcNode, CowWeakNode, Node, WeakNode};
use std::ffi::OsStr;
use std::path::Components;
use std::sync::{Arc, RwLock, Weak};

#[derive(Debug)]
pub struct NoopNode {
	this: RwLock<WeakNode>,
	parent: WeakNode,
}

impl NoopNode {
	pub fn new(parent: WeakNode) -> ArcNode {
		let self_arc = Arc::new(Self {
			this: RwLock::new(Weak::<Self>::new()),
			parent,
		});
		let weak = Arc::downgrade(&self_arc);
		*self_arc.this.write().expect("impossible?") = weak;
		self_arc
	}
}

impl Node for NoopNode {
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
		Err("unsupported operation")
	}

	fn remove_child_node_at(
		&self,
		_name: &OsStr,
		_components: &mut Components,
	) -> Result<(), &'static str> {
		Err("unsupported operation")
	}
}
