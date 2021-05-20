use anyhow::Context;
use futures_lite::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, StreamExt};
use std::io::SeekFrom;
use std::path::PathBuf;
use url::Url;
use vfs_nodes::scheme::NodeGetOptions;
use vfs_nodes::*;

pub async fn run_vfs_examples(
	fs_scheme: impl Fn(PathBuf) -> Box<dyn Scheme>,
) -> anyhow::Result<()> {
	println!(
		"Let's get a path to store the example files in to, this is not important to the
library, it's just to keep the example files distinct.\n"
	);
	let root_path = get_example_root_path()?;

	println!(
		"First let's construct a new Vfs.
`default` will load a DataLoader scheme at the `data` name, if you don't want that then do
`Vfs::empty()` instead and you will have to `add_scheme` everything.  You can also call
`add_default_schemes` afterwards to add the default schemes anyway.\n"
	);
	let mut vfs = Vfs::empty();

	println!(
		"Next we'll add some schemes, they can be added/removed dynamically at any time as long as you
have `mut` access to the `Vfs`.\n");

	println!(
		"First scheme we'll add will just be a data loader, just something to parse a data url and
return its contents straight.  It holds no information and changes no information.  We'll put
it at `data`, as is customary for URL's, but not required:\n"
	);
	vfs.add_scheme("data", DataLoaderScheme::new())?;

	println!("Next scheme we'll add is in-memory storage, we can put things in, read, write, whatever.\n");
	vfs.add_scheme("mem", MemoryScheme::new())?;

	println!(
		"Next scheme we'll add will just be a filesystem scheme (passed in via `fs_scheme` to allow
for different runtimes in this case, normally you'd just directly add whatever you normally
use for your app.\n"
	);
	vfs.add_boxed_scheme("fs", fs_scheme(root_path.join("fs")))?;

	println!(
		"Next will be an overlay scheme, where multiple other schemes will be given to it.  In this
case we will do a traditional read/write top level filesystem with a read-only system below
it to fall back to, but you can add more with a variety of read/write access.\n"
	);
	vfs.add_scheme(
		"overlay",
		// Top one is read/write
		OverlayScheme::builder_boxed_read_write(fs_scheme(root_path.join("fs/overlay_rw")))
			// Next one is read-only
			.boxed_read(fs_scheme(root_path.join("fs")))
			// And lastly let's fall back to a symlink to the prior-added `mem`ory scheme when the
			// path `/mem` is accessed or to the prior-added data scheme if `/data` is accessed.
			// The empty path is valid too, in which case everything is redirected.
			.read(
				SymLinkScheme::builder()
					// So accessing `"overlay:/mem/blah"` will direct to `"mem:/linked/blah"`
					.link("/mem", Url::parse("mem:/linked")?)
					.link("/data", Url::parse("data:")?)
					.build(),
			)
			.build(),
	)?;

	println!(
		"Just going to define some quick helpers here for the open options.  These are the same as
in `std::fs::OpenOptions` with the same meanings, although internally a scheme can do as it
wants on its own.\n"
	);
	let read = &NodeGetOptions::new().read(true);
	let write = &NodeGetOptions::new().write(true);
	// Create implies write, but setting it anyway for visibility.  Truncating as well.
	let create_read_write = &NodeGetOptions::new()
		.create(true)
		.read(true)
		.write(true)
		.truncate(true);

	println!(
		"Normal read stuff, let's make a buffer for use in this example to read strings in to.\n"
	);
	let buffer = &mut String::new();

	println!(
		"Let's start by getting a node from a scheme, first we'll get a data node.  Notice you have to
`.await` when getting a node, as it can be things like file or even network access, so it can
take time.\n");
	let mut node = vfs.get_node_at("data:test%20string", read).await?;

	println!(
		"When we have a node then we can read it, write it, or seek it.  First let's get what location
it it that we are at since data nodes support seeks.  Notice we have to await on the 'access'
first before we can perform the action as this itself can also be a request depending on the
scheme being accessed\n");
	let cur_pos = node
		.seek()
		.await
		.context("unsupported")? // `context` is just part of `anyhow` error reporting, ignore it
		.seek(SeekFrom::Current(0))
		.await?;
	assert_eq!(
		cur_pos, 0,
		"The data scheme starts at position 0 by default"
	);

	println!("Let's see what data it contains:\n");
	node.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "test string");
	buffer.clear(); // Normal read stuff, reset your buffer when done with it before the next read.

	println!("We can't write to a data node though!\n");
	assert!(node.write().await.is_none());

	println!(
		"But we can write to a memory storage by default, so let's get a node read and write!  We need
to pass one of the create options if we want to create it if it doesn't exist, or create_new
if we want to \"only\" create it and fail if it doesn't exist, as per normal `std`.\n");
	let mut node = vfs.get_node_at("mem:/testing", create_read_write).await?;

	println!("Well it's empty, so let's write to it:\n");
	node.write()
		.await
		.context("unsupported")?
		.write_all("test string".as_bytes())
		.await?;

	println!("And let's read it back:\n");
	node.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "");
	buffer.clear();
	println!(
		"Oh no, it's empty!  Just like when writing a standard file it leaves the cursor where you
wrote so let's move it back to the beginning:\n"
	);
	node.seek()
		.await
		.context("unsupported")?
		.seek(SeekFrom::Start(0))
		.await?;
	println!("And read it now\n");
	node.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "test string");
	buffer.clear();
	println!(
		"The data is there.  We can also close and re-open the node to still get the same data:\n"
	);
	vfs.get_node_at("mem:/testing", read)
		.await?
		.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "test string");
	buffer.clear();
	println!(
		"Still there, but let's go ahead and remove it, the argument is whether to force removal or
not, which for a memory storage also forces an early memory clear in case anyone else is
reading or writing it as well, otherwise they still get their instance of it.\n"
	);
	vfs.remove_node_at("mem:/testing", false).await?;
	println!("And now it's gone:\n");
	assert!(vfs.get_node_at("mem:/testing", read).await.is_err());

	println!(
		"We can read and write the filesystem as well, we don't have any files yet (unless a previous
example run was run, but we'll ignore that) so let's create a new file:\n");
	let mut node = vfs.get_node_at("fs:/test.txt", create_read_write).await?;

	println!("Let's write data to it:\n");
	let writer = node.write().await.context("unsupported")?;
	writer
		.write_all("A string inside the file".as_bytes())
		.await?;
	println!("Standard filesystem stuff, don't forget to flush if you are reading it back anytime soon (do it anyway)!\n");
	writer.flush().await?;
	println!(
		"Feel free to go look at {:?} file to see the data written to it.\n",
		root_path.join("fs/test.txt").as_os_str()
	);

	println!("Same thing works in the overlay as well.\n");
	let mut node = vfs.get_node_at("overlay:/test.txt", read).await?;
	println!("And we can read that file back:\n");
	node.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "A string inside the file");
	buffer.clear();

	println!("We can't write to the file though:");
	assert!(node.write().await.is_none());

	println!("Well let's change the access to write so we can write to it:\n");
	assert!(vfs.get_node_at("overlay:/test.txt", write).await.is_err());
	println!(
		"But it doesn't exist writeably!  Remember it's an overlay filesystem, and we only have\
write access into `fs/overlay_rw`, so let's create a test.txt file there then:\n"
	);
	let mut node = vfs
		.get_node_at("overlay:/test.txt", create_read_write)
		.await?;
	println!("And let's write to it:\n");
	let writer = node.write().await.context("unsupported")?;
	writer.write_all("A different file".as_bytes()).await?;
	writer.flush().await?;
	println!("and now let's read that file back, we'll even re-open it as read-only to see which we get:\n");
	let mut node = vfs.get_node_at("overlay:/test.txt", read).await?;
	node.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "A different file");
	buffer.clear();
	println!(
		"We got only the new version of the file!  Remember we mounted the read-write area as\
a subdirectory of the read-only fs, so we can read the file from there too:\n"
	);
	let mut node = vfs
		.get_node_at("overlay:/overlay_rw/test.txt", read)
		.await?;
	node.read()
		.await
		.context("unsupported")?
		.read_to_string(buffer)
		.await?;
	assert_eq!(buffer, "A different file");
	buffer.clear();
	println!("And indeed, it is that same `different` file!\n");

	println!("Let's see what entries are in the `fs` root path for this example now:");
	let count = vfs
		.read_dir_at("fs:/")
		.await?
		.inspect(|entry| println!("\t{}", entry.url))
		.count()
		.await;
	println!("Ended up being {} files.\n", count);

	println!("Let's see some metadata of the `test.txt` file:");
	println!("{:?}\n", vfs.metadata_at("fs:/test.txt").await?);

	Ok(())
}

fn get_example_root_path() -> Result<PathBuf, std::io::Error> {
	let example_path_parent = std::env::current_dir()?.join("target");
	assert!(
		example_path_parent.exists(),
		"Run this example from the project root"
	);
	let example_name = std::env::current_exe()
		.expect("must run via the executable")
		.file_name()
		.expect("executable has a directory name?")
		.to_string_lossy()
		.into_owned();
	let example_path_root = example_path_parent.join(example_name);
	if example_path_root.exists() {
		println!("The example_path_root already exists, cleaning it out...\n");
		std::fs::remove_dir_all(&example_path_root)?;
	}
	std::fs::create_dir_all(&example_path_root)?;
	println!("Example Root Path: {:?}", example_path_root.as_os_str());
	Ok(example_path_root)
}
